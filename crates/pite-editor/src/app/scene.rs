// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::{Path, PathBuf};

use pite_runtime::GameSession;

use crate::ops;

use super::{EditorApp, Tab};

impl EditorApp {
    pub(super) fn save_all(&mut self) {
        self.save_scene();
        if self.code_file.is_some() {
            self.save_code();
        }
    }

    pub(super) fn save_scene(&mut self) {
        let source = match std::fs::metadata(&self.scene_path) {
            Ok(_) => match pite_scene::load_scene(&self.scene_path) {
                Ok(doc) => Some(doc),
                Err(e) => {
                    self.log(format!(
                        "save aborted: cannot read existing scene {}: {e:#}",
                        self.scene_path.display()
                    ));
                    return;
                }
            },
            Err(_) => None,
        };
        let scene_dir = self
            .scene_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let refs = ops::InstanceRefs {
            scene_dir: &scene_dir,
            project_dir: self.project_dir.as_deref(),
        };
        let built = self
            .session
            .host()
            .with_tree(|tree| ops::build_doc(tree, source.as_ref(), Some(refs)));
        match built.and_then(|doc| pite_scene::save_scene(&doc).map_err(anyhow::Error::from)) {
            Ok(text) => match ops::save_text(&self.scene_path, &text) {
                Ok(()) => {
                    self.viewport_dirty = true;
                    self.log(format!("saved scene {}", self.scene_path.display()));
                }
                Err(e) => self.log(format!("save scene failed: {e:#}")),
            },
            Err(e) => self.log(format!("save scene failed: {e:#}")),
        }
    }

    pub(super) fn view_size_for(project_dir: Option<&Path>) -> (u32, u32) {
        project_dir
            .and_then(|dir| pite_project::load_manifest(dir).ok())
            .map_or_else(
                || {
                    let d = pite_project::ProjectMeta::default();
                    (d.window_width, d.window_height)
                },
                |m| (m.project.window_width, m.project.window_height),
            )
    }

    pub(super) fn load_custom_props(
        project_dir: Option<&Path>,
    ) -> std::collections::HashMap<String, String> {
        project_dir
            .and_then(|d| pite_project::load_manifest(d).ok())
            .map(|m| m.props)
            .unwrap_or_default()
    }

    pub(super) fn recent_file() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("pite").join("recent.json"))
    }

    pub(super) fn read_recent_file(path: &Path) -> Vec<PathBuf> {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let files: Vec<String> = serde_json::from_str(&text).unwrap_or_default();
        files
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .collect()
    }

    pub(super) fn write_recent_file(path: &Path, recent: &[PathBuf]) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let files: Vec<String> = recent
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        std::fs::write(path, serde_json::to_string_pretty(&files)?)?;
        Ok(())
    }

    pub(super) fn load_recent() -> Vec<PathBuf> {
        Self::recent_file()
            .map(|p| Self::read_recent_file(&p))
            .unwrap_or_default()
    }

    pub(super) fn push_recent(&mut self, file: &Path) {
        self.recent.retain(|p| p != file);
        self.recent.insert(0, file.to_path_buf());
        self.recent.truncate(10);
        if let Some(path) = Self::recent_file() {
            if let Err(e) = Self::write_recent_file(&path, &self.recent) {
                self.log(format!("recent list not saved: {e:#}"));
            }
        }
    }

    /// Root-only scene: `New Scene` starts blank, never from the example scaffold.
    pub(super) fn blank_scene_text() -> String {
        format!(
            "format_version = {}\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n",
            pite_scene::FORMAT_VERSION
        )
    }

    pub(super) fn open_scene_file(&mut self, file: &Path) {
        match GameSession::open(file, false) {
            Ok(mut session) => {
                session.set_pause_on_error(true);
                self.session = session;
                self.scene_path = file.to_path_buf();
                self.project_dir = pite_project::find_project_root(file);
                self.custom_props = Self::load_custom_props(self.project_dir.as_deref());
                self.view_size = Self::view_size_for(self.project_dir.as_deref());
                self.selected = None;
                self.viewport_dirty = true;
                self.tab = Tab::Viewport;
                self.push_recent(file);
                self.log(format!("opened {}", file.display()));
            }
            Err(e) => self.log(format!("open failed: {e:#}")),
        }
    }

    /// Native dialogs return the path as-is: enforce `.ptscn` so a bare
    /// name cannot scatter extensionless scenes through the project.
    pub(super) fn with_scene_extension(mut path: PathBuf) -> PathBuf {
        if path.extension().and_then(|e| e.to_str()) != Some("ptscn") {
            path.set_extension("ptscn");
        }
        path
    }

    pub(super) fn new_scene_via_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Pite scene", &["ptscn"])
            .set_file_name("main.ptscn")
            .save_file()
        else {
            return;
        };
        let path = Self::with_scene_extension(path);
        match std::fs::write(&path, Self::blank_scene_text()) {
            Ok(()) => self.open_scene_file(&path),
            Err(e) => self.log(format!("new scene failed: {e:#}")),
        }
    }

    pub(super) fn open_scene_via_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Pite scene", &["ptscn"])
            .pick_file()
        else {
            return;
        };
        self.open_scene_file(&path);
    }

    pub(super) fn save_scene_as_via_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Pite scene", &["ptscn"])
            .set_file_name(
                self.scene_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("main.ptscn"),
            )
            .save_file()
        else {
            return;
        };
        self.scene_path = Self::with_scene_extension(path);
        self.save_scene();
        let path = self.scene_path.clone();
        self.push_recent(&path);
    }
}
