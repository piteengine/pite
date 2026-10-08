// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite-export`: desktop export. Copies the engine binary plus only the
//! referenced game content into a runnable directory, together with a pinned
//! CPython unless `--no-bundle-python` keeps the system-Python requirement.

pub mod player;
pub mod python_bundle;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use pite_render::atlas::{sheet_for_sidecar, SIDECAR_EXT};

pub const SUPPORTED_PLATFORMS: &[&str] = &["linux", "windows"];

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

pub fn export_project(root: &Path, opts: &ExportOptions) -> Result<ExportReport> {
    if !SUPPORTED_PLATFORMS.contains(&opts.platform.as_str()) {
        anyhow::bail!(
            "unsupported export platform {:?} (supported: linux, windows)",
            opts.platform
        );
    }
    let manifest = pite_project::load_manifest(root)?;
    let game_name = manifest
        .export
        .binary_name
        .clone()
        .unwrap_or_else(|| manifest.project.name.clone());
    let python = if opts.bundle_python {
        let spec = python_bundle::pinned(&opts.platform).with_context(|| {
            format!(
                "no pinned Python build for {}; pass --no-bundle-python to require \
                 system Python 3.12 instead",
                opts.platform
            )
        })?;
        let archive = python_bundle::ensure_archive(&spec)?;
        PythonChoice::Bundled {
            zip: archive.clone(),
        }
    } else {
        if !opts.skip_python_check && find_python().is_none() {
            anyhow::bail!(
                "export needs system Python 3.12 on PATH (or PITE_PYTHON); \
                 the target machine must have it too. Pass --skip-python-check \
                 for cross-machine builds, or drop --no-bundle-python to ship a \
                 pinned interpreter."
            );
        }
        PythonChoice::System
    };
    let binary_src = select_binary(opts)?;

    let out = opts.out_dir.clone().unwrap_or_else(|| {
        root.join("dist")
            .join(format!("{game_name}-{}", opts.platform))
    });
    if out.exists() {
        anyhow::bail!("{} exists; remove it or pass --out <dir>", out.display());
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
    std::fs::create_dir_all(&game_dir)?;
    std::fs::create_dir_all(&bin_dir)?;

    let mut files = collect_referenced(root, &manifest)?;
    files.sort();
    for abs in &files {
        let rel = abs.strip_prefix(root).with_context(|| {
            format!("export cannot reach outside the project: {}", abs.display())
        })?;
        let dest = game_dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(abs, &dest)?;
    }
    std::fs::copy(
        root.join(pite_project::MANIFEST_FILE),
        game_dir.join(pite_project::MANIFEST_FILE),
    )?;

    let bin_dest = bin_dir.join(&bin_name);
    std::fs::copy(&binary_src, &bin_dest)?;
    python_bundle::strip_debug(&bin_dest);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin_dest, std::fs::Permissions::from_mode(0o755))?;
    }

    let staged = match &python {
        PythonChoice::Bundled { zip } => {
            let spec = python_bundle::pinned(&opts.platform).expect("checked above");
            let mut staged = python_bundle::stage(&spec, zip, &out.join("python"))
                .context("cannot stage the bundled Python")?;
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
                    .context("cannot place the interpreter DLL next to the binary")?;
                std::fs::remove_file(&staged.lib)?;
                staged.lib = dest;
            }
            if let Some(pth) = spec.pth_name {
                let pth_body = windows_pth(&bin_dir, &staged)?;
                std::fs::write(bin_dir.join(pth), pth_body)
                    .context("cannot write the interpreter loader config")?;
            }
            Some(staged)
        }
        PythonChoice::System => None,
    };

    let main_rel = manifest
        .project
        .main_scene
        .strip_prefix("res://")
        .map(str::to_string)
        .unwrap_or_else(|| manifest.project.main_scene.clone());
    if opts.platform == "windows" {
        std::fs::write(
            out.join("run.bat"),
            windows_launcher(&game_name, &bin_name, &main_rel, staged.as_ref()),
        )?;
    } else {
        let sh = out.join("run.sh");
        std::fs::write(
            &sh,
            unix_launcher(&game_name, &bin_name, &main_rel, &out, staged.as_ref()),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    std::fs::write(
        out.join("README.txt"),
        readme(&game_name, &opts.platform, &python),
    )?;

    let python = match staged {
        Some(staged) => PythonChoice::Bundled {
            zip: staged.stdlib_zip,
        },
        None => PythonChoice::System,
    };
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
/// `LD_LIBRARY_PATH` (or a `$ORIGIN` RUNPATH when the binary was built with one)
/// and the stdlib comes from the zip. Proved on Linux.
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
                 PYTHONPATH=\"$HERE/{zip_rel}\"\n\
                 export PYTHONPATH\n\
                 exec \"$HERE/bin/{bin}\" --scene \"$HERE/game/{main_rel}\" \"$@\"\n",
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
              exec \"$HERE/bin/{bin}\" --scene \"$HERE/game/{main_rel}\" \"$@\"\n"
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
        .context("staged python directory has a name")?
        .to_string_lossy();
    let zip = bundle
        .stdlib_zip
        .file_name()
        .context("staged stdlib zip has a name")?
        .to_string_lossy();
    let parent = bin_dir.parent().context("bin directory has a parent")?;
    if bundle.root.parent() != Some(parent) {
        anyhow::bail!(
            "staged python directory {} is not a sibling of {}; the loader config \
             cannot name a relative path to the stdlib",
            bundle.root.display(),
            bin_dir.display()
        );
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
             \"%HERE%bin\\{bin}\" --scene \"%HERE%game/{main_rel}\" %*\r\n"
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
             \"%HERE%bin\\{bin}\" --scene \"%HERE%game/{main_rel}\" %*\r\n"
        ),
    }
}

/// Which binary an export ships, in priority order: an explicit `--binary`,
/// a `pite-player` sibling of the running `pite` for same-platform exports,
/// else the pinned release template (what makes cross-platform export work
/// with no compiler installed). The full `pite` binary is never shipped:
/// it carries the editor.
fn select_binary(opts: &ExportOptions) -> Result<PathBuf> {
    select_binary_from(opts, std::env::current_exe().ok())
}

fn select_binary_from(opts: &ExportOptions, current_exe: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = &opts.binary {
        if !p.is_file() {
            anyhow::bail!("engine binary not found: {}", p.display());
        }
        return Ok(p.clone());
    }
    if opts.platform == std::env::consts::OS {
        if let Some(dir) = current_exe.as_ref().and_then(|e| e.parent()) {
            let sibling = dir.join(player::bin_name(&opts.platform));
            if sibling.is_file() {
                return Ok(sibling);
            }
        }
        anyhow::bail!(
            "export needs a pite-player binary beside {} (`cargo build -p pite-player`), \
             or pass --binary with a player binary",
            current_exe
                .as_deref()
                .unwrap_or(Path::new("pite"))
                .display()
        )
    } else {
        let spec = player::pinned(&opts.platform).with_context(|| {
            format!(
                "no pinned player template for {}; pass --binary with a locally built \
                 pite-player instead",
                opts.platform
            )
        })?;
        player::ensure_cached(&spec)
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

fn collect_referenced(root: &Path, manifest: &pite_project::PiteManifest) -> Result<Vec<PathBuf>> {
    let mut files = HashSet::new();
    let mut scenes = vec![manifest.project.main_scene.clone()];
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(scene_ref) = scenes.pop() {
        if !visited.insert(scene_ref.clone()) {
            continue;
        }
        let scene_path = resolve_ref(root, None, &scene_ref)
            .with_context(|| format!("export cannot resolve scene {scene_ref:?}"))?;
        files.insert(scene_path.clone());
        let scene_dir = scene_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf());
        let doc = pite_scene::load_scene(&scene_path)
            .with_context(|| format!("export cannot parse {}", scene_path.display()))?;
        for node in &doc.node {
            if let Some(script) = &node.script {
                let p = resolve_ref(root, Some(&scene_dir), &script.path)
                    .with_context(|| format!("export cannot resolve script {:?}", script.path))?;
                files.insert(p.clone());
                for audio in played_assets(
                    &std::fs::read_to_string(&p)
                        .with_context(|| format!("export cannot read {}", p.display()))?,
                ) {
                    files.insert(
                        resolve_ref(root, Some(&scene_dir), audio)
                            .with_context(|| format!("export cannot resolve audio {audio:?}"))?,
                    );
                }
            }
            for value in node.props.values() {
                if let toml::Value::String(s) = value {
                    if s.starts_with("res://") {
                        let asset = resolve_ref(root, Some(&scene_dir), s)
                            .with_context(|| format!("export cannot resolve asset {s:?}"))?;
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
    let mut files: Vec<PathBuf> = files.into_iter().collect();
    for f in &files {
        if !f.is_file() {
            anyhow::bail!("export references missing file: {}", f.display());
        }
    }
    files.sort();
    Ok(files)
}

fn resolve_ref(root: &Path, scene_dir: Option<&Path>, r: &str) -> Option<PathBuf> {
    if let Some(rel) = r.strip_prefix("res://") {
        return Some(root.join(rel));
    }
    let p = PathBuf::from(r);
    if p.is_absolute() {
        return Some(p);
    }
    scene_dir.map(|d| d.join(p))
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

fn played_assets(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        for marker in ["play(\"", "play('"] {
            let mut rest = line;
            while let Some(pos) = rest.find(marker) {
                let before = &rest[..pos];
                if before.ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    rest = &rest[pos + marker.len()..];
                    continue;
                }
                let quote = marker.as_bytes()[marker.len() - 1] as char;
                rest = &rest[pos + marker.len()..];
                if let Some(end) = rest.find(quote) {
                    out.push(&rest[..end]);
                    rest = &rest[end + 1..];
                } else {
                    break;
                }
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SERIAL;

    const MAIN: &str = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "hero"
type = "Sprite2D"
name = "Hero"
parent = "root"

[node.props]
atlas = "res://assets/sheet.atlas.json"
frame = "player"

[[node]]
id = "spr"
type = "Sprite2D"
name = "Spr"
parent = "root"

[node.props]
texture = "res://assets/a.png"

[node.script]
path = "res://scripts/m.py"
class = "M"

[[instance]]
scene = "res://scenes/sub.pitescene"
parent = "root"
prefix = "s_"
"#;

    const SUB: &str = r#"format_version = 1
root = "subroot"

[[node]]
id = "subroot"
type = "Node"
name = "Sub"

[[node]]
id = "pic"
type = "Sprite2D"
name = "Pic"
parent = "subroot"

[node.props]
texture = "res://assets/b.png"
"#;

    const SCRIPT: &str = r#"import pite

class M(pite.Node):
    pass

def _once():
    pite.play("res://sfx/hit.wav")
    # play("res://sfx/comment.wav")
    display("res://sfx/nope.wav")
"#;

    const ATLAS: &str = r#"{
  "texture": "sheet.png",
  "size": [64, 32],
  "frames": { "player": { "x": 0, "y": 0, "w": 32, "h": 32 } }
}
"#;

    fn fixture(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("pite-exp-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::create_dir_all(dir.join("sfx")).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"fixt\"\n").unwrap();
        std::fs::write(dir.join("scenes").join("main.pitescene"), MAIN).unwrap();
        std::fs::write(dir.join("scenes").join("sub.pitescene"), SUB).unwrap();
        std::fs::write(dir.join("scripts").join("m.py"), SCRIPT).unwrap();
        std::fs::write(dir.join("assets").join("a.png"), "A").unwrap();
        std::fs::write(dir.join("assets").join("b.png"), "B").unwrap();
        std::fs::write(dir.join("assets").join("unused.png"), "U").unwrap();
        std::fs::write(dir.join("assets").join("sheet.atlas.json"), ATLAS).unwrap();
        std::fs::write(dir.join("assets").join("sheet.png"), "S").unwrap();
        std::fs::write(dir.join("assets").join("orphan.png"), "O").unwrap();
        std::fs::write(dir.join("sfx").join("hit.wav"), "W").unwrap();
        let fake_bin = dir.join("fakebin");
        std::fs::write(&fake_bin, "FAKEBIN").unwrap();
        (dir, fake_bin)
    }

    fn opts(bin: &Path) -> ExportOptions {
        ExportOptions {
            platform: "linux".to_string(),
            out_dir: None,
            binary: Some(bin.to_path_buf()),
            skip_python_check: true,
            bundle_python: false,
        }
    }

    fn game_has(out: &Path, rel: &str) -> bool {
        out.join("game").join(rel).is_file()
    }

    #[test]
    fn layout_copies_only_referenced_files() {
        let (dir, bin) = fixture("layout");
        let report = export_project(&dir, &opts(&bin)).unwrap();
        let out = &report.out_dir;
        for rel in [
            "pite.toml",
            "scenes/main.pitescene",
            "scenes/sub.pitescene",
            "scripts/m.py",
            "assets/a.png",
            "assets/b.png",
            "assets/sheet.atlas.json",
            "assets/sheet.png",
            "sfx/hit.wav",
        ] {
            assert!(game_has(out, rel), "missing {rel}");
        }
        assert!(!game_has(out, "assets/unused.png"));
        assert!(!game_has(out, "assets/orphan.png"));
        assert!(out.join("bin").join("fixt").is_file());
        let sh = std::fs::read_to_string(out.join("run.sh")).unwrap();
        assert!(sh.contains("--scene"));
        assert!(!sh.contains(" run --scene"));
        assert!(sh.contains("game/scenes/main.pitescene"));
        assert!(out.join("README.txt").is_file());
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(out).ok();
    }

    #[test]
    fn missing_python_fails_loudly() {
        let _guard = SERIAL.lock().unwrap();
        let prev = std::env::var_os("PITE_PYTHON");
        std::env::set_var("PITE_PYTHON", "/nonexistent/python3.12");
        let (dir, bin) = fixture("nopy");
        let err = export_project(
            &dir,
            &ExportOptions {
                skip_python_check: false,
                ..opts(&bin)
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("Python 3.12"), "{err:#}");
        match prev {
            Some(v) => std::env::set_var("PITE_PYTHON", v),
            None => std::env::remove_var("PITE_PYTHON"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn python_override_passes_gate() {
        let _guard = SERIAL.lock().unwrap();
        let prev = std::env::var_os("PITE_PYTHON");
        let (dir, bin) = fixture("pyok");
        let fake = dir.join("fake312");
        std::fs::write(&fake, "#!/bin/sh\necho \"Python 3.12.99\"\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::env::set_var("PITE_PYTHON", &fake);
        let report = export_project(
            &dir,
            &ExportOptions {
                skip_python_check: false,
                ..opts(&bin)
            },
        )
        .unwrap();
        match prev {
            Some(v) => std::env::set_var("PITE_PYTHON", v),
            None => std::env::remove_var("PITE_PYTHON"),
        }
        std::fs::remove_dir_all(&report.out_dir).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bundling_stages_runtime_and_stdlib_next_to_the_game() {
        let Some(spec) = python_bundle::pinned("linux") else {
            panic!("linux must have a pinned bundle");
        };
        let dir = std::env::temp_dir().join(format!("pite-stage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("python/lib/python3.12/site-packages")).unwrap();
        std::fs::write(
            dir.join("python/lib").join("libpython3.12.so.1.0"),
            b"fake-so",
        )
        .unwrap();
        std::fs::write(
            dir.join("python/lib/python3.12").join("os.py"),
            b"import sys\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("python/lib/python3.12/site-packages")
                .join("pip.py"),
            b"raise SystemExit\n",
        )
        .unwrap();
        let archive = dir.join("archive.tar.gz");
        let tar = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&dir)
            .arg("python")
            .output();
        if !tar.as_ref().is_ok_and(|o| o.status.success()) {
            eprintln!("skipped: tar unavailable");
            return;
        }
        let out = dir.join("out");
        let staged = python_bundle::stage(&spec, &archive, &out).unwrap();
        assert!(staged.lib.ends_with("libpython3.12.so.1.0"), "{staged:?}");
        assert!(staged.lib.is_file());
        assert!(staged.stdlib_zip.is_file());
        let listing = std::process::Command::new("python3")
            .arg("-c")
            .arg("import zipfile,sys; print(sorted(zipfile.ZipFile(sys.argv[1]).namelist()))")
            .arg(&staged.stdlib_zip)
            .output()
            .expect("python3 lists the zip");
        let listing = String::from_utf8_lossy(&listing.stdout).into_owned();
        assert!(listing.contains("os.py"), "{listing}");
        assert!(!listing.contains("site-packages"), "{listing}");
        assert!(
            !out.join(".staging").exists(),
            "staging dir must be cleaned up"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Real download + export layout. Gated: it fetches a pinned 60 MB archive
    /// the first time, then runs from the local toolchain cache.
    #[test]
    fn bundled_export_layout_has_runtime_and_stdlib() {
        if std::env::var("PITE_BUNDLE_SMOKE").is_err() {
            eprintln!("skipped: set PITE_BUNDLE_SMOKE=1 (downloads the pinned Python once)");
            return;
        }
        let _guard = SERIAL.lock().unwrap();
        let (dir, bin) = fixture("bundle");
        let report = export_project(
            &dir,
            &ExportOptions {
                platform: "linux".to_string(),
                out_dir: None,
                binary: Some(bin.clone()),
                skip_python_check: false,
                bundle_python: true,
            },
        )
        .expect("bundled export must succeed");
        match &report.python {
            PythonChoice::Bundled { zip } => assert!(zip.is_file(), "{zip:?}"),
            other => panic!("expected a bundled interpreter, got {other:?}"),
        }
        let out = &report.out_dir;
        assert!(
            out.join("bin").join("fixt").is_file(),
            "engine binary missing"
        );
        assert!(
            python_bundle::pinned("linux").is_some_and(|spec| {
                let staged = spec.lib.rsplit('/').next().unwrap_or(spec.lib);
                out.join("python/lib").join(staged).is_file()
            }),
            "bundled libpython missing under python/lib"
        );
        assert!(
            out.join("python").join("python312.zip").is_file(),
            "stdlib zip missing"
        );
        let sh = std::fs::read_to_string(out.join("run.sh")).unwrap();
        assert!(sh.contains("LD_LIBRARY_PATH"), "{sh}");
        assert!(sh.contains("PYTHONPATH"), "{sh}");
        assert!(
            !sh.contains("needs system Python"),
            "bundled launcher must not gate"
        );
        let readme = std::fs::read_to_string(out.join("README.txt")).unwrap();
        assert!(readme.contains("bundled CPython"), "{readme}");
        std::fs::remove_dir_all(&report.out_dir).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bundling_without_network_support_points_at_the_opt_out() {
        let _guard = SERIAL.lock().unwrap();
        let prev = std::env::var_os("PITE_TOOLCHAIN_DIR");
        let dir = std::env::temp_dir().join(format!("pite-offline-{}", std::process::id()));
        std::env::set_var("PITE_TOOLCHAIN_DIR", &dir);
        let err = export_project(
            &dir,
            &ExportOptions {
                platform: "ps5".to_string(),
                ..opts(Path::new("unused"))
            },
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("unsupported export platform"),
            "{err:#}"
        );
        match prev {
            Some(v) => std::env::set_var("PITE_TOOLCHAIN_DIR", v),
            None => std::env::remove_var("PITE_TOOLCHAIN_DIR"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn exported_manifest_runs_round_trip() {
        let (dir, bin) = fixture("roundtrip");
        let report = export_project(&dir, &opts(&bin)).unwrap();
        let scene = report
            .out_dir
            .join("game")
            .join("scenes")
            .join("main.pitescene");
        let manifest = pite_project::load_manifest(&report.out_dir.join("game")).unwrap();
        assert_eq!(manifest.project.name, "fixt");
        let session =
            pite_runtime::GameSession::open(&scene, true).expect("exported scene must open");
        assert_eq!(session.tree_len(), 5);
        assert_eq!(session.script_count(), 1);
        std::fs::remove_dir_all(&report.out_dir).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A run that dies after creating its output must not leave that directory
    /// behind: the next attempt refuses to overwrite it, so the failure becomes
    /// sticky. Unreadable engine binary fails the copy step, which runs after
    /// the game and bin directories already exist.
    #[cfg(unix)]
    #[test]
    fn failed_export_leaves_no_directory_behind() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = SERIAL.lock().unwrap();
        let (dir, bin) = fixture("rollback");
        let out = dir.join("rollback-out");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&bin).is_ok() {
            eprintln!("skipped: cannot make the binary unreadable as this user");
            std::fs::remove_dir_all(&dir).ok();
            return;
        }
        let err = export_project(
            &dir,
            &ExportOptions {
                out_dir: Some(out.clone()),
                ..opts(&bin)
            },
        )
        .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("permission")
                || err.to_string().to_lowercase().contains("denied"),
            "expected the copy to fail on permissions, got: {err:#}"
        );
        assert!(!out.exists(), "failed export left {} behind", out.display());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The loader config is the only thing pointing the interpreter at the stdlib
    /// once it exists, and its entries resolve against the DLL's directory. An
    /// absolute path would work on the build machine and break the moment the
    /// export is copied, so pin the exact shape.
    #[test]
    fn windows_loader_config_names_the_stdlib_relatively() {
        let bundle = python_bundle::StagedBundle {
            root: PathBuf::from("/game/python"),
            lib: PathBuf::from("/game/python/lib/python312.dll"),
            stdlib_zip: PathBuf::from("/game/python/python312.zip"),
        };
        let pth = windows_pth(Path::new("/game/bin"), &bundle).unwrap();
        assert_eq!(pth, "..\\python\\python312.zip\r\n.\r\n");
        assert!(
            !pth.contains("/game"),
            "loader config must stay relative: {pth:?}"
        );
    }

    /// A staged `python/` that is not a sibling of `bin/` has no nameable
    /// relative path: error rather than write an entry that cannot resolve.
    #[test]
    fn windows_loader_config_rejects_an_unreachable_stdlib() {
        let bundle = python_bundle::StagedBundle {
            root: PathBuf::from("/elsewhere/python"),
            lib: PathBuf::from("/elsewhere/python/lib/python312.dll"),
            stdlib_zip: PathBuf::from("/elsewhere/python/python312.zip"),
        };
        let err = windows_pth(Path::new("/game/bin"), &bundle)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a sibling"), "{err}");
    }

    /// Windows export layout, end to end against the real pinned archive. Skips
    /// when that archive is not cached, so CI stays offline.
    #[test]
    fn windows_export_ships_the_interpreter_beside_the_binary() {
        let Some(spec) = python_bundle::pinned("windows") else {
            panic!("windows must have a pinned bundle");
        };
        if !python_bundle::archive_path(&spec).is_file() {
            eprintln!("skipped: windows archive not cached (export once to populate it)");
            return;
        }
        let _guard = SERIAL.lock().unwrap();
        let (dir, bin) = fixture("winbundle");
        let report = export_project(
            &dir,
            &ExportOptions {
                platform: "windows".to_string(),
                out_dir: None,
                binary: Some(bin),
                skip_python_check: false,
                bundle_python: true,
            },
        )
        .expect("windows bundled export must succeed");
        let out = &report.out_dir;
        let dll = spec.lib.rsplit('/').next().unwrap_or(spec.lib);
        assert!(
            out.join("bin").join(dll).is_file(),
            "interpreter must sit next to the exe, not in python/lib"
        );
        assert!(
            !out.join("python/lib").join(dll).exists(),
            "interpreter must not ship twice"
        );
        let pth = out
            .join("bin")
            .join(spec.pth_name.expect("windows needs a loader config"));
        let body = std::fs::read_to_string(&pth).unwrap();
        assert!(
            body.contains("python312.zip") && !body.contains(&out.display().to_string()),
            "loader config must name the zip relatively: {body:?}"
        );
        let bat = std::fs::read_to_string(out.join("run.bat")).unwrap();
        assert!(!bat.contains("PYTHONPATH"), "{bat}");
        assert!(
            !bat.contains("needs system Python"),
            "bundled launcher must not gate"
        );
        assert!(
            !out.join("python/.staging").exists(),
            "staging dir must be cleaned up"
        );
        std::fs::remove_dir_all(&report.out_dir).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    fn bin_opts(platform: &str, binary: Option<PathBuf>) -> ExportOptions {
        ExportOptions {
            platform: platform.to_string(),
            out_dir: None,
            binary,
            skip_python_check: true,
            bundle_python: false,
        }
    }

    fn sel_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pite-sel-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn explicit_binary_wins_over_sibling() {
        let dir = sel_dir("explicit");
        let fake_exe = dir.join("pite");
        let explicit = dir.join("custom-player");
        let sibling = dir.join(player::bin_name(std::env::consts::OS));
        for p in [&fake_exe, &explicit, &sibling] {
            std::fs::write(p, "x").unwrap();
        }
        let got = select_binary_from(
            &bin_opts(std::env::consts::OS, Some(explicit.clone())),
            Some(fake_exe),
        )
        .unwrap();
        assert_eq!(got, explicit);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sibling_player_serves_native_export() {
        let dir = sel_dir("sibling");
        let fake_exe = dir.join("pite");
        let sibling = dir.join(player::bin_name(std::env::consts::OS));
        std::fs::write(&fake_exe, "x").unwrap();
        std::fs::write(&sibling, "x").unwrap();
        let got =
            select_binary_from(&bin_opts(std::env::consts::OS, None), Some(fake_exe)).unwrap();
        assert_eq!(got, sibling);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_sibling_fails_with_build_hint() {
        let dir = sel_dir("nosibling");
        let fake_exe = dir.join("pite");
        std::fs::write(&fake_exe, "x").unwrap();
        let err = select_binary_from(&bin_opts(std::env::consts::OS, None), Some(fake_exe))
            .unwrap_err()
            .to_string();
        assert!(err.contains("pite-player"), "got: {err}");
        assert!(err.contains("--binary"), "got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn foreign_platform_skips_sibling_for_template() {
        let foreign = if std::env::consts::OS == "windows" {
            "linux"
        } else {
            "windows"
        };
        let dir = sel_dir("foreign");
        let fake_exe = dir.join("pite");
        let decoy = dir.join(player::bin_name(foreign));
        std::fs::write(&fake_exe, "x").unwrap();
        std::fs::write(&decoy, "x").unwrap();
        let err = select_binary_from(&bin_opts(foreign, None), Some(fake_exe))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no published player template"), "got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_platform_fails_loudly() {
        let (dir, bin) = fixture("plat");
        let err = export_project(
            &dir,
            &ExportOptions {
                platform: "ps5".to_string(),
                ..opts(&bin)
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("unsupported export platform"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
