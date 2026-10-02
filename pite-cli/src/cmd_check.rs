use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Serialize)]
struct CheckReport {
    schema_version: u32,
    ok: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
    cache: Vec<String>,
}

pub fn run(path: Option<&str>, strict: bool, json: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let start = match path {
        Some(p) => {
            let pb = PathBuf::from(p);
            if pb.is_absolute() {
                pb
            } else {
                cwd.join(pb)
            }
        }
        None => cwd,
    };
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut cache: Vec<String> = Vec::new();
    let mut atlas_uses: Vec<pite_assets::AtlasUse> = Vec::new();

    let root = pite_project::find_project_root(&start).or_else(|| {
        let dogfood = start.join("examples").join("minimal-2d");
        if dogfood.join(pite_project::MANIFEST_FILE).is_file() {
            Some(dogfood)
        } else {
            None
        }
    });
    let root = match root {
        Some(root) => root,
        None => {
            errors.push(format!(
                "no {} found above {}",
                pite_project::MANIFEST_FILE,
                start.display()
            ));
            start.clone()
        }
    };
    let manifest = if errors.is_empty() {
        match pite_project::load_manifest(&root) {
            Ok(m) => Some(m),
            Err(e) => {
                errors.push(format!("manifest: {e:#}"));
                None
            }
        }
    } else {
        None
    };

    if let Some(manifest) = &manifest {
        let (manifest_errors, manifest_warnings) = pite_project::validate_manifest(manifest);
        errors.extend(manifest_errors);
        warnings.extend(manifest_warnings);
        if let Some(rel) = manifest.project.icon.strip_prefix("res://") {
            if !root.join(rel).is_file() {
                warnings.push(format!(
                    "manifest: icon {:?} missing",
                    manifest.project.icon
                ));
            }
        }
        let registry = pite_assets::load_registry(&root).unwrap_or_default();
        let scanned = match pite_assets::scan_files(&root) {
            Ok(files) => files,
            Err(e) => {
                warnings.push(format!("assets: scan failed: {e:#}"));
                Vec::new()
            }
        };
        let by_path: std::collections::HashMap<&str, &pite_assets::ScannedFile> =
            scanned.iter().map(|f| (f.res_path.as_str(), f)).collect();
        let by_hash: std::collections::HashMap<&str, &str> = scanned
            .iter()
            .map(|f| (f.hash.as_str(), f.res_path.as_str()))
            .collect();
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut referenced: std::collections::HashSet<String> = std::collections::HashSet::new();
        if manifest.project.icon.starts_with("res://") {
            referenced.insert(manifest.project.icon.clone());
        }
        check_scene_ref(
            &root,
            &manifest.project.main_scene,
            &mut errors,
            &mut warnings,
            &mut cache,
            &mut atlas_uses,
            &registry,
            &by_path,
            &by_hash,
            &mut visited,
            &mut referenced,
        );
        let (atlas_errors, atlas_warnings) = pite_assets::atlas_report(&root, &atlas_uses);
        errors.extend(atlas_errors);
        warnings.extend(atlas_warnings);
        // Sheets are referenced through their sidecar, never directly: mark
        // each used sidecar's sheet so packed PNGs don't read as orphans.
        let mut sheets = Vec::new();
        for r in &referenced {
            if r.ends_with(".atlas.json") {
                let sheet = pite_assets::sheet_path(&root, r);
                if let Ok(rel) = sheet.strip_prefix(&root) {
                    let joined = rel
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/");
                    sheets.push(format!("res://{joined}"));
                }
            }
        }
        referenced.extend(sheets);
        warnings.extend(orphan_warnings(&by_path, &referenced));
        for p in &manifest.export.platforms {
            if !pite_export::SUPPORTED_PLATFORMS.contains(&p.as_str()) {
                warnings.push(format!(
                    "manifest: unknown export platform {p:?} (supported: linux, windows)"
                ));
            }
        }
    }

    let failed = !errors.is_empty() || (strict && !warnings.is_empty());
    let report = CheckReport {
        schema_version: SCHEMA_VERSION,
        ok: !failed,
        errors,
        warnings,
        cache,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for w in &report.warnings {
            eprintln!("warning: {w}");
        }
        for c in &report.cache {
            println!("cache: {c}");
        }
        if report.ok {
            println!("check ok: {}", root.display());
        } else {
            for e in &report.errors {
                eprintln!("error: {e}");
            }
        }
    }
    if failed {
        anyhow::bail!("check failed");
    }
    Ok(())
}

fn check_scene_ref(
    root: &Path,
    scene_ref: &str,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
    cache: &mut Vec<String>,
    atlas_uses: &mut Vec<pite_assets::AtlasUse>,
    uid_registry: &pite_assets::UidRegistry,
    by_path: &std::collections::HashMap<&str, &pite_assets::ScannedFile>,
    by_hash: &std::collections::HashMap<&str, &str>,
    visited: &mut std::collections::HashSet<String>,
    referenced: &mut std::collections::HashSet<String>,
) {
    let path = if let Some(rel) = scene_ref.strip_prefix("res://") {
        root.join(rel)
    } else {
        root.join(scene_ref)
    };
    if !visited.insert(path.to_string_lossy().to_string()) {
        return;
    }
    let doc = match pite_scene::load_cached(&path) {
        Ok((doc, status, warning)) => {
            if let Some(w) = warning {
                warnings.push(format!("scene {}: {w}", path.display()));
            }
            cache.push(format!("{} {}", status, path.display()));
            doc
        }
        Err(e) => {
            errors.push(format!("scene {}: {e:#}", path.display()));
            return;
        }
    };
    let scene_dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.to_path_buf());
    let ids: std::collections::HashSet<&str> = doc.node.iter().map(|n| n.id.as_str()).collect();
    if !ids.contains(doc.root.as_str()) {
        errors.push(format!(
            "scene {}: root {:?} not found in node list",
            path.display(),
            doc.root
        ));
    }
    let registry = pite_core::NodeTypeRegistry::new();
    for node in &doc.node {
        if !registry.contains(&node.type_name) {
            errors.push(format!(
                "scene {}: node {:?} has unregistered type {:?}",
                path.display(),
                node.id,
                node.type_name
            ));
        }
        if let Some(parent) = &node.parent {
            if !ids.contains(parent.as_str()) {
                errors.push(format!(
                    "scene {}: node {:?} has unknown parent {parent:?}",
                    path.display(),
                    node.id
                ));
            }
        }
        if let Some(script) = &node.script {
            let script_path = if let Some(rel) = script.path.strip_prefix("res://") {
                root.join(rel)
            } else {
                root.join(&script.path)
            };
            if !script_path.is_file() {
                warnings.push(format!(
                    "scene {}: node {:?} script {:?} missing, using placeholder",
                    path.display(),
                    node.id,
                    script.path
                ));
            }
            if pite_script::is_reserved(&script.class) {
                warnings.push(format!(
                    "scene {}: node {:?} script class {:?} is reserved for the signal system",
                    path.display(),
                    node.id,
                    script.class
                ));
            }
            if let Ok(text) = std::fs::read_to_string(&script_path) {
                let declared = declared_signals(&text);
                for used in used_signals(&text) {
                    if !declared.contains(used) {
                        warnings.push(format!(
                            "scene {}: node {:?} uses undeclared signal {used:?}",
                            path.display(),
                            node.id
                        ));
                    }
                }
                for asset in played_assets(&text) {
                    referenced.insert(normalize_asset_ref(asset));
                    let asset_path = if let Some(rel) = asset.strip_prefix("res://") {
                        root.join(rel)
                    } else {
                        root.join(&asset)
                    };
                    if !asset_path.is_file() {
                        errors.push(format!(
                            "scene {}: node {:?} plays missing audio {asset:?}",
                            path.display(),
                            node.id
                        ));
                    } else if let Err(e) = pite_audio::decode_wav(&asset_path) {
                        errors.push(format!(
                            "scene {}: node {:?} audio {asset:?} undecodable: {e:#}",
                            path.display(),
                            node.id
                        ));
                    }
                }
            }
        }
        if node.type_name == "Sprite2D" {
            let prop_str = |key: &str| node.props.get(key).and_then(|v| v.as_str());
            match pite_assets::resolve_sprite_source(
                prop_str("texture"),
                prop_str("atlas"),
                prop_str("frame"),
            ) {
                Ok(pite_assets::SpriteSource::Atlas { atlas, frame }) => {
                    atlas_uses.push(pite_assets::AtlasUse {
                        scene: path.to_string_lossy().into_owned(),
                        node: node.id.clone(),
                        atlas,
                        frame,
                    });
                }
                Ok(_) => {}
                Err(e) => errors.push(format!(
                    "scene {}: node {:?} {e:#}",
                    path.display(),
                    node.id
                )),
            }
        }
        for value in node.props.values() {
            let Some(s) = value.as_str() else {
                continue;
            };
            if !s.starts_with("res://") {
                continue;
            }
            referenced.insert(s.to_string());
            let asset_path = if let Some(rel) = s.strip_prefix("res://") {
                root.join(rel)
            } else {
                let p = PathBuf::from(s);
                if p.is_absolute() {
                    p
                } else {
                    scene_dir.join(p)
                }
            };
            if !asset_path.is_file() {
                let moved_to: Option<&str> = uid_registry
                    .iter()
                    .find(|e| e.path == *s)
                    .and_then(|e| {
                        if e.hash.is_empty() {
                            None
                        } else {
                            by_hash.get(e.hash.as_str()).copied()
                        }
                    })
                    .filter(|new| *new != s);
                if let Some(new) = moved_to {
                    errors.push(format!(
                        "scene {}: node {:?} references moved asset {s:?} (now at {new:?}); run 'pite reimport'",
                        path.display(),
                        node.id
                    ));
                } else {
                    errors.push(format!(
                        "scene {}: node {:?} references missing asset {s:?}",
                        path.display(),
                        node.id
                    ));
                }
            }
        }
    }
    for inst in &doc.instance {
        let ref_path = if let Some(rel) = inst.scene.strip_prefix("res://") {
            root.join(rel)
        } else {
            scene_dir.join(&inst.scene)
        };
        let ref_doc = match pite_scene::load_cached(&ref_path) {
            Ok((doc, status, warning)) => {
                if let Some(w) = warning {
                    warnings.push(format!("scene {}: {w}", ref_path.display()));
                }
                cache.push(format!("{} {}", status, ref_path.display()));
                doc
            }
            Err(e) => {
                errors.push(format!(
                    "scene {}: instance of {} cannot load: {e:#}",
                    path.display(),
                    inst.scene
                ));
                continue;
            }
        };
        let attach_at = inst.parent.clone().unwrap_or_else(|| doc.root.clone());
        if !ids.contains(attach_at.as_str()) {
            errors.push(format!(
                "scene {}: instance of {} attaches to unknown parent {attach_at:?}",
                path.display(),
                inst.scene
            ));
        }
        let ref_ids: std::collections::HashSet<&str> =
            ref_doc.node.iter().map(|n| n.id.as_str()).collect();
        for key in inst.overrides.keys() {
            match key.split_once('.') {
                Some((node_id, _)) if ref_ids.contains(node_id) => {}
                _ => errors.push(format!(
                    "scene {}: instance of {} has bad override target {key:?}",
                    path.display(),
                    inst.scene
                )),
            }
        }
        // Override values apply as props at runtime, so asset refs hiding
        // in them count exactly like prop refs (same top-level-strings
        // convention as the node-props loop above).
        for value in inst.overrides.values() {
            if let Some(s) = value.as_str() {
                if s.starts_with("res://") {
                    referenced.insert(s.to_string());
                }
            }
        }
        let nested_ref = if inst.scene.starts_with("res://") {
            inst.scene.clone()
        } else {
            scene_dir.join(&inst.scene).to_string_lossy().to_string()
        };
        check_scene_ref(
            root,
            &nested_ref,
            errors,
            warnings,
            cache,
            atlas_uses,
            uid_registry,
            by_path,
            by_hash,
            visited,
            referenced,
        );
    }
}

fn normalize_asset_ref(asset: &str) -> String {
    if asset.starts_with("res://") {
        return asset.to_string();
    }
    let rel = asset.replace('\\', "/");
    let rel = rel.strip_prefix("./").unwrap_or(&rel);
    format!("res://{rel}")
}

/// Scanned files no reachable scene references. Only `res://` strings from
/// reachable scenes feed `referenced`, so anything unvisited (uninstantiated
/// scenes, loose scripts) keeps its assets silent — reachable-only by design.
fn orphan_warnings(
    by_path: &std::collections::HashMap<&str, &pite_assets::ScannedFile>,
    referenced: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut orphans: Vec<&str> = by_path
        .keys()
        .copied()
        .filter(|p| !referenced.contains(*p))
        .collect();
    orphans.sort_unstable();
    orphans
        .into_iter()
        .map(|p| format!("asset {p:?} is never referenced by a reachable scene"))
        .collect()
}

fn declared_signals(text: &str) -> std::collections::HashSet<String> {
    let mut declared = std::collections::HashSet::new();
    for line in text.lines() {
        let Some(pos) = line.find("pite.signal(") else {
            continue;
        };
        let mut lhs = line[..pos].trim_end();
        lhs = lhs.strip_suffix(['=', ':']).map(str::trim).unwrap_or(lhs);
        if let Some(name) = lhs.split_whitespace().last() {
            declared.insert(name.to_string());
        }
    }
    declared
}

fn used_signals(text: &str) -> Vec<&str> {
    let mut used = Vec::new();
    for line in text.lines() {
        for marker in [".emit(", ".connect("] {
            let mut rest = line;
            while let Some(pos) = rest.find(marker) {
                rest = &rest[pos + marker.len()..].trim_start();
                if let Some(quoted) = rest.strip_prefix('"') {
                    if let Some(end) = quoted.find('"') {
                        used.push(&quoted[..end]);
                        rest = &quoted[end + 1..];
                        continue;
                    }
                }
                break;
            }
        }
    }
    used
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
    use std::collections::{HashMap, HashSet};

    const FIXTURE_SCENE: &str = r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "sprite"
type = "Sprite2D"
name = "Sprite"
parent = "root"

[node.props]
texture = "res://assets/gone.png"
"#;

    fn fixture_root(n: u32) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pite-check-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).expect("create scenes dir");
        std::fs::create_dir_all(dir.join("assets")).expect("create assets dir");
        std::fs::write(dir.join("scenes").join("main.pitescene"), FIXTURE_SCENE)
            .expect("write fixture scene");
        dir
    }

    fn run_ref(
        root: &Path,
        registry: &pite_assets::UidRegistry,
        by_path: &HashMap<&str, &pite_assets::ScannedFile>,
        by_hash: &HashMap<&str, &str>,
    ) -> (Vec<String>, Vec<String>, HashSet<String>) {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut cache = Vec::new();
        let mut atlas_uses = Vec::new();
        let mut visited = HashSet::new();
        let mut referenced = HashSet::new();
        check_scene_ref(
            root,
            "res://scenes/main.pitescene",
            &mut errors,
            &mut warnings,
            &mut cache,
            &mut atlas_uses,
            registry,
            by_path,
            by_hash,
            &mut visited,
            &mut referenced,
        );
        (errors, cache, referenced)
    }

    #[test]
    fn missing_asset_is_error() {
        let root = fixture_root(1);
        let registry = pite_assets::UidRegistry::new();
        let by_path: HashMap<&str, &pite_assets::ScannedFile> = HashMap::new();
        let by_hash: HashMap<&str, &str> = HashMap::new();
        let (errors, _, _) = run_ref(&root, &registry, &by_path, &by_hash);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("references missing asset")),
            "expected a missing-asset error, got: {errors:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn present_asset_is_clean() {
        let root = fixture_root(2);
        std::fs::write(root.join("assets").join("gone.png"), b"fake-png-bytes")
            .expect("write fixture asset");
        let registry = pite_assets::UidRegistry::new();
        let by_path: HashMap<&str, &pite_assets::ScannedFile> = HashMap::new();
        let by_hash: HashMap<&str, &str> = HashMap::new();
        let (errors, cache, _) = run_ref(&root, &registry, &by_path, &by_hash);
        assert!(
            !errors.iter().any(|e| e.contains("asset")),
            "expected zero asset errors, got: {errors:?}"
        );
        assert_eq!(
            cache.len(),
            1,
            "check must report cache status, got: {cache:?}"
        );
        assert!(
            cache[0].starts_with("miss "),
            "first run populates the cache, got: {cache:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    fn scanned(paths: &[&str]) -> Vec<pite_assets::ScannedFile> {
        paths
            .iter()
            .map(|p| pite_assets::ScannedFile {
                res_path: p.to_string(),
                fs_path: PathBuf::from(&p["res://".len()..]),
                hash: String::new(),
                size: 0,
            })
            .collect()
    }

    #[test]
    fn prop_references_leave_no_orphan() {
        let root = fixture_root(3);
        let files = scanned(&["res://assets/gone.png"]);
        let by_path: HashMap<&str, &pite_assets::ScannedFile> =
            files.iter().map(|f| (f.res_path.as_str(), f)).collect();
        let by_hash: HashMap<&str, &str> = HashMap::new();
        let registry = pite_assets::UidRegistry::new();
        let (_, _, referenced) = run_ref(&root, &registry, &by_path, &by_hash);
        assert!(
            referenced.contains("res://assets/gone.png"),
            "got: {referenced:?}"
        );
        assert!(orphan_warnings(&by_path, &referenced).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unreferenced_assets_warn_sorted() {
        let files = scanned(&[
            "res://assets/zebra.png",
            "res://assets/apple.png",
            "res://assets/used.wav",
        ]);
        let by_path: HashMap<&str, &pite_assets::ScannedFile> =
            files.iter().map(|f| (f.res_path.as_str(), f)).collect();
        let mut referenced = HashSet::new();
        referenced.insert("res://assets/used.wav".to_string());
        assert_eq!(
            orphan_warnings(&by_path, &referenced),
            vec![
                "asset \"res://assets/apple.png\" is never referenced by a reachable scene",
                "asset \"res://assets/zebra.png\" is never referenced by a reachable scene",
            ]
        );
    }

    #[test]
    fn played_literals_count_as_references() {
        let root = fixture_root(4);
        std::fs::create_dir_all(root.join("scripts")).expect("create scripts dir");
        std::fs::write(
            root.join("scenes").join("main.pitescene"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"sfx\"\ntype = \"Node\"\nname = \"Sfx\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/sfx.py\"\nclass = \"Sfx\"\n",
        )
        .expect("write scene");
        std::fs::write(
            root.join("scripts").join("sfx.py"),
            "import pite\n\nclass Sfx(pite.Node):\n    def _ready(self):\n        pite.play(\"res://assets/hit.wav\")\n",
        )
        .expect("write script");
        let files = scanned(&["res://assets/hit.wav"]);
        let by_path: HashMap<&str, &pite_assets::ScannedFile> =
            files.iter().map(|f| (f.res_path.as_str(), f)).collect();
        let by_hash: HashMap<&str, &str> = HashMap::new();
        let registry = pite_assets::UidRegistry::new();
        let (_, _, referenced) = run_ref(&root, &registry, &by_path, &by_hash);
        assert!(
            referenced.contains("res://assets/hit.wav"),
            "got: {referenced:?}"
        );
        assert!(orphan_warnings(&by_path, &referenced).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
