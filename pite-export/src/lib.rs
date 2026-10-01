// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite-export`: desktop export. Copies the engine binary plus only the
//! referenced game content into a runnable directory. Python is not
//! bundled: the launcher fails loudly without system Python 3.12.

pub mod python_bundle;
pub mod sha256;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

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
    let binary_src = match &opts.binary {
        Some(p) => p.clone(),
        None => std::env::current_exe().context("cannot locate engine binary")?,
    };
    if !binary_src.is_file() {
        anyhow::bail!("engine binary not found: {}", binary_src.display());
    }
    if opts.platform == "windows"
        && std::env::consts::OS != "windows"
        && opts.binary.is_none()
    {
        anyhow::bail!(
            "cross-compiling a windows binary is out of scope: build on Windows \
             (`cargo build --release -p pite-cli`) and pass it with --binary"
        );
    }

    let out = opts.out_dir.clone().unwrap_or_else(|| {
        root.join("dist")
            .join(format!("{game_name}-{}", opts.platform))
    });
    if out.exists() {
        anyhow::bail!("{} exists; remove it or pass --out <dir>", out.display());
    }
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin_dest, std::fs::Permissions::from_mode(0o755))?;
    }

    let staged = match &python {
        PythonChoice::Bundled { zip } => {
            let spec = python_bundle::pinned(&opts.platform).expect("checked above");
            let staged = python_bundle::stage(&spec, zip, &out.join("python"))
                .context("cannot stage the bundled Python")?;
            if opts.platform == "windows" {
                // Windows resolves DLLs from the exe's own directory before
                // PATH, so the interpreter DLL sits next to the binary.
                let dll = staged
                    .lib
                    .file_name()
                    .expect("lib file has a name")
                    .to_string_lossy()
                    .into_owned();
                std::fs::copy(&staged.lib, bin_dir.join(&dll))
                    .context("cannot place the interpreter DLL next to the binary")?;
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
            windows_launcher(&game_name, &bin_name, &main_rel, &out, staged.as_ref()),
        )?;
    } else {
        let sh = out.join("run.sh");
        std::fs::write(&sh, unix_launcher(&game_name, &bin_name, &main_rel, &out, staged.as_ref()))?;
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
        Some(staged) => PythonChoice::Bundled { zip: staged.stdlib_zip },
        None => PythonChoice::System,
    };
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
                 exec \"$HERE/bin/{bin}\" run --scene \"$HERE/game/{main_rel}\" --no-reload \"$@\"\n",
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
             exec \"$HERE/bin/{bin}\" run --scene \"$HERE/game/{main_rel}\" --no-reload \"$@\"\n"
        ),
    }
}

/// Windows resolves DLLs from the exe's own directory first, so the staged
/// `python3.dll` is copied next to `bin/<game>.exe`; `PYTHONPATH` carries the
/// stdlib zip. No Python install, no PATH juggling.
fn windows_launcher(
    game: &str,
    bin: &str,
    main_rel: &str,
    out: &Path,
    bundle: Option<&python_bundle::StagedBundle>,
) -> String {
    match bundle {
        Some(bundle) => {
            let zip_rel = rel_to(out, &bundle.stdlib_zip);
            format!(
                "@echo off\r\n\
                 set HERE=%~dp0\r\n\
                 set PYTHONPATH=\"%HERE%{zip_rel}\"\r\n\
                 \"%HERE%bin\\{bin}\" run --scene \"%HERE%game/{main_rel}\" --no-reload %*\r\n"
            )
        }
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
             \"%HERE%bin\\{bin}\" run --scene \"%HERE%game/{main_rel}\" --no-reload %*\r\n"
        ),
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
        PythonChoice::System => "Requires system Python 3.12 on PATH (PITE_PYTHON overrides detection).\n"
            .to_string(),
    };
    format!(
        "{game} ({platform} export)\n\
         \n\
         Run: ./run.sh   (or run.bat on Windows)\n\
         {python_line}\
         Exported runs never watch files: hot reload is a dev-only feature.\n"
    )
}

fn collect_referenced(
    root: &Path,
    manifest: &pite_project::PiteManifest,
) -> Result<Vec<PathBuf>> {
    let mut files = HashSet::new();
    let mut scenes = vec![manifest.project.main_scene.clone()];
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(scene_ref) = scenes.pop() {
        if !visited.insert(scene_ref.clone()) {
            continue;
        }
        let scene_path = resolve_ref(root, None, &scene_ref).with_context(|| {
            format!("export cannot resolve scene {scene_ref:?}")
        })?;
        files.insert(scene_path.clone());
        let scene_dir = scene_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf());
        let doc = pite_scene::load_scene(&scene_path)
            .with_context(|| format!("export cannot parse {}", scene_path.display()))?;
        for node in &doc.node {
            if let Some(script) = &node.script {
                let p = resolve_ref(root, Some(&scene_dir), &script.path).with_context(|| {
                    format!("export cannot resolve script {:?}", script.path)
                })?;
                files.insert(p.clone());
                for audio in played_assets(&std::fs::read_to_string(&p).with_context(|| {
                    format!("export cannot read {}", p.display())
                })?) {
                    files.insert(resolve_ref(root, Some(&scene_dir), audio).with_context(|| {
                        format!("export cannot resolve audio {audio:?}")
                    })?);
                }
            }
            for value in node.props.values() {
                if let toml::Value::String(s) = value {
                    if s.starts_with("res://") {
                        files.insert(resolve_ref(root, Some(&scene_dir), s).with_context(|| {
                            format!("export cannot resolve asset {s:?}")
                        })?);
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
mod tests {
    use super::*;

    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    const MAIN: &str = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

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
            "sfx/hit.wav",
        ] {
            assert!(game_has(out, rel), "missing {rel}");
        }
        assert!(!game_has(out, "assets/unused.png"));
        assert!(out.join("bin").join("fixt").is_file());
        let sh = std::fs::read_to_string(out.join("run.sh")).unwrap();
        assert!(sh.contains("--no-reload"));
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
        let err = export_project(&dir, &ExportOptions {
            skip_python_check: false,
            ..opts(&bin)
        })
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
        let report = export_project(&dir, &ExportOptions {
            skip_python_check: false,
            ..opts(&bin)
        })
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
        std::fs::write(dir.join("python/lib/python3.12").join("os.py"), b"import sys\n").unwrap();
        std::fs::write(
            dir.join("python/lib/python3.12/site-packages").join("pip.py"),
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
        assert!(!out.join(".staging").exists(), "staging dir must be cleaned up");
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
        let report = export_project(&dir, &ExportOptions {
            platform: "linux".to_string(),
            out_dir: None,
            binary: Some(bin.clone()),
            skip_python_check: false,
            bundle_python: true,
        })
        .expect("bundled export must succeed");
        match &report.python {
            PythonChoice::Bundled { zip } => assert!(zip.is_file(), "{zip:?}"),
            other => panic!("expected a bundled interpreter, got {other:?}"),
        }
        let out = &report.out_dir;
        assert!(out.join("bin").join("fixt").is_file(), "engine binary missing");
        assert!(
            python_bundle::pinned("linux").is_some_and(|spec| {
                out.join("python").join(&spec.lib).is_file()
            }),
            "bundled libpython missing under python/"
        );
        assert!(out.join("python").join("python312.zip").is_file(), "stdlib zip missing");
        let sh = std::fs::read_to_string(out.join("run.sh")).unwrap();
        assert!(sh.contains("LD_LIBRARY_PATH"), "{sh}");
        assert!(sh.contains("PYTHONPATH"), "{sh}");
        assert!(!sh.contains("needs system Python"), "bundled launcher must not gate");
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
        let err = export_project(&dir, &ExportOptions {
            platform: "ps5".to_string(),
            ..opts(Path::new("unused"))
        })
        .unwrap_err();
        assert!(err.to_string().contains("unsupported export platform"), "{err:#}");
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
        let scene = report.out_dir.join("game").join("scenes").join("main.pitescene");
        let manifest = pite_project::load_manifest(&report.out_dir.join("game")).unwrap();
        assert_eq!(manifest.project.name, "fixt");
        let session =
            pite_runtime::GameSession::open(&scene, true).expect("exported scene must open");
        assert_eq!(session.tree_len(), 4);
        assert_eq!(session.script_count(), 1);
        std::fs::remove_dir_all(&report.out_dir).ok();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_platform_fails_loudly() {
        let (dir, bin) = fixture("plat");
        let err = export_project(&dir, &ExportOptions {
            platform: "ps5".to_string(),
            ..opts(&bin)
        })
        .unwrap_err();
        assert!(err.to_string().contains("unsupported export platform"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
