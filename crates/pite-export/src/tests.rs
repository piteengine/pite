// SPDX-License-Identifier: MIT OR Apache-2.0
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
atlas = "res://assets/sheet.ptatlas"
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
scene = "res://scenes/sub.ptscn"
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

const ATLAS: &str = r#"format_version = 1
texture = "sheet.png"
size = [64, 32]

[frames.player]
x = 0
y = 0
w = 32
h = 32
"#;

fn fixture(tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("pite-exp-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::create_dir_all(dir.join("sfx")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"fixt\"\n").unwrap();
    std::fs::write(dir.join("scenes").join("main.ptscn"), MAIN).unwrap();
    std::fs::write(dir.join("scenes").join("sub.ptscn"), SUB).unwrap();
    std::fs::write(dir.join("scripts").join("m.py"), SCRIPT).unwrap();
    std::fs::write(dir.join("assets").join("a.png"), "A").unwrap();
    std::fs::write(dir.join("assets").join("b.png"), "B").unwrap();
    std::fs::write(dir.join("assets").join("unused.png"), "U").unwrap();
    std::fs::write(dir.join("assets").join("sheet.ptatlas"), ATLAS).unwrap();
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
    let report = export_project(&dir, &opts(&bin), &mut ()).unwrap();
    let out = &report.out_dir;
    for rel in [
        "pite.toml",
        "scenes/main.ptscn",
        "scenes/sub.ptscn",
        "scripts/m.py",
        "assets/a.png",
        "assets/b.png",
        "assets/sheet.ptatlas",
        "assets/sheet.png",
        "sfx/hit.wav",
    ] {
        assert!(game_has(out, rel), "missing {rel}");
    }
    assert!(!game_has(out, "assets/unused.png"));
    assert!(!game_has(out, "assets/orphan.png"));
    assert!(out.join("bin").join("fixt").is_file());
    let sh = std::fs::read_to_string(out.join("run.sh")).unwrap();
    assert!(sh.contains("bin/fixt"));
    assert!(!sh.contains(" run "));
    assert!(sh.contains("game/scenes/main.ptscn"));
    assert!(out.join("README.txt").is_file());
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(out).ok();
}

#[test]
fn layout_stages_vendor_whole() {
    let (dir, bin) = fixture("vendor-stage");
    std::fs::create_dir_all(dir.join("vendor").join("vpkg")).unwrap();
    std::fs::write(dir.join("vendor").join("vpkg").join("__init__.py"), b"x=1").unwrap();
    std::fs::write(
        dir.join("vendor").join("pure-1.0-py3-none-any.whl"),
        b"fake-wheel",
    )
    .unwrap();
    let report = export_project(&dir, &opts(&bin), &mut ()).unwrap();
    let out = &report.out_dir;
    assert!(game_has(out, "vendor/vpkg/__init__.py"), "missing tree");
    assert!(
        game_has(out, "vendor/pure-1.0-py3-none-any.whl"),
        "missing wheel"
    );
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
        &mut (),
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
    // Windows cannot exec a shebang script — it needs `.bat`/`.cmd`/`.exe`.
    // Give each platform its natural fake-Python shape.
    #[cfg(unix)]
    let fake = {
        let path = dir.join("fake312");
        std::fs::write(&path, "#!/bin/sh\necho \"Python 3.12.99\"\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    #[cfg(windows)]
    let fake = {
        let path = dir.join("fake312.bat");
        std::fs::write(&path, "@echo off\r\necho Python 3.12.99\r\n").unwrap();
        path
    };
    std::env::set_var("PITE_PYTHON", &fake);
    let report = export_project(
        &dir,
        &ExportOptions {
            skip_python_check: false,
            ..opts(&bin)
        },
        &mut (),
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
    let staged = python_bundle::stage(&spec, &archive, &out, &mut ()).unwrap();
    assert!(staged.lib.ends_with("libpython3.12.so.1.0"), "{staged:?}");
    assert!(staged.lib.is_file());
    assert!(staged.stdlib_zip.is_file());
    // Inspect the stdlib zip with the `zip` crate, not `python3`
    // (Windows names it `python.exe`; the crate is portable).
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(&staged.stdlib_zip).unwrap()).unwrap();
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().unwrap().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n == "os.py"),
        "os.py missing, got: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.starts_with("site-packages/")),
        "site-packages leaked into the stdlib zip: {names:?}"
    );
    assert!(
        !out.join(".staging").exists(),
        "staging dir must be cleaned up"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn macos_bundling_stages_runtime_and_stdlib_next_to_the_game() {
    let Some(spec) = python_bundle::pinned("macos") else {
        panic!("macos must have a pinned bundle");
    };
    let dir = std::env::temp_dir().join(format!("pite-stage-mac-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("python/lib/python3.12/site-packages")).unwrap();
    std::fs::write(
        dir.join("python/lib").join("libpython3.12.dylib"),
        b"fake-dylib",
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
    let staged = python_bundle::stage(&spec, &archive, &out, &mut ()).unwrap();
    assert!(staged.lib.ends_with("libpython3.12.dylib"), "{staged:?}");
    assert!(staged.lib.is_file());
    assert!(staged.stdlib_zip.is_file());
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(&staged.stdlib_zip).unwrap()).unwrap();
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().unwrap().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n == "os.py"),
        "os.py missing, got: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.starts_with("site-packages/")),
        "site-packages leaked into the stdlib zip: {names:?}"
    );
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
        &mut (),
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
        &mut (),
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
    let report = export_project(&dir, &opts(&bin), &mut ()).unwrap();
    let scene = report
        .out_dir
        .join("game")
        .join("scenes")
        .join("main.ptscn");
    let manifest = pite_project::load_manifest(&report.out_dir.join("game")).unwrap();
    assert_eq!(manifest.project.name, "fixt");
    let session = pite_runtime::GameSession::open(&scene, true).expect("exported scene must open");
    assert_eq!(session.tree_len(), 5);
    // Renamed from `script_count` in commit 7. Only the M script is
    // attached; every other node is a base pyclass instance.
    assert_eq!(session.scripted_count(), 1);
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
        &mut (),
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
        &mut (),
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
    let sibling = dir.join(player_bin_name(std::env::consts::OS));
    for p in [&fake_exe, &explicit, &sibling] {
        std::fs::write(p, "x").unwrap();
    }
    let got = select_binary_from(
        &bin_opts(std::env::consts::OS, Some(explicit.clone())),
        Some(fake_exe),
        &mut (),
    )
    .unwrap();
    assert_eq!(got, explicit);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sibling_player_serves_native_export() {
    let dir = sel_dir("sibling");
    let fake_exe = dir.join("pite");
    let sibling = dir.join(player_bin_name(std::env::consts::OS));
    std::fs::write(&fake_exe, "x").unwrap();
    std::fs::write(&sibling, "x").unwrap();
    let got = select_binary_from(
        &bin_opts(std::env::consts::OS, None),
        Some(fake_exe),
        &mut (),
    )
    .unwrap();
    assert_eq!(got, sibling);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_sibling_fails_with_build_hint() {
    let _guard = SERIAL.lock().unwrap();
    let dir = sel_dir("nosibling");
    let fake_exe = dir.join("pite");
    std::fs::write(&fake_exe, "x").unwrap();
    let empty_cache = dir.join("empty_cache");
    std::fs::create_dir_all(&empty_cache).unwrap();
    std::env::set_var("PITE_PLAYER_DIR", &empty_cache);
    std::env::set_var("PITE_PLAYER_URL", "http://127.0.0.1:9/nonexistent");
    let err = select_binary_from(
        &bin_opts(std::env::consts::OS, None),
        Some(fake_exe),
        &mut (),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("pite-player"), "got: {err}");
    assert!(err.contains("--binary"), "got: {err}");
    std::env::remove_var("PITE_PLAYER_DIR");
    std::env::remove_var("PITE_PLAYER_URL");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_sibling_uses_cached_player() {
    let _guard = SERIAL.lock().unwrap();
    let dir = sel_dir("cached_player");
    let fake_exe = dir.join("pite");
    std::fs::write(&fake_exe, "x").unwrap();
    let cache = dir.join("cache");
    let platform = std::env::consts::OS;
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let cached_bin = cache
        .join(&version)
        .join(platform)
        .join(player_bin_name(platform));
    std::fs::create_dir_all(cached_bin.parent().unwrap()).unwrap();
    std::fs::write(&cached_bin, "cached-binary-content").unwrap();

    std::env::set_var("PITE_PLAYER_DIR", &cache);
    let got = select_binary_from(&bin_opts(platform, None), Some(fake_exe), &mut ()).unwrap();
    assert_eq!(got, cached_bin);
    std::env::remove_var("PITE_PLAYER_DIR");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_sibling_downloads_and_unpacks_player_tar_gz() {
    let _guard = SERIAL.lock().unwrap();
    let dir = sel_dir("dl_tar_gz");
    let fake_exe = dir.join("pite");
    std::fs::write(&fake_exe, "x").unwrap();
    let cache = dir.join("cache");
    let archive_src = dir.join("src");
    std::fs::create_dir_all(&archive_src).unwrap();
    let bin_src = archive_src.join("pite-player-linux");
    std::fs::write(&bin_src, "downloaded-player-content").unwrap();
    let archive_tar = dir.join("pite-player-v0.4.0-linux.tar.gz");

    let status = std::process::Command::new("tar")
        .args([
            "-czf",
            archive_tar.to_str().unwrap(),
            "-C",
            archive_src.to_str().unwrap(),
            "pite-player-linux",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    std::env::set_var("PITE_PLAYER_DIR", &cache);
    std::env::set_var(
        "PITE_PLAYER_URL",
        format!("file://{}", archive_tar.display()),
    );
    let got = select_binary_from(&bin_opts("linux", None), Some(fake_exe), &mut ()).unwrap();
    assert!(got.is_file());
    let content = std::fs::read_to_string(&got).unwrap();
    assert_eq!(content, "downloaded-player-content");

    std::env::remove_var("PITE_PLAYER_DIR");
    std::env::remove_var("PITE_PLAYER_URL");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_sibling_downloads_and_unpacks_player_zip() {
    let _guard = SERIAL.lock().unwrap();
    let dir = sel_dir("dl_zip");
    let fake_exe = dir.join("pite");
    std::fs::write(&fake_exe, "x").unwrap();
    let cache = dir.join("cache");
    let archive_zip = dir.join("pite-player-v0.4.0-windows.zip");

    {
        let file = std::fs::File::create(&archive_zip).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("pite-player-windows.exe", options).unwrap();
        std::io::Write::write_all(&mut zip, b"windows-player-content").unwrap();
        zip.finish().unwrap();
    }

    std::env::set_var("PITE_PLAYER_DIR", &cache);
    std::env::set_var(
        "PITE_PLAYER_URL",
        format!("file://{}", archive_zip.display()),
    );
    let got = select_binary_from(&bin_opts("windows", None), Some(fake_exe), &mut ()).unwrap();
    assert!(got.is_file());
    let content = std::fs::read_to_string(&got).unwrap();
    assert_eq!(content, "windows-player-content");

    std::env::remove_var("PITE_PLAYER_DIR");
    std::env::remove_var("PITE_PLAYER_URL");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn foreign_platform_uses_bundled_sibling() {
    // Engine bundles ship both players, so a foreign-named sibling next
    // to the exe is picked with no download and no compiler involved.
    let foreign = if std::env::consts::OS == "windows" {
        "linux"
    } else {
        "windows"
    };
    let dir = sel_dir("foreign");
    let fake_exe = dir.join("pite");
    let sibling = dir.join(player_bin_name(foreign));
    std::fs::write(&fake_exe, "x").unwrap();
    std::fs::write(&sibling, "x").unwrap();
    let got = select_binary_from(&bin_opts(foreign, None), Some(fake_exe), &mut ()).unwrap();
    assert_eq!(got, sibling);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn foreign_macos_uses_bundled_sibling() {
    let dir = sel_dir("foreign-macos");
    let fake_exe = dir.join("pite");
    let sibling = dir.join("pite-player-macos");
    std::fs::write(&fake_exe, "x").unwrap();
    std::fs::write(&sibling, "x").unwrap();
    let got = select_binary_from(&bin_opts("macos", None), Some(fake_exe), &mut ()).unwrap();
    assert_eq!(got, sibling);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn macos_export_layout_without_bundle() {
    let (dir, bin) = fixture("mac-layout");
    let report = export_project(
        &dir,
        &ExportOptions {
            platform: "macos".to_string(),
            out_dir: None,
            binary: Some(bin),
            skip_python_check: true,
            bundle_python: false,
        },
        &mut (),
    )
    .unwrap();
    let out = &report.out_dir;
    assert!(out.join("bin").join("fixt").is_file());
    assert!(out.join("run.sh").is_file());
    let sh = std::fs::read_to_string(out.join("run.sh")).unwrap();
    assert!(sh.contains("needs system Python 3.12"));
    assert!(sh.contains("game/scenes/main.ptscn"));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(out).ok();
}

#[test]
fn unix_launcher_sets_both_ld_and_dyld_library_paths() {
    let bundle = python_bundle::StagedBundle {
        root: PathBuf::from("/game/python"),
        lib: PathBuf::from("/game/python/lib/libpython3.12.dylib"),
        stdlib_zip: PathBuf::from("/game/python/python312.zip"),
    };
    let sh = unix_launcher(
        "game",
        "bin",
        "main.ptscn",
        Path::new("/game"),
        Some(&bundle),
    );
    assert!(sh.contains("LD_LIBRARY_PATH"));
    assert!(sh.contains("DYLD_LIBRARY_PATH"));
    assert!(sh.contains("PYTHONPATH"));
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
        &mut (),
    )
    .unwrap_err();
    assert!(err.to_string().contains("unsupported export platform"));
    std::fs::remove_dir_all(&dir).ok();
}
