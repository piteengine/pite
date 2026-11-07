// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::PathBuf;

use eframe::egui;

use super::EditorApp;

/// Unsaved Project Settings edits. `None` while the window is closed; `base`
/// carries the unknown keys so saving only swaps the known fields.
pub(crate) struct SettingsDraft {
    pub(crate) base: pite_project::PiteManifest,
    pub(crate) name: String,
    pub(crate) author: String,
    pub(crate) main_scene: String,
    pub(crate) python: String,
    pub(crate) icon: String,
    pub(crate) width: String,
    pub(crate) height: String,
    pub(crate) binary_name: String,
    pub(crate) include: String,
    pub(crate) error: String,
}

impl EditorApp {
    pub(super) fn open_settings(&mut self) {
        let dir = match self.project_dir.clone() {
            Some(dir) => dir,
            None => {
                self.log("project settings: no project open.");
                return;
            }
        };
        match pite_project::load_manifest(&dir) {
            Ok(base) => {
                let binary_name = base.export.binary_name.clone().unwrap_or_default();
                let include = base.export.include.join("\n");
                let project = &base.project;
                self.settings = Some(SettingsDraft {
                    name: project.name.clone(),
                    author: project.author.clone(),
                    main_scene: project.main_scene.clone(),
                    python: project.python.clone(),
                    icon: project.icon.clone(),
                    width: project.window_width.to_string(),
                    height: project.window_height.to_string(),
                    binary_name,
                    include,
                    error: String::new(),
                    base,
                });
            }
            Err(e) => self.log(format!("project settings: cannot load manifest: {e:#}")),
        }
    }

    pub(super) fn save_settings(&mut self) {
        let (manifest, dir) = {
            let draft = match self.settings.as_mut() {
                Some(draft) => draft,
                None => return,
            };
            let width = match draft.width.trim().parse::<u32>() {
                Ok(width) => width,
                Err(_) => {
                    draft.error = format!("window width {:?} is not a number", draft.width);
                    return;
                }
            };
            let height = match draft.height.trim().parse::<u32>() {
                Ok(height) => height,
                Err(_) => {
                    draft.error = format!("window height {:?} is not a number", draft.height);
                    return;
                }
            };
            let mut manifest = draft.base.clone();
            let project = &mut manifest.project;
            project.name = draft.name.clone();
            project.author = draft.author.clone();
            project.main_scene = draft.main_scene.clone();
            project.python = draft.python.clone();
            project.icon = draft.icon.clone();
            project.window_width = width;
            project.window_height = height;
            let binary_name = draft.binary_name.trim().to_string();
            manifest.export.binary_name = if binary_name.is_empty() {
                None
            } else {
                Some(binary_name)
            };
            manifest.export.include = draft
                .include
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect();
            let (errors, _) = pite_project::validate_manifest(&manifest);
            if !errors.is_empty() {
                draft.error = errors.join("\n");
                return;
            }
            (
                manifest,
                self.project_dir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(".")),
            )
        };
        match pite_project::save_manifest(&dir, &manifest) {
            Ok(()) => {
                self.view_size = (
                    manifest.project.window_width,
                    manifest.project.window_height,
                );
                self.custom_props = manifest.props.clone();
                self.viewport_dirty = true;
                self.settings = None;
                self.log("saved project settings.");
            }
            Err(e) => match self.settings.as_mut() {
                Some(draft) => draft.error = format!("{e:#}"),
                None => self.log(format!("save project settings failed: {e:#}")),
            },
        }
    }

    pub(super) fn show_settings(&mut self, ctx: &egui::Context) {
        let mut open = self.settings.is_some();
        egui::Window::new("Project Settings")
            .open(&mut open)
            .show(ctx, |ui| {
                let Some(draft) = self.settings.as_mut() else {
                    return;
                };
                ui.label(format!(
                    "engine version: {}",
                    draft.base.project.pite_version
                ));
                ui.separator();
                egui::Grid::new("settings-grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut draft.name);
                        ui.end_row();
                        ui.label("Author");
                        ui.text_edit_singleline(&mut draft.author);
                        ui.end_row();
                        ui.label("Main scene");
                        ui.text_edit_singleline(&mut draft.main_scene);
                        ui.end_row();
                        ui.label("Python");
                        ui.text_edit_singleline(&mut draft.python);
                        ui.end_row();
                        ui.label("Icon");
                        ui.text_edit_singleline(&mut draft.icon);
                        ui.end_row();
                        ui.label("Window width");
                        ui.text_edit_singleline(&mut draft.width);
                        ui.end_row();
                        ui.label("Window height");
                        ui.text_edit_singleline(&mut draft.height);
                        ui.end_row();
                    });
                ui.separator();
                ui.heading("Export");
                egui::Grid::new("settings-export-grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Binary name");
                        ui.text_edit_singleline(&mut draft.binary_name);
                        ui.end_row();
                    });
                ui.label("Extra staged files, one per line:");
                ui.text_edit_multiline(&mut draft.include);
                if !draft.error.is_empty() {
                    ui.label(format!("error: {}", draft.error));
                }
                if ui.button("Save").clicked() {
                    self.save_settings();
                }
            });
        if !open {
            self.settings = None;
        }
    }
}
