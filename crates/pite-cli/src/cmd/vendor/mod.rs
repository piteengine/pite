// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite vendor`: pin third-party Python packages into `vendor/`.
//!
//! Policy (see docs): zipped wheels by default, unpacked when the wheel
//! carries native modules (`.so` can't load from a zip) or `--unpacked`.
//! Absent form means zipped; the lock records version + artifact hash only.
//! Transitive deps resolve at vendor-time via pip and each gets a row.
//! Nothing here touches the network except `add` (pip download).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::Serialize;

mod wheel;

pub use wheel::{
    find_dist_info, find_wheel, hash_unpacked, normalize_dist, parse_wheel_filename, read_record,
    sha256_file, vendor_provides, wheel_has_native, wheel_top_levels,
};

use crate::output::{Outcome, SCHEMA_VERSION};
use crate::text::{capitalize, Spans};
use crate::ui::{BarTone, Cell, Table, Ui};

#[derive(Debug, Subcommand)]
pub enum VendorAction {
    /// Download wheels (plus transitive deps) into vendor/ and pin them.
    Add {
        /// Package specification (`name` or `name==version`).
        spec: String,
        /// Keep the wheel file even for pure-Python packages.
        #[arg(long, default_value_t = false)]
        zipped: bool,
        /// Unpack into vendor/ even without native modules.
        #[arg(long, default_value_t = false)]
        unpacked: bool,
    },
    /// Remove a vendored package and clean up its files from vendor/.
    Remove {
        /// Name of the package to remove.
        name: String,
    },
    /// List all currently vendored packages and their pinned versions.
    List,
    /// Verify that all vendored packages match their pinned SHA-256 hashes.
    Verify,
}

/// One vendored package row.
#[derive(Debug, Serialize)]
struct Package {
    name: String,
    version: String,
    direct: bool,
    status: &'static str,
    /// `zipped` or `unpacked` for packages that are present.
    #[serde(skip_serializing_if = "Option::is_none")]
    storage: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct ListData {
    schema_version: u32,
    packages: Vec<Package>,
}

#[derive(Debug, Serialize)]
struct AddEntry {
    name: String,
    version: String,
    storage: &'static str,
}

#[derive(Debug, Serialize)]
struct AddData {
    schema_version: u32,
    added: Vec<AddEntry>,
}

#[derive(Debug, Serialize)]
struct RemoveData {
    schema_version: u32,
    removed: String,
}

#[derive(Debug, Serialize)]
struct VerifyData {
    schema_version: u32,
    ok: bool,
    verified: usize,
    failed: usize,
    packages: Vec<Package>,
}

pub fn run(ui: &mut Ui, action: VendorAction) -> Result<Outcome> {
    match action {
        VendorAction::Add {
            spec,
            zipped,
            unpacked,
        } => add(ui, &spec, zipped, unpacked),
        VendorAction::Remove { name } => remove(ui, &name),
        VendorAction::List => list(ui),
        VendorAction::Verify => verify(ui),
    }
}

/// Project-relative vendor directory.
pub const VENDOR_DIR: &str = "vendor";

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    pite_project::find_project_root(&cwd)
        .with_context(|| "no pite.toml found above cwd; run inside a project")
}

fn pip_program() -> String {
    if let Some(bin) = std::env::var_os("PITE_PIP_BIN") {
        return bin.to_string_lossy().into_owned();
    }
    "pip".to_string()
}

/// `pip download` honoring the offline test seam: `PITE_PIP_FIND_LINKS`
/// switches to `--no-index` plus a local directory.
fn pip_download(spec: &str, dest: &Path) -> Result<()> {
    let mut cmd = std::process::Command::new(pip_program());
    cmd.arg("download").arg(spec).arg("-d").arg(dest);
    cmd.arg("--only-binary=:all:")
        .arg("--python-version")
        .arg("3.12");
    if let Some(links) = std::env::var_os("PITE_PIP_FIND_LINKS") {
        cmd.arg("--no-index").arg("--find-links").arg(links);
    }
    let output = cmd
        .output()
        .with_context(|| "cannot run pip (install pip or set PITE_PIP_BIN to a pip binary)")?;
    if !output.status.success() {
        let tail = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = tail.lines().collect();
        let tail = tail
            .iter()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev();
        anyhow::bail!(
            "pip download {spec:?} failed:\n{}",
            tail.cloned().collect::<Vec<_>>().join("\n")
        );
    }
    Ok(())
}

fn parse_spec(spec: &str) -> Result<String> {
    match spec.split_once("==") {
        Some((name, version)) if !name.is_empty() && !version.is_empty() => {
            Ok(format!("{}=={}", name.trim(), version.trim()))
        }
        None if !spec.is_empty() && !spec.contains(['>', '<', '~', '!', ';', ' ']) => {
            Ok(spec.to_string())
        }
        _ => anyhow::bail!("pin the spec with == (got {spec:?})"),
    }
}

fn add(ui: &mut Ui, spec: &str, zipped: bool, unpacked: bool) -> Result<Outcome> {
    if zipped && unpacked {
        anyhow::bail!("--zipped and --unpacked conflict; pick one");
    }
    let root = project_root()?;
    let mut manifest = pite_project::load_manifest(&root)?;
    let pinned = parse_spec(spec)?;
    ui.info(
        Spans::new()
            .plain("Downloading wheels for ")
            .info(pinned.as_str())
            .plain(" via pip…"),
    );
    let requested = normalize_dist(pinned.split("==").next().unwrap_or(&pinned));
    let tmp = std::env::temp_dir().join(format!("pite-vendor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    pip_download(&pinned, &tmp)?;
    let mut wheels: Vec<PathBuf> = std::fs::read_dir(&tmp)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "whl"))
        .collect();
    wheels.sort_unstable();
    if wheels.is_empty() {
        anyhow::bail!("pip downloaded nothing for {spec:?}");
    }
    let vendor_dir = root.join(VENDOR_DIR);
    std::fs::create_dir_all(&vendor_dir)?;
    let mut added: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for wheel in &wheels {
        let file_name = wheel
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        ui.verbose_note(Spans::new().faded("  Wheel ").strong(file_name.as_str()));
        let (dist_raw, version) = parse_wheel_filename(&file_name)
            .with_context(|| format!("cannot parse wheel filename {file_name:?}"))?;
        let dist = normalize_dist(&dist_raw);
        let native = wheel_has_native(wheel)?;
        let keep_zipped = if unpacked {
            false
        } else if zipped {
            if native {
                anyhow::bail!(
                    "{dist_raw} ships native modules, which can't load from a zip; drop --zipped"
                );
            }
            true
        } else {
            !native
        };
        let sha = sha256_file(wheel)?;
        if keep_zipped {
            let dest = vendor_dir.join(&file_name);
            if dest.is_file() && sha256_file(&dest)? == sha {
                // Re-adding the same bytes is a no-op, not an error.
            } else {
                std::fs::copy(wheel, &dest).with_context(|| {
                    format!("cannot stage {} into {}", wheel.display(), dest.display())
                })?;
            }
        } else {
            let tops = wheel_top_levels(wheel)?;
            for top in &tops {
                if vendor_dir.join(top).exists() {
                    anyhow::bail!(
                        "{dist_raw} wants vendor/{top}, which already exists; remove the owner first"
                    );
                }
            }
            let file = std::fs::File::open(wheel)?;
            let mut zip = zip::ZipArchive::new(file)?;
            zip.extract(&vendor_dir).with_context(|| {
                format!(
                    "cannot unpack {} into {}",
                    wheel.display(),
                    vendor_dir.display()
                )
            })?;
            manifest.vendor.insert(
                dist.clone(),
                pite_project::VendorEntry {
                    version: version.clone(),
                    sha256: tree_hash_for(&vendor_dir, &dist, &version)?,
                    direct: dist == requested,
                },
            );
            added.insert(dist.clone(), (version.clone(), false));
            continue;
        }
        manifest.vendor.insert(
            dist.clone(),
            pite_project::VendorEntry {
                version: version.clone(),
                sha256: sha,
                direct: dist == requested,
            },
        );
        added.insert(dist, (version, true));
    }
    pite_project::save_manifest(&root, &manifest)?;
    let _ = std::fs::remove_dir_all(&tmp);

    let mut table = Table::new(&["PACKAGE", "VERSION", "STORAGE"]);
    let mut entries = Vec::new();
    for (dist, (version, was_zipped)) in &added {
        let storage = if *was_zipped { "zipped" } else { "unpacked" };
        table = table.row(vec![
            Cell::info(dist.as_str()),
            Cell::plain(version.as_str()),
            Cell::success(storage),
        ]);
        entries.push(AddEntry {
            name: dist.clone(),
            version: version.clone(),
            storage,
        });
    }
    ui.section("Vendored packages");
    ui.table(&table);
    if !ui.quiet() {
        ui.blank();
        let zipped = added.values().filter(|(_, zipped)| *zipped).count();
        let unpacked = added.len() - zipped;
        let right = if unpacked == 0 {
            format!("{} added · zipped", package_count(added.len()))
        } else if zipped == 0 {
            format!("{} added · unpacked", package_count(added.len()))
        } else {
            format!(
                "{} added · {zipped} zipped · {unpacked} unpacked",
                package_count(added.len())
            )
        };
        ui.bar("Vendored", &right, BarTone::Success);
    }

    Outcome::completed(AddData {
        schema_version: SCHEMA_VERSION,
        added: entries,
    })
}

fn tree_hash_for(vendor_dir: &Path, dist: &str, version: &str) -> Result<String> {
    let info = find_dist_info(vendor_dir, dist, version)?;
    let rels = read_record(&info.join("RECORD"))?;
    hash_unpacked(vendor_dir, &rels)
}

/// First 16 hex characters of a hash, for verbose lines.
fn short_hash(hash: &str) -> &str {
    &hash[..hash.len().min(16)]
}

/// `1 package` / `3 packages`, for summary bars.
fn package_count(count: usize) -> String {
    if count == 1 {
        "1 package".to_string()
    } else {
        format!("{count} packages")
    }
}

fn remove(ui: &mut Ui, name: &str) -> Result<Outcome> {
    let root = project_root()?;
    let mut manifest = pite_project::load_manifest(&root)?;
    let dist = normalize_dist(name);
    let entry = manifest
        .vendor
        .get(&dist)
        .with_context(|| format!("{name:?} is not vendored"))?
        .clone();
    let vendor_dir = root.join(VENDOR_DIR);
    if let Ok(wheel) = find_wheel(&vendor_dir, &dist, &entry.version) {
        std::fs::remove_file(&wheel)
            .with_context(|| format!("cannot delete {}", wheel.display()))?;
    } else {
        let info = find_dist_info(&vendor_dir, &dist, &entry.version)?;
        for rel in read_record(&info.join("RECORD"))? {
            let p = vendor_dir.join(&rel);
            if p.is_file() {
                std::fs::remove_file(&p)
                    .with_context(|| format!("cannot delete {}", p.display()))?;
            }
        }
        remove_empty_parents(&info, &vendor_dir);
    }
    manifest.vendor.remove(&dist);
    pite_project::save_manifest(&root, &manifest)?;
    ui.success(
        Spans::new()
            .plain("Removed ")
            .info(dist.as_str())
            .plain(" from ")
            .strong(format!("{VENDOR_DIR}/"))
            .plain("."),
    );

    Outcome::completed(RemoveData {
        schema_version: SCHEMA_VERSION,
        removed: dist,
    })
}

fn remove_empty_parents(dir: &Path, stop: &Path) {
    let mut dir = dir.to_path_buf();
    while dir.as_path() != stop && dir.starts_with(stop) {
        match std::fs::remove_dir(&dir) {
            Ok(()) => {}
            Err(_) => return,
        }
        if !dir.pop() {
            return;
        }
    }
}

fn list(ui: &mut Ui) -> Result<Outcome> {
    let root = project_root()?;
    let manifest = pite_project::load_manifest(&root)?;
    let vendor_dir = root.join(VENDOR_DIR);
    let mut names: Vec<&String> = manifest.vendor.keys().collect();
    names.sort_unstable();
    if names.is_empty() {
        ui.info(
            Spans::new()
                .plain("No packages are vendored yet. Add one with ")
                .strong("pite vendor add pkg==version")
                .plain("."),
        );
        return Outcome::completed(ListData {
            schema_version: SCHEMA_VERSION,
            packages: Vec::new(),
        });
    }

    let mut table = Table::new(&["PACKAGE", "VERSION", "KIND", "STATUS", "STORAGE"]);
    let mut packages = Vec::new();
    let mut notes = Vec::new();
    let mut missing = 0usize;
    let mut direct = 0usize;
    for name in names {
        let entry = &manifest.vendor[name];
        let wheel = find_wheel(&vendor_dir, name, &entry.version);
        let unpacked = find_dist_info(&vendor_dir, name, &entry.version);
        let (status, storage, cells) = match (&wheel, &unpacked) {
            (Ok(_), _) => ("ok", Some("zipped"), Cell::success("Cached")),
            (_, Ok(_)) => ("ok", Some("unpacked"), Cell::success("Cached")),
            _ => {
                missing += 1;
                ("missing", None, Cell::warning("Missing"))
            }
        };
        if entry.direct {
            direct += 1;
        }
        table = table.row(vec![
            Cell::info(name.as_str()),
            Cell::plain(entry.version.as_str()),
            Cell::plain(if entry.direct { "direct" } else { "transitive" }),
            cells,
            Cell::plain(storage.unwrap_or("-")),
        ]);
        packages.push(Package {
            name: name.clone(),
            version: entry.version.clone(),
            direct: entry.direct,
            status,
            storage,
        });
        notes.push(format!("    Pinned to {}", short_hash(&entry.sha256)));
    }

    ui.section("Vendored packages");
    ui.table(&table);
    for note in &notes {
        ui.verbose_line(note);
    }
    if !ui.quiet() {
        ui.blank();
        let right = if missing == 0 {
            format!("{} · {direct} direct", package_count(packages.len()))
        } else {
            format!("{} · {missing} missing", package_count(packages.len()))
        };
        ui.bar(
            "Vendored packages",
            &right,
            if missing == 0 {
                BarTone::Faded
            } else {
                BarTone::Warning
            },
        );
    }

    Outcome::completed(ListData {
        schema_version: SCHEMA_VERSION,
        packages,
    })
}

fn verify(ui: &mut Ui) -> Result<Outcome> {
    let root = project_root()?;
    let manifest = pite_project::load_manifest(&root)?;
    let vendor_dir = root.join(VENDOR_DIR);
    let mut names: Vec<&String> = manifest.vendor.keys().collect();
    names.sort_unstable();
    if names.is_empty() {
        ui.info(
            Spans::new()
                .plain("No packages are vendored to verify. Add one with ")
                .strong("pite vendor add pkg==version")
                .plain("."),
        );
        return Outcome::completed(VerifyData {
            schema_version: SCHEMA_VERSION,
            ok: true,
            verified: 0,
            failed: 0,
            packages: Vec::new(),
        });
    }

    let mut verified = 0usize;
    let mut failed = 0usize;
    let mut table = Table::new(&["PACKAGE", "VERSION", "STATUS", "DETAIL"]);
    let mut packages = Vec::new();
    for name in names {
        let entry = &manifest.vendor[name];
        let hash = find_wheel(&vendor_dir, name, &entry.version)
            .and_then(|wheel| sha256_file(&wheel))
            .or_else(|_| tree_hash_for(&vendor_dir, name, &entry.version));
        let (status, detail, cell) = match hash {
            Ok(actual) if actual == entry.sha256 => {
                verified += 1;
                (
                    "ok",
                    "Matches the pinned hash".to_string(),
                    Cell::success("Verified"),
                )
            }
            Ok(actual) => {
                failed += 1;
                (
                    "mismatch",
                    format!("Got unexpected {}", short_hash(&actual)),
                    Cell::error("Mismatch"),
                )
            }
            Err(error) => {
                failed += 1;
                (
                    "missing",
                    capitalize(&format!("{error:#}")),
                    Cell::warning("Missing"),
                )
            }
        };
        table = table.row(vec![
            Cell::info(name.as_str()),
            Cell::plain(entry.version.as_str()),
            cell,
            Cell::plain(detail.clone()),
        ]);
        packages.push(Package {
            name: name.clone(),
            version: entry.version.clone(),
            direct: entry.direct,
            status,
            storage: None,
        });
    }

    ui.section("Verification");
    ui.table(&table);
    let ok = failed == 0;
    if !ui.quiet() || !ok {
        ui.blank();
        if ok {
            ui.bar(
                "Vendored packages verified",
                &package_count(verified),
                BarTone::Success,
            );
        } else {
            ui.bar(
                "Vendor verify failed",
                &format!("{failed} failed · {verified} verified"),
                BarTone::Error,
            );
        }
    }

    let data = VerifyData {
        schema_version: SCHEMA_VERSION,
        ok,
        verified,
        failed,
        packages,
    };
    if ok {
        Outcome::completed(data)
    } else {
        Outcome::failed(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::ui::Ui;
    use std::io::Write;

    /// `add`/`remove` mutate the process cwd and env: never run together.
    #[cfg(unix)]
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn make_wheel(dir: &Path, dist: &str, version: &str, native: bool) -> PathBuf {
        let name = format!("{dist}-{version}-py3-none-any.whl");
        let path = dir.join(&name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file(format!("{dist}/__init__.py"), opts).unwrap();
        zip.write_all(b"x = 1\n").unwrap();
        if native {
            zip.start_file(format!("{dist}/_n.so"), opts).unwrap();
            zip.write_all(b"\x7fELF").unwrap();
        }
        let dist_info = format!("{dist}-{version}.dist-info");
        for f in ["METADATA", "WHEEL", "RECORD"] {
            zip.start_file(format!("{dist_info}/{f}"), opts).unwrap();
        }
        zip.write_all(b"").unwrap();
        zip.finish().unwrap();
        path
    }

    #[test]
    fn wheel_helpers_behave() {
        assert_eq!(normalize_dist("Pillow"), "pillow");
        assert_eq!(normalize_dist("a_b.c-d"), "a-b-c-d");
        assert_eq!(
            parse_wheel_filename("demo_pkg-1.0-py3-none-any.whl"),
            Some(("demo_pkg".to_string(), "1.0".to_string()))
        );
        assert_eq!(parse_wheel_filename("nope.zip"), None);
        let dir = std::env::temp_dir().join(format!("pite-vt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pure = make_wheel(&dir, "pure", "1.0", false);
        let nat = make_wheel(&dir, "nat", "2.0", true);
        assert!(!wheel_has_native(&pure).unwrap());
        assert!(wheel_has_native(&nat).unwrap());
        assert_eq!(wheel_top_levels(&pure).unwrap(), vec!["pure".to_string()]);
        assert!(parse_spec("a==1.0").is_ok());
        assert!(parse_spec("a>=1.0").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn vendor_provides_sees_trees_and_zips() {
        let dir = std::env::temp_dir().join(format!("pite-vp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!vendor_provides(&dir, "anything"));
        std::fs::create_dir_all(dir.join("treepkg")).unwrap();
        std::fs::write(dir.join("treepkg").join("__init__.py"), b"x=1").unwrap();
        std::fs::write(dir.join("mod.py"), b"y=2").unwrap();
        make_wheel(&dir, "zippkg", "1.0", false);
        assert!(vendor_provides(&dir, "treepkg"));
        assert!(vendor_provides(&dir, "mod"));
        assert!(vendor_provides(&dir, "zippkg"));
        assert!(!vendor_provides(&dir, "missing"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fake `pip download SPEC -d DIR …`: copies fixture wheels, ignoring
    /// flags. Drives `add` end to end with no network. A POSIX shell script,
    /// so the end-to-end tests that use it are Unix-only.
    #[cfg(unix)]
    const FAKE_PIP: &str = "#!/bin/sh\ndest=\"\"\nprev=\"\"\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-d\" ]; then dest=\"$a\"; fi\n  prev=\"$a\"\ndone\ncp \"$PITE_FAKE_WHEELS\"/*.whl \"$dest\"/\n";

    #[cfg(unix)]
    fn fake_project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pite-vendor-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn add_verify_remove_round_trip() {
        let _guard = SERIAL.lock().unwrap();
        let wheels = std::env::temp_dir().join(format!("pite-vw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wheels);
        std::fs::create_dir_all(&wheels).unwrap();
        make_wheel(&wheels, "demo", "1.0", false);
        make_wheel(&wheels, "natpkg", "2.0", true);
        let fake = wheels.join("pip");
        std::fs::write(&fake, FAKE_PIP).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let dir = fake_project("roundtrip");
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        std::env::set_var("PITE_PIP_BIN", &fake);
        std::env::set_var("PITE_FAKE_WHEELS", &wheels);
        add(&mut Ui::silent(), "demo==1.0", false, false).unwrap();
        let manifest = pite_project::load_manifest(&dir).unwrap();
        assert!(manifest.vendor["demo"].direct);
        assert!(!manifest.vendor["natpkg"].direct);
        assert!(dir
            .join("vendor")
            .join("demo-1.0-py3-none-any.whl")
            .is_file());
        assert!(dir
            .join("vendor")
            .join("natpkg")
            .join("__init__.py")
            .is_file());
        assert_eq!(verify(&mut Ui::silent()).unwrap().exit, 0);
        std::fs::write(
            dir.join("vendor").join("demo-1.0-py3-none-any.whl"),
            b"corrupt",
        )
        .unwrap();
        assert_eq!(verify(&mut Ui::silent()).unwrap().exit, 1);
        std::env::remove_var("PITE_PIP_BIN");
        std::env::remove_var("PITE_FAKE_WHEELS");
        std::env::set_current_dir(&prev).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&wheels);
    }

    #[cfg(unix)]
    #[test]
    fn remove_deletes_artifact_and_row() {
        let _guard = SERIAL.lock().unwrap();
        let wheels = std::env::temp_dir().join(format!("pite-vw2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wheels);
        std::fs::create_dir_all(&wheels).unwrap();
        make_wheel(&wheels, "solo", "3.0", false);
        let fake = wheels.join("pip");
        std::fs::write(&fake, FAKE_PIP).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let dir = fake_project("remove");
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        std::env::set_var("PITE_PIP_BIN", &fake);
        std::env::set_var("PITE_FAKE_WHEELS", &wheels);
        add(&mut Ui::silent(), "solo", false, false).unwrap();
        remove(&mut Ui::silent(), "solo").unwrap();
        let manifest = pite_project::load_manifest(&dir).unwrap();
        assert!(manifest.vendor.is_empty());
        assert!(!dir
            .join("vendor")
            .join("solo-3.0-py3-none-any.whl")
            .exists());
        assert!(remove(&mut Ui::silent(), "solo").is_err());
        std::env::remove_var("PITE_PIP_BIN");
        std::env::remove_var("PITE_FAKE_WHEELS");
        std::env::set_current_dir(&prev).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&wheels);
    }
}
