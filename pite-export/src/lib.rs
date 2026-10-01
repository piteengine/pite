// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite-export`: desktop export. Copies the engine binary plus only the
//! referenced game content into a runnable directory. Python is not
//! bundled: the launcher fails loudly without system Python 3.12.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub const SUPPORTED_PLATFORMS: &[&str] = &["linux", "windows"];

#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    pub platform: String,
    pub out_dir: Option<PathBuf>,
    pub binary: Option<PathBuf>,
    pub skip_python_check: bool,
}

#[derive(Debug)]
pub struct ExportReport {
    pub out_dir: PathBuf,
    pub binary: PathBuf,
    pub files: Vec<PathBuf>,
    pub main_scene: String,
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
    if !opts.skip_python_check && find_python().is_none() {
        anyhow::bail!(
            "export needs system Python 3.12 on PATH (or PITE_PYTHON); \
             the target machine must have it too. Pass --skip-python-check \
             for cross-machine builds. Pite does not bundle Python yet."
        );
    }
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

    let main_rel = manifest
        .project
        .main_scene
        .strip_prefix("res://")
        .map(str::to_string)
        .unwrap_or_else(|| manifest.project.main_scene.clone());
    if opts.platform == "windows" {
        std::fs::write(out.join("run.bat"), windows_launcher(&game_name, &bin_name, &main_rel))?;
    } else {
        let sh = out.join("run.sh");
        std::fs::write(&sh, unix_launcher(&game_name, &bin_name, &main_rel))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    std::fs::write(out.join("README.txt"), readme(&game_name, &opts.platform))?;

    Ok(ExportReport {
        out_dir: out,
        binary: bin_dest,
        files,
        main_scene: manifest.project.main_scene.clone(),
    })
}

fn unix_launcher(game: &str, bin: &str, main_rel: &str) -> String {
    format!(
        "#!/bin/sh\n\
         set -e\n\
         HERE=\"$(cd \"$(dirname \"$0\")\" && pwd)\"\n\
         if [ -n \"$PITE_PYTHON\" ]; then PY=\"$PITE_PYTHON\"; else PY=\"python3.12\"; fi\n\
         if ! command -v \"$PY\" >/dev/null 2>&1; then\n\
         echo \"error: {game} needs system Python 3.12 (tried $PY).\" >&2\n\
         echo \"Pite does not bundle Python yet; install CPython 3.12 or set PITE_PYTHON.\" >&2\n\
         exit 1\n\
         fi\n\
         exec \"$HERE/bin/{bin}\" run --scene \"$HERE/game/{main_rel}\" --no-reload \"$@\"\n"
    )
}

fn windows_launcher(game: &str, bin: &str, main_rel: &str) -> String {
    format!(
        "@echo off\r\n\
         set HERE=%~dp0\r\n\
         if defined PITE_PYTHON (set PY=%PITE_PYTHON%) else (set PY=python3.12)\r\n\
         where %PY% >nul 2>nul\r\n\
         if errorlevel 1 (\r\n\
         echo error: {game} needs system Python 3.12 on PATH. 1>&2\r\n\
         echo Pite does not bundle Python yet; install CPython 3.12 or set PITE_PYTHON. 1>&2\r\n\
         exit /b 1\r\n\
         )\r\n\
         \"%HERE%bin\\{bin}\" run --scene \"%HERE%game/{main_rel}\" --no-reload %*\r\n"
    )
}

fn readme(game: &str, platform: &str) -> String {
    format!(
        "{game} ({platform} export)\n\
         \n\
         Run: ./run.sh   (or run.bat on Windows)\n\
         Requires system Python 3.12 on PATH (PITE_PYTHON overrides detection).\n\
         Pite does not bundle Python yet.\n\
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
