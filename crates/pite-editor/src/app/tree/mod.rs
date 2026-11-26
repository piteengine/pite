// SPDX-License-Identifier: MIT OR Apache-2.0
mod modals;
mod node_types;

#[cfg(test)]
pub(super) use node_types::build_node_hierarchy_from;
pub(super) use node_types::{
    AddNodeModal, DropZone, InstanceSceneModal, NodeDndPayload, RenameNodeModal,
};

use std::path::Path;

use eframe::egui;
use pite_core::NodeId;

use crate::icons::{self, Icon};
use crate::ops;

use super::EditorApp;

impl EditorApp {
    pub(super) fn delete_node(&mut self, id: &NodeId) {
        match self
            .session
            .host()
            .with_tree_mut(|t| ops::remove_node(t, id))
        {
            Ok(()) => {
                if self.selected.as_deref() == Some(id.as_str()) {
                    self.selected = None;
                }
                self.viewport_dirty = true;
                self.log(format!("deleted {id}."));
            }
            Err(e) => self.log(format!("delete failed: {e:#}")),
        }
    }

    pub(super) fn move_sibling(&mut self, id: &NodeId, delta: i32) {
        match self
            .session
            .host()
            .with_tree_mut(|t| ops::move_sibling(t, id, delta))
        {
            Ok(()) => {
                self.viewport_dirty = true;
                self.log(format!("moved {id}."));
            }
            Err(e) => self.log(format!("move failed: {e:#}")),
        }
    }

    pub(super) fn duplicate_selected_node(&mut self) {
        let Some(sel) = self.selected.clone().map(NodeId::from) else {
            return;
        };
        let is_root = self.session.host().with_tree(|t| t.root() == Some(&sel));
        if is_root {
            self.log("Cannot duplicate the root node.");
            return;
        }
        match self
            .session
            .host()
            .with_tree_mut(|t| ops::duplicate_node(t, &sel))
        {
            Ok(new_id) => {
                self.selected = Some(new_id.to_string());
                self.viewport_dirty = true;
                self.log(format!("Duplicated {sel} -> {new_id}"));
            }
            Err(e) => self.log(format!("Duplicate failed: {e:#}")),
        }
    }

    pub(super) fn open_add_node_modal(&mut self, parent: Option<NodeId>) {
        self.add_node_modal = Some(AddNodeModal {
            parent,
            selected_type: "Node2D".to_string(),
            name: "Node2D".to_string(),
            search: String::new(),
        });
    }

    pub(super) fn open_rename_node_modal(&mut self, id: NodeId) {
        let current_name = self
            .session
            .host()
            .with_tree(|t| t.get(&id).map(|n| n.name.clone()))
            .unwrap_or_else(|| id.to_string());
        self.rename_node_modal = Some(RenameNodeModal {
            id,
            name: current_name,
        });
    }

    pub(super) fn open_instance_modal(&mut self, parent: Option<NodeId>) {
        self.instance_modal = Some(InstanceSceneModal { parent });
    }

    pub(super) fn drop_node(&mut self, dragged: &str, target: &NodeId, zone: DropZone) {
        let dragged_id = NodeId::from(dragged);
        if dragged_id == *target || zone == DropZone::None {
            return;
        }
        let result = self.session.host().with_tree_mut(|t| match zone {
            DropZone::Under => ops::move_node(t, &dragged_id, Some(target.clone())),
            DropZone::Before => {
                let parent = t
                    .get(target)
                    .and_then(|n| n.parent.clone())
                    .ok_or_else(|| anyhow::anyhow!("cannot reorder at the top level"))?;
                let sibs = t.children_of(&parent);
                let pos = sibs
                    .iter()
                    .position(|s| s == target)
                    .ok_or_else(|| anyhow::anyhow!("target `{target}` not found"))?;
                ops::place_node(t, &dragged_id, &parent, pos)
            }
            DropZone::After => {
                let parent = t
                    .get(target)
                    .and_then(|n| n.parent.clone())
                    .ok_or_else(|| anyhow::anyhow!("cannot reorder at the top level"))?;
                let sibs = t.children_of(&parent);
                let pos = sibs
                    .iter()
                    .position(|s| s == target)
                    .ok_or_else(|| anyhow::anyhow!("target `{target}` not found"))?;
                ops::place_node(t, &dragged_id, &parent, pos + 1)
            }
            DropZone::None => Ok(()),
        });
        match result {
            Ok(()) => {
                self.viewport_dirty = true;
                self.log(format!("moved {dragged_id}."));
            }
            Err(e) => self.log(format!("move failed: {e:#}")),
        }
    }

    pub(super) fn show_node(&mut self, ui: &mut egui::Ui, id: &NodeId) {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 1.0);
        ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
        ui.spacing_mut().indent = 14.0;

        let (name, type_name, children, parent_id) = self
            .session
            .host()
            .with_tree(|t| {
                t.get(id).map(|n| {
                    (
                        n.name.clone(),
                        n.type_name.clone(),
                        t.children_of(id),
                        n.parent.clone(),
                    )
                })
            })
            .unwrap_or_else(|| ("?".to_string(), "?".to_string(), vec![], None));

        let script = self.session.host().with_tree(|t| {
            t.get(id).and_then(|n| {
                n.script.as_ref().and_then(|s| {
                    self.resolve_res(&s.path)
                        .map(|path| (s.class_name.clone(), path))
                })
            })
        });

        let selected = self.selected.as_ref() == Some(&id.to_string());
        let id_str = id.to_string();
        let target = id.clone();
        let is_root = self.session.host().with_tree(|t| t.root() == Some(id));
        let open = self.is_open(&id_str);

        // Check if this node is the first child of its parent branch
        let is_first_child = if let Some(parent) = &parent_id {
            self.session
                .host()
                .with_tree(|t| t.children_of(parent).first() == Some(id))
        } else {
            false
        };

        let row_width = ui.available_width().max(160.0);
        let (rect, row_resp) =
            ui.allocate_exact_size(egui::vec2(row_width, 20.0), egui::Sense::click_and_drag());

        // Start drag when threshold passed
        if !is_root && row_resp.drag_started() {
            egui::DragAndDrop::set_payload(
                ui.ctx(),
                NodeDndPayload {
                    id: id_str.clone(),
                    name: name.clone(),
                    type_name: type_name.clone(),
                },
            );
        }

        let has_node_dnd = egui::DragAndDrop::has_payload_of_type::<NodeDndPayload>(ui.ctx());
        let has_asset_dnd =
            egui::DragAndDrop::has_payload_of_type::<super::assets::AssetDndPayload>(ui.ctx());

        let is_source =
            if let Some(dragged) = egui::DragAndDrop::payload::<NodeDndPayload>(ui.ctx()) {
                dragged.id == id_str
            } else {
                false
            };

        // Check cycle: cannot drop into self or descendant
        let is_descendant_of_dragged =
            if let Some(dragged) = egui::DragAndDrop::payload::<NodeDndPayload>(ui.ctx()) {
                let dragged_id = NodeId::from(dragged.id.as_str());
                self.session.host().with_tree(|t| {
                    let mut curr = parent_id.clone();
                    while let Some(c) = curr {
                        if c == dragged_id {
                            return true;
                        }
                        curr = t.get(&c).and_then(|n| n.parent.clone());
                    }
                    false
                })
            } else {
                false
            };

        let is_node_target = has_node_dnd && !is_source && row_resp.contains_pointer();
        let is_valid_node_target = is_node_target && !is_descendant_of_dragged;
        let is_invalid_node_target = is_node_target && is_descendant_of_dragged;

        // Drop zones calculation (3-2 method requested by user):
        // Center: Child (DropZone::Under)
        // Bottom edge (> 0.75): Sibling After (DropZone::After)
        // Top edge (< 0.25, ONLY for the first child of a branch): Sibling Before (DropZone::Before)
        let zone = if is_valid_node_target {
            let pointer_pos = ui
                .ctx()
                .input(|i| i.pointer.latest_pos())
                .or_else(|| ui.ctx().pointer_interact_pos())
                .unwrap_or(rect.center());
            let frac = (pointer_pos.y - rect.top()) / rect.height().max(1.0);
            if is_root {
                DropZone::Under
            } else if is_first_child && frac < 0.25 {
                DropZone::Before
            } else if frac > 0.75 {
                DropZone::After
            } else {
                DropZone::Under
            }
        } else {
            DropZone::None
        };

        if is_valid_node_target && zone != DropZone::None {
            self.hovered_node_target = Some((name.clone(), zone));
        }

        // Asset drop target calculation
        let is_asset_target = has_asset_dnd && row_resp.contains_pointer();
        let is_valid_asset_target = if let Some(payload) =
            egui::DragAndDrop::payload::<super::assets::AssetDndPayload>(ui.ctx())
        {
            if payload.rel_path.ends_with(".py") {
                true
            } else if payload.rel_path.ends_with(".ptatlas") {
                type_name == "Sprite2D" || type_name == "AnimatedSprite2D"
            } else if payload.rel_path.ends_with(".png") {
                type_name == "Sprite2D"
            } else {
                false
            }
        } else {
            false
        };
        let is_invalid_asset_target = is_asset_target && !is_valid_asset_target;

        if is_asset_target && is_valid_asset_target {
            self.hovered_node_target = Some((format!("Node '{name}'"), DropZone::Under));
        }

        // Clicks
        let mut toggled = false;
        if row_resp.clicked() {
            if let Some(pos) = row_resp.interact_pointer_pos() {
                if pos.x < rect.left() + 16.0 && !children.is_empty() {
                    toggled = true;
                } else {
                    self.selected = Some(id_str.clone());
                }
            } else {
                self.selected = Some(id_str.clone());
            }
        }
        if row_resp.double_clicked() {
            if !children.is_empty() {
                toggled = true;
            } else {
                self.open_rename_node_modal(id.clone());
            }
        }

        // Drops from node dragging
        if let Some(dragged) = row_resp.dnd_release_payload::<NodeDndPayload>() {
            if is_valid_node_target {
                self.drop_node(&dragged.id, &target, zone);
            }
        }

        // Drops from asset panel (attaching script or assigning texture)
        if let Some(payload) = row_resp.dnd_release_payload::<super::assets::AssetDndPayload>() {
            if is_valid_asset_target {
                if payload.rel_path.ends_with(".py") {
                    let class_name = Path::new(&payload.rel_path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Script")
                        .to_string();
                    let res_path = format!("res://{}", payload.rel_path);
                    self.session.host().with_tree_mut(|t| {
                        if let Some(n) = t.get_mut(&target) {
                            n.script = Some(pite_core::ScriptRef {
                                path: res_path.clone(),
                                class_name: class_name.clone(),
                            });
                        }
                    });
                    self.viewport_dirty = true;
                    self.log(format!("Attached script {res_path} to {target}"));
                } else if payload.rel_path.ends_with(".png")
                    || payload.rel_path.ends_with(".ptatlas")
                {
                    let res_path = format!("res://{}", payload.rel_path);
                    self.session.host().with_tree_mut(|t| {
                        if let Some(n) = t.get_mut(&target) {
                            if n.type_name == "Sprite2D" {
                                if payload.rel_path.ends_with(".ptatlas") {
                                    n.props.insert(
                                        "atlas".to_string(),
                                        pite_core::PropValue::Str(res_path.clone()),
                                    );
                                } else {
                                    n.props.insert(
                                        "texture".to_string(),
                                        pite_core::PropValue::Str(res_path.clone()),
                                    );
                                }
                            } else if n.type_name == "AnimatedSprite2D"
                                && payload.rel_path.ends_with(".ptatlas")
                            {
                                n.props.insert(
                                    "atlas".to_string(),
                                    pite_core::PropValue::Str(res_path.clone()),
                                );
                            }
                        }
                    });
                    self.viewport_dirty = true;
                    self.log(format!("Assigned {res_path} to {target}"));
                }
            }
        }

        // Background and Drop Target Indicators painting
        let p = ui.painter();
        if zone == DropZone::Under || (is_asset_target && is_valid_asset_target) {
            p.rect_filled(
                rect,
                2.0,
                egui::Color32::from_rgba_unmultiplied(11, 147, 135, 45),
            );
            p.rect_stroke(
                rect,
                2.0,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(11, 147, 135)),
                egui::StrokeKind::Middle,
            );
        } else if is_invalid_node_target || is_invalid_asset_target {
            p.rect_filled(
                rect,
                2.0,
                egui::Color32::from_rgba_unmultiplied(220, 80, 80, 30),
            );
            p.rect_stroke(
                rect,
                2.0,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(220, 80, 80)),
                egui::StrokeKind::Middle,
            );
        } else if is_source && has_node_dnd {
            p.rect_filled(
                rect,
                2.0,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 12),
            );
        } else if selected {
            p.rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
        } else if row_resp.hovered() && !has_node_dnd && !has_asset_dnd {
            p.rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
        }

        // Insertion line indicators for Before and After
        if zone == DropZone::Before {
            let y = rect.top();
            p.line_segment(
                [
                    egui::Pos2::new(rect.left(), y),
                    egui::Pos2::new(rect.right(), y),
                ],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(11, 147, 135)),
            );
            p.circle_filled(
                egui::Pos2::new(rect.left() + 4.0, y),
                3.0,
                egui::Color32::from_rgb(11, 147, 135),
            );
        } else if zone == DropZone::After {
            let y = rect.bottom();
            p.line_segment(
                [
                    egui::Pos2::new(rect.left(), y),
                    egui::Pos2::new(rect.right(), y),
                ],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(11, 147, 135)),
            );
            p.circle_filled(
                egui::Pos2::new(rect.left() + 4.0, y),
                3.0,
                egui::Color32::from_rgb(11, 147, 135),
            );
        }

        // Caret arrow
        if !children.is_empty() {
            let caret_icon = if open {
                Icon::CaretDown
            } else {
                Icon::CaretRight
            };
            let caret_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.left() + 2.0, rect.center().y - 6.0),
                egui::vec2(12.0, 12.0),
            );
            icons::draw_icon(p, caret_rect, caret_icon, crate::theme::FAINT);
        }

        // Dedicated Node Type Icon
        let schemas = self.session.schemas().clone();
        let node_icon = icons::node_type_icon_in(&schemas, &type_name);
        let node_color = icons::node_type_color_in(&schemas, &type_name);
        let icon_rect = egui::Rect::from_min_size(
            egui::Pos2::new(rect.left() + 16.0, rect.center().y - 8.0),
            egui::vec2(16.0, 16.0),
        );
        icons::draw_icon(p, icon_rect, node_icon, node_color);

        // Node name and type text
        let text_pos = egui::Pos2::new(rect.left() + 36.0, rect.center().y);
        let label_text = &name;
        p.text(
            text_pos,
            egui::Align2::LEFT_CENTER,
            label_text,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
            crate::theme::TEXT,
        );

        let description = schemas
            .get(&type_name)
            .map(|s| s.description.as_ref())
            .unwrap_or("");
        let mut hover_info = String::new();
        if !description.is_empty() {
            hover_info.push_str(description);
            hover_info.push_str("\n\n");
        }
        hover_info.push_str(&format!("Type: {type_name}"));
        if let Some((class, _)) = &script {
            hover_info.push_str(&format!("\nScript: {class}"));
        }
        let row_resp = row_resp.on_hover_text(hover_info);

        // Script badge on right if script is attached
        if let Some((class, file)) = script.as_ref() {
            let badge_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.right() - 22.0, rect.center().y - 8.0),
                egui::vec2(16.0, 16.0),
            );
            let badge_resp = ui.interact(
                badge_rect,
                egui::Id::unique(("script-badge", id_str.clone())),
                egui::Sense::click(),
            );
            let is_badge_hovered = badge_resp.hovered();
            let fg = if is_badge_hovered {
                crate::theme::TEXT
            } else {
                crate::theme::FAINT
            };
            if is_badge_hovered {
                p.rect_filled(badge_rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
            }
            icons::draw_icon(p, badge_rect, Icon::Script, fg);
            if badge_resp
                .on_hover_text(format!("Open script: {class}"))
                .clicked()
            {
                let file = file.clone();
                self.open_asset(&file);
            }
        }

        // Context Menu
        let menu_id = id.clone();
        let menu_kids = !children.is_empty();
        let menu_open = open;
        let has_script = script.is_some();
        row_resp.context_menu(|ui| {
            let mut menu = super::MenuSeparatorTracker::new(ui);
            if menu.button("Add Child Node…  Ctrl+A").clicked() {
                self.open_add_node_modal(Some(menu_id.clone()));
                menu.close();
            }
            if menu
                .button("Instantiate Child Scene…  Ctrl+Shift+A")
                .clicked()
            {
                self.open_instance_modal(Some(menu_id.clone()));
                menu.close();
            }

            menu.start_section();
            if !is_root && menu.button("Duplicate  Ctrl+D").clicked() {
                match self
                    .session
                    .host()
                    .with_tree_mut(|t| ops::duplicate_node(t, &menu_id))
                {
                    Ok(new_id) => {
                        self.selected = Some(new_id.to_string());
                        self.viewport_dirty = true;
                        self.log(format!("Duplicated {menu_id} -> {new_id}"));
                    }
                    Err(e) => self.log(format!("Duplicate failed: {e:#}")),
                }
                menu.close();
            }
            if menu.button("Rename…  F2").clicked() {
                self.open_rename_node_modal(menu_id.clone());
                menu.close();
            }
            if !is_root && menu.button("Delete  Delete").clicked() {
                self.delete_node(&menu_id);
                menu.close();
            }

            if !is_root {
                menu.start_section();
                if menu.button("Move Up  Ctrl+Up").clicked() {
                    self.move_sibling(&menu_id, -1);
                    menu.close();
                }
                if menu.button("Move Down  Ctrl+Down").clicked() {
                    self.move_sibling(&menu_id, 1);
                    menu.close();
                }
            }

            if has_script {
                menu.start_section();
                if menu.button("Open Script").clicked() {
                    if let Some((_, file)) = script.as_ref() {
                        let file = file.clone();
                        self.open_asset(&file);
                    }
                    menu.close();
                }
                if menu.button("Detach Script").clicked() {
                    self.session.host().with_tree_mut(|t| {
                        if let Some(n) = t.get_mut(&menu_id) {
                            n.script = None;
                        }
                    });
                    self.viewport_dirty = true;
                    self.log(format!("Detached script from {menu_id}"));
                    menu.close();
                }
            }

            if menu_kids {
                menu.start_section();
                let label = if menu_open { "Collapse" } else { "Expand" };
                if menu.button(label).clicked() {
                    if menu_open {
                        self.open_nodes.remove(&menu_id.to_string());
                    } else {
                        self.open_nodes.insert(menu_id.to_string());
                    }
                    menu.close();
                }
            }
        });

        if toggled {
            if open {
                self.open_nodes.remove(&id_str);
            } else {
                self.open_nodes.insert(id_str.clone());
            }
        }

        if open && !children.is_empty() {
            ui.indent(id_str, |ui| {
                for child in children {
                    self.show_node(ui, &child);
                }
            });
        }
    }

    pub(super) fn show_node_drag_preview(&self, ctx: &egui::Context) {
        let Some(payload) = egui::DragAndDrop::payload::<NodeDndPayload>(ctx) else {
            return;
        };
        let pointer_pos = ctx
            .input(|i| i.pointer.latest_pos())
            .or_else(|| ctx.pointer_interact_pos());
        let Some(pos) = pointer_pos else {
            return;
        };

        egui::Area::new(egui::Id::unique("node_drag_preview"))
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
                        let icon = icons::node_type_icon(&payload.type_name);
                        let color = icons::node_type_color(&payload.type_name);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(icon.as_str()).color(color));
                            ui.label(egui::RichText::new(&payload.name).strong());
                            if let Some((target_name, zone)) = &self.hovered_node_target {
                                match zone {
                                    DropZone::Under => {
                                        ui.label(
                                            egui::RichText::new(format!("-> Inside {target_name}"))
                                                .color(egui::Color32::from_rgb(11, 147, 135)),
                                        );
                                    }
                                    DropZone::Before => {
                                        ui.label(
                                            egui::RichText::new(format!("-> Before {target_name}"))
                                                .color(egui::Color32::from_rgb(11, 147, 135)),
                                        );
                                    }
                                    DropZone::After => {
                                        ui.label(
                                            egui::RichText::new(format!("-> After {target_name}"))
                                                .color(egui::Color32::from_rgb(11, 147, 135)),
                                        );
                                    }
                                    DropZone::None => {}
                                }
                            }
                        });
                    });
            });
    }
}
