use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize)]
struct CheckReport {
    schema_version: u32,
    ok: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
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
        check_scene_ref(&root, &manifest.project.main_scene, &mut errors, &mut warnings);
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
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for w in &report.warnings {
            eprintln!("warning: {w}");
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
) {
    let path = if let Some(rel) = scene_ref.strip_prefix("res://") {
        root.join(rel)
    } else {
        root.join(scene_ref)
    };
    let doc = match pite_scene::load_scene(&path) {
        Ok(doc) => doc,
        Err(e) => {
            errors.push(format!("scene {}: {e:#}", path.display()));
            return;
        }
    };
    let scene_dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| root.to_path_buf());
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
                    "scene {}: node {:?} script class {:?} is reserved for M2 signals",
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
    }
    for inst in &doc.instance {
        let ref_path = if let Some(rel) = inst.scene.strip_prefix("res://") {
            root.join(rel)
        } else {
            scene_dir.join(&inst.scene)
        };
        let ref_doc = match pite_scene::load_scene(&ref_path) {
            Ok(doc) => doc,
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
    }
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
