// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::Path;

use eframe::egui;
use egui_phosphor::regular;

use crate::icons;

use super::super::{EditorApp, MenuSeparatorTracker};
use super::{
    list_asset_dir, show_in_file_manager, AssetClipboardEntry, AssetDndPayload, NewFileKind,
};

impl EditorApp {
    pub(in crate::app) fn show_asset_dir(&mut self, ui: &mut egui::Ui, root: &Path, dir: &str) {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 1.0);
        ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
        ui.spacing_mut().indent = 14.0;

        let has_dnd = egui::DragAndDrop::has_any_payload(ui.ctx());

        // Root entry shown once at the top of the tree
        if dir.is_empty() {
            let root_selected = self.selected_asset.as_deref() == Some("");
            let is_drop_target_root =
                if let Some(dragged) = egui::DragAndDrop::payload::<AssetDndPayload>(ui.ctx()) {
                    let parent = Path::new(&dragged.rel_path)
                        .parent()
                        .unwrap_or(Path::new(""));
                    !parent.as_os_str().is_empty()
                } else {
                    false
                };

            let row_width = ui.available_width().max(160.0);
            let (root_rect, root_resp) =
                ui.allocate_exact_size(egui::vec2(row_width, 20.0), egui::Sense::click());

            let is_pointer_over = root_resp.contains_pointer();

            if is_pointer_over && has_dnd {
                if is_drop_target_root {
                    self.hovered_asset_folder = Some(String::new());
                    ui.painter().rect_filled(
                        root_rect,
                        2.0,
                        egui::Color32::from_rgba_unmultiplied(11, 147, 135, 45),
                    );
                    ui.painter().rect_stroke(
                        root_rect,
                        2.0,
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(11, 147, 135)),
                        egui::StrokeKind::Middle,
                    );
                } else {
                    ui.painter().rect_filled(
                        root_rect,
                        2.0,
                        egui::Color32::from_rgba_unmultiplied(220, 80, 80, 30),
                    );
                    ui.painter().rect_stroke(
                        root_rect,
                        2.0,
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(220, 80, 80)),
                        egui::StrokeKind::Middle,
                    );
                }
            } else if root_selected {
                ui.painter()
                    .rect_filled(root_rect, 2.0, ui.visuals().selection.bg_fill);
            } else if root_resp.hovered() && !has_dnd {
                ui.painter()
                    .rect_filled(root_rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
            }

            if root_resp.clicked() {
                self.selected_asset = Some(String::new());
            }

            if let Some(dragged) = root_resp.dnd_release_payload::<AssetDndPayload>() {
                let parent = Path::new(&dragged.rel_path)
                    .parent()
                    .unwrap_or(Path::new(""));
                if !parent.as_os_str().is_empty() {
                    self.execute_asset_move(&dragged.rel_path, "");
                }
            }

            let p = ui.painter();
            let fg = if root_selected {
                crate::theme::TEXT
            } else {
                crate::theme::FAINT
            };
            let icon_rect = egui::Rect::from_min_size(
                egui::Pos2::new(root_rect.left() + 4.0, root_rect.center().y - 8.0),
                egui::vec2(16.0, 16.0),
            );
            icons::draw_icon(p, icon_rect, icons::Icon::FolderOpen, fg);

            let text_pos = egui::Pos2::new(root_rect.left() + 24.0, root_rect.center().y);
            p.text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                "res:// (project root)",
                egui::FontId::new(13.0, egui::FontFamily::Proportional),
                crate::theme::TEXT,
            );

            root_resp.context_menu(|ui| {
                let mut menu = MenuSeparatorTracker::new(ui);
                if menu.button("New File…").clicked() {
                    self.open_new_file_modal("", NewFileKind::Empty);
                    menu.close();
                }
                if menu.button("New Script (.py)").clicked() {
                    self.open_new_file_modal("", NewFileKind::Script);
                    menu.close();
                }
                if menu.button("New Scene (.ptscn)").clicked() {
                    self.open_new_file_modal("", NewFileKind::Scene);
                    menu.close();
                }
                if menu.button("New Folder…").clicked() {
                    self.open_new_folder_modal("");
                    menu.close();
                }
                menu.start_section();
                let can_paste = self.asset_clipboard.is_some();
                if menu
                    .add_enabled(can_paste, egui::Button::new("Paste"))
                    .clicked()
                {
                    self.execute_asset_paste("");
                    menu.close();
                }
                menu.start_section();
                if menu.button("Show in File Manager").clicked() {
                    show_in_file_manager(root);
                    menu.close();
                }
            });
        }

        let (folders, files) = list_asset_dir(root, dir);

        // 1. Folders first (alphabetical)
        for folder in folders {
            let full = folder.rel_path.clone();
            let key = format!("assets:{full}");
            let open = self.open_nodes.contains(&key);
            let selected = self.selected_asset.as_deref() == Some(&full);

            let row_width = ui.available_width().max(160.0);
            let (rect, row_resp) =
                ui.allocate_exact_size(egui::vec2(row_width, 20.0), egui::Sense::click_and_drag());

            let is_source =
                if let Some(dragged) = egui::DragAndDrop::payload::<AssetDndPayload>(ui.ctx()) {
                    dragged.rel_path == full
                } else {
                    false
                };

            let is_valid_drop_target =
                if let Some(dragged) = egui::DragAndDrop::payload::<AssetDndPayload>(ui.ctx()) {
                    let dragged_parent = Path::new(&dragged.rel_path)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or("")
                        .replace('\\', "/");
                    dragged.rel_path != full
                        && dragged_parent != full
                        && !full.starts_with(&format!("{}/", dragged.rel_path))
                } else {
                    false
                };

            // Start drag only when threshold is passed
            if row_resp.drag_started() {
                egui::DragAndDrop::set_payload(
                    ui.ctx(),
                    AssetDndPayload {
                        rel_path: full.clone(),
                        is_dir: true,
                    },
                );
            }

            let mut toggled = false;
            if row_resp.clicked() {
                // If clicked on the left arrow region, toggle open/close
                if let Some(pos) = row_resp.interact_pointer_pos() {
                    if pos.x < rect.left() + 16.0 {
                        toggled = true;
                    } else {
                        self.selected_asset = Some(full.clone());
                    }
                } else {
                    self.selected_asset = Some(full.clone());
                }
            }
            if row_resp.double_clicked() {
                toggled = true;
            }

            if let Some(dragged) = row_resp.dnd_release_payload::<AssetDndPayload>() {
                let dragged_parent = Path::new(&dragged.rel_path)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("")
                    .replace('\\', "/");
                if dragged.rel_path != full
                    && dragged_parent != full
                    && !full.starts_with(&format!("{}/", dragged.rel_path))
                {
                    self.execute_asset_move(&dragged.rel_path, &full);
                }
            }

            let is_target = has_dnd && !is_source && row_resp.contains_pointer();
            if is_target {
                if is_valid_drop_target {
                    self.hovered_asset_folder = Some(full.clone());
                    ui.painter().rect_filled(
                        rect,
                        2.0,
                        egui::Color32::from_rgba_unmultiplied(11, 147, 135, 45),
                    );
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(11, 147, 135)),
                        egui::StrokeKind::Middle,
                    );
                } else {
                    ui.painter().rect_filled(
                        rect,
                        2.0,
                        egui::Color32::from_rgba_unmultiplied(220, 80, 80, 30),
                    );
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(220, 80, 80)),
                        egui::StrokeKind::Middle,
                    );
                }
            } else if is_source && has_dnd {
                ui.painter().rect_filled(
                    rect,
                    2.0,
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 12),
                );
            } else if selected {
                ui.painter()
                    .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
            } else if row_resp.hovered() && !has_dnd {
                ui.painter()
                    .rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
            }

            let p = ui.painter();
            let fg = if selected {
                crate::theme::TEXT
            } else {
                crate::theme::FAINT
            };

            // Caret expand/collapse arrow
            let caret_icon = if open {
                icons::Icon::CaretDown
            } else {
                icons::Icon::CaretRight
            };
            let caret_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.left() + 2.0, rect.center().y - 6.0),
                egui::vec2(12.0, 12.0),
            );
            icons::draw_icon(p, caret_rect, caret_icon, fg);

            // Folder icon
            let folder_icon = if open {
                icons::Icon::FolderOpen
            } else {
                icons::Icon::Folder
            };
            let f_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.left() + 16.0, rect.center().y - 8.0),
                egui::vec2(16.0, 16.0),
            );
            icons::draw_icon(p, f_rect, folder_icon, fg);

            // Folder label
            let text_pos = egui::Pos2::new(rect.left() + 36.0, rect.center().y);
            p.text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                &folder.name,
                egui::FontId::new(13.0, egui::FontFamily::Proportional),
                crate::theme::TEXT,
            );

            // Context menu for folder
            let folder_rel = full.clone();
            row_resp.context_menu(|ui| {
                let mut menu = MenuSeparatorTracker::new(ui);
                if menu.button("New File…").clicked() {
                    self.open_new_file_modal(&folder_rel, NewFileKind::Empty);
                    menu.close();
                }
                if menu.button("New Script (.py)").clicked() {
                    self.open_new_file_modal(&folder_rel, NewFileKind::Script);
                    menu.close();
                }
                if menu.button("New Scene (.ptscn)").clicked() {
                    self.open_new_file_modal(&folder_rel, NewFileKind::Scene);
                    menu.close();
                }
                if menu.button("New Folder…").clicked() {
                    self.open_new_folder_modal(&folder_rel);
                    menu.close();
                }

                menu.start_section();
                if menu.button("Cut").clicked() {
                    self.asset_clipboard = Some(AssetClipboardEntry {
                        rel_path: folder_rel.clone(),
                        is_cut: true,
                    });
                    menu.close();
                }
                if menu.button("Copy").clicked() {
                    self.asset_clipboard = Some(AssetClipboardEntry {
                        rel_path: folder_rel.clone(),
                        is_cut: false,
                    });
                    menu.close();
                }
                let can_paste = self.asset_clipboard.is_some();
                if menu
                    .add_enabled(can_paste, egui::Button::new("Paste"))
                    .clicked()
                {
                    self.execute_asset_paste(&folder_rel);
                    menu.close();
                }
                if menu.button("Rename…").clicked() {
                    self.open_rename_modal(&folder_rel);
                    menu.close();
                }
                if menu.button("Delete…").clicked() {
                    self.open_delete_modal(&folder_rel, true);
                    menu.close();
                }

                menu.start_section();
                if menu.button("Copy Relative Path (res://…)").clicked() {
                    menu.ctx().copy_text(format!("res://{folder_rel}"));
                    self.log(format!("Copied res://{folder_rel}"));
                    menu.close();
                }
                if menu.button("Copy Absolute Path").clicked() {
                    let abs = root.join(folder_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    menu.ctx().copy_text(abs.to_string_lossy().to_string());
                    menu.close();
                }
                if menu.button("Show in File Manager").clicked() {
                    let abs = root.join(folder_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    show_in_file_manager(&abs);
                    menu.close();
                }
            });

            if toggled {
                if open {
                    self.open_nodes.remove(&key);
                } else {
                    self.open_nodes.insert(key.clone());
                }
            }
            if open {
                ui.indent(format!("assets-indent:{full}"), |ui| {
                    self.show_asset_dir(ui, root, &full);
                });
            }
        }

        // 2. Files next (alphabetical)
        for file in files {
            let full = file.rel_path.clone();
            let selected = self.selected_asset.as_deref() == Some(&full);

            let kind = match file
                .name
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "py" => icons::Icon::Script,
                "ptscn" => icons::Icon::Scene,
                "toml" | "cfg" | "ini" | "json" | "yaml" | "yml" => icons::Icon::Gear,
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" => icons::Icon::Image,
                "wav" | "ogg" | "mp3" | "flac" => icons::Icon::Audio,
                _ => icons::Icon::File,
            };

            let row_width = ui.available_width().max(160.0);
            let (rect, row_resp) =
                ui.allocate_exact_size(egui::vec2(row_width, 20.0), egui::Sense::click_and_drag());

            let is_source =
                if let Some(dragged) = egui::DragAndDrop::payload::<AssetDndPayload>(ui.ctx()) {
                    dragged.rel_path == full
                } else {
                    false
                };

            // Dropping onto a file targets this file's parent folder `dir`!
            let is_valid_drop_target =
                if let Some(dragged) = egui::DragAndDrop::payload::<AssetDndPayload>(ui.ctx()) {
                    let dragged_parent = Path::new(&dragged.rel_path)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or("")
                        .replace('\\', "/");
                    dragged.rel_path != dir
                        && dragged_parent != dir
                        && !dir.starts_with(&format!("{}/", dragged.rel_path))
                } else {
                    false
                };

            // Start drag only when threshold is passed
            if row_resp.drag_started() {
                egui::DragAndDrop::set_payload(
                    ui.ctx(),
                    AssetDndPayload {
                        rel_path: full.clone(),
                        is_dir: false,
                    },
                );
            }

            if row_resp.clicked() {
                self.selected_asset = Some(full.clone());
            }
            if row_resp.double_clicked() {
                let abs = root.join(full.replace('/', std::path::MAIN_SEPARATOR_STR));
                self.open_asset(&abs);
            }

            // Dropping onto a file moves into parent folder `dir`
            if let Some(dragged) = row_resp.dnd_release_payload::<AssetDndPayload>() {
                if is_valid_drop_target {
                    self.execute_asset_move(&dragged.rel_path, dir);
                }
            }

            let is_target = has_dnd && !is_source && row_resp.contains_pointer();
            if is_target {
                if is_valid_drop_target {
                    self.hovered_asset_folder = Some(dir.to_string());
                    ui.painter().rect_filled(
                        rect,
                        2.0,
                        egui::Color32::from_rgba_unmultiplied(11, 147, 135, 45),
                    );
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(11, 147, 135)),
                        egui::StrokeKind::Middle,
                    );
                } else {
                    ui.painter().rect_filled(
                        rect,
                        2.0,
                        egui::Color32::from_rgba_unmultiplied(220, 80, 80, 30),
                    );
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(220, 80, 80)),
                        egui::StrokeKind::Middle,
                    );
                }
            } else if is_source && has_dnd {
                ui.painter().rect_filled(
                    rect,
                    2.0,
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 12),
                );
            } else if selected {
                ui.painter()
                    .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
            } else if row_resp.hovered() && !has_dnd {
                ui.painter()
                    .rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
            }

            let p = ui.painter();
            let fg = if selected {
                crate::theme::TEXT
            } else {
                crate::theme::FAINT
            };

            let f_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.left() + 16.0, rect.center().y - 8.0),
                egui::vec2(16.0, 16.0),
            );
            icons::draw_icon(p, f_rect, kind, fg);

            let text_pos = egui::Pos2::new(rect.left() + 36.0, rect.center().y);
            p.text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                &file.name,
                egui::FontId::new(13.0, egui::FontFamily::Proportional),
                crate::theme::TEXT,
            );

            // Context menu for file
            let file_rel = full.clone();
            row_resp.context_menu(|ui| {
                let mut menu = MenuSeparatorTracker::new(ui);
                if menu.button("Open").clicked() {
                    let abs = root.join(file_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    self.open_asset(&abs);
                    menu.close();
                }

                menu.start_section();
                if menu.button("Cut").clicked() {
                    self.asset_clipboard = Some(AssetClipboardEntry {
                        rel_path: file_rel.clone(),
                        is_cut: true,
                    });
                    menu.close();
                }
                if menu.button("Copy").clicked() {
                    self.asset_clipboard = Some(AssetClipboardEntry {
                        rel_path: file_rel.clone(),
                        is_cut: false,
                    });
                    menu.close();
                }
                if menu.button("Duplicate").clicked() {
                    self.execute_asset_duplicate(&file_rel);
                    menu.close();
                }
                if menu.button("Rename…").clicked() {
                    self.open_rename_modal(&file_rel);
                    menu.close();
                }
                if menu.button("Delete…").clicked() {
                    self.open_delete_modal(&file_rel, false);
                    menu.close();
                }

                menu.start_section();
                if menu.button("Copy Relative Path (res://…)").clicked() {
                    menu.ctx().copy_text(format!("res://{file_rel}"));
                    self.log(format!("Copied res://{file_rel}"));
                    menu.close();
                }
                if menu.button("Copy Absolute Path").clicked() {
                    let abs = root.join(file_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    menu.ctx().copy_text(abs.to_string_lossy().to_string());
                    menu.close();
                }
                if menu.button("Show in File Manager").clicked() {
                    let abs = root.join(file_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    show_in_file_manager(&abs);
                    menu.close();
                }
            });
        }
    }

    pub(in crate::app) fn show_asset_drag_preview(&self, ctx: &egui::Context) {
        let Some(payload) = egui::DragAndDrop::payload::<AssetDndPayload>(ctx) else {
            return;
        };
        let pointer_pos = ctx
            .input(|i| i.pointer.latest_pos())
            .or_else(|| ctx.pointer_interact_pos());
        let Some(pos) = pointer_pos else {
            return;
        };

        egui::Area::new(egui::Id::unique("asset_drag_preview"))
            .fixed_pos(pos + egui::vec2(16.0, 16.0))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(egui::Color32::from_rgb(20, 24, 29))
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(11, 147, 135),
                    ))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        let icon_str = if payload.is_dir {
                            regular::FOLDER
                        } else {
                            match payload
                                .rel_path
                                .rsplit('.')
                                .next()
                                .unwrap_or("")
                                .to_ascii_lowercase()
                                .as_str()
                            {
                                "py" => regular::FILE_CODE,
                                "ptscn" => regular::FILM_SLATE,
                                "toml" | "cfg" | "ini" | "json" | "yaml" | "yml" => regular::GEAR,
                                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" => {
                                    regular::IMAGE
                                }
                                "wav" | "ogg" | "mp3" | "flac" => regular::SPEAKER_HIGH,
                                _ => regular::FILE,
                            }
                        };
                        let name = std::path::Path::new(&payload.rel_path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&payload.rel_path);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(icon_str)
                                    .color(egui::Color32::from_rgb(11, 147, 135)),
                            );
                            ui.label(egui::RichText::new(name).strong());
                            if let Some(dest) = &self.hovered_asset_folder {
                                ui.label(egui::RichText::new("->").weak());
                                if dest.is_empty() {
                                    ui.label(
                                        egui::RichText::new("res://")
                                            .color(egui::Color32::from_rgb(11, 147, 135)),
                                    );
                                } else {
                                    ui.label(
                                        egui::RichText::new(format!("{dest}/"))
                                            .color(egui::Color32::from_rgb(11, 147, 135)),
                                    );
                                }
                            } else if let Some((target_name, _)) = &self.hovered_node_target {
                                ui.label(egui::RichText::new("->").weak());
                                ui.label(
                                    egui::RichText::new(target_name)
                                        .color(egui::Color32::from_rgb(11, 147, 135)),
                                );
                            }
                        });
                    });
            });
    }
}
