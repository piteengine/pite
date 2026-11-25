// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::PathBuf;

use eframe::egui;
use egui_phosphor::regular;
use pite_core::NodeId;

use crate::icons::{self, Icon};
use crate::ops;

use super::super::EditorApp;
use super::node_types::{node_hierarchy, node_type_matches_search, NodeTypeInfo};

impl EditorApp {
    pub(in crate::app) fn show_node_modals(&mut self, ctx: &egui::Context) {
        self.show_add_node_modal(ctx);
        self.show_rename_node_modal(ctx);
        self.show_instance_scene_modal(ctx);
    }

    fn show_add_node_modal(&mut self, ctx: &egui::Context) {
        let Some(mut modal) = self.add_node_modal.clone() else {
            return;
        };
        let mut close = false;
        let mut create = false;

        let res = egui::Window::new("Create New Node")
            .id(egui::Id::unique("add_node_modal_window"))
            .default_size(egui::vec2(420.0, 480.0))
            .min_size(egui::vec2(340.0, 400.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(regular::MAGNIFYING_GLASS).color(crate::theme::FAINT),
                    );
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut modal.search)
                            .hint_text("Search node types…")
                            .desired_width(f32::INFINITY),
                    );
                    if modal.search.is_empty() && ui.ctx().input(|i| i.time < 0.1) {
                        resp.request_focus();
                    }
                });

                ui.separator();

                let search_lower = modal.search.to_lowercase();
                let tree_height = ui.available_height() - 140.0;
                egui::ScrollArea::vertical()
                    .max_height(tree_height)
                    .show(ui, |ui| {
                        render_node_hierarchy(
                            ui,
                            node_hierarchy(),
                            &search_lower,
                            &mut modal.selected_type,
                            &mut modal.name,
                            &mut create,
                        );
                    });

                ui.separator();

                // Description card
                let desc = pite_core::node_type_schema(&modal.selected_type)
                    .map(|s| s.description)
                    .unwrap_or("Engine scene node.");
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(16, 20, 24))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        let icon = icons::node_type_icon(&modal.selected_type);
                        let color = icons::node_type_color(&modal.selected_type);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(icon.as_str()).color(color).size(16.0));
                            ui.label(egui::RichText::new(&modal.selected_type).strong());
                        });
                        ui.label(egui::RichText::new(desc).weak().size(12.0));
                    });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Node Name:");
                    let resp = ui.text_edit_singleline(&mut modal.name);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        create = true;
                    }
                });

                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        create = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if res.is_none() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }

        if create && !modal.selected_type.is_empty() && !modal.name.is_empty() {
            let parent = modal
                .parent
                .clone()
                .or_else(|| self.selected.clone().map(NodeId::from))
                .or_else(|| self.session.host().with_tree(|t| t.root().cloned()));

            match self.session.host().with_tree_mut(|t| {
                ops::add_node(t, parent.clone(), &modal.selected_type, &modal.name)
            }) {
                Ok(new_id) => {
                    self.selected = Some(new_id.to_string());
                    if let Some(p) = &parent {
                        self.open_nodes.insert(p.to_string());
                    }
                    self.viewport_dirty = true;
                    self.log(format!("Added node {} ({})", new_id, modal.selected_type));
                    close = true;
                }
                Err(e) => self.log(format!("Failed to add node: {e:#}")),
            }
        }

        if close {
            self.add_node_modal = None;
        } else {
            self.add_node_modal = Some(modal);
        }
    }

    fn show_rename_node_modal(&mut self, ctx: &egui::Context) {
        let Some(mut modal) = self.rename_node_modal.clone() else {
            return;
        };
        let mut close = false;
        let mut rename = false;

        let res = egui::Window::new("Rename Node")
            .id(egui::Id::unique("rename_node_modal_window"))
            .default_size(egui::vec2(280.0, 120.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let resp = ui.text_edit_singleline(&mut modal.name);
                    resp.request_focus();
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        rename = true;
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        rename = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if res.is_none() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }

        if rename && !modal.name.trim().is_empty() {
            match self
                .session
                .host()
                .with_tree_mut(|t| ops::rename_node(t, &modal.id, &modal.name))
            {
                Ok(()) => {
                    self.viewport_dirty = true;
                    self.log(format!("Renamed node {} to {}", modal.id, modal.name));
                    close = true;
                }
                Err(e) => self.log(format!("Failed to rename node: {e:#}")),
            }
        }

        if close {
            self.rename_node_modal = None;
        } else {
            self.rename_node_modal = Some(modal);
        }
    }

    fn show_instance_scene_modal(&mut self, ctx: &egui::Context) {
        let Some(modal) = self.instance_modal.clone() else {
            return;
        };
        let mut close = false;
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));

        let mut scenes = Vec::new();
        for file in self.asset_files() {
            if file.extension().and_then(|e| e.to_str()) == Some("ptscn") {
                if let Ok(rel) = file.strip_prefix(&root) {
                    scenes.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }

        let res = egui::Window::new("Instantiate Child Scene")
            .id(egui::Id::unique("instance_scene_modal_window"))
            .default_size(egui::vec2(340.0, 360.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                if scenes.is_empty() {
                    ui.label("No .ptscn scene files found in project.");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            for sc in scenes {
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 22.0),
                                    egui::Sense::click(),
                                );
                                if resp.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        2.0,
                                        ui.visuals().widgets.hovered.bg_fill,
                                    );
                                }
                                let p = ui.painter();
                                let icon_rect = egui::Rect::from_min_size(
                                    egui::Pos2::new(rect.left() + 4.0, rect.center().y - 8.0),
                                    egui::vec2(16.0, 16.0),
                                );
                                icons::draw_icon(p, icon_rect, Icon::Scene, crate::theme::TEXT);
                                p.text(
                                    egui::Pos2::new(rect.left() + 26.0, rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &sc,
                                    egui::FontId::new(13.0, egui::FontFamily::Proportional),
                                    crate::theme::TEXT,
                                );
                                if resp.double_clicked() || resp.clicked() {
                                    let fs_path =
                                        root.join(sc.replace('/', std::path::MAIN_SEPARATOR_STR));
                                    let parent = modal
                                        .parent
                                        .clone()
                                        .or_else(|| self.selected.clone().map(NodeId::from))
                                        .or_else(|| {
                                            self.session.host().with_tree(|t| t.root().cloned())
                                        });
                                    match self.session.host().with_tree_mut(|t| {
                                        ops::instantiate_scene_as_child(t, parent, &fs_path)
                                    }) {
                                        Ok(new_root) => {
                                            self.selected = Some(new_root.to_string());
                                            self.viewport_dirty = true;
                                            self.log(format!(
                                                "Instantiated scene res://{sc} as {new_root}"
                                            ));
                                            close = true;
                                        }
                                        Err(e) => {
                                            self.log(format!("Failed to instantiate scene: {e:#}"))
                                        }
                                    }
                                }
                            }
                        });
                }
                ui.add_space(8.0);
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });

        if res.is_none() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }

        if close {
            self.instance_modal = None;
        }
    }
}

fn render_node_hierarchy(
    ui: &mut egui::Ui,
    items: &[NodeTypeInfo],
    search: &str,
    selected_type: &mut String,
    name: &mut String,
    create: &mut bool,
) {
    for item in items {
        if !search.is_empty() && !node_type_matches_search(item, search) {
            continue;
        }

        let is_selected = selected_type == item.type_name;
        let has_children = !item.children.is_empty();

        let row_width = ui.available_width();
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(row_width, 22.0), egui::Sense::click());

        if is_selected {
            ui.painter()
                .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
        } else if resp.hovered() {
            ui.painter()
                .rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
        }

        let p = ui.painter();
        let icon = icons::node_type_icon(item.type_name);
        let color = icons::node_type_color(item.type_name);
        let icon_rect = egui::Rect::from_min_size(
            egui::Pos2::new(rect.left() + 4.0, rect.center().y - 8.0),
            egui::vec2(16.0, 16.0),
        );
        icons::draw_icon(p, icon_rect, icon, color);

        p.text(
            egui::Pos2::new(rect.left() + 24.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            item.type_name,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
            crate::theme::TEXT,
        );

        if resp.clicked() {
            *selected_type = item.type_name.to_string();
            *name = item.type_name.to_string();
        }
        if resp.double_clicked() {
            *selected_type = item.type_name.to_string();
            *name = item.type_name.to_string();
            *create = true;
        }

        if has_children {
            ui.indent(item.type_name, |ui| {
                render_node_hierarchy(ui, &item.children, search, selected_type, name, create);
            });
        }
    }
}
