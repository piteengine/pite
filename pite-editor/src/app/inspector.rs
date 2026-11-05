// SPDX-License-Identifier: MIT OR Apache-2.0
use eframe::egui;
use pite_core::{NodeId, PropValue};

use crate::ops;

use super::EditorApp;

impl EditorApp {
    pub(super) fn set_prop(&mut self, id: &NodeId, key: &str, value: PropValue) {
        let result = self
            .session
            .host()
            .with_tree_mut(|t| ops::set_prop(t, id, key, value));
        match result {
            Ok(()) => self.viewport_dirty = true,
            Err(e) => self.log(format!("set prop failed: {e:#}")),
        }
    }

    pub(super) fn show_inspector(&mut self, ui: &mut egui::Ui, id: &str) {
        let node_id = NodeId::from(id.to_string());
        let snapshot = self.session.host().with_tree(|t| t.get(&node_id).cloned());
        let Some(node) = snapshot else {
            ui.label("Node no longer exists.");
            return;
        };
        ui.label(format!("{} ({})", node.name, node.type_name));
        let mut keys: Vec<String> = node.props.0.keys().cloned().collect();
        keys.sort();
        for key in keys {
            let value = node
                .props
                .get(&key)
                .cloned()
                .unwrap_or(PropValue::Str(String::new()));
            match value {
                PropValue::Num(f) => {
                    let mut v = f;
                    ui.horizontal(|ui| {
                        ui.label(&key);
                        if ui.add(egui::DragValue::new(&mut v).speed(0.1)).changed() {
                            self.set_prop(&node_id, &key, PropValue::Num(v));
                        }
                    });
                }
                PropValue::Int(i) => {
                    let mut v = i;
                    ui.horizontal(|ui| {
                        ui.label(&key);
                        if ui.add(egui::DragValue::new(&mut v)).changed() {
                            self.set_prop(&node_id, &key, PropValue::Int(v));
                        }
                    });
                }
                PropValue::Str(s) => {
                    let mut v = s;
                    ui.horizontal(|ui| {
                        ui.label(&key);
                        if key == "shape" && node.type_name == "CollisionShape2D" {
                            let mut selected = v.clone();
                            egui::ComboBox::from_id_salt("shape_select")
                                .selected_text(&selected)
                                .show_ui(ui, |ui| {
                                    for opt in ["box", "circle", "capsule"] {
                                        if ui
                                            .selectable_value(&mut selected, opt.to_string(), opt)
                                            .changed()
                                        {
                                            self.set_prop(
                                                &node_id,
                                                &key,
                                                PropValue::Str(selected.clone()),
                                            );
                                        }
                                    }
                                });
                        } else if ui.text_edit_singleline(&mut v).changed() {
                            self.set_prop(&node_id, &key, PropValue::Str(v));
                        }
                    });
                }
                PropValue::Bool(b) => {
                    let mut v = b;
                    ui.horizontal(|ui| {
                        ui.label(&key);
                        if ui.checkbox(&mut v, "").changed() {
                            self.set_prop(&node_id, &key, PropValue::Bool(v));
                        }
                    });
                }
                PropValue::Vec2(x, y) => {
                    let mut v = (x, y);
                    ui.horizontal(|ui| {
                        ui.label(&key);
                        if ui.add(egui::DragValue::new(&mut v.0).speed(1.0)).changed()
                            || ui.add(egui::DragValue::new(&mut v.1).speed(1.0)).changed()
                        {
                            self.set_prop(&node_id, &key, PropValue::Vec2(v.0, v.1));
                        }
                    });
                }
            }
        }
        let known_props: &[(&str, PropValue)] = match node.type_name.as_str() {
            "StaticBody2D" | "Area2D" => &[("position", PropValue::Vec2(0.0, 0.0))],
            "CharacterBody2D" => &[
                ("position", PropValue::Vec2(0.0, 0.0)),
                ("velocity", PropValue::Vec2(0.0, 0.0)),
            ],
            "RigidBody2D" => &[
                ("position", PropValue::Vec2(0.0, 0.0)),
                ("mass", PropValue::Num(1.0)),
                ("gravity_scale", PropValue::Num(1.0)),
                ("lock_rotation", PropValue::Bool(false)),
            ],
            "CollisionShape2D" => &[
                ("position", PropValue::Vec2(0.0, 0.0)),
                ("shape", PropValue::Str("box".to_string())),
                ("size", PropValue::Vec2(32.0, 32.0)),
                ("radius", PropValue::Num(16.0)),
                ("height", PropValue::Num(32.0)),
                ("disabled", PropValue::Bool(false)),
            ],
            "TileMap2D" => &[
                ("position", PropValue::Vec2(0.0, 0.0)),
                ("tilemap", PropValue::Str(String::new())),
            ],
            _ => &[],
        };
        let missing: Vec<(&str, PropValue)> = known_props
            .iter()
            .filter(|(k, _)| !node.props.0.contains_key(*k))
            .cloned()
            .collect();
        if !missing.is_empty() {
            ui.add_space(4.0);
            ui.label("Add property:");
            ui.horizontal_wrapped(|ui| {
                for (k, default_val) in missing {
                    if ui.button(format!("+ {k}")).clicked() {
                        self.set_prop(&node_id, k, default_val);
                    }
                }
            });
        }

        match &node.script {
            Some(script) => {
                ui.label(format!("script: {} ({})", script.path, script.class_name));
                let script_path = script.path.clone();
                if ui.button("Open script").clicked() {
                    let resolved = self.resolve_res(&script_path);
                    self.open_asset(&resolved);
                }
            }
            None => {
                ui.label("no script attached.");
            }
        }

        if node.type_name == "TileMap2D" {
            self.show_tilemap_inspector(ui, &node_id);
        }
    }

    fn show_tilemap_inspector(&mut self, ui: &mut egui::Ui, node_id: &NodeId) {
        let doc_opt = self.session.host().get_tilemap_doc(node_id.as_str());
        let Some(mut doc) = doc_opt else {
            ui.add_space(8.0);
            ui.label("No tilemap loaded. Set `tilemap` property to a valid .ptmap file.");
            return;
        };

        ui.add_space(8.0);
        ui.separator();
        ui.heading("Tilemap Editor");

        let mut changed = false;

        // Layer selection
        ui.horizontal(|ui| {
            ui.label("Active Layer:");
            let layer_count = doc.layers.len();
            if layer_count == 0 {
                ui.label("None");
            } else {
                if self.tilemap_active_layer >= layer_count {
                    self.tilemap_active_layer = 0;
                }
                let current_name = doc.layers[self.tilemap_active_layer].name.clone();
                egui::ComboBox::from_id_salt("tilemap_layers")
                    .selected_text(format!("[{}] {}", self.tilemap_active_layer, current_name))
                    .show_ui(ui, |ui| {
                        for (i, layer) in doc.layers.iter().enumerate() {
                            ui.selectable_value(
                                &mut self.tilemap_active_layer,
                                i,
                                format!("[{i}] {}", layer.name),
                            );
                        }
                    });
            }
        });

        // Layer properties
        if let Some(layer) = doc.layers.get_mut(self.tilemap_active_layer) {
            ui.horizontal(|ui| {
                if ui.checkbox(&mut layer.visible, "Visible").changed() {
                    changed = true;
                    self.viewport_dirty = true;
                }
                if ui
                    .checkbox(&mut layer.collision_enabled, "Collisions")
                    .changed()
                {
                    changed = true;
                    self.session.rebuild_physics();
                    self.viewport_dirty = true;
                }
            });
        }

        // Tool toggles: Brush vs Eraser
        ui.horizontal(|ui| {
            let brush_active = !self.tilemap_eraser && self.tilemap_selected_frame.is_some();
            if ui.selectable_label(brush_active, "🖌 Brush").clicked() {
                self.tilemap_eraser = false;
            }
            if ui
                .selectable_label(self.tilemap_eraser, "🧹 Eraser")
                .clicked()
            {
                self.tilemap_eraser = true;
            }
        });

        if let Some(frame) = &self.tilemap_selected_frame {
            ui.label(format!("Brush tile: {frame}"));
        } else if self.tilemap_eraser {
            ui.label("Brush: Erase mode");
        } else {
            ui.label("Select a tile below to paint");
        }

        // Palette
        ui.add_space(4.0);
        ui.label("Tile Palette:");
        let sidecar_path = self.resolve_res(&doc.atlas);
        if let Ok(atlas) = pite_render::atlas::load_atlas(&sidecar_path) {
            egui::ScrollArea::vertical()
                .id_salt("tile_palette")
                .max_height(140.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for name in atlas.names() {
                            let selected = !self.tilemap_eraser
                                && self.tilemap_selected_frame.as_deref() == Some(name);
                            if ui.selectable_label(selected, name).clicked() {
                                self.tilemap_selected_frame = Some(name.to_string());
                                self.tilemap_eraser = false;
                            }
                        }
                    });
                });
        } else {
            ui.label(format!("Cannot load atlas: {}", doc.atlas));
        }

        if changed {
            let node_path = self.session.host().with_tree(|t| {
                t.get(node_id).and_then(|n| match n.props.get("tilemap") {
                    Some(pite_core::PropValue::Str(s)) if !s.is_empty() => Some(s.clone()),
                    _ => None,
                })
            });
            if let Some(map_ref) = node_path {
                let fs_path = self.resolve_res(&map_ref);
                let _ = pite_scene::save_tilemap(&fs_path, &doc);
            }
            self.session.host().set_tilemap_doc(node_id.as_str(), doc);
        }
    }
}
