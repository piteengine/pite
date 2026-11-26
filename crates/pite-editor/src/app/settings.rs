// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui;

use super::EditorApp;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum SettingsTab {
    #[default]
    General,
    InputMap,
}

/// Unsaved Project Settings edits. `None` while the window is closed; `base`
/// carries the unknown keys so saving only swaps the known fields.
pub(crate) struct SettingsDraft {
    pub(crate) tab: SettingsTab,
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
    pub(crate) input_map: pite_project::InputMapFile,
    pub(crate) input_dirty: bool,
    pub(crate) new_action_name: String,
    pub(crate) new_binding_draft: HashMap<String, String>,
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
                let input_map = pite_project::load_input_map(&dir).unwrap_or_default();
                let binary_name = base.export.binary_name.clone().unwrap_or_default();
                let include = base.export.include.join("\n");
                let project = &base.project;
                self.settings = Some(SettingsDraft {
                    tab: SettingsTab::General,
                    name: project.name.clone(),
                    author: project.author.clone(),
                    main_scene: project.main_scene.clone(),
                    python: project.python.clone(),
                    icon: project.icon.clone(),
                    width: project.window_width.to_string(),
                    height: project.window_height.to_string(),
                    binary_name,
                    include,
                    input_map,
                    input_dirty: false,
                    new_action_name: String::new(),
                    new_binding_draft: HashMap::new(),
                    error: String::new(),
                    base,
                });
            }
            Err(e) => self.log(format!("project settings: cannot load manifest: {e:#}")),
        }
    }

    pub(super) fn save_settings(&mut self) {
        let (manifest, input_map_to_save, dir) = {
            let draft = match self.settings.as_mut() {
                Some(draft) => draft,
                None => return,
            };
            let input_errs = pite_project::validate_input_map(&draft.input_map);
            if let Some(first) = input_errs.first() {
                draft.error = first.clone();
                return;
            }
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
            let to_save_input = if draft.input_dirty || !draft.input_map.actions.is_empty() {
                Some(draft.input_map.clone())
            } else {
                None
            };
            (
                manifest,
                to_save_input,
                self.project_dir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(".")),
            )
        };
        match pite_project::save_manifest(&dir, &manifest) {
            Ok(()) => {
                if let Some(input_map) = input_map_to_save {
                    if let Err(e) = pite_project::save_input_map(&dir, &input_map) {
                        if let Some(draft) = self.settings.as_mut() {
                            draft.error = format!("{e:#}");
                        }
                        return;
                    }
                    self.session.reload_input_map();
                }
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
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut draft.tab, SettingsTab::General, "General");
                    ui.selectable_value(&mut draft.tab, SettingsTab::InputMap, "Input Map");
                });
                ui.separator();
                match draft.tab {
                    SettingsTab::General => {
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
                    }
                    SettingsTab::InputMap => {
                        show_input_map_tab(ui, draft);
                    }
                }
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

fn show_input_map_tab(ui: &mut egui::Ui, draft: &mut SettingsDraft) {
    ui.horizontal(|ui| {
        ui.label("New action:");
        let resp = ui.text_edit_singleline(&mut draft.new_action_name);
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button("+ Add Action").clicked() || enter {
            let name = draft.new_action_name.trim().to_string();
            if name.is_empty() {
                draft.error = "action name cannot be empty".to_string();
            } else {
                draft.input_map.actions.entry(name).or_default();
                draft.new_action_name.clear();
                draft.input_dirty = true;
                draft.error.clear();
            }
        }
    });
    ui.label(
        egui::RichText::new("Chords use '+' (e.g. Space, Control+S, Shift+W, MouseLeft).")
            .small()
            .color(crate::theme::FAINT),
    );
    ui.separator();

    let mut remove_action: Option<String> = None;
    let action_names: Vec<String> = draft.input_map.actions.keys().cloned().collect();
    egui::ScrollArea::vertical()
        .max_height(260.0)
        .show(ui, |ui| {
            for action in &action_names {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.strong(action);
                        if ui.small_button("Remove Action").clicked() {
                            remove_action = Some(action.clone());
                        }
                    });
                    if let Some(bindings) = draft.input_map.actions.get_mut(action) {
                        let mut remove_idx: Option<usize> = None;
                        for (idx, b) in bindings.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.monospace(b);
                                if ui.small_button("×").clicked() {
                                    remove_idx = Some(idx);
                                }
                            });
                        }
                        if let Some(idx) = remove_idx {
                            bindings.remove(idx);
                            draft.input_dirty = true;
                        }
                        let entry = draft.new_binding_draft.entry(action.clone()).or_default();
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(entry)
                                    .hint_text("e.g. Space or Control+S")
                                    .desired_width(160.0),
                            );
                            if ui.small_button("+ Add Binding").clicked() {
                                let raw = entry.trim().to_string();
                                match pite_project::parse_chord(&raw) {
                                    Ok(canonical) => {
                                        let formatted = canonical.join("+");
                                        if !bindings.contains(&formatted) {
                                            bindings.push(formatted);
                                            draft.input_dirty = true;
                                        }
                                        entry.clear();
                                        draft.error.clear();
                                    }
                                    Err(err) => draft.error = err,
                                }
                            }
                        });
                    }
                });
            }
        });
    if let Some(name) = remove_action {
        draft.input_map.actions.remove(&name);
        draft.new_binding_draft.remove(&name);
        draft.input_dirty = true;
    }
}
