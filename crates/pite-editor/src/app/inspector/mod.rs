// SPDX-License-Identifier: MIT OR Apache-2.0
mod atlas;
mod atlas_ui;
mod props;
mod sections;

pub(in crate::app) use atlas::AtlasEditorState;

use eframe::egui;
use pite_core::{NodeId, PropValue};

use crate::icons;
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

    pub(super) fn remove_prop(&mut self, id: &NodeId, key: &str) {
        let result = self
            .session
            .host()
            .with_tree_mut(|t| ops::remove_prop(t, id, key));
        match result {
            Ok(()) => self.viewport_dirty = true,
            Err(e) => self.log(format!("remove prop failed: {e:#}")),
        }
    }

    pub(super) fn show_inspector(&mut self, ui: &mut egui::Ui, id: &str) {
        let node_id = NodeId::from(id.to_string());
        let snapshot = self.session.host().with_tree(|t| t.get(&node_id).cloned());
        let Some(node) = snapshot else {
            ui.label("Node no longer exists.");
            return;
        };

        let panel_w = ui.available_width();
        ui.set_max_width(panel_w);
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);

        // 1. Node Header with type icon, working inline rename edit, and type badge
        let schemas = self.session.schemas().clone();
        let icon = icons::node_type_icon_in(&schemas, &node.type_name);
        let color = icons::node_type_color_in(&schemas, &node.type_name);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
            ui.spacing_mut().interact_size.y = 28.0;
            ui.label(egui::RichText::new(icon.as_str()).color(color).size(16.0));

            let rename_id = ui.make_persistent_id(format!("rename_{}", id));
            let mut current_text = ui.data_mut(|d| {
                d.get_temp::<String>(rename_id)
                    .unwrap_or_else(|| node.name.clone())
            });

            let type_font = egui::FontId::new(11.0, egui::FontFamily::Proportional);
            let type_galley =
                ui.painter()
                    .layout_no_wrap(node.type_name.clone(), type_font, crate::theme::FAINT);
            let type_w = type_galley.size().x;

            let remaining_w = ui.available_width();
            let name_w = (remaining_w - type_w - 12.0).max(40.0);

            let name_resp = ui.add(
                egui::TextEdit::singleline(&mut current_text)
                    .desired_width(name_w)
                    .min_size(egui::vec2(name_w, 28.0))
                    .font(egui::FontId::new(13.0, egui::FontFamily::Proportional)),
            );

            if name_resp.changed() {
                ui.data_mut(|d| d.insert_temp(rename_id, current_text.clone()));
            }

            let enter_pressed =
                name_resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if name_resp.lost_focus() || enter_pressed {
                let trimmed = current_text.trim();
                if !trimmed.is_empty() && trimmed != node.name {
                    let _ = self
                        .session
                        .host()
                        .with_tree_mut(|t| ops::rename_node(t, &node_id, trimmed));
                    self.viewport_dirty = true;
                }
                ui.data_mut(|d| d.remove_temp::<String>(rename_id));
            }

            ui.label(
                egui::RichText::new(&node.type_name)
                    .color(crate::theme::FAINT)
                    .size(11.0),
            );
        });

        ui.add_space(2.0);

        self.show_script_card(ui, &node_id, &node);

        ui.add_space(4.0);

        let handled_keys = self.show_inherited_props(ui, id, &node_id, &node);
        self.show_custom_props(ui, id, &node_id, &node, &handled_keys);

        if node.type_name == "TileMap2D" {
            self.show_tilemap_inspector(ui, &node_id);
        }

        let snapshot = self.editor_snapshot();
        let (errors, actions) = self.editor_plugins.draw_inspector_sections(
            ui,
            snapshot,
            node.id.as_str(),
            &node.type_name,
        );
        self.apply_plugin_feedback(errors, actions);
    }
}
