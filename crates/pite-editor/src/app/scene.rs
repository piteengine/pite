// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::{Path, PathBuf};

use eframe::egui;
use egui_phosphor::regular;
use pite_runtime::{GameSession, SessionOptions};

use crate::ops;

use super::{EditorApp, Tab};

impl EditorApp {
    pub(super) fn save_all(&mut self) {
        self.save_scene();
        if self.code_file.is_some() {
            self.save_code();
        }
        if self.atlas_editor.as_ref().is_some_and(|ed| ed.dirty) {
            self.save_atlas_editor();
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
        let schemas = self.session.schemas().clone();
        let built = self.session.host().with_tree(|tree| {
            ops::build_doc_with_schemas(tree, source.as_ref(), Some(refs), &schemas)
        });
        match built.and_then(|doc| Ok(pite_scene::save_scene(&doc)?)) {
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
        pite_project::config_dir().map(|dir| dir.join("recent.json"))
    }

    pub(super) fn is_trusted_in_store(store: Option<&Path>, workspace: &Path) -> bool {
        store.is_some_and(|path| pite_project::is_workspace_trusted_in(path, workspace))
    }

    pub(super) fn trust_current_workspace(&mut self) -> bool {
        if let Some(store) = self.trust_store.as_deref() {
            if let Err(err) = pite_project::trust_workspace_in(store, &self.workspace_root) {
                self.log(format!("cannot save workspace trust: {err:#}"));
                return false;
            }
        }
        if let Err(err) = self.session.set_safe_mode(false) {
            self.log(format!("cannot enable scripts: {err:#}"));
            return false;
        }
        self.workspace_trusted = true;
        self.trust_modal_open = false;
        self.viewport_dirty = true;
        self.log(format!(
            "trusted workspace {}; scripts enabled.",
            self.workspace_root.display()
        ));
        true
    }

    pub(super) fn show_safe_mode_banner(&mut self, ui: &mut egui::Ui) {
        if self.workspace_trusted {
            return;
        }
        egui::Panel::top("safe_mode_banner")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(18, 77, 73))
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(regular::SHIELD)
                            .color(crate::theme::TEXT)
                            .size(15.0),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Safe Mode — untrusted workspace. Python scripts are disabled until you trust this folder.",
                        )
                        .color(crate::theme::TEXT)
                        .size(13.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Trust Workspace").clicked() {
                            self.trust_current_workspace();
                        }
                    });
                });
            });
    }

    pub(super) fn show_trust_modal(&mut self, ctx: &egui::Context) {
        if !self.trust_modal_open {
            return;
        }
        let mut close = false;
        let mut trust_and_play = false;

        let res = egui::Window::new("Trust Workspace to Run Scripts?")
            .id(egui::Id::unique("workspace_trust_modal"))
            .default_width(440.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    "Running this scene executes Python scripts in the workspace with your user permissions.",
                );
                ui.add_space(4.0);
                ui.monospace(self.workspace_root.display().to_string());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Trust & Play").clicked() {
                        trust_and_play = true;
                    }
                    if ui.button("Stay in Safe Mode").clicked() {
                        close = true;
                    }
                });
            });

        if res.is_none() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }

        if trust_and_play {
            if self.trust_current_workspace() {
                self.toggle_play();
            }
        } else if close {
            self.trust_modal_open = false;
        }
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
        let project_dir = pite_project::find_project_root(file);
        let workspace_root = pite_project::workspace_root(file, project_dir.as_deref());
        let workspace_trusted =
            Self::is_trusted_in_store(self.trust_store.as_deref(), &workspace_root);
        match GameSession::open_with_options(
            file,
            SessionOptions {
                no_reload: false,
                safe_mode: !workspace_trusted,
            },
        ) {
            Ok(mut session) => {
                session.set_pause_on_error(true);
                self.session = session;
                self.scene_path = file.to_path_buf();
                self.project_dir = project_dir;
                self.workspace_root = workspace_root;
                self.workspace_trusted = workspace_trusted;
                self.trust_modal_open = false;
                self.playing = false;
                self.custom_props = Self::load_custom_props(self.project_dir.as_deref());
                self.view_size = Self::view_size_for(self.project_dir.as_deref());
                self.selected = None;
                self.viewport_dirty = true;
                self.tab = Tab::Viewport;
                self.push_recent(file);
                if workspace_trusted {
                    self.log(format!("opened {}", file.display()));
                } else {
                    self.log(format!(
                        "opened {} (Safe Mode: untrusted workspace)",
                        file.display()
                    ));
                }
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
