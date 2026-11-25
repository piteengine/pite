// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::Path;

use eframe::egui;

use super::super::inspector::AtlasEditorState;
use super::super::EditorApp;
use super::{list_asset_dir, rename_asset_rel, AssetModal, NewFileKind};

impl EditorApp {
    pub(in crate::app) fn show_asset_modals(&mut self, ctx: &egui::Context, root: &Path) {
        let Some(modal) = self.asset_modal.clone() else {
            return;
        };
        let mut close = false;

        enum Action {
            CreateFolder {
                parent_rel: String,
                name: String,
            },
            CreateFile {
                parent_rel: String,
                name: String,
                kind: NewFileKind,
            },
            CreateAtlas {
                parent_rel: String,
                png_rel: String,
                atlas_name: String,
                use_grid: bool,
                tile_w: u32,
                tile_h: u32,
                assign_node: Option<String>,
            },
            Rename {
                rel_path: String,
                new_name: String,
            },
            Delete {
                rel_path: String,
                is_dir: bool,
            },
        }
        let mut action: Option<Action> = None;

        match modal {
            AssetModal::NewFolder {
                parent_rel,
                mut name,
                error,
            } => {
                egui::Window::new("New Folder")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        let dest_desc = if parent_rel.is_empty() {
                            "res:// (project root)".to_string()
                        } else {
                            format!("res://{parent_rel}")
                        };
                        ui.label(format!("Create inside: {dest_desc}"));
                        ui.add_space(4.0);

                        let text_edit = ui.add(
                            egui::TextEdit::singleline(&mut name)
                                .hint_text("folder_name")
                                .desired_width(240.0),
                        );
                        if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            action = Some(Action::CreateFolder {
                                parent_rel: parent_rel.clone(),
                                name: name.clone(),
                            });
                        }

                        if let Some(err) = &error {
                            ui.colored_label(egui::Color32::from_rgb(230, 80, 80), err);
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Create").clicked() {
                                action = Some(Action::CreateFolder {
                                    parent_rel: parent_rel.clone(),
                                    name: name.clone(),
                                });
                            }
                            if ui.button("Cancel").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                close = true;
                            }
                        });
                        self.asset_modal = Some(AssetModal::NewFolder {
                            parent_rel,
                            name,
                            error,
                        });
                    });
            }
            AssetModal::NewFile {
                parent_rel,
                mut name,
                kind,
                error,
            } => {
                let title = match kind {
                    NewFileKind::Empty => "New File",
                    NewFileKind::Script => "New Script",
                    NewFileKind::Scene => "New Scene",
                };
                egui::Window::new(title)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        let dest_desc = if parent_rel.is_empty() {
                            "res:// (project root)".to_string()
                        } else {
                            format!("res://{parent_rel}")
                        };
                        ui.label(format!("Create inside: {dest_desc}"));
                        ui.add_space(4.0);

                        let hint = match kind {
                            NewFileKind::Empty => "filename.ext",
                            NewFileKind::Script => "script_name.py",
                            NewFileKind::Scene => "scene_name.ptscn",
                        };
                        let text_edit = ui.add(
                            egui::TextEdit::singleline(&mut name)
                                .hint_text(hint)
                                .desired_width(240.0),
                        );
                        if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            action = Some(Action::CreateFile {
                                parent_rel: parent_rel.clone(),
                                name: name.clone(),
                                kind: kind.clone(),
                            });
                        }

                        if let Some(err) = &error {
                            ui.colored_label(egui::Color32::from_rgb(230, 80, 80), err);
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Create").clicked() {
                                action = Some(Action::CreateFile {
                                    parent_rel: parent_rel.clone(),
                                    name: name.clone(),
                                    kind: kind.clone(),
                                });
                            }
                            if ui.button("Cancel").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                close = true;
                            }
                        });
                        self.asset_modal = Some(AssetModal::NewFile {
                            parent_rel,
                            name,
                            kind,
                            error,
                        });
                    });
            }
            AssetModal::NewAtlas {
                parent_rel,
                mut png_rel,
                mut atlas_name,
                mut use_grid,
                mut tile_w,
                mut tile_h,
                assign_node,
                error,
            } => {
                let (_, files) = list_asset_dir(root, &parent_rel);
                let png_options: Vec<String> = files
                    .into_iter()
                    .filter(|f| f.name.to_ascii_lowercase().ends_with(".png"))
                    .map(|f| f.rel_path)
                    .collect();
                egui::Window::new("Create Atlas (.ptatlas)")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!(
                            "Folder: res://{}",
                            if parent_rel.is_empty() {
                                "(project root)"
                            } else {
                                &parent_rel
                            }
                        ));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label("Sheet PNG:");
                            if png_options.is_empty() {
                                ui.add(
                                    egui::TextEdit::singleline(&mut png_rel)
                                        .hint_text("assets/sheet.png")
                                        .desired_width(180.0),
                                );
                            } else {
                                egui::ComboBox::from_id_salt("new_atlas_png_combo")
                                    .width(180.0)
                                    .selected_text(if png_rel.is_empty() {
                                        "Select a PNG…"
                                    } else {
                                        &png_rel
                                    })
                                    .show_ui(ui, |ui| {
                                        for opt in &png_options {
                                            if ui
                                                .selectable_value(&mut png_rel, opt.clone(), opt)
                                                .changed()
                                            {
                                                let stem = Path::new(opt)
                                                    .file_stem()
                                                    .and_then(|s| s.to_str())
                                                    .unwrap_or("sheet");
                                                atlas_name = format!("{stem}.ptatlas");
                                            }
                                        }
                                    });
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Atlas File:");
                            ui.add(
                                egui::TextEdit::singleline(&mut atlas_name)
                                    .hint_text("sheet.ptatlas")
                                    .desired_width(180.0),
                            );
                        });
                        ui.checkbox(&mut use_grid, "Auto-slice grid on create");
                        if use_grid {
                            ui.horizontal(|ui| {
                                ui.label("Tile W:");
                                ui.add(egui::DragValue::new(&mut tile_w).range(1..=4096));
                                ui.label("H:");
                                ui.add(egui::DragValue::new(&mut tile_h).range(1..=4096));
                            });
                        }
                        if let Some(err) = &error {
                            ui.colored_label(egui::Color32::from_rgb(230, 80, 80), err);
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Create").clicked() {
                                action = Some(Action::CreateAtlas {
                                    parent_rel: parent_rel.clone(),
                                    png_rel: png_rel.clone(),
                                    atlas_name: atlas_name.clone(),
                                    use_grid,
                                    tile_w,
                                    tile_h,
                                    assign_node: assign_node.clone(),
                                });
                            }
                            if ui.button("Cancel").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                close = true;
                            }
                        });
                        self.asset_modal = Some(AssetModal::NewAtlas {
                            parent_rel,
                            png_rel,
                            atlas_name,
                            use_grid,
                            tile_w,
                            tile_h,
                            assign_node,
                            error,
                        });
                    });
            }
            AssetModal::Rename {
                rel_path,
                mut new_name,
                error,
            } => {
                egui::Window::new("Rename Asset")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Rename: res://{rel_path}"));
                        ui.add_space(4.0);

                        let text_edit =
                            ui.add(egui::TextEdit::singleline(&mut new_name).desired_width(240.0));
                        if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            action = Some(Action::Rename {
                                rel_path: rel_path.clone(),
                                new_name: new_name.clone(),
                            });
                        }

                        if let Some(err) = &error {
                            ui.colored_label(egui::Color32::from_rgb(230, 80, 80), err);
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Rename").clicked() {
                                action = Some(Action::Rename {
                                    rel_path: rel_path.clone(),
                                    new_name: new_name.clone(),
                                });
                            }
                            if ui.button("Cancel").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                close = true;
                            }
                        });
                        self.asset_modal = Some(AssetModal::Rename {
                            rel_path,
                            new_name,
                            error,
                        });
                    });
            }
            AssetModal::ConfirmDelete { rel_path, is_dir } => {
                egui::Window::new("Delete Asset")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        let kind_str = if is_dir {
                            "folder and all its contents"
                        } else {
                            "file"
                        };
                        ui.label(format!("Are you sure you want to delete this {kind_str}?"));
                        ui.colored_label(
                            egui::Color32::from_rgb(230, 80, 80),
                            format!("res://{rel_path}"),
                        );
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            if ui.button("Delete").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                action = Some(Action::Delete {
                                    rel_path: rel_path.clone(),
                                    is_dir,
                                });
                            }
                            if ui.button("Cancel").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                close = true;
                            }
                        });
                    });
            }
        }

        if close {
            self.asset_modal = None;
        }

        if let Some(act) = action {
            match act {
                Action::CreateFolder { parent_rel, name } => {
                    let trimmed = name.trim();
                    if trimmed.is_empty() {
                        self.asset_modal = Some(AssetModal::NewFolder {
                            parent_rel,
                            name,
                            error: Some("Folder name cannot be empty".to_string()),
                        });
                        return;
                    }
                    if trimmed.contains('/') || trimmed.contains('\\') {
                        self.asset_modal = Some(AssetModal::NewFolder {
                            parent_rel,
                            name,
                            error: Some("Folder name cannot contain slashes".to_string()),
                        });
                        return;
                    }
                    let target_fs = if parent_rel.is_empty() {
                        root.join(trimmed)
                    } else {
                        root.join(parent_rel.replace('/', std::path::MAIN_SEPARATOR_STR))
                            .join(trimmed)
                    };
                    if target_fs.exists() {
                        self.asset_modal = Some(AssetModal::NewFolder {
                            parent_rel,
                            name,
                            error: Some("A folder with this name already exists".to_string()),
                        });
                        return;
                    }
                    if let Err(e) = std::fs::create_dir_all(&target_fs) {
                        self.asset_modal = Some(AssetModal::NewFolder {
                            parent_rel,
                            name,
                            error: Some(format!("Failed to create folder: {e:#}")),
                        });
                        return;
                    }
                    if !parent_rel.is_empty() {
                        self.open_nodes.insert(format!("assets:{parent_rel}"));
                    }
                    self.asset_modal = None;
                    self.log(format!("Created folder res://{parent_rel}/{trimmed}"));
                }
                Action::CreateFile {
                    parent_rel,
                    name,
                    kind,
                } => {
                    let trimmed = name.trim();
                    if trimmed.is_empty() {
                        self.asset_modal = Some(AssetModal::NewFile {
                            parent_rel,
                            name,
                            kind,
                            error: Some("File name cannot be empty".to_string()),
                        });
                        return;
                    }
                    if trimmed.contains('/') || trimmed.contains('\\') {
                        self.asset_modal = Some(AssetModal::NewFile {
                            parent_rel,
                            name,
                            kind,
                            error: Some("File name cannot contain slashes".to_string()),
                        });
                        return;
                    }
                    let full_name = match kind {
                        NewFileKind::Script if !trimmed.ends_with(".py") => {
                            format!("{trimmed}.py")
                        }
                        NewFileKind::Scene if !trimmed.ends_with(".ptscn") => {
                            format!("{trimmed}.ptscn")
                        }
                        _ => trimmed.to_string(),
                    };
                    let target_fs = if parent_rel.is_empty() {
                        root.join(&full_name)
                    } else {
                        root.join(parent_rel.replace('/', std::path::MAIN_SEPARATOR_STR))
                            .join(&full_name)
                    };
                    if target_fs.exists() {
                        self.asset_modal = Some(AssetModal::NewFile {
                            parent_rel,
                            name: full_name,
                            kind,
                            error: Some("A file with this name already exists".to_string()),
                        });
                        return;
                    }
                    let content = match kind {
                        NewFileKind::Script => {
                            let class_name = Path::new(&full_name)
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("MyScript");
                            let mut c_chars = class_name.chars();
                            let pascal_case = match c_chars.next() {
                                Some(first) => {
                                    first.to_uppercase().collect::<String>() + c_chars.as_str()
                                }
                                None => "MyScript".to_string(),
                            };
                            format!(
                                "import pite\n\nclass {pascal_case}(pite.Node):\n    def _ready(self):\n        pass\n\n    def _process(self, delta):\n        pass\n"
                            )
                        }
                        NewFileKind::Scene => {
                            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n".to_string()
                        }
                        NewFileKind::Empty => String::new(),
                    };
                    if let Some(parent) = target_fs.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(&target_fs, content) {
                        self.asset_modal = Some(AssetModal::NewFile {
                            parent_rel,
                            name: full_name,
                            kind,
                            error: Some(format!("Failed to write file: {e:#}")),
                        });
                        return;
                    }
                    if !parent_rel.is_empty() {
                        self.open_nodes.insert(format!("assets:{parent_rel}"));
                    }
                    let _ = pite_assets::reimport_project(root);
                    self.asset_modal = None;
                    self.open_asset(&target_fs);
                    self.log(format!("Created file res://{parent_rel}/{full_name}"));
                }
                Action::CreateAtlas {
                    parent_rel,
                    png_rel,
                    atlas_name,
                    use_grid,
                    tile_w,
                    tile_h,
                    assign_node,
                } => {
                    let trimmed = atlas_name.trim();
                    if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains('\\') {
                        self.asset_modal = Some(AssetModal::NewAtlas {
                            parent_rel,
                            png_rel,
                            atlas_name,
                            use_grid,
                            tile_w,
                            tile_h,
                            assign_node,
                            error: Some("Enter a valid .ptatlas filename".to_string()),
                        });
                        return;
                    }
                    let full_name = if trimmed.ends_with(".ptatlas") {
                        trimmed.to_string()
                    } else {
                        format!("{trimmed}.ptatlas")
                    };
                    let png_fs = match pite_core::resolve_res(root, &format!("res://{png_rel}")) {
                        Ok(p) if p.is_file() => p,
                        _ => {
                            self.asset_modal = Some(AssetModal::NewAtlas {
                                parent_rel,
                                png_rel,
                                atlas_name,
                                use_grid,
                                tile_w,
                                tile_h,
                                assign_node,
                                error: Some("Source PNG file does not exist".to_string()),
                            });
                            return;
                        }
                    };
                    let target_rel = if parent_rel.is_empty() {
                        full_name.clone()
                    } else {
                        format!("{parent_rel}/{full_name}")
                    };
                    let target_res = format!("res://{target_rel}");
                    let target_fs = match pite_core::resolve_res(root, &target_res) {
                        Ok(p) => p,
                        Err(e) => {
                            self.asset_modal = Some(AssetModal::NewAtlas {
                                parent_rel,
                                png_rel,
                                atlas_name,
                                use_grid,
                                tile_w,
                                tile_h,
                                assign_node,
                                error: Some(format!("Invalid atlas path: {e}")),
                            });
                            return;
                        }
                    };
                    if target_fs.exists() {
                        self.asset_modal = Some(AssetModal::NewAtlas {
                            parent_rel,
                            png_rel,
                            atlas_name,
                            use_grid,
                            tile_w,
                            tile_h,
                            assign_node,
                            error: Some(format!("{full_name} already exists")),
                        });
                        return;
                    }
                    let grid = use_grid.then_some((tile_w, tile_h));
                    match AtlasEditorState::create_from_png(
                        &png_fs,
                        &target_fs,
                        target_res.clone(),
                        grid,
                    ) {
                        Ok(state) => {
                            let first_frame = state.atlas.frames.keys().next().cloned();
                            if !parent_rel.is_empty() {
                                self.open_nodes.insert(format!("assets:{parent_rel}"));
                            }
                            let _ = pite_assets::reimport_project(root);
                            if let Some(nid_str) = assign_node {
                                let nid = pite_core::NodeId::from(nid_str);
                                self.remove_prop(&nid, "texture");
                                self.set_prop(
                                    &nid,
                                    "atlas",
                                    pite_core::PropValue::Str(target_res.clone()),
                                );
                                if let Some(frame) = first_frame {
                                    let is_sprite = self.session.host().with_tree(|t| {
                                        t.get(&nid).is_some_and(|n| n.type_name == "Sprite2D")
                                    });
                                    if is_sprite {
                                        self.set_prop(
                                            &nid,
                                            "frame",
                                            pite_core::PropValue::Str(frame),
                                        );
                                    }
                                }
                            }
                            self.atlas_editor = Some(state);
                            self.asset_modal = None;
                            self.session.dirty_assets.insert(target_fs);
                            self.viewport_dirty = true;
                            self.log(format!("Created atlas {target_res}"));
                        }
                        Err(e) => {
                            self.asset_modal = Some(AssetModal::NewAtlas {
                                parent_rel,
                                png_rel,
                                atlas_name,
                                use_grid,
                                tile_w,
                                tile_h,
                                assign_node,
                                error: Some(format!("{e:#}")),
                            });
                        }
                    }
                }
                Action::Rename { rel_path, new_name } => {
                    let trimmed = new_name.trim();
                    if trimmed.is_empty() {
                        self.asset_modal = Some(AssetModal::Rename {
                            rel_path,
                            new_name,
                            error: Some("Name cannot be empty".to_string()),
                        });
                        return;
                    }
                    if trimmed.contains('/') || trimmed.contains('\\') {
                        self.asset_modal = Some(AssetModal::Rename {
                            rel_path,
                            new_name,
                            error: Some("Name cannot contain slashes".to_string()),
                        });
                        return;
                    }
                    match rename_asset_rel(root, &rel_path, trimmed) {
                        Ok(renames) => {
                            self.apply_asset_renames(&renames);
                            if self.selected_asset.as_deref() == Some(&rel_path) {
                                let parent = Path::new(&rel_path).parent().unwrap_or(Path::new(""));
                                let updated_rel = if parent.as_os_str().is_empty() {
                                    trimmed.to_string()
                                } else {
                                    format!(
                                        "{}/{trimmed}",
                                        parent.to_string_lossy().replace('\\', "/")
                                    )
                                };
                                self.selected_asset = Some(updated_rel);
                            }
                            self.asset_modal = None;
                            self.log(format!("Renamed res://{rel_path} -> {trimmed}"));
                        }
                        Err(e) => {
                            self.asset_modal = Some(AssetModal::Rename {
                                rel_path,
                                new_name,
                                error: Some(format!("{e:#}")),
                            });
                        }
                    }
                }
                Action::Delete { rel_path, is_dir } => {
                    self.execute_asset_delete(&rel_path, is_dir);
                    self.asset_modal = None;
                }
            }
        }
    }
}
