// SPDX-License-Identifier: MIT OR Apache-2.0
//! Structured findings for `pite check`.
//!
//! Findings are built where a problem is detected (typed constructors) or
//! adapted from the string errors that lower-level crates still return
//! ([`Finding::from_raw`]). Rendering and JSON both consume this one type, so
//! the human card and the machine payload can never drift apart.
//!
//! Each text field is a [`Spans`]: plain prose plus the pieces that deserve
//! emphasis (bold paths and commands, cyan identifiers, dim error detail).
//! JSON serializes the concatenated plain text, so styling never reaches
//! machine consumers.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::text::Spans;

/// How serious a finding is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// The project domain a finding belongs to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Project,
    Manifest,
    Scene,
    Asset,
    Audio,
    Script,
    Atlas,
    Reimport,
}

/// Where a finding points, when it points anywhere.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Location {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
}

impl Location {
    pub fn file(file: impl Into<String>) -> Self {
        Self {
            file: Some(file.into()),
            node: None,
        }
    }

    pub fn node(file: impl Into<String>, node: impl Into<String>) -> Self {
        Self {
            file: Some(file.into()),
            node: Some(node.into()),
        }
    }

    /// Human form: `scenes/main.ptscn › node "sprite"`.
    pub fn render(&self) -> Option<String> {
        match (&self.file, &self.node) {
            (Some(file), Some(node)) => Some(format!("{file} › node \"{node}\"")),
            (Some(file), None) => Some(file.clone()),
            (None, Some(node)) => Some(format!("node \"{node}\"")),
            (None, None) => None,
        }
    }
}

/// One check result: what went wrong, why it matters, and how to fix it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub kind: Kind,
    pub location: Location,
    pub message: Spans,
    pub why: Spans,
    pub fix: Spans,
}

const GENERIC_WHY: &str =
    "The project verification check could not map this message to a known rule.";
const GENERIC_FIX: &str = "Inspect the referenced file and fix any syntax or configuration errors.";

impl Finding {
    fn new(
        severity: Severity,
        kind: Kind,
        location: Location,
        message: Spans,
        why: Spans,
        fix: Spans,
    ) -> Self {
        Self {
            severity,
            kind,
            location,
            message,
            why,
            fix,
        }
    }

    fn error(kind: Kind, location: Location, message: Spans, why: Spans, fix: Spans) -> Self {
        Self::new(Severity::Error, kind, location, message, why, fix)
    }

    fn warning(kind: Kind, location: Location, message: Spans, why: Spans, fix: Spans) -> Self {
        Self::new(Severity::Warning, kind, location, message, why, fix)
    }

    /// Whether running `pite check --reimport-assets` would resolve this
    /// finding: its fix points at that command and the finding is not a
    /// failure of reimport itself.
    pub fn fixed_by_reimport(&self) -> bool {
        self.kind != Kind::Reimport
            && self
                .fix
                .to_string()
                .contains("pite check --reimport-assets")
    }

    // ---- Project-level -------------------------------------------------

    pub fn missing_project_root(start: &Path) -> Self {
        Self::error(
            Kind::Project,
            Location::default(),
            Spans::text("Not a Pite project."),
            Spans::new()
                .strong("pite check")
                .plain(" must run inside a game project, but no ")
                .info(pite_project::MANIFEST_FILE)
                .plain(" was found above ")
                .strong(start.display().to_string())
                .plain("."),
            Spans::new()
                .plain("Run inside a project, pass the project path with ")
                .info("--path")
                .plain(", or create one with ")
                .strong("pite new")
                .plain("."),
        )
    }

    pub fn icon_missing(icon: &str) -> Self {
        Self::warning(
            Kind::Manifest,
            Location::file(pite_project::MANIFEST_FILE),
            Spans::new()
                .plain("Project icon ")
                .strong(icon)
                .plain(" is missing."),
            Spans::new()
                .plain("The icon named by ")
                .info("project.icon")
                .plain(" in ")
                .info("pite.toml")
                .plain(" is not on disk."),
            Spans::new()
                .plain("Add the icon at that path, or point ")
                .info("project.icon")
                .plain(" at another image."),
        )
    }

    pub fn asset_scan_failed(error: &str) -> Self {
        Self::warning(
            Kind::Asset,
            Location::default(),
            Spans::text("Asset scan failed."),
            Spans::new()
                .plain("The engine could not list project files: ")
                .faded(error),
            Spans::new()
                .plain("Check file permissions, then rerun ")
                .strong("pite check")
                .plain("."),
        )
    }

    pub fn reimport_failed(asset: &str) -> Self {
        Self::error(
            Kind::Reimport,
            Location::file(asset),
            Spans::new()
                .plain("Could not reimport ")
                .strong(asset)
                .plain("."),
            Spans::text("The importer could not parse the file or update its manifest entry."),
            Spans::new()
                .plain("Make sure the file is readable and not corrupted, then rerun ")
                .info("pite check --reimport-assets")
                .plain("."),
        )
    }

    pub fn assets_manifest_missing() -> Self {
        Self::warning(
            Kind::Asset,
            Location::file(pite_assets::MANIFEST_FILE),
            Spans::new()
                .plain("Asset manifest ")
                .strong(pite_assets::MANIFEST_FILE)
                .plain(" is missing."),
            Spans::new()
                .plain("Pite tracks asset content hashes in ")
                .info(pite_assets::MANIFEST_FILE)
                .plain(" to enable rename detection and keep references synchronized."),
            Spans::new()
                .plain("Run ")
                .info("pite check --reimport-assets")
                .plain(" to generate it, and commit it to version control."),
        )
    }

    pub fn assets_manifest_stale() -> Self {
        Self::warning(
            Kind::Asset,
            Location::file(pite_assets::MANIFEST_FILE),
            Spans::new()
                .plain("Asset manifest ")
                .strong(pite_assets::MANIFEST_FILE)
                .plain(" is out of sync with files on disk."),
            Spans::new()
                .plain("One or more assets have been added, modified, renamed, or deleted since ")
                .info(pite_assets::MANIFEST_FILE)
                .plain(" was last updated."),
            Spans::new()
                .plain("Run ")
                .info("pite check --reimport-assets")
                .plain(" to update it, and commit the changes."),
        )
    }

    pub fn orphan_asset(asset: &str) -> Self {
        Self::warning(
            Kind::Asset,
            Location::file(asset),
            Spans::new()
                .plain("Asset ")
                .strong(asset)
                .plain(" is never referenced by any reachable scene."),
            Spans::new()
                .plain("Only scenes reachable from ")
                .info("main_scene")
                .plain(" count as references."),
            Spans::text("Reference it from a scene, or delete it if it is no longer needed."),
        )
    }

    pub fn legacy_atlas_sidecar(rel_path: &str) -> Self {
        Self::warning(
            Kind::Atlas,
            Location::file(rel_path),
            Spans::new()
                .plain("Legacy atlas sidecar format ")
                .info(rel_path)
                .plain("."),
            Spans::text(
                "Pite uses `.ptatlas` (TOML) for sprite-sheet sidecars instead of legacy `.atlas.json`.",
            ),
            Spans::new()
                .plain("Run ")
                .info("pite check --reimport-assets")
                .plain(" to automatically migrate to `.ptatlas`."),
        )
    }

    // ---- Scene walk ----------------------------------------------------

    pub fn scene_root_missing(scene: &str, root: &str) -> Self {
        Self::error(
            Kind::Scene,
            Location::file(scene),
            Spans::new()
                .plain("Root node ")
                .info(root)
                .plain(" is missing from the node list."),
            Spans::new()
                .plain("The scene header declares ")
                .info("root")
                .plain(", but no node with that id exists."),
            Spans::new()
                .plain("Add the node, or point ")
                .info("root")
                .plain(" at an existing node id."),
        )
    }

    pub fn unregistered_type(scene: &str, node: &str, ty: &str) -> Self {
        Self::error(
            Kind::Scene,
            Location::node(scene, node),
            Spans::new().plain("Unknown node type ").info(ty).plain("."),
            Spans::text("The engine can only instantiate types that are in its node registry."),
            Spans::new()
                .plain("Use a registered type such as ")
                .info("Sprite2D")
                .plain(", ")
                .info("Node2D")
                .plain(", or ")
                .info("CharacterBody2D")
                .plain("."),
        )
    }

    pub fn unknown_parent(scene: &str, node: &str, parent: &str) -> Self {
        Self::error(
            Kind::Scene,
            Location::node(scene, node),
            Spans::new()
                .plain("Parent node ")
                .info(parent)
                .plain(" does not exist."),
            Spans::new()
                .plain("Every ")
                .info("parent")
                .plain(" must name a node in the same scene."),
            Spans::new()
                .plain("Point ")
                .info("parent")
                .plain(" at an existing node id, or remove it to attach to the root."),
        )
    }

    pub fn missing_script(scene: &str, node: &str, script: &str) -> Self {
        Self::warning(
            Kind::Script,
            Location::node(scene, node),
            Spans::new()
                .plain("Script ")
                .strong(script)
                .plain(" does not exist."),
            Spans::text("The node falls back to an inert placeholder at runtime."),
            Spans::new()
                .plain("Create the file at that ")
                .info("res://")
                .plain(" path, or remove the ")
                .info("script")
                .plain(" binding."),
        )
    }

    pub fn reserved_class(scene: &str, node: &str, class: &str) -> Self {
        Self::warning(
            Kind::Script,
            Location::node(scene, node),
            Spans::new()
                .plain("Script class ")
                .info(class)
                .plain(" is reserved by the engine."),
            Spans::text("The name collides with an engine signal dispatcher."),
            Spans::text("Rename the class in the script and in its scene binding."),
        )
    }

    pub fn third_party_import(scene: &str, node: &str, module: &str) -> Self {
        Self::warning(
            Kind::Script,
            Location::node(scene, node),
            Spans::new()
                .plain("Third-party module ")
                .info(module)
                .plain(" is not vendored."),
            Spans::new()
                .plain("Exports drop host ")
                .info("site-packages")
                .plain(", so the import fails at runtime."),
            Spans::new()
                .plain("Run ")
                .strong(format!("pite vendor add {module}"))
                .plain(" to vendor it into ")
                .info("vendor/")
                .plain("."),
        )
    }

    pub fn missing_audio(scene: &str, node: &str, asset: &str) -> Self {
        Self::error(
            Kind::Audio,
            Location::node(scene, node),
            Spans::new()
                .plain("Audio asset ")
                .strong(asset)
                .plain(" is missing."),
            Spans::new()
                .plain("A ")
                .info("pite.play()")
                .plain(" call references a file that does not exist."),
            Spans::new()
                .plain("Add the file at that ")
                .info("res://")
                .plain(" path, or update the ")
                .info("play()")
                .plain(" call."),
        )
    }

    pub fn undecodable_audio(scene: &str, node: &str, asset: &str, error: &str) -> Self {
        Self::error(
            Kind::Audio,
            Location::node(scene, node),
            Spans::new()
                .plain("Audio asset ")
                .strong(asset)
                .plain(" cannot be decoded."),
            Spans::new()
                .faded(error)
                .plain(" Supported formats are WAV, OGG, MP3, and FLAC."),
            Spans::text("Re-encode or replace the file with a valid WAV, OGG, MP3, or FLAC."),
        )
    }

    pub fn animated_requires_atlas(scene: &str, node: &str, prop: &str) -> Self {
        Self::error(
            Kind::Scene,
            Location::node(scene, node),
            Spans::new()
                .plain("AnimatedSprite2D ")
                .info(prop)
                .plain(" requires an ")
                .info("atlas")
                .plain("."),
            Spans::text("Animations and autoplay read their frames from an atlas sidecar."),
            Spans::new()
                .plain("Set ")
                .info("atlas = \"res://…\"")
                .plain(" on the node."),
        )
    }

    pub fn invalid_tilemap(scene: &str, node: &str, map: &str, error: &str) -> Self {
        Self::error(
            Kind::Scene,
            Location::node(scene, node),
            Spans::new()
                .plain("Tilemap ")
                .strong(map)
                .plain(" is invalid."),
            Spans::new()
                .faded(error)
                .plain(" The ")
                .info(".ptmap")
                .plain(" sidecar could not be loaded."),
            Spans::text("Fix the tilemap file and the atlas it references."),
        )
    }

    pub fn missing_asset(scene: &str, node: &str, asset: &str) -> Self {
        Self::error(
            Kind::Asset,
            Location::node(scene, node),
            Spans::new()
                .plain("Asset ")
                .strong(asset)
                .plain(" does not exist."),
            Spans::text("The node points at a resource file that is not on disk."),
            Spans::new()
                .plain("Add the file at that ")
                .info("res://")
                .plain(" path, or update the property."),
        )
    }

    pub fn moved_asset(scene: &str, node: &str, asset: &str, new_path: &str) -> Self {
        Self::error(
            Kind::Asset,
            Location::node(scene, node),
            Spans::new()
                .plain("Asset moved: ")
                .strong(asset)
                .plain(" → ")
                .strong(new_path)
                .plain("."),
            Spans::text("The file was renamed or moved outside the editor."),
            Spans::new()
                .plain("Update the reference to ")
                .strong(new_path)
                .plain(", or run ")
                .info("pite check --reimport-assets")
                .plain(" to rewrite it."),
        )
    }

    pub fn custom_prop_type(scene: &str, node: &str, key: &str, error: &str) -> Self {
        Self::error(
            Kind::Scene,
            Location::node(scene, node),
            Spans::new()
                .plain("Custom property ")
                .info(key)
                .plain(" has the wrong type."),
            Spans::new()
                .plain("The scene value does not match the type declared under ")
                .info("[props]")
                .plain(" in ")
                .info("pite.toml")
                .plain(": ")
                .faded(error),
            Spans::new()
                .plain("Change the value to match, or edit the declared type under ")
                .info("[props]")
                .plain("."),
        )
    }

    pub fn unknown_prop(scene: &str, node: &str, key: &str) -> Self {
        Self::warning(
            Kind::Scene,
            Location::node(scene, node),
            Spans::new().plain("Unknown property ").info(key).plain("."),
            Spans::new()
                .plain("It is not a schema property and is not declared under ")
                .info("[props]")
                .plain(" in ")
                .info("pite.toml")
                .plain("."),
            Spans::new()
                .plain("Declare it under ")
                .info("[props]")
                .plain(" to silence this, or remove it from the scene."),
        )
    }

    pub fn instance_load(scene: &str, instance: &str, error: &str) -> Self {
        Self::error(
            Kind::Scene,
            Location::file(scene),
            Spans::new()
                .plain("Child scene ")
                .strong(instance)
                .plain(" cannot load."),
            Spans::new()
                .faded(error)
                .plain(" The referenced scene file is missing or invalid."),
            Spans::text("Make sure the child scene exists and loads cleanly."),
        )
    }

    pub fn instance_parent(scene: &str, instance: &str, parent: &str) -> Self {
        Self::error(
            Kind::Scene,
            Location::file(scene),
            Spans::new()
                .plain("Instance of ")
                .strong(instance)
                .plain(" attaches to unknown parent ")
                .info(parent)
                .plain("."),
            Spans::new()
                .plain("The ")
                .info("parent")
                .plain(" id does not exist in this scene."),
            Spans::new()
                .plain("Attach it to an existing node id, or drop ")
                .info("parent")
                .plain(" to attach at the root."),
        )
    }

    pub fn instance_override(scene: &str, instance: &str, key: &str) -> Self {
        Self::error(
            Kind::Scene,
            Location::file(scene),
            Spans::new()
                .plain("Instance of ")
                .strong(instance)
                .plain(" has a bad override target ")
                .info(key)
                .plain("."),
            Spans::new()
                .plain("Override keys must be ")
                .info("node_id.prop")
                .plain(" for a node in the child scene."),
            Spans::text("Point the override at a node in the child scene, or remove it."),
        )
    }

    /// A benign scene-load warning (stale cache, forward-compatible field).
    pub fn scene_warning(scene: &str, raw: &str) -> Self {
        Self::warning(
            Kind::Scene,
            Location::file(scene),
            Spans::text(raw.trim_end_matches('.').to_string() + "."),
            Spans::text("The scene loaded, but the loader reported a compatibility note."),
            Spans::new()
                .plain("Rebuild the scene cache (")
                .info("_cache/")
                .plain("), or bring the field up to the current scene format."),
        )
    }

    // ---- Boundary adapters ---------------------------------------------

    /// Adapt a string finding from a lower-level crate (`pite-project`,
    /// `pite-assets`, `pite-scene`) into a structured finding.
    pub fn from_raw(root: &Path, severity: Severity, raw: &str) -> Self {
        let raw = raw.trim();

        if let Some(scene_rest) = raw.strip_prefix("scene ") {
            if let Some((path_str, after_path)) = scene_rest.split_once(": ") {
                let rel_file = format_path_rel(root, path_str);
                if let Some(node_rest) = after_path.strip_prefix("node ") {
                    if let Some((node_id, rest)) = extract_quoted_or_word(node_rest) {
                        let (message, why, fix) = parse_node_diagnostic(rest);
                        return Self::new(
                            severity,
                            Kind::Scene,
                            Location::node(rel_file, node_id),
                            message,
                            why,
                            fix,
                        );
                    }
                }
                let (message, why, fix) = parse_scene_diagnostic(after_path);
                Self::new(
                    severity,
                    Kind::Scene,
                    Location::file(rel_file),
                    message,
                    why,
                    fix,
                )
            } else {
                Self::new(
                    severity,
                    Kind::Scene,
                    Location::default(),
                    Spans::text(raw),
                    Spans::text(GENERIC_WHY),
                    Spans::text(GENERIC_FIX),
                )
            }
        } else if let Some(manifest_rest) = raw.strip_prefix("manifest: ") {
            let (message, why, fix) = parse_manifest_diagnostic(manifest_rest);
            Self::new(
                severity,
                Kind::Manifest,
                Location::file(pite_project::MANIFEST_FILE),
                message,
                why,
                fix,
            )
        } else if let Some(atlas_rest) = raw.strip_prefix("atlas ") {
            if let Some((atlas_str, rest)) = atlas_rest.split_once(": ") {
                let file = format_path_rel(root, atlas_str);
                let (message, why, fix) = parse_atlas_diagnostic(rest);
                Self::new(
                    severity,
                    Kind::Atlas,
                    Location::file(file),
                    message,
                    why,
                    fix,
                )
            } else {
                Self::new(
                    severity,
                    Kind::Atlas,
                    Location::default(),
                    Spans::text(raw),
                    Spans::text(GENERIC_WHY),
                    Spans::text(GENERIC_FIX),
                )
            }
        } else if raw.starts_with("asset ")
            && raw.ends_with(" is never referenced by a reachable scene")
        {
            let inner = raw
                .strip_prefix("asset ")
                .and_then(|s| s.strip_suffix(" is never referenced by a reachable scene"))
                .unwrap_or(raw);
            Self::orphan_asset(&format_path_rel(root, inner))
        } else if let Some(file_str) = raw.strip_prefix("failed to reimport asset ") {
            Self::reimport_failed(&format_path_rel(root, file_str))
        } else if raw.starts_with("no ") && raw.contains("found above") {
            let start = raw
                .split("found above ")
                .nth(1)
                .unwrap_or("the current directory");
            Self::missing_project_root(Path::new(start))
        } else {
            Self::new(
                severity,
                Kind::Project,
                Location::default(),
                Spans::text(raw),
                Spans::text(GENERIC_WHY),
                Spans::text(GENERIC_FIX),
            )
        }
    }
}

/// Project-relative, `/`-separated path for display and JSON.
pub fn format_path_rel(root: &Path, raw_path: &str) -> String {
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

fn quoted_values(s: &str) -> Vec<&str> {
    let mut quotes = Vec::new();
    let mut cur = s;
    while let Some(start) = cur.find('"') {
        let after_start = &cur[start + 1..];
        if let Some(end) = after_start.find('"') {
            quotes.push(&after_start[..end]);
            cur = &after_start[end + 1..];
        } else {
            break;
        }
    }
    quotes
}

fn parse_node_diagnostic(rest: &str) -> (Spans, Spans, Spans) {
    if rest.starts_with("has unregistered type") {
        let ty = extract_quoted_word(rest).unwrap_or_else(|| "unknown".to_string());
        (
            Spans::new().plain("Unknown node type ").info(ty).plain("."),
            Spans::text("The engine can only instantiate types that are in its node registry."),
            Spans::new()
                .plain("Use a registered type such as ")
                .info("Sprite2D")
                .plain(", ")
                .info("Node2D")
                .plain(", or ")
                .info("CharacterBody2D")
                .plain("."),
        )
    } else if rest.starts_with("has unknown parent") {
        let parent = extract_quoted_word(rest).unwrap_or_else(|| "unknown".to_string());
        (
            Spans::new()
                .plain("Parent node ")
                .info(parent)
                .plain(" does not exist."),
            Spans::new()
                .plain("Every ")
                .info("parent")
                .plain(" must name a node in the same scene."),
            Spans::new()
                .plain("Point ")
                .info("parent")
                .plain(" at an existing node id, or remove it to attach to the root."),
        )
    } else if rest.contains("script") && rest.contains("missing, using placeholder") {
        let script = extract_quoted_word(rest).unwrap_or_else(|| "script".to_string());
        (
            Spans::new()
                .plain("Script ")
                .strong(script)
                .plain(" does not exist."),
            Spans::text("The node falls back to an inert placeholder at runtime."),
            Spans::new()
                .plain("Create the file at that ")
                .info("res://")
                .plain(" path, or remove the ")
                .info("script")
                .plain(" binding."),
        )
    } else if rest.contains("is reserved for the signal system") {
        let class = extract_quoted_word(rest).unwrap_or_else(|| "class".to_string());
        (
            Spans::new()
                .plain("Script class ")
                .info(class)
                .plain(" is reserved by the engine."),
            Spans::text("The name collides with an engine signal dispatcher."),
            Spans::text("Rename the class in the script and in its scene binding."),
        )
    } else if rest.contains("imports third-party") {
        let module = extract_quoted_word(rest).unwrap_or_else(|| "module".to_string());
        (
            Spans::new()
                .plain("Third-party module ")
                .info(module.clone())
                .plain(" is not vendored."),
            Spans::new()
                .plain("Exports drop host ")
                .info("site-packages")
                .plain(", so the import fails at runtime."),
            Spans::new()
                .plain("Run ")
                .strong(format!("pite vendor add {module}"))
                .plain(" to vendor it into ")
                .info("vendor/")
                .plain("."),
        )
    } else if rest.contains("plays missing audio") {
        let audio = extract_quoted_word(rest).unwrap_or_else(|| "audio".to_string());
        (
            Spans::new()
                .plain("Audio asset ")
                .strong(audio)
                .plain(" is missing."),
            Spans::new()
                .plain("A ")
                .info("pite.play()")
                .plain(" call references a file that does not exist."),
            Spans::new()
                .plain("Add the file at that ")
                .info("res://")
                .plain(" path, or update the ")
                .info("play()")
                .plain(" call."),
        )
    } else if rest.contains("undecodable:") {
        let audio = extract_quoted_word(rest).unwrap_or_else(|| "audio".to_string());
        let err = rest
            .split_once("undecodable: ")
            .map(|(_, e)| e)
            .unwrap_or("format error");
        (
            Spans::new()
                .plain("Audio asset ")
                .strong(audio)
                .plain(" cannot be decoded."),
            Spans::new()
                .faded(err)
                .plain(" Supported formats are WAV, OGG, MP3, and FLAC."),
            Spans::text("Re-encode or replace the file with a valid WAV, OGG, MP3, or FLAC."),
        )
    } else if rest.contains("references moved asset") {
        let quotes = quoted_values(rest);
        let (old, new) = match quotes.as_slice() {
            [old, new, ..] => (*old, *new),
            [old] => (*old, "new location"),
            _ => ("asset", "new location"),
        };
        (
            Spans::new()
                .plain("Asset moved: ")
                .strong(old)
                .plain(" → ")
                .strong(new)
                .plain("."),
            Spans::text("The file was renamed or moved outside the editor."),
            Spans::new()
                .plain("Update the reference to ")
                .strong(new)
                .plain(", or run ")
                .info("pite check --reimport-assets")
                .plain(" to rewrite it."),
        )
    } else if rest.contains("references missing asset") {
        let asset = extract_quoted_word(rest).unwrap_or_else(|| "asset".to_string());
        (
            Spans::new()
                .plain("Asset ")
                .strong(asset)
                .plain(" does not exist."),
            Spans::text("The node points at a resource file that is not on disk."),
            Spans::new()
                .plain("Add the file at that ")
                .info("res://")
                .plain(" path, or update the property."),
        )
    } else if rest.contains("custom prop") && rest.contains("expected") {
        (
            Spans::text("Custom property has the wrong type."),
            Spans::new()
                .plain("The scene value does not match the type declared under ")
                .info("[props]")
                .plain(" in ")
                .info("pite.toml")
                .plain(": ")
                .faded(rest),
            Spans::new()
                .plain("Change the value to match, or edit the declared type under ")
                .info("[props]")
                .plain("."),
        )
    } else if rest.contains("has unknown prop") {
        (
            Spans::text("Unknown property."),
            Spans::new()
                .plain("It is not a schema property and is not declared under ")
                .info("[props]")
                .plain(" in ")
                .info("pite.toml")
                .plain("."),
            Spans::new()
                .plain("Declare it under ")
                .info("[props]")
                .plain(" to silence this, or remove it from the scene."),
        )
    } else if rest.contains("texture and atlas are mutually exclusive") {
        (
            Spans::new()
                .plain("Sprite2D sets both ")
                .info("texture")
                .plain(" and ")
                .info("atlas")
                .plain("."),
            Spans::text("A sprite draws either a standalone image or an atlas frame, never both."),
            Spans::new()
                .plain("Remove either ")
                .info("texture")
                .plain(" or ")
                .info("atlas")
                .plain(" from the node."),
        )
    } else if rest.contains("frame requires an atlas and atlas requires a frame") {
        (
            Spans::text("Incomplete atlas setup on Sprite2D."),
            Spans::new()
                .plain("When using an atlas, ")
                .info("atlas")
                .plain(" and ")
                .info("frame")
                .plain(" must be set together."),
            Spans::new()
                .plain("Set both ")
                .info("atlas")
                .plain(" and ")
                .info("frame")
                .plain(", or use ")
                .info("texture")
                .plain(" instead."),
        )
    } else if rest.contains("missing required 'atlas' property") {
        (
            Spans::new()
                .plain("AnimatedSprite2D is missing its ")
                .info("atlas")
                .plain("."),
            Spans::text("AnimatedSprite2D reads its animation frames from an atlas sidecar."),
            Spans::new()
                .plain("Set ")
                .info("atlas = \"res://…\"")
                .plain(" on the node."),
        )
    } else if rest.contains("not found in atlas") {
        (
            Spans::text("Atlas reference is invalid."),
            Spans::new()
                .plain("The frame or animation is not defined in the sidecar: ")
                .faded(rest),
            Spans::text("Define it in the atlas sidecar, or update the node property."),
        )
    } else if rest.contains("invalid tilemap") {
        (
            Spans::text("Tilemap is invalid."),
            Spans::new()
                .faded(rest)
                .plain(" The ")
                .info(".ptmap")
                .plain(" sidecar could not be loaded."),
            Spans::text("Fix the tilemap file and the atlas it references."),
        )
    } else {
        (
            Spans::text(rest.trim_end_matches('.')),
            Spans::text("The node property or reference does not meet engine requirements."),
            Spans::text("Inspect the node in the scene and correct the configuration."),
        )
    }
}

fn parse_scene_diagnostic(after_path: &str) -> (Spans, Spans, Spans) {
    if after_path.contains("root") && after_path.contains("not found in node list") {
        (
            Spans::text("Root node is missing from the node list."),
            Spans::new()
                .plain("The scene header declares a root id that no node uses: ")
                .faded(after_path),
            Spans::new()
                .plain("Add the node, or point ")
                .info("root")
                .plain(" at an existing node id."),
        )
    } else if after_path.contains("instance of") && after_path.contains("cannot load") {
        (
            Spans::text("Child scene cannot load."),
            Spans::new()
                .plain("The referenced scene file is missing or invalid: ")
                .faded(after_path),
            Spans::text("Make sure the child scene exists and loads cleanly."),
        )
    } else if after_path.contains("instance of")
        && after_path.contains("attaches to unknown parent")
    {
        (
            Spans::text("Instance attaches to an unknown parent."),
            Spans::new()
                .plain("The ")
                .info("parent")
                .plain(" id does not exist in this scene: ")
                .faded(after_path),
            Spans::new()
                .plain("Attach it to an existing node id, or drop ")
                .info("parent")
                .plain(" to attach at the root."),
        )
    } else if after_path.contains("instance of") && after_path.contains("bad override target") {
        (
            Spans::text("Instance override targets a missing node."),
            Spans::new()
                .plain("Override keys must be ")
                .info("node_id.prop")
                .plain(" for a node in the child scene: ")
                .faded(after_path),
            Spans::text("Point the override at a node in the child scene, or remove it."),
        )
    } else if after_path.contains("references moved asset") {
        let quotes = quoted_values(after_path);
        let (old, new) = match quotes.as_slice() {
            [old, new, ..] => (*old, *new),
            [old] => (*old, "new location"),
            _ => ("asset", "new location"),
        };
        (
            Spans::new()
                .plain("Asset moved: ")
                .strong(old)
                .plain(" → ")
                .strong(new)
                .plain("."),
            Spans::text("The file was renamed or moved outside the editor."),
            Spans::new()
                .plain("Update the reference to ")
                .strong(new)
                .plain(", or run ")
                .info("pite check --reimport-assets")
                .plain(" to rewrite it."),
        )
    } else {
        (
            Spans::text(format!(
                "Scene error: {}.",
                after_path.trim_end_matches('.')
            )),
            Spans::text("The scene file could not be parsed or contains syntax violations."),
            Spans::new()
                .plain("Check the scene TOML, or delete ")
                .info("_cache/")
                .plain(" to force a rebuild."),
        )
    }
}

fn parse_manifest_diagnostic(msg: &str) -> (Spans, Spans, Spans) {
    if msg.contains("project.python must not be empty") {
        (
            Spans::new().info("project.python").plain(" is empty."),
            Spans::text("The engine needs a Python version to select the right runtime."),
            Spans::new()
                .plain("Set ")
                .info("python = \"3.12\"")
                .plain(" under ")
                .info("[project]")
                .plain(" in ")
                .info("pite.toml")
                .plain("."),
        )
    } else if msg.contains("dimensions must be positive") {
        (
            Spans::text("Window dimensions are invalid."),
            Spans::new()
                .plain("The window cannot open with zero or negative size: ")
                .faded(msg),
            Spans::new()
                .plain("Set positive integers for ")
                .info("window_width")
                .plain(" and ")
                .info("window_height")
                .plain("."),
        )
    } else if msg.contains("icon") && msg.contains("missing") {
        let icon = extract_quoted_word(msg).unwrap_or_else(|| "icon".to_string());
        (
            Spans::new()
                .plain("Project icon ")
                .strong(icon)
                .plain(" is missing."),
            Spans::new()
                .plain("The icon named by ")
                .info("project.icon")
                .plain(" in ")
                .info("pite.toml")
                .plain(" is not on disk."),
            Spans::new()
                .plain("Add the icon at that path, or point ")
                .info("project.icon")
                .plain(" at another image."),
        )
    } else if msg.contains("icon") && msg.contains("must be res:// or empty") {
        (
            Spans::new()
                .plain("Project icon must be a ")
                .info("res://")
                .plain(" path or empty."),
            Spans::text("Absolute paths are forbidden; asset references use res:// URIs."),
            Spans::new()
                .plain("Change it to a ")
                .info("res://")
                .plain(" URI (for example ")
                .strong("res://assets/icon.png")
                .plain("), or leave it empty."),
        )
    } else if msg.contains("[props]") && msg.contains("unknown type") {
        (
            Spans::text("Custom property type is not supported."),
            Spans::new()
                .plain("The declared type is not one the engine knows: ")
                .faded(msg),
            Spans::new()
                .plain("Use one of ")
                .info("string")
                .plain(", ")
                .info("int")
                .plain(", ")
                .info("number")
                .plain(", ")
                .info("bool")
                .plain(", ")
                .info("vec2")
                .plain(", or ")
                .info("color")
                .plain("."),
        )
    } else if msg.contains("[props]") && msg.contains("collides with a schema prop") {
        (
            Spans::text("Custom property name collides with a schema property."),
            Spans::new()
                .plain("Schema property names are reserved by node types: ")
                .faded(msg),
            Spans::new()
                .plain("Pick a different name under ")
                .info("[props]")
                .plain(" in ")
                .info("pite.toml")
                .plain("."),
        )
    } else if msg.contains("unknown field") {
        (
            Spans::text("Unknown manifest field."),
            Spans::new()
                .plain("The field is not part of the project manifest schema: ")
                .faded(msg),
            Spans::new()
                .plain("Remove or correct the field in ")
                .info("pite.toml")
                .plain("."),
        )
    } else {
        (
            Spans::text(format!("Manifest error: {}.", msg.trim_end_matches('.'))),
            Spans::text("The manifest does not meet engine requirements."),
            Spans::new()
                .plain("Edit ")
                .info("pite.toml")
                .plain(" to correct the configuration."),
        )
    }
}

fn parse_atlas_diagnostic(msg: &str) -> (Spans, Spans, Spans) {
    if msg.contains("frame") && msg.contains("is never referenced") {
        let frame = extract_quoted_word(msg).unwrap_or_else(|| "frame".to_string());
        (
            Spans::new()
                .plain("Unused atlas frame ")
                .info(frame)
                .plain("."),
            Spans::text(
                "The frame is defined in the atlas but no scene, tilemap, or animation uses it.",
            ),
            Spans::text(
                "Reference it from a sprite or tilemap, or remove it from the atlas sidecar.",
            ),
        )
    } else if msg.contains("animation") && msg.contains("is never referenced") {
        let anim = extract_quoted_word(msg).unwrap_or_else(|| "animation".to_string());
        (
            Spans::new()
                .plain("Unused atlas animation ")
                .info(anim)
                .plain("."),
            Spans::text("The animation is defined in the sidecar but no node plays it."),
            Spans::text(
                "Reference it from an AnimatedSprite2D, or remove it from the atlas sidecar.",
            ),
        )
    } else {
        (
            Spans::text(format!("Atlas error: {}.", msg.trim_end_matches('.'))),
            Spans::text("The atlas sidecar does not meet engine requirements."),
            Spans::new()
                .plain("Inspect the ")
                .info(".ptatlas")
                .plain(" sidecar and fix the inconsistency."),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::COLOR_ENABLED;

    #[test]
    fn raw_manifest_message_is_structured() {
        let root = Path::new("/proj");
        let finding = Finding::from_raw(
            root,
            Severity::Error,
            "manifest: project.python must not be empty",
        );
        assert_eq!(finding.kind, Kind::Manifest);
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.location.file.as_deref(), Some("pite.toml"));
        assert!(finding.message.to_string().contains("project.python"));
    }

    #[test]
    fn raw_scene_message_keeps_node_location() {
        let root = Path::new("/proj");
        let finding = Finding::from_raw(
            root,
            Severity::Error,
            "scene /proj/scenes/main.ptscn: node \"sprite\" has unknown parent \"ghost\"",
        );
        assert_eq!(finding.kind, Kind::Scene);
        assert_eq!(finding.location.file.as_deref(), Some("scenes/main.ptscn"));
        assert_eq!(finding.location.node.as_deref(), Some("sprite"));
        assert!(finding.message.to_string().contains("ghost"));
    }

    #[test]
    fn moved_asset_fix_mentions_reimport() {
        let root = Path::new("/proj");
        let finding = Finding::from_raw(
            root,
            Severity::Error,
            "scene /proj/scenes/main.ptscn: node \"sprite\" references moved asset \"res://a.png\" to \"res://b.png\"",
        );
        assert!(finding.fix.to_string().contains("--reimport-assets"));
        assert!(finding.fixed_by_reimport());
        assert!(finding
            .fix
            .render(&COLOR_ENABLED)
            .contains("\u{1b}[1;36mpite check --reimport-assets\u{1b}[0m"));
    }

    #[test]
    fn missing_asset_is_not_fixed_by_reimport() {
        let root = Path::new("/proj");
        let finding = Finding::from_raw(
            root,
            Severity::Error,
            "scene /proj/scenes/main.ptscn: node \"sprite\" references missing asset \"res://a.png\"",
        );
        assert!(!finding.fixed_by_reimport());
    }

    #[test]
    fn reimport_failure_is_typed() {
        let root = Path::new("/proj");
        let finding = Finding::from_raw(
            root,
            Severity::Error,
            "failed to reimport asset /proj/assets/x.png",
        );
        assert_eq!(finding.kind, Kind::Reimport);
        assert_eq!(finding.location.file.as_deref(), Some("assets/x.png"));
    }

    #[test]
    fn typed_missing_asset_has_node_location() {
        let finding = Finding::missing_asset("scenes/main.ptscn", "sprite", "res://a.png");
        assert_eq!(finding.kind, Kind::Asset);
        assert_eq!(
            finding.location.render().as_deref(),
            Some("scenes/main.ptscn › node \"sprite\"")
        );
    }

    #[test]
    fn json_is_plain_sentences_without_escapes() {
        let finding = Finding::missing_project_root(Path::new("C:/work"));
        let json = serde_json::to_string(&finding).expect("serialize");
        assert!(
            !json.contains("\\u{1b}"),
            "escapes leaked into JSON: {json}"
        );
        assert!(
            !json.contains('<'),
            "styling syntax leaked into JSON: {json}"
        );
        assert!(json.contains("pite.toml"));
        assert!(json.contains("--path"));
    }

    #[test]
    fn paths_and_identifiers_are_curated_spans() {
        let finding = Finding::missing_asset("scenes/main.ptscn", "sprite", "res://a.png");
        assert!(
            finding
                .message
                .render(&crate::ui::COLOR_ENABLED)
                .contains("\u{1b}[1mres://a.png\u{1b}[0m"),
            "paths must be bold"
        );
        let finding = Finding::unknown_prop("scenes/main.ptscn", "sprite", "texture");
        assert!(
            finding
                .message
                .render(&crate::ui::COLOR_ENABLED)
                .contains("\u{1b}[1;36mtexture\u{1b}[0m"),
            "identifiers must be cyan"
        );
    }
}
