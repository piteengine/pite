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
                        if ui.text_edit_singleline(&mut v).changed() {
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
