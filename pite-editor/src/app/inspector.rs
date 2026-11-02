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
    }
}
