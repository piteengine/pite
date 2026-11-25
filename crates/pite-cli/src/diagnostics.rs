// SPDX-License-Identifier: MIT OR Apache-2.0
//! Structured findings for `pite check`.
//!
//! Findings are built directly from typed diagnostics and domain enums
//! (`pite_project::ManifestIssue`, `pite_assets::AtlasIssue`, etc.).
//! Rendering and JSON both consume this one type, so the human card and
//! the machine payload can never drift apart.
//!
//! Each text field is a [`Spans`]: plain prose plus the pieces that deserve
//! emphasis (bold paths and commands, cyan identifiers, dim error detail).
//! JSON serializes the concatenated plain text, so styling never reaches
//! machine consumers.

use std::path::Path;

use pite_assets::{AssetError, AtlasIssue, SpriteSourceError};
use pite_project::ManifestIssue;
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

    // ---- Structured lower-crate constructors ---------------------------

    pub fn manifest_load_failed(error: &pite_project::ProjectError) -> Self {
        Self::error(
            Kind::Manifest,
            Location::file(pite_project::MANIFEST_FILE),
            Spans::text(format!("Manifest error: {error}.")),
            Spans::text("The project manifest could not be read or parsed."),
            Spans::new()
                .plain("Check ")
                .info(pite_project::MANIFEST_FILE)
                .plain(" syntax and permissions."),
        )
    }

    pub fn manifest_issue(severity: Severity, issue: &ManifestIssue) -> Self {
        let (message, why, fix) = match issue {
            ManifestIssue::EmptyPython => (
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
            ),
            ManifestIssue::InvalidWindowDimensions { width, height } => (
                Spans::text("Window dimensions are invalid."),
                Spans::new()
                    .plain("The window cannot open with zero or negative size: ")
                    .faded(format!("{width}x{height}"))
                    .plain("."),
                Spans::new()
                    .plain("Set positive integers for ")
                    .info("window_width")
                    .plain(" and ")
                    .info("window_height")
                    .plain("."),
            ),
            ManifestIssue::InvalidIconPath { icon } => (
                Spans::new()
                    .plain("Project icon ")
                    .strong(icon)
                    .plain(" must be a ")
                    .info("res://")
                    .plain(" path or empty."),
                Spans::text("Absolute paths are forbidden; asset references use res:// URIs."),
                Spans::new()
                    .plain("Change it to a ")
                    .info("res://")
                    .plain(" URI (for example ")
                    .strong("res://assets/icon.png")
                    .plain("), or leave it empty."),
            ),
            ManifestIssue::UnknownCustomPropType { name, ty } => (
                Spans::new()
                    .plain("Custom property ")
                    .info(name)
                    .plain(" has unsupported type ")
                    .info(ty)
                    .plain("."),
                Spans::text("The declared type is not one the engine knows."),
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
            ),
            ManifestIssue::CollidingCustomProp { name } => (
                Spans::new()
                    .plain("Custom property ")
                    .info(name)
                    .plain(" collides with a schema property."),
                Spans::text("Schema property names are reserved by node types."),
                Spans::new()
                    .plain("Pick a different name under ")
                    .info("[props]")
                    .plain(" in ")
                    .info("pite.toml")
                    .plain("."),
            ),
            ManifestIssue::DeprecatedIgnored { field, removed_in } => (
                Spans::new()
                    .plain("Deprecated manifest field ")
                    .info(field)
                    .plain(" is ignored."),
                Spans::text(format!("This field was removed in {removed_in}.")),
                Spans::new()
                    .plain("Remove ")
                    .info(field)
                    .plain(" from ")
                    .info("pite.toml")
                    .plain("."),
            ),
            ManifestIssue::DeprecatedReplacement { field, replacement } => (
                Spans::new()
                    .plain("Deprecated manifest field ")
                    .info(field)
                    .plain("."),
                Spans::new()
                    .plain("Use ")
                    .info(*replacement)
                    .plain(" instead."),
                Spans::new()
                    .plain("Replace ")
                    .info(field)
                    .plain(" with ")
                    .info(*replacement)
                    .plain(" in ")
                    .info("pite.toml")
                    .plain("."),
            ),
            ManifestIssue::UnknownField { field } => (
                Spans::new()
                    .plain("Unknown manifest field ")
                    .info(field)
                    .plain("."),
                Spans::text("The field is not part of the project manifest schema."),
                Spans::new()
                    .plain("Remove or correct ")
                    .info(field)
                    .plain(" in ")
                    .info("pite.toml")
                    .plain("."),
            ),
        };
        Self::new(
            severity,
            Kind::Manifest,
            Location::file(pite_project::MANIFEST_FILE),
            message,
            why,
            fix,
        )
    }

    pub fn atlas_issue(root: &Path, severity: Severity, issue: &AtlasIssue) -> Self {
        match issue {
            AtlasIssue::InvalidFrame {
                scene,
                node,
                atlas,
                message,
            }
            | AtlasIssue::InvalidAnimation {
                scene,
                node,
                atlas,
                message,
            } => Self::new(
                severity,
                Kind::Atlas,
                Location::node(scene, node),
                Spans::new()
                    .plain("Atlas reference ")
                    .strong(atlas)
                    .plain(" is invalid."),
                Spans::new()
                    .plain("The frame or animation is not defined in the sidecar: ")
                    .faded(message),
                Spans::text("Define it in the atlas sidecar, or update the node property."),
            ),
            AtlasIssue::LoadFailed {
                scene,
                node,
                atlas,
                message,
            } => Self::new(
                severity,
                Kind::Atlas,
                Location::node(scene, node),
                Spans::new()
                    .plain("Atlas ")
                    .strong(atlas)
                    .plain(" cannot be loaded."),
                Spans::new().faded(message),
                Spans::new()
                    .plain("Inspect the ")
                    .info(".ptatlas")
                    .plain(" sidecar and ensure the file exists and is valid TOML."),
            ),
            AtlasIssue::UnusedFrame { atlas, frame } => Self::new(
                severity,
                Kind::Atlas,
                Location::file(format_path_rel(root, atlas)),
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
            ),
            AtlasIssue::UnusedAnimation { atlas, animation } => Self::new(
                severity,
                Kind::Atlas,
                Location::file(format_path_rel(root, atlas)),
                Spans::new()
                    .plain("Unused atlas animation ")
                    .info(animation)
                    .plain("."),
                Spans::text("The animation is defined in the sidecar but no node plays it."),
                Spans::text(
                    "Reference it from an AnimatedSprite2D, or remove it from the atlas sidecar.",
                ),
            ),
        }
    }

    pub fn scene_load_failed(scene: &str, error: &str) -> Self {
        Self::error(
            Kind::Scene,
            Location::file(scene),
            Spans::new()
                .plain("Scene ")
                .strong(scene)
                .plain(" cannot load."),
            Spans::new()
                .faded(error)
                .plain(" The scene file could not be read or parsed."),
            Spans::new()
                .plain("Check the scene TOML, or delete ")
                .info("_cache/")
                .plain(" to force a rebuild."),
        )
    }

    pub fn sprite_source_invalid(scene: &str, node: &str, error: &AssetError) -> Self {
        let (message, why, fix) = match error {
            AssetError::SpriteSource(SpriteSourceError::TextureAndAtlasConflict) => (
                Spans::new()
                    .plain("Sprite2D sets both ")
                    .info("texture")
                    .plain(" and ")
                    .info("atlas")
                    .plain("."),
                Spans::text(
                    "A sprite draws either a standalone image or an atlas frame, never both.",
                ),
                Spans::new()
                    .plain("Remove either ")
                    .info("texture")
                    .plain(" or ")
                    .info("atlas")
                    .plain(" from the node."),
            ),
            AssetError::SpriteSource(
                SpriteSourceError::AtlasRequiresFrame | SpriteSourceError::FrameRequiresAtlas,
            ) => (
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
            ),
            other => (
                Spans::text(other.to_string()),
                Spans::text("The Sprite2D asset configuration is invalid."),
                Spans::text("Inspect the node's texture, atlas, and frame properties."),
            ),
        };
        Self::error(Kind::Scene, Location::node(scene, node), message, why, fix)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::COLOR_ENABLED;

    #[test]
    fn manifest_issue_is_structured() {
        let finding = Finding::manifest_issue(Severity::Error, &ManifestIssue::EmptyPython);
        assert_eq!(finding.kind, Kind::Manifest);
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.location.file.as_deref(), Some("pite.toml"));
        assert!(finding.message.to_string().contains("project.python"));
    }

    #[test]
    fn unknown_parent_keeps_node_location() {
        let finding = Finding::unknown_parent("scenes/main.ptscn", "sprite", "ghost");
        assert_eq!(finding.kind, Kind::Scene);
        assert_eq!(finding.location.file.as_deref(), Some("scenes/main.ptscn"));
        assert_eq!(finding.location.node.as_deref(), Some("sprite"));
        assert!(finding.message.to_string().contains("ghost"));
    }

    #[test]
    fn moved_asset_fix_mentions_reimport() {
        let finding =
            Finding::moved_asset("scenes/main.ptscn", "sprite", "res://a.png", "res://b.png");
        assert!(finding.fix.to_string().contains("--reimport-assets"));
        assert!(finding.fixed_by_reimport());
        assert!(finding
            .fix
            .render(&COLOR_ENABLED)
            .contains("\u{1b}[1;36mpite check --reimport-assets\u{1b}[0m"));
    }

    #[test]
    fn missing_asset_is_not_fixed_by_reimport() {
        let finding = Finding::missing_asset("scenes/main.ptscn", "sprite", "res://a.png");
        assert!(!finding.fixed_by_reimport());
    }

    #[test]
    fn sprite_source_conflict_is_typed() {
        let err: AssetError = SpriteSourceError::TextureAndAtlasConflict.into();
        let finding = Finding::sprite_source_invalid("scenes/main.ptscn", "sprite", &err);
        assert_eq!(finding.kind, Kind::Scene);
        assert_eq!(finding.location.node.as_deref(), Some("sprite"));
        assert!(finding.message.to_string().contains("texture"));
        assert!(finding.message.to_string().contains("atlas"));
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
