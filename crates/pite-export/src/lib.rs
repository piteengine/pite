// SPDX-License-Identifier: MIT OR Apache-2.0
//! Export pipeline and standalone packaging tooling for the Pite game engine.
//!
//! Bundles standalone desktop game distributions for Linux, Windows, and macOS,
//! packaging the lean `pite-player` runner, referenced assets, and a portable
//! CPython 3.12 environment requiring no system dependencies on players' machines.

mod error;
pub mod player;
pub mod progress;
pub mod python_bundle;

pub use error::{ExportError, Result};
pub use player::{candidate_player_names, player_bin_name};
pub use progress::Progress;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use error::io_err;
use pite_assets::played_assets;
use pite_render::atlas::{sheet_for_sidecar, SIDECAR_EXT};

pub const SUPPORTED_PLATFORMS: &[&str] = &["linux", "windows", "macos"];

/// Human display name for an export platform (`windows` → `Windows`).
pub fn platform_label(platform: &str) -> &str {
    match platform {
        "windows" => "Windows",
        "linux" => "Linux",
        "macos" => "macOS",
        other => other,
    }
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub platform: String,
    pub out_dir: Option<PathBuf>,
    pub binary: Option<PathBuf>,
    pub skip_python_check: bool,
    /// Bundle a pinned CPython so the export runs without system Python.
    pub bundle_python: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            platform: String::new(),
            out_dir: None,
            binary: None,
            skip_python_check: false,
            bundle_python: true,
        }
    }
}

/// How the exported launcher finds its interpreter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonChoice {
    /// A pinned CPython staged into `<out>/python`.
    Bundled { zip: PathBuf },
    /// Fall back to whatever Python 3.12 the machine has.
    System,
}

#[derive(Debug)]
pub struct ExportReport {
    pub out_dir: PathBuf,
    pub binary: PathBuf,
    pub files: Vec<PathBuf>,
    pub main_scene: String,
    pub python: PythonChoice,
}

pub fn find_python() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PITE_PYTHON") {
        let p = PathBuf::from(p);
        return is_python312(&p).then_some(p);
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in ["python3.12", "python3.12.exe"] {
            let p = dir.join(name);
            if is_python312(&p) {
                return Some(p);
            }
        }
    }
    None
}

fn is_python312(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    let Ok(out) = std::process::Command::new(p).arg("--version").output() else {
        return false;
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    text.contains("Python 3.12")
}

/// Owns the export directory for the length of an export. Every failure path is
/// a `?`, so without this a run that dies partway — a wrong archive layout once
/// left 136 MB of unpacked CPython behind — also wedges the next attempt,
/// because the exists-guard refuses to overwrite what the failed run left.
struct ExportDir {
    path: PathBuf,
    complete: bool,
}

impl ExportDir {
    fn commit(&mut self) {
        self.complete = true;
    }
}

impl Drop for ExportDir {
    fn drop(&mut self) {
        if !self.complete {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

pub fn export_project(
    root: &Path,
    opts: &ExportOptions,
    progress: &mut dyn Progress,
) -> Result<ExportReport> {
    if !SUPPORTED_PLATFORMS.contains(&opts.platform.as_str()) {
        return Err(ExportError::UnsupportedPlatform {
            platform: opts.platform.clone(),
        });
    }
    let manifest = pite_project::load_manifest(root)?;
    let game_name = manifest
        .export
        .binary_name
        .clone()
        .unwrap_or_else(|| manifest.project.name.clone());
    let out = opts.out_dir.clone().unwrap_or_else(|| {
        root.join("dist")
            .join(format!("{game_name}-{}", opts.platform))
    });
    if out.exists() {
        return Err(ExportError::OutputExists { path: out });
    }
    let mut owned = ExportDir {
        path: out.clone(),
        complete: false,
    };
    let bin_name = if opts.platform == "windows" {
        format!("{game_name}.exe")
    } else {
        game_name.clone()
    };
    let game_dir = out.join("game");
    let bin_dir = out.join("bin");
    std::fs::create_dir_all(&game_dir)
        .map_err(|e| io_err(format!("cannot create {}", game_dir.display()), e))?;
    std::fs::create_dir_all(&bin_dir)
        .map_err(|e| io_err(format!("cannot create {}", bin_dir.display()), e))?;

    // The player binary and the interpreter are separate sections: each is
    // fetched (or reused from the cache) before any game content is copied.
    progress.section("Binary");
    let binary_src = select_binary(opts, progress)?;

    let staged = if opts.bundle_python {
        progress.section("Python");
        let spec =
            python_bundle::pinned(&opts.platform).ok_or_else(|| ExportError::NoBundledPython {
                platform: opts.platform.clone(),
            })?;
        let archive = python_bundle::ensure_archive(&spec, progress)?;
        let mut staged = python_bundle::stage(&spec, &archive, &out.join("python"), progress)?;
        if opts.platform == "windows" {
            // Windows resolves DLLs from the exe's own directory before
            // PATH, so the interpreter DLL lives next to the binary and
            // the staged copy is dropped instead of shipped twice.
            let dll = staged
                .lib
                .file_name()
                .expect("lib file has a name")
                .to_string_lossy()
                .into_owned();
            let dest = bin_dir.join(&dll);
            std::fs::copy(&staged.lib, &dest)
                .map_err(|e| io_err("cannot place the interpreter DLL next to the binary", e))?;
            std::fs::remove_file(&staged.lib)
                .map_err(|e| io_err(format!("cannot remove {}", staged.lib.display()), e))?;
            staged.lib = dest;
        }
        if let Some(pth) = spec.pth_name {
            let pth_body = windows_pth(&bin_dir, &staged)?;
            std::fs::write(bin_dir.join(pth), pth_body)
                .map_err(|e| io_err("cannot write the interpreter loader config", e))?;
        }
        Some(staged)
    } else {
        if !opts.skip_python_check && find_python().is_none() {
            return Err(ExportError::MissingSystemPython);
        }
        None
    };

    let mut files = collect_referenced(root, &manifest)?;
    files.sort();
    progress.section("Files");
    progress.step(&format!("copying {} project files", files.len()));
    for abs in &files {
        let rel = abs
            .strip_prefix(root)
            .map_err(|_| ExportError::OutsideProject { path: abs.clone() })?;
        let dest = game_dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| io_err(format!("cannot create {}", parent.display()), e))?;
        }
        std::fs::copy(abs, &dest)
            .map_err(|e| io_err(format!("cannot copy {}", abs.display()), e))?;
        progress.detail(&format!("Copied {}", rel_to(root, abs)));
    }
    let manifest_src = root.join(pite_project::MANIFEST_FILE);
    std::fs::copy(&manifest_src, game_dir.join(pite_project::MANIFEST_FILE))
        .map_err(|e| io_err(format!("cannot copy {}", manifest_src.display()), e))?;

    let input_map_src = root.join(pite_project::INPUT_MAP_FILE);
    if input_map_src.is_file() {
        std::fs::copy(&input_map_src, game_dir.join(pite_project::INPUT_MAP_FILE))
            .map_err(|e| io_err(format!("cannot copy {}", input_map_src.display()), e))?;
    }

    let bin_dest = bin_dir.join(&bin_name);
    std::fs::copy(&binary_src, &bin_dest)
        .map_err(|e| io_err(format!("cannot copy {}", binary_src.display()), e))?;
    python_bundle::strip_debug(&bin_dest);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin_dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| io_err(format!("cannot chmod {}", bin_dest.display()), e))?;
    }

    let main_rel = if manifest.project.main_scene.starts_with("res://") {
        pite_core::parse_res_rel(&manifest.project.main_scene)?
            .to_string_lossy()
            .replace('\\', "/")
    } else {
        manifest.project.main_scene.clone()
    };
    if opts.platform == "windows" {
        let bat = out.join("run.bat");
        std::fs::write(
            &bat,
            windows_launcher(&game_name, &bin_name, &main_rel, staged.as_ref()),
        )
        .map_err(|e| io_err(format!("cannot write {}", bat.display()), e))?;
    } else {
        let sh = out.join("run.sh");
        std::fs::write(
            &sh,
            unix_launcher(&game_name, &bin_name, &main_rel, &out, staged.as_ref()),
        )
        .map_err(|e| io_err(format!("cannot write {}", sh.display()), e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| io_err(format!("cannot chmod {}", sh.display()), e))?;
        }
    }
    let python = match staged {
        Some(staged) => PythonChoice::Bundled {
            zip: staged.stdlib_zip,
        },
        None => PythonChoice::System,
    };
    let readme_path = out.join("README.txt");
    std::fs::write(&readme_path, readme(&game_name, &opts.platform, &python))
        .map_err(|e| io_err(format!("cannot write {}", readme_path.display()), e))?;

    owned.commit();
    Ok(ExportReport {
        out_dir: out,
        binary: bin_dest,
        files,
        main_scene: manifest.project.main_scene.clone(),
        python,
    })
}

/// Bundled mode needs no Python on PATH: the loader finds `libpython` through
/// `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` (or a `$ORIGIN` RUNPATH when the binary was built with one)
/// and the stdlib comes from the zip. Proved on Linux and macOS.
fn unix_launcher(
    game: &str,
    bin: &str,
    main_rel: &str,
    out: &Path,
    bundle: Option<&python_bundle::StagedBundle>,
) -> String {
    match bundle {
        Some(bundle) => {
            let lib_dir = bundle.lib.parent().unwrap_or(&bundle.root);
            format!(
                "#!/bin/sh\n\
                 set -e\n\
                 HERE=\"$(cd \"$(dirname \"$0\")\" && pwd)\"\n\
                 LD_LIBRARY_PATH=\"$HERE/{lib_rel}\"${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\n\
                 export LD_LIBRARY_PATH\n\
                 DYLD_LIBRARY_PATH=\"$HERE/{lib_rel}\"${{DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}}\n\
                 export DYLD_LIBRARY_PATH\n\
                 PYTHONPATH=\"$HERE/{zip_rel}\"\n\
                 export PYTHONPATH\n\
                 exec \"$HERE/bin/{bin}\" \"$HERE/game/{main_rel}\" \"$@\"\n",
                lib_rel = rel_to(out, lib_dir),
                zip_rel = rel_to(out, &bundle.stdlib_zip),
            )
        }
        None => format!(
            "#!/bin/sh\n\
             set -e\n\
             HERE=\"$(cd \"$(dirname \"$0\")\" && pwd)\"\n\
             if [ -n \"$PITE_PYTHON\" ]; then PY=\"$PITE_PYTHON\"; else PY=\"python3.12\"; fi\n\
             if ! command -v \"$PY\" >/dev/null 2>&1; then\n\
             echo \"error: {game} needs system Python 3.12 (tried $PY).\" >&2\n\
             echo \"This export has no bundled interpreter; install CPython 3.12 or set PITE_PYTHON.\" >&2\n\
             exit 1\n\
             fi\n\
              exec \"$HERE/bin/{bin}\" \"$HERE/game/{main_rel}\" \"$@\"\n"
        ),
    }
}

/// Body of the Windows loader config. The interpreter reads this from beside its
/// own DLL and ignores `PYTHONPATH` entirely once it exists, so it is the only
/// thing that points it at the stdlib zip. Entries resolve against the DLL's
/// directory, and must stay relative: an absolute path would break the export as
/// soon as the folder is moved or copied to another machine.
fn windows_pth(bin_dir: &Path, bundle: &python_bundle::StagedBundle) -> Result<String> {
    let python_dir = bundle
        .root
        .file_name()
        .ok_or_else(|| ExportError::Message("staged python directory has no name".into()))?
        .to_string_lossy();
    let zip = bundle
        .stdlib_zip
        .file_name()
        .ok_or_else(|| ExportError::Message("staged stdlib zip has no name".into()))?
        .to_string_lossy();
    let parent = bin_dir
        .parent()
        .ok_or_else(|| ExportError::Message("bin directory has no parent".into()))?;
    if bundle.root.parent() != Some(parent) {
        return Err(ExportError::Message(format!(
            "staged python directory {} is not a sibling of {}; the loader config \
             cannot name a relative path to the stdlib",
            bundle.root.display(),
            bin_dir.display()
        )));
    }
    Ok(format!("..\\{python_dir}\\{zip}\r\n.\r\n"))
}

/// Windows resolves DLLs from the exe's own directory first, so the staged
/// interpreter sits next to `bin/<game>.exe`, and the `_pth` file beside it
/// carries the stdlib zip. No Python install, no PATH juggling.
fn windows_launcher(
    game: &str,
    bin: &str,
    main_rel: &str,
    bundle: Option<&python_bundle::StagedBundle>,
) -> String {
    match bundle {
        Some(_) => format!(
            "@echo off\r\n\
             set HERE=%~dp0\r\n\
             \"%HERE%bin\\{bin}\" \"%HERE%game/{main_rel}\" %*\r\n"
        ),
        None => format!(
            "@echo off\r\n\
             set HERE=%~dp0\r\n\
             if defined PITE_PYTHON (set PY=%PITE_PYTHON%) else (set PY=python3.12)\r\n\
             where %PY% >nul 2>nul\r\n\
             if errorlevel 1 (\r\n\
             echo error: {game} needs system Python 3.12 on PATH. 1>&2\r\n\
             echo This export has no bundled interpreter; install CPython 3.12 or set PITE_PYTHON. 1>&2\r\n\
             exit /b 1\r\n\
             )\r\n\
             \"%HERE%bin\\{bin}\" \"%HERE%game/{main_rel}\" %*\r\n"
        ),
    }
}

/// Which binary an export ships: an explicit `--binary`, else a `pite-player`
/// sibling named for the target platform. Engine bundles ship player binaries
/// for all platforms. If no sibling binary is found beside the engine, export
/// automatically downloads and caches the target platform's player binary.
/// The full `pite` binary is never shipped: it carries the editor.
fn select_binary(opts: &ExportOptions, progress: &mut dyn Progress) -> Result<PathBuf> {
    select_binary_from(opts, std::env::current_exe().ok(), progress)
}

fn select_binary_from(
    opts: &ExportOptions,
    current_exe: Option<PathBuf>,
    progress: &mut dyn Progress,
) -> Result<PathBuf> {
    if let Some(p) = &opts.binary {
        if !p.is_file() {
            return Err(ExportError::BinaryNotFound { path: p.clone() });
        }
        let name = p
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.display().to_string());
        progress.step(&format!("using provided player binary ({name})"));
        return Ok(p.clone());
    }
    if let Some(dir) = current_exe.as_ref().and_then(|e| e.parent()) {
        for name in candidate_player_names(&opts.platform, std::env::consts::OS) {
            let sibling = dir.join(&name);
            if sibling.is_file() {
                progress.step(&format!(
                    "using local player binary for {} ({name})",
                    platform_label(&opts.platform)
                ));
                return Ok(sibling);
            }
        }
    }
    match player::ensure_player_binary(&opts.platform, progress) {
        Ok(path) => Ok(path),
        Err(e) => Err(ExportError::Message(format!(
            "export needs a pite-player binary for {} next to {} (engine bundles ship player \
             binaries for all platforms), or pass --binary with a player binary; \
             download fallback failed: {e}",
            opts.platform,
            current_exe
                .as_deref()
                .unwrap_or(Path::new("pite"))
                .display()
        ))),
    }
}

/// Path of `target` relative to the export root, with `/` separators.
fn rel_to(root: &Path, target: &Path) -> String {
    target
        .strip_prefix(root)
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| target.to_string_lossy().into_owned())
}

fn readme(game: &str, platform: &str, python: &PythonChoice) -> String {
    let python_line = match python {
        PythonChoice::Bundled { .. } => format!(
            "Python: bundled CPython {} (pinned build {})
",
            python_bundle::PINNED_PYTHON,
            python_bundle::PINNED_BUILD
        ),
        PythonChoice::System => {
            "Requires system Python 3.12 on PATH (PITE_PYTHON overrides detection).\n".to_string()
        }
    };
    format!(
        "{game} ({platform} export)\n\
         \n\
         Run: ./run.sh   (or run.bat on Windows)\n\
         {python_line}\
         Exported runs never watch files: hot reload is a dev-only feature.\n"
    )
}

/// Every file under `dir`, recursively.
fn stage_tree(dir: &Path, out: &mut std::collections::HashSet<PathBuf>) -> Result<()> {
    for entry in
        std::fs::read_dir(dir).map_err(|e| io_err(format!("cannot read {}", dir.display()), e))?
    {
        let path = entry
            .map_err(|e| io_err(format!("cannot list entry in {}", dir.display()), e))?
            .path();
        if path.is_dir() {
            stage_tree(&path, out)?;
        } else if path.is_file() {
            out.insert(path);
        }
    }
    Ok(())
}

fn stage_plugin_tree(
    plugin_root: &Path,
    dir: &Path,
    editor_root: Option<&str>,
    out: &mut std::collections::HashSet<PathBuf>,
) -> Result<()> {
    for entry in
        std::fs::read_dir(dir).map_err(|e| io_err(format!("cannot read {}", dir.display()), e))?
    {
        let path = entry
            .map_err(|e| io_err(format!("cannot list entry in {}", dir.display()), e))?
            .path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if matches!(name, ".git" | "__pycache__" | "_cache") {
            continue;
        }
        if dir == plugin_root {
            if let Some(ed) = editor_root {
                if name == ed || name == format!("{ed}.py") {
                    continue;
                }
            }
        }
        if path.is_dir() {
            stage_plugin_tree(plugin_root, &path, editor_root, out)?;
        } else if path.is_file() {
            out.insert(path);
        }
    }
    Ok(())
}

fn collect_referenced(root: &Path, manifest: &pite_project::PiteManifest) -> Result<Vec<PathBuf>> {
    let mut files = HashSet::new();
    let mut scenes = vec![manifest.project.main_scene.clone()];
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(scene_ref) = scenes.pop() {
        if !visited.insert(scene_ref.clone()) {
            continue;
        }
        let scene_path = resolve_ref(root, None, &scene_ref).ok_or_else(|| {
            ExportError::UnresolvedReference {
                kind: "scene",
                reference: scene_ref.clone(),
            }
        })?;
        files.insert(scene_path.clone());
        let scene_dir = scene_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf());
        let doc = pite_scene::load_scene(&scene_path)?;
        for node in &doc.node {
            if let Some(script) = &node.script {
                let p = resolve_ref(root, Some(&scene_dir), &script.path).ok_or_else(|| {
                    ExportError::UnresolvedReference {
                        kind: "script",
                        reference: script.path.clone(),
                    }
                })?;
                files.insert(p.clone());
                let script_text = std::fs::read_to_string(&p)
                    .map_err(|e| io_err(format!("export cannot read {}", p.display()), e))?;
                for audio in played_assets(&script_text) {
                    files.insert(resolve_ref(root, Some(&scene_dir), audio).ok_or_else(|| {
                        ExportError::UnresolvedReference {
                            kind: "audio",
                            reference: audio.to_string(),
                        }
                    })?);
                }
            }
            for value in node.props.values() {
                if let toml::Value::String(s) = value {
                    if s.starts_with("res://") {
                        let asset = resolve_ref(root, Some(&scene_dir), s).ok_or_else(|| {
                            ExportError::UnresolvedReference {
                                kind: "asset",
                                reference: s.clone(),
                            }
                        })?;
                        if let Some(sheet) = atlas_sheet(&asset) {
                            files.insert(sheet);
                        }
                        files.insert(asset);
                    }
                }
            }
        }
        for inst in &doc.instance {
            scenes.push(inst.scene.clone());
        }
    }
    for extra in &manifest.export.include {
        files.insert(root.join(extra));
    }
    // Vendored third-party packages ship whole: the lock pins them, the
    // runtime puts them on sys.path in dev and export alike.
    let vendor_dir = root.join("vendor");
    if vendor_dir.is_dir() {
        stage_tree(&vendor_dir, &mut files)?;
    }

    // Stage enabled plugins' runtime files while stripping disabled plugins
    // and editor-only entrypoints so `pite-player` bundles stay lean.
    let loaded_plugins = pite_project::load_enabled_plugins(root, manifest);
    if let Some(err) = loaded_plugins.errors.first() {
        return Err(ExportError::Message(format!("plugin error: {err}")));
    }
    for plug in &loaded_plugins.plugins {
        let editor_root = plug
            .manifest
            .editor
            .as_deref()
            .and_then(|ep| ep.split(':').next())
            .and_then(|m| m.split('.').next())
            .filter(|s| !s.is_empty());
        stage_plugin_tree(&plug.dir, &plug.dir, editor_root, &mut files)?;
    }
    let mut files: Vec<PathBuf> = files.into_iter().collect();
    for f in &files {
        if !f.is_file() {
            return Err(ExportError::MissingReferencedFile { path: f.clone() });
        }
    }
    files.sort();
    Ok(files)
}

fn resolve_ref(root: &Path, scene_dir: Option<&Path>, r: &str) -> Option<PathBuf> {
    let base = scene_dir.unwrap_or(root);
    pite_core::resolve_project_ref(Some(root), base, r).ok()
}

/// An atlas sidecar is only half an asset: without its sheet the runtime draws
/// its magenta fallback. Resolved through the same helper `pite_runtime` uses,
/// so an export can never ship a different file than the game looks for.
fn atlas_sheet(atlas: &Path) -> Option<PathBuf> {
    atlas
        .file_name()?
        .to_str()?
        .ends_with(SIDECAR_EXT)
        .then(|| sheet_for_sidecar(atlas))
}

#[cfg(test)]
pub(crate) static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests;
