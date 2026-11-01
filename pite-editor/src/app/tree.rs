// SPDX-License-Identifier: MIT OR Apache-2.0
use eframe::egui;
use pite_core::NodeId;

use crate::icons::{self, Icon};
use crate::ops;

use super::EditorApp;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum DropZone {
    None,
    Before,
    Under,
}

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

    pub(super) fn show_add_child(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        let types = ops::registered_types();
        if !types.contains(&self.add_type) {
            self.add_type = types.first().cloned().unwrap_or_default();
        }
        egui::ComboBox::from_label("Type")
            .selected_text(&self.add_type)
            .show_ui(ui, |ui| {
                for t in &types {
                    ui.selectable_value(&mut self.add_type, t.clone(), t);
                }
            });
        ui.text_edit_singleline(&mut self.add_name);
        ui.horizontal(|ui| {
            if ui.button("Add").clicked() {
                let type_name = self.add_type.clone();
                let name = self.add_name.clone();
                let parent = self
                    .selected
                    .clone()
                    .map(NodeId::from)
                    .or_else(|| self.session.host().with_tree(|t| t.root().cloned()));
                match self
                    .session
                    .host()
                    .with_tree_mut(|t| ops::add_node(t, parent, &type_name, &name))
                {
                    Ok(id) => {
                        self.selected = Some(id.to_string());
                        self.add_open = false;
                        self.viewport_dirty = true;
                        self.log(format!("added {id} ({type_name})."));
                    }
                    Err(e) => self.log(format!("add node failed: {e:#}")),
                }
            }
            if ui.button("Cancel").clicked() {
                self.add_open = false;
            }
        });
    }

    pub(super) fn show_node(&mut self, ui: &mut egui::Ui, id: &NodeId) {
        let (name, type_name, children) = self
            .session
            .host()
            .with_tree(|t| {
                t.get(id)
                    .map(|n| (n.name.clone(), n.type_name.clone(), t.children_of(id)))
            })
            .unwrap_or_else(|| ("?".to_string(), "?".to_string(), vec![]));
        let script = self.session.host().with_tree(|t| {
            t.get(id).and_then(|n| {
                n.script
                    .as_ref()
                    .map(|s| (s.class_name.clone(), self.resolve_res(&s.path)))
            })
        });
        let selected = self.selected.as_ref() == Some(&id.to_string());
        let id_str = id.to_string();
        let target = id.clone();
        let is_root = self.session.host().with_tree(|t| t.root() == Some(id));
        let open = self.is_open(&id_str);
        let show_grip = self.grip_hover.as_deref() == Some(id_str.as_str());
        let mut row_resp: Option<egui::Response> = None;
        let mut grip_rect: Option<egui::Rect> = None;
        let hresp = ui.horizontal(|ui| {
            if !is_root {
                let g = ui.dnd_drag_source(
                    egui::Id::new(("tree-grip", id_str.clone())),
                    id_str.clone(),
                    |ui| {
                        ui.allocate_exact_size(
                            egui::Vec2::new(12.0, ui.text_style_height(&egui::TextStyle::Body)),
                            egui::Sense::hover(),
                        )
                    },
                );
                grip_rect = Some(g.inner.0);
            }
            let r = ui.selectable_label(selected, format!("{name} ({type_name})"));
            if let Some((class, file)) = script.as_ref() {
                let (badge, resp) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
                if ui.is_rect_visible(badge) {
                    icons::draw_icon(ui.painter(), badge, Icon::Script, crate::theme::FAINT);
                }
                let resp = resp.on_hover_text(format!("open {class}"));
                if resp.clicked() {
                    let file = file.clone();
                    self.open_asset(&file);
                }
            }
            if r.clicked() {
                self.selected = Some(id_str.clone());
            }
            if !children.is_empty() && r.double_clicked() {
                if open {
                    self.open_nodes.remove(&id_str);
                } else {
                    self.open_nodes.insert(id_str.clone());
                }
            }
            let menu_id = id.clone();
            let menu_open = open;
            let menu_kids = !children.is_empty();
            let _ = r.context_menu(|ui| {
                if ui.button("Delete").clicked() {
                    self.delete_node(&menu_id);
                    ui.close();
                }
                if ui.button("Move up").clicked() {
                    self.move_sibling(&menu_id, -1);
                    ui.close();
                }
                if ui.button("Move down").clicked() {
                    self.move_sibling(&menu_id, 1);
                    ui.close();
                }
                if menu_kids {
                    let label = if menu_open { "Collapse" } else { "Expand" };
                    if ui.button(label).clicked() {
                        if menu_open {
                            self.open_nodes.remove(&menu_id.to_string());
                        } else {
                            self.open_nodes.insert(menu_id.to_string());
                        }
                        ui.close();
                    }
                }
            });
            row_resp = Some(r);
        });
        if hresp.response.contains_pointer() {
            self.grip_hover = Some(id_str.clone());
        } else if self.grip_hover.as_deref() == Some(id_str.as_str()) {
            self.grip_hover = None;
        }
        if show_grip {
            if let (Some(g), Some(r)) = (grip_rect, &row_resp) {
                let p = ui.painter();
                let x = g.center().x;
                let y = r.rect.center().y;
                for row in 0..3 {
                    for col in 0..2 {
                        p.circle_filled(
                            egui::Pos2::new(
                                x - 2.25 + col as f32 * 4.5,
                                y - 4.5 + row as f32 * 4.5,
                            ),
                            1.2,
                            crate::theme::FAINT,
                        );
                    }
                }
            }
        }
        if let Some(r) = &row_resp {
            let dragging = egui::DragAndDrop::has_any_payload(ui.ctx());
            let zone = if dragging && r.contains_pointer() {
                let pos = ui.ctx().pointer_hover_pos().unwrap_or(r.rect.center());
                let frac = (pos.y - r.rect.top()) / r.rect.height().max(1.0);
                if is_root {
                    DropZone::Under
                } else if frac < 0.35 || frac > 0.65 {
                    DropZone::Before
                } else {
                    DropZone::Under
                }
            } else {
                DropZone::None
            };
            let p = ui.painter();
            let edge = egui::Stroke::new(1.5_f32, crate::theme::FAINT);
            match zone {
                DropZone::Before => {
                    p.line_segment(
                        [
                            egui::Pos2::new(r.rect.left(), r.rect.top() + 1.0),
                            egui::Pos2::new(r.rect.right(), r.rect.top() + 1.0),
                        ],
                        edge,
                    );
                }
                DropZone::Under => {
                    p.rect_stroke(r.rect, 3.0, edge, egui::StrokeKind::Middle);
                }
                DropZone::None => {}
            }
            if let Some(dragged) = r.dnd_release_payload::<String>() {
                self.drop_node(dragged.as_str(), &target, zone);
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
}
