// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    pub fn is_enabled(self, is_terminal: bool) -> bool {
        match self {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => is_terminal && std::env::var_os("NO_COLOR").is_none(),
        }
    }
}

pub struct Colors {
    pub bold_red: &'static str,
    pub bold_yellow: &'static str,
    pub bold_green: &'static str,
    pub bold_cyan: &'static str,
    pub bold: &'static str,
    pub dim: &'static str,
    pub reset: &'static str,
}

pub const COLOR_ENABLED: Colors = Colors {
    bold_red: "\x1b[1;31m",
    bold_yellow: "\x1b[1;33m",
    bold_green: "\x1b[1;32m",
    bold_cyan: "\x1b[1;36m",
    bold: "\x1b[1m",
    dim: "\x1b[2m",
    reset: "\x1b[0m",
};

pub const COLOR_DISABLED: Colors = Colors {
    bold_red: "",
    bold_yellow: "",
    bold_green: "",
    bold_cyan: "",
    bold: "",
    dim: "",
    reset: "",
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    pub what: String,
    pub why: String,
    pub fix: String,
}

impl Diagnostic {
    pub fn from_message(root: &Path, severity: DiagnosticSeverity, raw: &str) -> Self {
        let raw = raw.trim();

        if let Some(scene_rest) = raw.strip_prefix("scene ") {
            if let Some((path_str, after_path)) = scene_rest.split_once(": ") {
                let rel_file = format_path_rel(root, path_str);
                if let Some(node_rest) = after_path.strip_prefix("node ") {
                    if let Some((node_id, rest)) = extract_quoted_or_word(node_rest) {
                        let (what, why, fix) = parse_node_diagnostic(rest);
                        return Self {
                            severity,
                            file: Some(rel_file),
                            node: Some(node_id),
                            what,
                            why,
                            fix,
                        };
                    }
                }
                let (what, why, fix) = parse_scene_diagnostic(after_path);
                return Self {
                    severity,
                    file: Some(rel_file),
                    node: None,
                    what,
                    why,
                    fix,
                };
            }
        }

        if let Some(manifest_rest) = raw.strip_prefix("manifest: ") {
            let (what, why, fix) = parse_manifest_diagnostic(manifest_rest);
            return Self {
                severity,
                file: Some(pite_project::MANIFEST_FILE.to_string()),
                node: None,
                what,
                why,
                fix,
            };
        }

        if let Some(atlas_rest) = raw.strip_prefix("atlas ") {
            if let Some((atlas_str, rest)) = atlas_rest.split_once(": ") {
                let file = format_path_rel(root, atlas_str);
                let (what, why, fix) = parse_atlas_diagnostic(rest);
                return Self {
                    severity,
                    file: Some(file),
                    node: None,
                    what,
                    why,
                    fix,
                };
            }
        }

        if raw.starts_with("asset ") && raw.ends_with(" is never referenced by a reachable scene") {
            let inner = raw
                .strip_prefix("asset ")
                .and_then(|s| s.strip_suffix(" is never referenced by a reachable scene"))
                .unwrap_or(raw);
            let file = format_path_rel(root, inner);
            return Self {
                severity,
                file: Some(file.clone()),
                node: None,
                what: format!("Asset \"{file}\" is never referenced by any reachable scene."),
                why: "Unused assets inflate project size and export packages.".to_string(),
                fix: "Reference the asset in a scene or delete it if no longer needed.".to_string(),
            };
        }

        if let Some(file_str) = raw.strip_prefix("failed to reimport asset ") {
            let file = format_path_rel(root, file_str);
            return Self {
                severity,
                file: Some(file.clone()),
                node: None,
                what: format!("Failed to reimport asset \"{file}\"."),
                why: "The asset could not be parsed, scanned, or reconciled with the UID registry."
                    .to_string(),
                fix: "Inspect the file format and ensure the file is readable and not corrupted."
                    .to_string(),
            };
        }

        if raw.starts_with("no ") && raw.contains("found above") {
            return Self {
                severity,
                file: Some(pite_project::MANIFEST_FILE.to_string()),
                node: None,
                what: "No pite.toml manifest found in current or parent directories.".to_string(),
                why: "Pite commands require a project root containing a valid pite.toml manifest."
                    .to_string(),
                fix: "Run inside a valid project directory or pass <PATH>.".to_string(),
            };
        }

        Self {
            severity,
            file: None,
            node: None,
            what: raw.to_string(),
            why: "The project verification check detected an issue.".to_string(),
            fix: "Inspect the referenced file and correct any syntax or configuration errors."
                .to_string(),
        }
    }
}

fn format_path_rel(root: &Path, raw_path: &str) -> String {
    let raw = raw_path.trim().trim_matches('"');
    let raw = raw.strip_prefix("res://").unwrap_or(raw);
    let p = Path::new(raw);
    if let Ok(rel) = p.strip_prefix(root) {
        rel.to_string_lossy().replace('\\', "/")
    } else {
        raw.replace('\\', "/")
    }
}

fn extract_quoted_or_word(s: &str) -> Option<(String, &str)> {
    if let Some(rest) = s.strip_prefix('"') {
        let (val, after) = rest.split_once('"')?;
        Some((val.to_string(), after.trim_start()))
    } else {
        let (val, after) = s.split_once(' ')?;
        Some((val.to_string(), after.trim_start()))
    }
}

fn extract_quoted_word(s: &str) -> Option<String> {
    let start = s.find('"')?;
    let rest = &s[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_node_diagnostic(rest: &str) -> (String, String, String) {
    if rest.starts_with("has unregistered type") {
        let ty = extract_quoted_word(rest).unwrap_or_else(|| "unknown".to_string());
        (
            format!("Node type \"{ty}\" is not registered in the engine node schema."),
            "The engine cannot instantiate a node with an unrecognized type at runtime."
                .to_string(),
            "Change the type to a registered node class (e.g. Sprite2D, Node2D, CharacterBody2D)."
                .to_string(),
        )
    } else if rest.starts_with("has unknown parent") {
        let parent = extract_quoted_word(rest).unwrap_or_else(|| "unknown".to_string());
        (
            format!("Parent node \"{parent}\" does not exist in the scene tree."),
            "Nodes must attach to an existing node in the scene hierarchy.".to_string(),
            "Set 'parent' to a valid node ID or the scene root.".to_string(),
        )
    } else if rest.contains("script") && rest.contains("missing, using placeholder") {
        let script = extract_quoted_word(rest).unwrap_or_else(|| "script".to_string());
        (
            format!("Referenced Python script \"{script}\" does not exist on disk."),
            "The engine will instantiate a fallback placeholder node without script behavior.".to_string(),
            "Create the Python script file at the referenced res:// path or remove the script binding.".to_string(),
        )
    } else if rest.contains("is reserved for the signal system") {
        let class = extract_quoted_word(rest).unwrap_or_else(|| "class".to_string());
        (
            format!("Script class name \"{class}\" is reserved by the engine."),
            "The class name conflicts with internal engine signal dispatchers.".to_string(),
            "Rename the class in the script and scene to a non-reserved identifier.".to_string(),
        )
    } else if rest.contains("imports third-party") {
        let module = extract_quoted_word(rest).unwrap_or_else(|| "module".to_string());
        (
            format!("Script imports third-party module \"{module}\" without vendoring."),
            "Packaged game exports drop host site-packages and will fail at runtime.".to_string(),
            format!("Run 'pite vendor add {module}' to vendor the package into vendor/."),
        )
    } else if rest.contains("plays missing audio") {
        let audio = extract_quoted_word(rest).unwrap_or_else(|| "audio".to_string());
        (
            format!("Script plays missing audio asset \"{audio}\"."),
            "Audio playback will fail or produce silent errors at runtime.".to_string(),
            "Place the audio file at the referenced res:// path or update the play() call."
                .to_string(),
        )
    } else if rest.contains("undecodable:") {
        let audio = extract_quoted_word(rest).unwrap_or_else(|| "audio".to_string());
        let err = rest
            .split_once("undecodable: ")
            .map(|(_, e)| e)
            .unwrap_or("format error");
        (
            format!("Audio file \"{audio}\" cannot be decoded: {err}."),
            "The audio format is corrupted, headerless, or unsupported (supported: WAV, OGG, MP3, FLAC).".to_string(),
            "Re-encode or replace the audio file with a valid supported format.".to_string(),
        )
    } else if rest.contains("references moved asset") {
        (
            format!("Asset reference needs updating: {rest}."),
            "The referenced asset was renamed or moved outside the editor.".to_string(),
            "Run 'pite check --reimport-assets' to update references automatically.".to_string(),
        )
    } else if rest.contains("references missing asset") {
        let asset = extract_quoted_word(rest).unwrap_or_else(|| "asset".to_string());
        (
            format!("Referenced asset \"{asset}\" does not exist."),
            "The scene points to a resource file that cannot be found on disk.".to_string(),
            "Place the missing asset at the res:// path or update the property in the scene."
                .to_string(),
        )
    } else if rest.contains("custom prop") && rest.contains("expected") {
        (
            format!("Custom property type mismatch: {rest}."),
            "The property value in the scene does not match its declared type in pite.toml [props].".to_string(),
            "Update the property value to match the declared type or edit [props] in pite.toml.".to_string(),
        )
    } else if rest.contains("has unknown prop") {
        (
            format!("Undeclared property: {rest}."),
            "The property is not defined in the node schema and not declared in pite.toml [props]."
                .to_string(),
            "Declare the property under [props] in pite.toml or remove it from the scene."
                .to_string(),
        )
    } else if rest.contains("texture and atlas are mutually exclusive") {
        (
            "Sprite2D specifies both 'texture' and 'atlas' properties.".to_string(),
            "A sprite can draw either a standalone image or an atlas frame, but not both."
                .to_string(),
            "Remove either 'texture' or 'atlas' from the node properties.".to_string(),
        )
    } else if rest.contains("frame requires an atlas and atlas requires a frame") {
        (
            "Incomplete atlas configuration on Sprite2D.".to_string(),
            "When using an atlas, both 'atlas' and 'frame' properties must be provided together."
                .to_string(),
            "Set both 'atlas' and 'frame' together, or use 'texture' instead.".to_string(),
        )
    } else if rest.contains("missing required 'atlas' property") {
        (
            "AnimatedSprite2D is missing required 'atlas' property.".to_string(),
            "AnimatedSprite2D requires an atlas sidecar containing animation definitions."
                .to_string(),
            "Set atlas = \"res://...\" on the AnimatedSprite2D node.".to_string(),
        )
    } else if rest.contains("not found in atlas") {
        (
            format!("Atlas reference error: {rest}."),
            "The frame or animation name is not defined in the referenced .atlas.json sidecar."
                .to_string(),
            "Define the frame or animation in the atlas sidecar or update the node property."
                .to_string(),
        )
    } else if rest.contains("invalid tilemap") {
        (
            format!("TileMap2D error: {rest}."),
            "The .ptmap sidecar file is malformed or could not be loaded.".to_string(),
            "Verify the tilemap file syntax and referenced atlas.".to_string(),
        )
    } else {
        (
            rest.to_string(),
            "The node property or reference does not conform to engine requirements.".to_string(),
            "Inspect the node in the scene and correct the configuration.".to_string(),
        )
    }
}

fn parse_scene_diagnostic(after_path: &str) -> (String, String, String) {
    if after_path.contains("root") && after_path.contains("not found in node list") {
        (
            format!("Scene root error: {after_path}."),
            "The scene declares a root node ID that is missing from its node list.".to_string(),
            "Add the missing root node or change 'root' in the scene header.".to_string(),
        )
    } else if after_path.contains("instance of") && after_path.contains("cannot load") {
        (
            format!("Child scene instance load failed: {after_path}."),
            "The referenced child scene file cannot be loaded or parsed.".to_string(),
            "Ensure the child scene file exists and is valid.".to_string(),
        )
    } else if after_path.contains("instance of")
        && after_path.contains("attaches to unknown parent")
    {
        (
            format!("Child scene parent error: {after_path}."),
            "The instance attaches to a parent node ID that does not exist.".to_string(),
            "Attach the instance to an existing node ID or omit 'parent' to attach to root."
                .to_string(),
        )
    } else if after_path.contains("instance of") && after_path.contains("bad override target") {
        (
            format!("Child scene override error: {after_path}."),
            "The override targets a node ID that is not present in the child scene.".to_string(),
            "Specify override keys in the format 'node_id.prop' matching nodes in the instance."
                .to_string(),
        )
    } else {
        (
            format!("Scene error: {after_path}."),
            "The scene file could not be parsed or contains syntax violations.".to_string(),
            "Check the scene TOML formatting or delete _cache/ to force a rebuild.".to_string(),
        )
    }
}

fn parse_manifest_diagnostic(msg: &str) -> (String, String, String) {
    if msg.contains("project.python must not be empty") {
        (
            "Field project.python is empty in [project].".to_string(),
            "The engine requires a Python version to select the appropriate runtime.".to_string(),
            "Set python = \"3.12\" under [project] in pite.toml.".to_string(),
        )
    } else if msg.contains("dimensions must be positive") {
        (
            format!("Invalid window dimensions: {msg}."),
            "The game window cannot initialize with zero or negative dimensions.".to_string(),
            "Set positive integers for window_width and window_height in pite.toml.".to_string(),
        )
    } else if msg.contains("icon") && msg.contains("missing") {
        let icon = extract_quoted_word(msg).unwrap_or_else(|| "icon".to_string());
        (
            format!("Project icon missing: \"{icon}\"."),
            "The icon file referenced in [project] does not exist on disk.".to_string(),
            "Provide the icon image at the path or update project.icon in pite.toml.".to_string(),
        )
    } else if msg.contains("icon") && msg.contains("must be res:// or empty") {
        (
            "Project icon must be a res:// path or empty.".to_string(),
            "Absolute paths are prohibited; all asset references must use res:// URIs.".to_string(),
            "Change icon to a res:// URI (e.g. res://assets/icon.png) or leave it empty."
                .to_string(),
        )
    } else if msg.contains("[props]") && msg.contains("unknown type") {
        (
            format!("Invalid custom property type: {msg}."),
            "Custom properties must declare an engine-supported type.".to_string(),
            "Set type to one of: string, int, number, bool, vec2, color.".to_string(),
        )
    } else if msg.contains("[props]") && msg.contains("collides with a schema prop") {
        (
            format!("Custom property name collision: {msg}."),
            "Schema property names are reserved by engine node types.".to_string(),
            "Choose a different name for the custom property in pite.toml [props].".to_string(),
        )
    } else if msg.contains("unknown field") {
        (
            format!("Unknown manifest field: {msg}."),
            "The field is not recognized by the Pite project manifest schema.".to_string(),
            "Remove or correct the unrecognized field name in pite.toml.".to_string(),
        )
    } else {
        (
            format!("Manifest error: {msg}."),
            "The manifest configuration does not conform to engine requirements.".to_string(),
            "Edit pite.toml to correct the invalid configuration.".to_string(),
        )
    }
}

fn parse_atlas_diagnostic(msg: &str) -> (String, String, String) {
    if msg.contains("frame") && msg.contains("is never referenced") {
        let frame = extract_quoted_word(msg).unwrap_or_else(|| "frame".to_string());
        (
            format!("Unused atlas frame: \"{frame}\"."),
            "The frame is defined in the atlas but not used by any scene, tilemap, or animation."
                .to_string(),
            "Reference the frame in a Sprite2D / TileMap2D or remove it from the atlas sidecar."
                .to_string(),
        )
    } else if msg.contains("animation") && msg.contains("is never referenced") {
        let anim = extract_quoted_word(msg).unwrap_or_else(|| "animation".to_string());
        (
            format!("Unused atlas animation: \"{anim}\"."),
            "The animation is defined in the atlas sidecar but never played by any node."
                .to_string(),
            "Reference the animation in an AnimatedSprite2D or remove it from the atlas sidecar."
                .to_string(),
        )
    } else {
        (
            format!("Atlas error: {msg}."),
            "The atlas sidecar does not meet engine requirements.".to_string(),
            "Inspect the .atlas.json sidecar file and correct any inconsistencies.".to_string(),
        )
    }
}

pub fn format_diagnostic_card_to_string(diag: &Diagnostic, c: &Colors) -> String {
    let (badge, badge_color) = match diag.severity {
        DiagnosticSeverity::Error => ("✖ ERROR", c.bold_red),
        DiagnosticSeverity::Warning => ("▲ WARN ", c.bold_yellow),
    };

    let loc = match (&diag.file, &diag.node) {
        (Some(f), Some(n)) => format!("  {f} › node \"{n}\""),
        (Some(f), None) => format!("  {f}"),
        (None, Some(n)) => format!("  node \"{n}\""),
        (None, None) => String::new(),
    };

    format!(
        "{badge_color}{badge}{reset}{loc_color}{loc}{reset}\n  {dim}What:{reset}  {what}\n  {dim}Why:{reset}   {why}\n  {dim}Fix:{reset}   {fix}",
        badge_color = badge_color,
        badge = badge,
        reset = c.reset,
        loc_color = c.bold_cyan,
        loc = loc,
        dim = c.dim,
        what = diag.what,
        why = diag.why,
        fix = diag.fix,
    )
}

pub fn print_diagnostic_card(diag: &Diagnostic, c: &Colors) {
    eprintln!("{}\n", format_diagnostic_card_to_string(diag, c));
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckSummary {
    pub errors: usize,
    pub warnings: usize,
    pub scenes_checked: usize,
    pub assets_scanned: usize,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckReport {
    pub schema_version: u32,
    pub ok: bool,
    pub summary: CheckSummary,
    pub diagnostics: Vec<Diagnostic>,
    pub cache: Vec<String>,
}

pub fn run(
    path: Option<&str>,
    reimport_assets: bool,
    strict: bool,
    json: bool,
    color: ColorChoice,
    verbose: bool,
) -> Result<()> {
    let start_time = std::time::Instant::now();
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
    let mut anim_uses: Vec<pite_assets::AnimationUse> = Vec::new();

    let root = match pite_project::find_project_root(&start) {
        Some(root) => root,
        None => {
            errors.push(format!(
                "no {} found above {}; run inside a project or pass --path",
                pite_project::MANIFEST_FILE,
                start.display()
            ));
            start.clone()
        }
    };

    if reimport_assets && errors.is_empty() {
        let old = pite_assets::load_registry(&root).unwrap_or_default();
        let files = pite_assets::scan_files(&root).unwrap_or_default();
        let (_next, scan) = pite_assets::reconcile(&old, &files);
        let (_reg, report) = pite_assets::reimport_project(&root)?;
        if !json {
            println!("scanned {} asset(s)", scan.scanned);
            for added in &scan.added {
                println!("added: {added}");
            }
            for (old_path, new_path) in &scan.renamed {
                println!("renamed: {old_path} -> {new_path}");
            }
            for removed in &scan.removed {
                println!("removed: {removed}");
            }
            for changed in &scan.changed {
                println!("changed: {changed}");
            }
            println!("reimported {} asset(s)", report.reimported.len());
            for r in &report.reimported {
                println!("reimported: {r}");
            }
            for r in &report.refs_updated {
                println!("refs updated: {r}");
            }
        }
        if !report.failed.is_empty() {
            for f in &report.failed {
                errors.push(format!("failed to reimport asset {f}"));
            }
        }
    }

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

    let mut scanned_count = 0;
    let mut visited_count = 0;

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
            Ok(files) => {
                scanned_count = files.len();
                files
            }
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
            &manifest.props,
            &mut errors,
            &mut warnings,
            &mut cache,
            &mut atlas_uses,
            &mut anim_uses,
            &registry,
            &by_path,
            &by_hash,
            &mut visited,
            &mut referenced,
        );
        visited_count = visited.len();
        let (atlas_errors, atlas_warnings) =
            pite_assets::atlas_report(&root, &atlas_uses, &anim_uses);
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
    }

    let elapsed = start_time.elapsed();
    let elapsed_ms = elapsed.as_millis() as u64;
    let elapsed_str = if elapsed_ms == 0 {
        "<1ms".to_string()
    } else if elapsed.as_secs() > 0 {
        format!("{:.2}s", elapsed.as_secs_f32())
    } else {
        format!("{elapsed_ms}ms")
    };

    let mut diagnostics = Vec::new();
    for e in &errors {
        diagnostics.push(Diagnostic::from_message(
            &root,
            DiagnosticSeverity::Error,
            e,
        ));
    }
    for w in &warnings {
        diagnostics.push(Diagnostic::from_message(
            &root,
            DiagnosticSeverity::Warning,
            w,
        ));
    }

    let failed = !errors.is_empty() || (strict && !warnings.is_empty());
    let report = CheckReport {
        schema_version: SCHEMA_VERSION,
        ok: !failed,
        summary: CheckSummary {
            errors: errors.len(),
            warnings: warnings.len(),
            scenes_checked: visited_count,
            assets_scanned: scanned_count,
            elapsed_ms,
        },
        diagnostics,
        cache,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        let is_term = std::io::stdout().is_terminal() || std::io::stderr().is_terminal();
        let use_color = color.is_enabled(is_term);
        let c = if use_color {
            COLOR_ENABLED
        } else {
            COLOR_DISABLED
        };

        if verbose {
            for line in &report.cache {
                println!("{}cache:{} {line}", c.dim, c.reset);
            }
        }

        for diag in &report.diagnostics {
            print_diagnostic_card(diag, &c);
        }

        let project_name = manifest
            .as_ref()
            .map(|m| m.project.name.as_str())
            .unwrap_or_else(|| {
                root.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("project")
            });

        if failed {
            let err_count = report.summary.errors;
            let warn_count = report.summary.warnings;
            let strict_label = if strict && err_count == 0 {
                " (strict)"
            } else {
                ""
            };
            eprintln!(
                "{err_color}{err_badge} Check failed{strict_label}:{reset} {err_count} error{err_s}, {warn_count} warning{warn_s} {dim}in {elapsed_str}{reset}",
                err_color = c.bold_red,
                err_badge = "✖",
                reset = c.reset,
                strict_label = strict_label,
                err_count = err_count,
                err_s = if err_count == 1 { "" } else { "s" },
                warn_count = warn_count,
                warn_s = if warn_count == 1 { "" } else { "s" },
                dim = c.dim,
                elapsed_str = elapsed_str,
            );
        } else if report.summary.warnings > 0 {
            let warn_count = report.summary.warnings;
            eprintln!(
                "{warn_color}{warn_badge} Check passed with warnings:{reset} 0 errors, {warn_count} warning{warn_s} {dim}in {elapsed_str}{reset}",
                warn_color = c.bold_yellow,
                warn_badge = "▲",
                reset = c.reset,
                warn_count = warn_count,
                warn_s = if warn_count == 1 { "" } else { "s" },
                dim = c.dim,
                elapsed_str = elapsed_str,
            );
        } else {
            let sc = report.summary.scenes_checked;
            let ac = report.summary.assets_scanned;
            println!(
                "{check_color}{check_badge} Checked{reset} {bold}{project_name}{reset} {dim}({sc} scene{sc_s}, {ac} asset{ac_s}) in {elapsed_str}{reset}",
                check_color = c.bold_green,
                check_badge = "✔",
                reset = c.reset,
                bold = c.bold,
                project_name = project_name,
                dim = c.dim,
                sc = sc,
                sc_s = if sc == 1 { "" } else { "s" },
                ac = ac,
                ac_s = if ac == 1 { "" } else { "s" },
                elapsed_str = elapsed_str,
            );
        }
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

fn check_scene_ref(
    root: &Path,
    scene_ref: &str,
    custom_props: &HashMap<String, String>,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
    cache: &mut Vec<String>,
    atlas_uses: &mut Vec<pite_assets::AtlasUse>,
    anim_uses: &mut Vec<pite_assets::AnimationUse>,
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
                for module in script_imports(&text) {
                    if module == "pite" || module == "__future__" {
                        continue;
                    }
                    let sibling = script_path.parent().is_some_and(|dir| {
                        dir.join(format!("{module}.py")).is_file()
                            || dir.join(module).join("__init__.py").is_file()
                    });
                    let vendored = crate::cmd_vendor::vendor_provides(&root.join("vendor"), module);
                    if sibling || vendored || pite_script::is_stdlib_module(module) {
                        continue;
                    }
                    warnings.push(format!(
                        "scene {}: node {:?} script imports third-party {module:?}, which exports drop (stdlib + project files only)",
                        path.display(),
                        node.id
                    ));
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
                    } else if let Err(e) = pite_audio::decode_audio(&asset_path) {
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
        if node.type_name == "AnimatedSprite2D" {
            let prop_str = |key: &str| node.props.get(key).and_then(|v| v.as_str());
            let atlas = prop_str("atlas").unwrap_or_default();
            let animation = prop_str("animation").unwrap_or_default();
            let autoplay = prop_str("autoplay").unwrap_or_default();

            if atlas.is_empty() {
                if !animation.is_empty() {
                    errors.push(format!(
                        "scene {}: node {:?} `animation` requires an `atlas`",
                        path.display(),
                        node.id
                    ));
                }
                if !autoplay.is_empty() {
                    errors.push(format!(
                        "scene {}: node {:?} `autoplay` requires an `atlas`",
                        path.display(),
                        node.id
                    ));
                }
            } else {
                let norm = normalize_asset_ref(atlas);
                referenced.insert(norm);
                if !animation.is_empty() {
                    anim_uses.push(pite_assets::AnimationUse {
                        scene: path.to_string_lossy().into_owned(),
                        node: node.id.clone(),
                        atlas: atlas.to_string(),
                        animation: animation.to_string(),
                    });
                }
                if !autoplay.is_empty() && autoplay != animation {
                    anim_uses.push(pite_assets::AnimationUse {
                        scene: path.to_string_lossy().into_owned(),
                        node: node.id.clone(),
                        atlas: atlas.to_string(),
                        animation: autoplay.to_string(),
                    });
                }
            }
        }
        if node.type_name == "TileMap2D" {
            if let Some(map_ref) = node.props.get("tilemap").and_then(|v| v.as_str()) {
                let map_path = if let Some(rel) = map_ref.strip_prefix("res://") {
                    root.join(rel)
                } else {
                    root.join(map_ref)
                };
                if map_path.is_file() {
                    match pite_scene::load_tilemap(&map_path) {
                        Ok(doc) => {
                            referenced.insert(normalize_asset_ref(&doc.atlas));
                            for layer in &doc.layers {
                                for tile in &layer.tiles {
                                    atlas_uses.push(pite_assets::AtlasUse {
                                        scene: path.to_string_lossy().into_owned(),
                                        node: node.id.clone(),
                                        atlas: doc.atlas.clone(),
                                        frame: tile.2.clone(),
                                    });
                                }
                            }
                        }
                        Err(e) => errors.push(format!(
                            "scene {}: node {:?} invalid tilemap {map_ref:?}: {e:#}",
                            path.display(),
                            node.id
                        )),
                    }
                }
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
                        "scene {}: node {:?} references moved asset {s:?} (now at {new:?}); run 'pite check --reimport-assets'",
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
        // Custom-prop validation: every non-schema prop either matches a
        // `[props]` declaration (silent), or is undeclared (warning).
        // Schema props skip — they have typed accessors and their own
        // per-type checks above.
        let schema_keys: HashSet<&str> = pite_core::all_props(&node.type_name)
            .iter()
            .map(|p| p.key)
            .collect();
        let mut prop_keys: Vec<&String> = node.props.keys().collect();
        prop_keys.sort();
        for key in prop_keys {
            if schema_keys.contains(key.as_str()) {
                continue;
            }
            let raw = &node.props[key];
            match custom_props.get(key) {
                Some(declared_type) => {
                    let value = pite_scene::toml_to_prop(raw);
                    if let Err(e) = pite_core::validate_custom_prop_type(declared_type, &value) {
                        errors.push(format!(
                            "scene {}: node {:?} custom prop {key:?} {e}",
                            path.display(),
                            node.id
                        ));
                    }
                }
                None => warnings.push(format!(
                    "scene {}: node {:?} has unknown prop {key:?}; declare it in [props] to silence",
                    path.display(),
                    node.id
                )),
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
            custom_props,
            errors,
            warnings,
            cache,
            atlas_uses,
            anim_uses,
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

/// Top-level module names imported by a script (`import a, b`, `from a
/// import …`), comment-stripped like `played_assets`. Relative imports
/// (`.x`) resolve to siblings, never third-party.
fn script_imports(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if let Some(list) = line.strip_prefix("import ") {
            for item in list.split(',') {
                let name = item
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(';');
                let top = name.split('.').next().unwrap_or("");
                if !top.is_empty() && !out.contains(&top) {
                    out.push(top);
                }
            }
        } else if let Some(after_from) = line.strip_prefix("from ") {
            let module = after_from.split_whitespace().next().unwrap_or("");
            if module.starts_with('.') {
                continue;
            }
            let top = module.split('.').next().unwrap_or("");
            if !top.is_empty() && !out.contains(&top) {
                out.push(top);
            }
        }
    }
    out
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
        std::fs::write(dir.join("scenes").join("main.ptscn"), FIXTURE_SCENE)
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
            "res://scenes/main.ptscn",
            &HashMap::new(),
            &mut errors,
            &mut warnings,
            &mut cache,
            &mut atlas_uses,
            &mut Vec::new(),
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
        let registry = pite_assets::UidRegistry::default();
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
        let registry = pite_assets::UidRegistry::default();
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
        let registry = pite_assets::UidRegistry::default();
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
            root.join("scenes").join("main.ptscn"),
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
        let registry = pite_assets::UidRegistry::default();
        let (_, _, referenced) = run_ref(&root, &registry, &by_path, &by_hash);
        assert!(
            referenced.contains("res://assets/hit.wav"),
            "got: {referenced:?}"
        );
        assert!(orphan_warnings(&by_path, &referenced).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn check_validates_audio_multiformat() {
        let root = fixture_root(42);
        std::fs::create_dir_all(root.join("scripts")).expect("create scripts dir");
        std::fs::create_dir_all(root.join("assets")).expect("create assets dir");
        std::fs::write(
            root.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"sfx\"\ntype = \"Node\"\nname = \"Sfx\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/sfx.py\"\nclass = \"Sfx\"\n",
        )
        .expect("write scene");
        std::fs::write(
            root.join("scripts").join("sfx.py"),
            "import pite\n\nclass Sfx(pite.Node):\n    def _ready(self):\n        pite.play(\"res://assets/music.ogg\")\n        pite.play(\"res://assets/bad.mp3\")\n",
        )
        .expect("write script");
        std::fs::write(
            root.join("assets").join("music.ogg"),
            pite_audio::fixtures::FIXTURE_OGG,
        )
        .unwrap();
        std::fs::write(root.join("assets").join("bad.mp3"), b"ID3_NOT_A_VALID_MP3").unwrap();

        let files = scanned(&["res://assets/music.ogg", "res://assets/bad.mp3"]);
        let by_path: HashMap<&str, &pite_assets::ScannedFile> =
            files.iter().map(|f| (f.res_path.as_str(), f)).collect();
        let by_hash: HashMap<&str, &str> = HashMap::new();
        let registry = pite_assets::UidRegistry::default();
        let (errors, _, referenced) = run_ref(&root, &registry, &by_path, &by_hash);
        assert!(referenced.contains("res://assets/music.ogg"));
        assert!(referenced.contains("res://assets/bad.mp3"));
        assert!(
            errors
                .iter()
                .any(|e| e.contains("bad.mp3") && e.contains("undecodable")),
            "expected undecodable error for bad.mp3, got: {errors:?}"
        );
        assert!(
            !errors.iter().any(|e| e.contains("music.ogg")),
            "music.ogg should decode cleanly, got: {errors:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn script_imports_extracts_top_levels() {
        let text = "import pite\nimport os, sys # comment\nfrom collections import OrderedDict\nfrom . import helper\nfrom pkg.sub import thing\nimport numpy as np\n# import commented_out\nx = \"import string_in_literal\"\n";
        assert_eq!(
            script_imports(text),
            vec!["pite", "os", "sys", "collections", "pkg", "numpy"]
        );
    }

    #[test]
    fn third_party_import_warns_once() {
        let root = fixture_root(5);
        std::fs::create_dir_all(root.join("scripts")).expect("create scripts dir");
        std::fs::write(
            root.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"sfx\"\ntype = \"Node\"\nname = \"Sfx\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/sfx.py\"\nclass = \"Sfx\"\n",
        )
        .expect("write scene");
        std::fs::write(
            root.join("scripts").join("sfx.py"),
            "import pite\nimport os\nimport numpy\nimport vpkg\nfrom collections import OrderedDict\nfrom . import helper\nimport helper\n",
        )
        .expect("write script");
        std::fs::write(root.join("scripts").join("helper.py"), "x = 1\n").expect("write sibling");
        std::fs::create_dir_all(root.join("vendor").join("vpkg")).expect("vendor dir");
        std::fs::write(
            root.join("vendor").join("vpkg").join("__init__.py"),
            "y = 2\n",
        )
        .expect("write vendored");
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut cache = Vec::new();
        let mut atlas_uses = Vec::new();
        let mut visited = HashSet::new();
        let mut referenced = HashSet::new();
        let registry = pite_assets::UidRegistry::default();
        let by_path: HashMap<&str, &pite_assets::ScannedFile> = HashMap::new();
        let by_hash: HashMap<&str, &str> = HashMap::new();
        check_scene_ref(
            &root,
            "res://scenes/main.ptscn",
            &HashMap::new(),
            &mut errors,
            &mut warnings,
            &mut cache,
            &mut atlas_uses,
            &mut Vec::new(),
            &registry,
            &by_path,
            &by_hash,
            &mut visited,
            &mut referenced,
        );
        assert!(errors.is_empty(), "got: {errors:?}");
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert!(warnings[0].contains("\"numpy\""), "got: {warnings:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn custom_props_are_validated() {
        let root = fixture_root(99);
        // Scene with a declared + an undeclared custom prop, plus a type
        // mismatch against a declared type.
        std::fs::write(
            root.join("scenes").join("main.ptscn"),
            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[node.props]\nspawn_group = \"warrior\"\nboss_hp = \"lots\"\ntypo_field = 42\n",
        )
        .unwrap();
        let mut custom = HashMap::new();
        custom.insert("spawn_group".to_string(), "string".to_string());
        custom.insert("boss_hp".to_string(), "int".to_string());

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut cache = Vec::new();
        let mut atlas = Vec::new();
        let mut visited = HashSet::new();
        let mut referenced = HashSet::new();
        check_scene_ref(
            &root,
            "res://scenes/main.ptscn",
            &custom,
            &mut errors,
            &mut warnings,
            &mut cache,
            &mut atlas,
            &mut Vec::new(),
            &pite_assets::UidRegistry::default(),
            &HashMap::new(),
            &HashMap::new(),
            &mut visited,
            &mut referenced,
        );
        // spawn_group: silent (declared, correct type).
        // boss_hp: error (declared as int, scene value is string).
        // typo_field: warning (undeclared).
        assert!(
            errors
                .iter()
                .any(|e| e.contains("custom prop \"boss_hp\"")
                    && e.contains("expected int, got string")),
            "got errors: {errors:?}"
        );
        assert!(
            !errors.iter().any(|e| e.contains("spawn_group")),
            "spawn_group must be silent, got errors: {errors:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("unknown prop \"typo_field\"")),
            "got warnings: {warnings:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn check_validates_tilemap_assets() {
        let root = fixture_root(6);
        std::fs::create_dir_all(root.join("maps")).expect("create maps dir");
        std::fs::write(
            root.join("scenes").join("main.ptscn"),
            r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "map"
type = "TileMap2D"
name = "TileMap"
parent = "root"

[node.props]
tilemap = "res://maps/level.ptmap"
"#,
        )
        .expect("write scene");

        std::fs::write(
            root.join("maps").join("level.ptmap"),
            r#"atlas = "res://tiles.atlas.json"
tile_size = [16, 16]

[[layer]]
name = "ground"
tiles = [[0, 0, "grass"], [1, 0, "dirt"]]
"#,
        )
        .expect("write tilemap");

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut cache = Vec::new();
        let mut atlas_uses = Vec::new();
        let mut visited = HashSet::new();
        let mut referenced = HashSet::new();
        let registry = pite_assets::UidRegistry::default();
        let by_path: HashMap<&str, &pite_assets::ScannedFile> = HashMap::new();
        let by_hash: HashMap<&str, &str> = HashMap::new();

        check_scene_ref(
            &root,
            "res://scenes/main.ptscn",
            &HashMap::new(),
            &mut errors,
            &mut warnings,
            &mut cache,
            &mut atlas_uses,
            &mut Vec::new(),
            &registry,
            &by_path,
            &by_hash,
            &mut visited,
            &mut referenced,
        );

        assert!(errors.is_empty(), "got: {errors:?}");
        assert!(referenced.contains("res://maps/level.ptmap"));
        assert!(referenced.contains("res://tiles.atlas.json"));
        assert_eq!(atlas_uses.len(), 2);
        assert_eq!(atlas_uses[0].frame, "grass");
        assert_eq!(atlas_uses[1].frame, "dirt");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn animated_sprite_validation() {
        let root = fixture_root(77);
        std::fs::write(
            root.join("assets").join("hero.atlas.json"),
            r#"{"texture":"hero.png","size":[64,32],
                "frames":{"idle1":{"x":0,"y":0,"w":32,"h":32}},
                "animations":{"idle":{"frames":["idle1"],"fps":6.0}}}"#,
        )
        .unwrap();

        std::fs::write(
            root.join("scenes").join("main.ptscn"),
            r#"format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "anim_spr"
type = "AnimatedSprite2D"
name = "Hero"
parent = "root"

[node.props]
atlas = "res://assets/hero.atlas.json"
animation = "idle"
"#,
        )
        .unwrap();

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut cache = Vec::new();
        let mut atlas_uses = Vec::new();
        let mut anim_uses = Vec::new();
        let mut visited = HashSet::new();
        let mut referenced = HashSet::new();
        let registry = pite_assets::UidRegistry::default();
        let by_path: HashMap<&str, &pite_assets::ScannedFile> = HashMap::new();
        let by_hash: HashMap<&str, &str> = HashMap::new();

        check_scene_ref(
            &root,
            "res://scenes/main.ptscn",
            &HashMap::new(),
            &mut errors,
            &mut warnings,
            &mut cache,
            &mut atlas_uses,
            &mut anim_uses,
            &registry,
            &by_path,
            &by_hash,
            &mut visited,
            &mut referenced,
        );

        assert!(errors.is_empty(), "got errors: {errors:?}");
        assert_eq!(anim_uses.len(), 1);
        assert_eq!(anim_uses[0].animation, "idle");
        assert!(referenced.contains("res://assets/hero.atlas.json"));

        let (report_errors, report_warnings) =
            pite_assets::atlas_report(&root, &atlas_uses, &anim_uses);
        assert!(
            report_errors.is_empty(),
            "got report errors: {report_errors:?}"
        );
        assert!(
            report_warnings.is_empty(),
            "got report warnings: {report_warnings:?}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diagnostic_card_formatting_error() {
        let diag = Diagnostic {
            severity: DiagnosticSeverity::Error,
            file: Some("scenes/main.ptscn".to_string()),
            node: Some("Player".to_string()),
            what: "Node type \"FooBar\" is not registered in the engine node schema.".to_string(),
            why: "The engine cannot instantiate a node with an unrecognized type at runtime."
                .to_string(),
            fix: "Change the type to a registered node class (e.g. Sprite2D, Node2D, CharacterBody2D)."
                .to_string(),
        };
        let formatted = format_diagnostic_card_to_string(&diag, &COLOR_DISABLED);
        assert!(formatted.starts_with("✖ ERROR  scenes/main.ptscn › node \"Player\"\n"));
        assert!(formatted.contains("  What:  Node type \"FooBar\""));
        assert!(formatted.contains("  Why:   The engine cannot instantiate"));
        assert!(formatted.contains("  Fix:   Change the type"));
    }

    #[test]
    fn diagnostic_card_formatting_warning() {
        let diag = Diagnostic {
            severity: DiagnosticSeverity::Warning,
            file: Some("assets/apple.png".to_string()),
            node: None,
            what: "Asset \"assets/apple.png\" is never referenced by any reachable scene."
                .to_string(),
            why: "Unused assets inflate project size and export packages.".to_string(),
            fix: "Reference the asset in a scene or delete it if no longer needed.".to_string(),
        };
        let formatted = format_diagnostic_card_to_string(&diag, &COLOR_DISABLED);
        assert!(formatted.starts_with("▲ WARN   assets/apple.png\n"));
        assert!(formatted.contains("  What:  Asset \"assets/apple.png\""));
        assert!(formatted.contains("  Why:   Unused assets inflate"));
        assert!(formatted.contains("  Fix:   Reference the asset"));
    }

    #[test]
    fn diagnostic_from_message_variations() {
        let root = Path::new("/workspace/game");

        // 1. Unregistered type
        let d1 = Diagnostic::from_message(
            root,
            DiagnosticSeverity::Error,
            "scene /workspace/game/scenes/main.ptscn: node \"Player\" has unregistered type \"Alien\"",
        );
        assert_eq!(d1.severity, DiagnosticSeverity::Error);
        assert_eq!(d1.file.as_deref(), Some("scenes/main.ptscn"));
        assert_eq!(d1.node.as_deref(), Some("Player"));
        assert!(d1.what.contains("Alien"));

        // 2. Missing script
        let d2 = Diagnostic::from_message(
            root,
            DiagnosticSeverity::Warning,
            "scene /workspace/game/scenes/main.ptscn: node \"Boss\" script \"res://scripts/boss.py\" missing, using placeholder",
        );
        assert_eq!(d2.severity, DiagnosticSeverity::Warning);
        assert_eq!(d2.node.as_deref(), Some("Boss"));
        assert!(d2.what.contains("res://scripts/boss.py"));

        // 3. Third-party import
        let d3 = Diagnostic::from_message(
            root,
            DiagnosticSeverity::Warning,
            "scene /workspace/game/scenes/main.ptscn: node \"Sfx\" script imports third-party \"scipy\", which exports drop (stdlib + project files only)",
        );
        assert!(d3.what.contains("scipy"));
        assert!(d3.fix.contains("pite vendor add scipy"));

        // 4. Orphan asset
        let d4 = Diagnostic::from_message(
            root,
            DiagnosticSeverity::Warning,
            "asset \"res://assets/ghost.png\" is never referenced by a reachable scene",
        );
        assert_eq!(d4.file.as_deref(), Some("assets/ghost.png"));
        assert!(d4.what.contains("assets/ghost.png"));

        // 5. Manifest error
        let d5 = Diagnostic::from_message(
            root,
            DiagnosticSeverity::Error,
            "manifest: project.python must not be empty",
        );
        assert_eq!(d5.file.as_deref(), Some("pite.toml"));
        assert!(d5.fix.contains("python = \"3.12\""));
    }

    #[test]
    fn check_report_v3_json_serialization() {
        let report = CheckReport {
            schema_version: SCHEMA_VERSION,
            ok: false,
            summary: CheckSummary {
                errors: 1,
                warnings: 0,
                scenes_checked: 2,
                assets_scanned: 10,
                elapsed_ms: 15,
            },
            diagnostics: vec![Diagnostic {
                severity: DiagnosticSeverity::Error,
                file: Some("scenes/main.ptscn".to_string()),
                node: Some("Hero".to_string()),
                what: "Test error".to_string(),
                why: "Test why".to_string(),
                fix: "Test fix".to_string(),
            }],
            cache: vec!["hit scenes/main.ptscn".to_string()],
        };

        let json = serde_json::to_string_pretty(&report).expect("serialize report");
        let val: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(val["schema_version"], 3);
        assert_eq!(val["ok"], false);
        assert_eq!(val["summary"]["errors"], 1);
        assert_eq!(val["summary"]["scenes_checked"], 2);
        assert_eq!(val["diagnostics"][0]["severity"], "error");
        assert_eq!(val["diagnostics"][0]["file"], "scenes/main.ptscn");
        assert_eq!(val["diagnostics"][0]["node"], "Hero");
        assert_eq!(val["diagnostics"][0]["what"], "Test error");
    }

    #[test]
    fn color_choice_detection() {
        assert!(ColorChoice::Always.is_enabled(false));
        assert!(ColorChoice::Always.is_enabled(true));
        assert!(!ColorChoice::Never.is_enabled(false));
        assert!(!ColorChoice::Never.is_enabled(true));
        assert!(!ColorChoice::Auto.is_enabled(false));
    }
}
