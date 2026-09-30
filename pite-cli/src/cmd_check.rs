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
    } else if report.ok {
        println!("check ok: {}", root.display());
    } else {
        for e in &report.errors {
            eprintln!("error: {e}");
        }
        for w in &report.warnings {
            eprintln!("warning: {w}");
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
    for issue in pite_scene::validate(&doc) {
        warnings.push(format!("scene {}: {issue}", path.display()));
    }
    let registry = pite_core::NodeTypeRegistry::new();
    for node in &doc.node {
        if !registry.contains(&node.type_name) {
            warnings.push(format!(
                "scene {}: node {:?} has unregistered type {:?}",
                path.display(),
                node.id,
                node.type_name
            ));
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
        }
    }
}
