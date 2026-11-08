// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::HashSet;
use std::f64::consts::PI;

use eframe::egui;
use egui_phosphor::regular;
use pite_core::schema::{self, PropHint};
use pite_core::{NodeId, PropValue};

use crate::icons;
use crate::ops;

use super::EditorApp;

fn parse_hex_color(hex: &str) -> Option<egui::Color32> {
    let s = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    let r = u8::from_str_radix(s.get(0..2)?, 16).ok()?;
    let g = u8::from_str_radix(s.get(2..4)?, 16).ok()?;
    let b = u8::from_str_radix(s.get(4..6)?, 16).ok()?;
    let a = if s.len() >= 8 {
        u8::from_str_radix(s.get(6..8)?, 16).ok()?
    } else {
        255
    };
    Some(egui::Color32::from_rgba_unmultiplied(r, g, b, a))
}

fn format_hex_color(c: egui::Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a)
}

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

    fn pick_asset_dialog(&self, filter: &[&str]) -> Option<String> {
        let mut dialog = rfd::FileDialog::new().add_filter("Asset", filter);
        if let Some(dir) = self.project_dir.as_ref() {
            let assets_dir = dir.join("assets");
            if assets_dir.is_dir() {
                dialog = dialog.set_directory(&assets_dir);
            } else {
                dialog = dialog.set_directory(dir);
            }
        }
        let path = dialog.pick_file()?;
        let res_path = if let Some(dir) = self.project_dir.as_ref() {
            if let Ok(rel) = path.strip_prefix(dir) {
                format!("res://{}", rel.to_string_lossy().replace('\\', "/"))
            } else {
                format!(
                    "res://{}",
                    path.file_name().and_then(|s| s.to_str()).unwrap_or("asset")
                )
            }
        } else {
            format!(
                "res://{}",
                path.file_name().and_then(|s| s.to_str()).unwrap_or("asset")
            )
        };
        Some(res_path)
    }

    fn attach_or_create_script_dialog(
        &mut self,
        node_id: &NodeId,
        node_name: &str,
        type_name: &str,
    ) {
        let default_file = format!("{}.py", node_name.to_lowercase().replace(' ', "_"));
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Python Script", &["py"])
            .set_file_name(&default_file);
        if let Some(dir) = self.project_dir.as_ref() {
            let scripts_dir = dir.join("scripts");
            if scripts_dir.is_dir() {
                dialog = dialog.set_directory(&scripts_dir);
            } else {
                dialog = dialog.set_directory(dir);
            }
        }
        let Some(path) = dialog.save_file() else {
            return;
        };
        let path = if path.extension().and_then(|e| e.to_str()) != Some("py") {
            path.with_extension("py")
        } else {
            path
        };
        let class_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| {
                let mut c = String::new();
                for part in s.split('_') {
                    let mut chars = part.chars();
                    if let Some(first) = chars.next() {
                        c.extend(first.to_uppercase());
                        c.push_str(chars.as_str());
                    }
                }
                if c.is_empty() {
                    "Script".to_string()
                } else {
                    c
                }
            })
            .unwrap_or_else(|| "Script".to_string());

        if !path.exists() {
            let template = format!(
                "import pite\n\n\nclass {class_name}(pite.{type_name}):\n    def _ready(self):\n        pass\n\n    def _process(self, delta: float):\n        pass\n"
            );
            if let Err(e) = std::fs::write(&path, template) {
                self.log(format!("create script failed: {e:#}"));
                return;
            }
        }

        let res_path = if let Some(dir) = self.project_dir.as_ref() {
            if let Ok(rel) = path.strip_prefix(dir) {
                format!("res://{}", rel.to_string_lossy().replace('\\', "/"))
            } else {
                format!(
                    "res://{}",
                    path.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("script.py")
                )
            }
        } else {
            format!(
                "res://{}",
                path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("script.py")
            )
        };

        self.session.host().with_tree_mut(|t| {
            if let Some(n) = t.get_mut(node_id) {
                n.script = Some(pite_core::ScriptRef {
                    path: res_path.clone(),
                    class_name,
                });
            }
        });
        self.viewport_dirty = true;
        self.open_asset(&path);
        self.log(format!("Attached script {res_path} to {node_id}"));
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
        let icon = icons::node_type_icon(&node.type_name);
        let color = icons::node_type_color(&node.type_name);

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

        // 2. Script Card
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(20, 24, 29))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 49, 56)))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                    ui.label(egui::RichText::new(regular::FILE_CODE).color(crate::theme::FAINT));
                    ui.label(egui::RichText::new("Script").strong());

                    if node.script.is_some() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let detach_btn = egui::Button::new(
                                egui::RichText::new("✕")
                                    .size(13.0)
                                    .color(crate::theme::FAINT),
                            )
                            .frame(false);
                            if ui
                                .add(detach_btn)
                                .on_hover_text("Detach script from node")
                                .clicked()
                            {
                                self.session.host().with_tree_mut(|t| {
                                    if let Some(n) = t.get_mut(&node_id) {
                                        n.script = None;
                                    }
                                });
                                self.viewport_dirty = true;
                                self.log(format!("Detached script from {node_id}"));
                            }
                        });
                    }
                });

                ui.add_space(4.0);

                let full_w = ui.available_width();
                let (btn_rect, btn_resp) =
                    ui.allocate_exact_size(egui::vec2(full_w, 28.0), egui::Sense::click());

                let is_hovered = btn_resp.hovered();
                let fill_color = if is_hovered {
                    egui::Color32::from_rgb(28, 34, 42)
                } else {
                    egui::Color32::from_rgb(24, 28, 34)
                };
                let stroke_color = if is_hovered {
                    egui::Color32::from_rgb(11, 147, 135)
                } else {
                    egui::Color32::from_rgb(42, 49, 56)
                };

                ui.painter().rect_filled(btn_rect, 3.0, fill_color);
                ui.painter().rect_stroke(
                    btn_rect,
                    3.0,
                    egui::Stroke::new(1.0, stroke_color),
                    egui::StrokeKind::Middle,
                );

                if let Some(script) = &node.script {
                    let path_text = script.path.strip_prefix("res://").unwrap_or(&script.path);
                    let max_chars = ((full_w / 7.5) as usize).max(16);
                    let display_text = if path_text.len() > max_chars {
                        format!("…{}", &path_text[path_text.len() - (max_chars - 1)..])
                    } else {
                        path_text.to_string()
                    };
                    ui.painter().text(
                        btn_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        display_text,
                        egui::FontId::new(12.5, egui::FontFamily::Proportional),
                        crate::theme::TEXT,
                    );
                    btn_resp.clone().on_hover_text(&script.path);

                    if btn_resp.clicked() {
                        let resolved = self.resolve_res(&script.path);
                        self.open_asset(&resolved);
                    }
                } else {
                    ui.painter().text(
                        btn_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "+ Attach or Create Script…",
                        egui::FontId::new(12.5, egui::FontFamily::Proportional),
                        crate::theme::FAINT,
                    );
                    btn_resp.clone().on_hover_text(
                        "Click to select or create a script, or drop a .py file here",
                    );

                    if btn_resp.clicked() {
                        self.attach_or_create_script_dialog(&node_id, &node.name, &node.type_name);
                    }
                }

                if let Some(payload) =
                    btn_resp.dnd_release_payload::<super::assets::AssetDndPayload>()
                {
                    if payload.rel_path.ends_with(".py") {
                        let class_name = std::path::Path::new(&payload.rel_path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Script")
                            .to_string();
                        let res_path = format!("res://{}", payload.rel_path);
                        self.session.host().with_tree_mut(|t| {
                            if let Some(n) = t.get_mut(&node_id) {
                                n.script = Some(pite_core::ScriptRef {
                                    path: res_path.clone(),
                                    class_name,
                                });
                            }
                        });
                        self.viewport_dirty = true;
                        self.log(format!("Attached script {res_path} to {node_id}"));
                    }
                }
            });

        ui.add_space(4.0);

        // 3. Inherited Property Sections (Driven by schema)
        let chain = schema::inheritance_chain(&node.type_name);
        let mut handled_keys = HashSet::new();

        for section_name in chain {
            let defs = schema::type_props(section_name);
            // Chain is leaf→root, so overrides in a descendant are seen first and
            // shadow the ancestor's declaration.
            let filtered: Vec<&pite_core::PropSchema> = defs
                .iter()
                .filter(|d| !handled_keys.contains(d.key))
                .collect();
            if filtered.is_empty() {
                continue;
            }
            for def in &filtered {
                handled_keys.insert(def.key);
            }

            let section_id = ui.make_persistent_id(format!("sec_{}_{}", id, section_name));
            let mut is_open = ui.data_mut(|d| d.get_temp::<bool>(section_id).unwrap_or(true));

            let header_w = ui.available_width();
            let (header_rect, header_resp) =
                ui.allocate_exact_size(egui::vec2(header_w, 22.0), egui::Sense::click());

            if header_resp.clicked() {
                is_open = !is_open;
                ui.data_mut(|d| d.insert_temp(section_id, is_open));
            }

            let is_hovered = header_resp.hovered();
            let bg_color = if is_hovered {
                egui::Color32::from_rgb(28, 34, 42)
            } else {
                egui::Color32::from_rgb(18, 22, 27)
            };
            ui.painter().rect_filled(header_rect, 3.0, bg_color);

            let sec_icon = icons::node_type_icon(section_name);
            let sec_color = icons::node_type_color(section_name);
            ui.painter().text(
                egui::Pos2::new(header_rect.left() + 6.0, header_rect.center().y),
                egui::Align2::LEFT_CENTER,
                sec_icon.as_str(),
                egui::FontId::new(13.0, egui::FontFamily::Proportional),
                sec_color,
            );

            ui.painter().text(
                egui::Pos2::new(header_rect.left() + 24.0, header_rect.center().y),
                egui::Align2::LEFT_CENTER,
                section_name,
                egui::FontId::new(12.0, egui::FontFamily::Proportional),
                crate::theme::TEXT,
            );

            let caret = if is_open {
                regular::CARET_DOWN
            } else {
                regular::CARET_RIGHT
            };
            ui.painter().text(
                egui::Pos2::new(header_rect.right() - 8.0, header_rect.center().y),
                egui::Align2::RIGHT_CENTER,
                caret,
                egui::FontId::new(11.0, egui::FontFamily::Proportional),
                crate::theme::FAINT,
            );

            if is_open {
                ui.add_space(2.0);
                let label_w = 80.0;
                let action_w = 20.0;
                let row_h = 28.0;
                let grid_spacing = egui::vec2(6.0, 4.0);
                let col1_w = (ui.available_width() - label_w - grid_spacing.x - 2.0).max(80.0);

                let grid_id = ui.make_persistent_id(format!("sec_grid_{}_{}", id, section_name));
                egui::Grid::new(grid_id)
                    .num_columns(2)
                    .min_col_width(label_w)
                    .max_col_width(col1_w)
                    .min_row_height(row_h)
                    .spacing(grid_spacing)
                    .show(ui, |ui| {
                        for def in &filtered {
                            let is_present = node.props.0.contains_key(def.key);
                            let live_val = node.props.get(def.key);
                            let is_modified = is_present
                                && live_val.map_or(false, |v| !def.default_value.matches(v));

                            let label_style = if is_modified {
                                egui::RichText::new(def.label)
                                    .color(crate::theme::TEXT)
                                    .strong()
                            } else {
                                egui::RichText::new(def.label).color(crate::theme::FAINT)
                            };

                            // Col 0: Label
                            ui.allocate_ui_with_layout(
                                egui::vec2(label_w, row_h),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let resp = ui.add(egui::Label::new(label_style).truncate());
                                    if !def.doc.is_empty() {
                                        resp.on_hover_text(def.doc);
                                    }
                                },
                            );

                            // Col 1: Value Editor + Action Button
                            ui.allocate_ui_with_layout(
                                egui::vec2(col1_w, row_h),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                                    ui.spacing_mut().interact_size.y = row_h;

                                    let (val_w, show_reset) = if is_modified {
                                        ((col1_w - action_w - 4.0).max(30.0), true)
                                    } else {
                                        (col1_w, false)
                                    };

                                    let current_val = live_val
                                        .cloned()
                                        .unwrap_or_else(|| def.default_value.to_prop_value());

                                    if let Some(new_val) =
                                        self.render_prop_editor(ui, &current_val, &def.hint, val_w)
                                    {
                                        self.set_prop(&node_id, def.key, new_val);
                                    }

                                    if show_reset {
                                        let rev_btn = egui::Button::new(
                                            egui::RichText::new(regular::ARROW_COUNTER_CLOCKWISE)
                                                .size(13.0)
                                                .color(crate::theme::FAINT),
                                        )
                                        .frame(false);
                                        if ui
                                            .add_sized(egui::vec2(action_w, row_h), rev_btn)
                                            .on_hover_text("Reset to default")
                                            .clicked()
                                        {
                                            self.remove_prop(&node_id, def.key);
                                        }
                                    }
                                },
                            );

                            ui.end_row();
                        }
                    });
                ui.add_space(2.0);
            }
        }

        // 4. Custom Properties Section (unknown / extra props)
        let mut custom_keys: Vec<String> = node
            .props
            .0
            .keys()
            .filter(|k| !handled_keys.contains(k.as_str()))
            .cloned()
            .collect();
        custom_keys.sort();

        let custom_sec_id = ui.make_persistent_id(format!("sec_custom_{}", id));
        let mut custom_is_open = ui.data_mut(|d| d.get_temp::<bool>(custom_sec_id).unwrap_or(true));

        let header_w = ui.available_width();
        let (header_rect, header_resp) =
            ui.allocate_exact_size(egui::vec2(header_w, 22.0), egui::Sense::click());

        if header_resp.clicked() {
            custom_is_open = !custom_is_open;
            ui.data_mut(|d| d.insert_temp(custom_sec_id, custom_is_open));
        }

        let is_hovered = header_resp.hovered();
        let bg_color = if is_hovered {
            egui::Color32::from_rgb(28, 34, 42)
        } else {
            egui::Color32::from_rgb(18, 22, 27)
        };
        ui.painter().rect_filled(header_rect, 3.0, bg_color);

        ui.painter().text(
            egui::Pos2::new(header_rect.left() + 6.0, header_rect.center().y),
            egui::Align2::LEFT_CENTER,
            regular::GEAR,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
            crate::theme::FAINT,
        );

        ui.painter().text(
            egui::Pos2::new(header_rect.left() + 24.0, header_rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Custom Properties",
            egui::FontId::new(12.0, egui::FontFamily::Proportional),
            crate::theme::TEXT,
        );

        let caret = if custom_is_open {
            regular::CARET_DOWN
        } else {
            regular::CARET_RIGHT
        };
        ui.painter().text(
            egui::Pos2::new(header_rect.right() - 8.0, header_rect.center().y),
            egui::Align2::RIGHT_CENTER,
            caret,
            egui::FontId::new(11.0, egui::FontFamily::Proportional),
            crate::theme::FAINT,
        );

        if custom_is_open {
            ui.add_space(2.0);
            let label_w = 80.0;
            let action_w = 20.0;
            let row_h = 28.0;
            let grid_spacing = egui::vec2(6.0, 4.0);
            let col1_w = (ui.available_width() - label_w - grid_spacing.x - 2.0).max(80.0);

            if !custom_keys.is_empty() {
                let custom_grid_id = ui.make_persistent_id(format!("sec_grid_custom_{}", id));
                egui::Grid::new(custom_grid_id)
                    .num_columns(2)
                    .min_col_width(label_w)
                    .max_col_width(col1_w)
                    .min_row_height(row_h)
                    .spacing(grid_spacing)
                    .show(ui, |ui| {
                        for key in &custom_keys {
                            // Col 0: Label
                            ui.allocate_ui_with_layout(
                                egui::vec2(label_w, row_h),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(key).color(crate::theme::TEXT),
                                        )
                                        .truncate(),
                                    );
                                },
                            );

                            // Col 1: Value Editor + Action / Delete
                            ui.allocate_ui_with_layout(
                                egui::vec2(col1_w, row_h),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                                    ui.spacing_mut().interact_size.y = row_h;

                                    let val_w = (col1_w - action_w - 4.0).max(30.0);

                                    let current_val = node
                                        .props
                                        .get(key)
                                        .cloned()
                                        .unwrap_or(PropValue::Str(String::new()));

                                    if let Some(new_val) =
                                        render_custom_value_editor(ui, &current_val, val_w)
                                    {
                                        self.set_prop(&node_id, key, new_val);
                                    }

                                    let del_btn = egui::Button::new(
                                        egui::RichText::new(regular::TRASH)
                                            .size(13.0)
                                            .color(crate::theme::FAINT),
                                    )
                                    .frame(false);
                                    if ui
                                        .add_sized(egui::vec2(action_w, row_h), del_btn)
                                        .on_hover_text("Delete property")
                                        .clicked()
                                    {
                                        self.remove_prop(&node_id, key);
                                    }
                                },
                            );

                            ui.end_row();
                        }
                    });
            } else {
                ui.label(
                    egui::RichText::new("No custom properties.")
                        .color(crate::theme::FAINT)
                        .size(11.0),
                );
            }

            ui.add_space(4.0);
            // Add UI: pick from `[props]` declared keys that aren't yet set
            // on this node. Type is declared in the manifest, not chosen here.
            let available: Vec<String> = self
                .custom_props
                .keys()
                .filter(|k| !node.props.0.contains_key(k.as_str()))
                .cloned()
                .collect();
            let prop_choice_id = ui.make_persistent_id("add_custom_prop_choice");
            let mut draft_choice: Option<String> =
                ui.data_mut(|d| d.get_temp::<Option<String>>(prop_choice_id).flatten());

            let add_grid_id = ui.make_persistent_id(format!("sec_grid_add_custom_{}", id));
            egui::Grid::new(add_grid_id)
                .num_columns(2)
                .min_col_width(label_w)
                .max_col_width(col1_w)
                .min_row_height(row_h)
                .spacing(grid_spacing)
                .show(ui, |ui| {
                    // Col 0: declared-prop ComboBox (or an "all declared" hint).
                    ui.allocate_ui_with_layout(
                        egui::vec2(label_w, row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            if available.is_empty() {
                                ui.label(
                                    egui::RichText::new(if self.custom_props.is_empty() {
                                        "(no [props])"
                                    } else {
                                        "(all set)"
                                    })
                                    .color(crate::theme::FAINT)
                                    .size(11.0),
                                );
                            } else {
                                let selected_text = draft_choice
                                    .clone()
                                    .filter(|c| available.contains(c))
                                    .unwrap_or_else(|| available[0].clone());
                                draft_choice = Some(selected_text.clone());
                                let combo_w = (label_w - 4.0).max(40.0);
                                egui::ComboBox::from_id_salt("custom_prop_choice_combo")
                                    .width(combo_w)
                                    .selected_text(&selected_text)
                                    .show_ui(ui, |ui| {
                                        for opt in &available {
                                            ui.selectable_value(
                                                &mut draft_choice,
                                                Some(opt.clone()),
                                                opt,
                                            );
                                        }
                                    });
                            }
                        },
                    );

                    // Col 1: type hint + Add button
                    ui.allocate_ui_with_layout(
                        egui::vec2(col1_w, row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                            ui.spacing_mut().interact_size.y = row_h;

                            let can_add = !available.is_empty() && draft_choice.is_some();
                            if let Some(choice) = draft_choice.as_ref().filter(|_| can_add) {
                                if let Some(ty) = self.custom_props.get(choice.as_str()) {
                                    ui.label(
                                        egui::RichText::new(format!(": {ty}"))
                                            .color(crate::theme::FAINT)
                                            .size(11.0),
                                    );
                                }
                            }

                            let add_btn = egui::Button::new(
                                egui::RichText::new(regular::PLUS)
                                    .size(13.0)
                                    .color(if can_add {
                                        crate::theme::ACCENT
                                    } else {
                                        crate::theme::FAINT
                                    }),
                            )
                            .frame(false);
                            if ui
                                .add_sized(egui::vec2(action_w, row_h), add_btn)
                                .on_hover_text("Add declared custom prop with its schema default")
                                .clicked()
                                && can_add
                            {
                                if let Some(choice) = draft_choice.clone() {
                                    if let Some(ty) = self.custom_props.get(choice.as_str()) {
                                        if let Some(default) = default_for_custom_type(ty) {
                                            self.set_prop(&node_id, &choice, default);
                                            draft_choice = None;
                                        }
                                    }
                                }
                            }
                        },
                    );

                    ui.end_row();
                });

            ui.data_mut(|d| {
                d.insert_temp::<Option<String>>(prop_choice_id, draft_choice);
            });
            ui.add_space(2.0);
        }
    }

    fn render_prop_editor(
        &self,
        ui: &mut egui::Ui,
        current: &PropValue,
        hint: &PropHint,
        available_w: f32,
    ) -> Option<PropValue> {
        match hint {
            PropHint::Vec2 { speed } => {
                let (x, y) = match current {
                    PropValue::Vec2(vx, vy) => (*vx, *vy),
                    _ => (0.0, 0.0),
                };
                let mut v = (x, y);
                if render_vec2_editor(ui, &mut v, *speed, available_w) {
                    Some(PropValue::Vec2(v.0, v.1))
                } else {
                    None
                }
            }
            PropHint::AngleDeg => {
                let rad = match current {
                    PropValue::Num(f) => *f,
                    PropValue::Int(i) => *i as f64,
                    _ => 0.0,
                };
                let mut deg = rad * 180.0 / PI;
                let drag = ui.add_sized(
                    egui::vec2(available_w, 28.0),
                    egui::DragValue::new(&mut deg).speed(1.0).suffix("°"),
                );
                if drag.changed() {
                    Some(PropValue::Num(deg * PI / 180.0))
                } else {
                    None
                }
            }
            PropHint::Num { speed, min, max } => {
                let mut f = match current {
                    PropValue::Num(v) => *v,
                    PropValue::Int(i) => *i as f64,
                    _ => 0.0,
                };
                let drag = ui.add_sized(
                    egui::vec2(available_w, 28.0),
                    egui::DragValue::new(&mut f)
                        .speed(*speed)
                        .range(*min..=*max),
                );
                if drag.changed() {
                    Some(PropValue::Num(f))
                } else {
                    None
                }
            }
            PropHint::Int { speed, min, max } => {
                let mut i = match current {
                    PropValue::Int(v) => *v,
                    PropValue::Num(f) => *f as i64,
                    _ => 0,
                };
                let drag = ui.add_sized(
                    egui::vec2(available_w, 28.0),
                    egui::DragValue::new(&mut i)
                        .speed(*speed)
                        .range(*min..=*max),
                );
                if drag.changed() {
                    Some(PropValue::Int(i))
                } else {
                    None
                }
            }
            PropHint::Bool => {
                let b = match current {
                    PropValue::Bool(v) => *v,
                    _ => false,
                };
                let (icon, label_text, fg_color, bg_color, stroke_color) = if b {
                    (
                        regular::CHECK,
                        "On",
                        crate::theme::ACCENT,
                        egui::Color32::from_rgb(18, 48, 44),
                        egui::Stroke::new(1.0, crate::theme::ACCENT),
                    )
                } else {
                    (
                        regular::X,
                        "Off",
                        crate::theme::FAINT,
                        egui::Color32::from_rgb(22, 26, 32),
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 49, 56)),
                    )
                };

                let btn = egui::Button::new(
                    egui::RichText::new(format!("{icon} {label_text}"))
                        .size(12.0)
                        .color(fg_color),
                )
                .fill(bg_color)
                .stroke(stroke_color);

                if ui.add_sized(egui::vec2(available_w, 28.0), btn).clicked() {
                    Some(PropValue::Bool(!b))
                } else {
                    None
                }
            }
            PropHint::Text => {
                let mut s = match current {
                    PropValue::Str(v) => v.clone(),
                    _ => String::new(),
                };
                let text_edit = ui.add_sized(
                    egui::vec2(available_w, 28.0),
                    egui::TextEdit::singleline(&mut s).desired_width(available_w),
                );
                if text_edit.changed() {
                    Some(PropValue::Str(s))
                } else {
                    None
                }
            }
            PropHint::Enum(options) => {
                let current_str = match current {
                    PropValue::Str(v) => v.as_str(),
                    _ => options.first().copied().unwrap_or(""),
                };
                let mut selected = current_str.to_string();
                let mut changed = false;
                let combo_w = (available_w - 18.0).max(30.0);
                egui::ComboBox::from_id_salt(ui.scope_id().with("enum_combo"))
                    .width(combo_w)
                    .selected_text(&selected)
                    .show_ui(ui, |ui| {
                        for opt in *options {
                            if ui
                                .selectable_value(&mut selected, opt.to_string(), *opt)
                                .changed()
                            {
                                changed = true;
                            }
                        }
                    });
                if changed {
                    Some(PropValue::Str(selected))
                } else {
                    None
                }
            }
            PropHint::Color => {
                let [r, g, b, a] = match current {
                    PropValue::Color(r, g, b, a) => [*r, *g, *b, *a],
                    _ => [255, 255, 255, 255],
                };
                let mut c = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
                let mut changed = false;

                let (btn_rect, btn_resp) =
                    ui.allocate_exact_size(egui::vec2(available_w, 28.0), egui::Sense::click());
                if ui.is_rect_visible(btn_rect) {
                    ui.painter().rect_filled(btn_rect, 3.0, c);
                    let border_stroke = if btn_resp.hovered() {
                        egui::Stroke::new(1.0, crate::theme::TEXT)
                    } else {
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 60, 70))
                    };
                    ui.painter().rect_stroke(
                        btn_rect,
                        3.0,
                        border_stroke,
                        egui::StrokeKind::Inside,
                    );

                    let [rr, gg, bb, _] = c.to_srgba_unmultiplied();
                    let hex_label = format!("#{:02x}{:02x}{:02x}", rr, gg, bb);
                    let text_color = contrast_color(c);
                    ui.painter().text(
                        btn_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        hex_label,
                        egui::FontId::new(11.0, egui::FontFamily::Monospace),
                        text_color,
                    );
                }

                let popup_id = ui.make_persistent_id("color_picker_popup");
                egui::Popup::menu(&btn_resp)
                    .id(popup_id)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        if render_color_picker_popup(ui, &mut c) {
                            changed = true;
                        }
                    });

                if changed {
                    let [r, g, b, a] = c.to_srgba_unmultiplied();
                    Some(PropValue::Color(r, g, b, a))
                } else {
                    None
                }
            }
            PropHint::Asset { filter } => {
                let mut current_str = match current {
                    PropValue::Str(v) => v.clone(),
                    _ => String::new(),
                };
                let mut changed_val = None;

                let btn_w = 26.0;
                let text_w = (available_w - btn_w - 4.0).max(20.0);

                let text_resp = ui.add_sized(
                    egui::vec2(text_w, 28.0),
                    egui::TextEdit::singleline(&mut current_str)
                        .desired_width(text_w)
                        .hint_text("(None)"),
                );
                if text_resp.changed() {
                    changed_val = Some(PropValue::Str(current_str.clone()));
                }

                if !current_str.is_empty() {
                    text_resp.clone().on_hover_text(&current_str);
                }

                let (browse_rect, browse_resp) =
                    ui.allocate_exact_size(egui::vec2(btn_w, 28.0), egui::Sense::click());
                let browse_hovered = browse_resp.hovered();
                let browse_bg = if browse_hovered {
                    ui.visuals().widgets.hovered.bg_fill
                } else {
                    ui.visuals().widgets.inactive.bg_fill
                };
                let browse_stroke = if browse_hovered {
                    egui::Stroke::new(1.0, crate::theme::ACCENT)
                } else {
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 60, 70))
                };
                if ui.is_rect_visible(browse_rect) {
                    let p = ui.painter();
                    p.rect_filled(browse_rect, 3.0, browse_bg);
                    p.rect_stroke(browse_rect, 3.0, browse_stroke, egui::StrokeKind::Inside);
                    p.text(
                        browse_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        regular::FOLDER_OPEN,
                        egui::FontId::new(13.0, egui::FontFamily::Proportional),
                        if browse_hovered {
                            crate::theme::TEXT
                        } else {
                            crate::theme::FAINT
                        },
                    );
                }

                if let Some(payload) = text_resp
                    .dnd_release_payload::<super::assets::AssetDndPayload>()
                    .or_else(|| browse_resp.dnd_release_payload::<super::assets::AssetDndPayload>())
                {
                    changed_val = Some(PropValue::Str(format!("res://{}", payload.rel_path)));
                }

                if browse_resp
                    .on_hover_text(format!("Browse asset ({})", filter.join(", ")))
                    .clicked()
                {
                    if let Some(picked) = self.pick_asset_dialog(filter) {
                        changed_val = Some(PropValue::Str(picked));
                    }
                }

                changed_val
            }
        }
    }
}

fn render_custom_value_editor(
    ui: &mut egui::Ui,
    current: &PropValue,
    available_w: f32,
) -> Option<PropValue> {
    match current {
        PropValue::Num(f) => {
            let mut v = *f;
            if ui
                .add_sized(
                    egui::vec2(available_w, 28.0),
                    egui::DragValue::new(&mut v).speed(0.1),
                )
                .changed()
            {
                Some(PropValue::Num(v))
            } else {
                None
            }
        }
        PropValue::Int(i) => {
            let mut v = *i;
            if ui
                .add_sized(
                    egui::vec2(available_w, 28.0),
                    egui::DragValue::new(&mut v).speed(1.0),
                )
                .changed()
            {
                Some(PropValue::Int(v))
            } else {
                None
            }
        }
        PropValue::Str(s) => {
            let mut v = s.clone();
            if ui
                .add_sized(
                    egui::vec2(available_w, 28.0),
                    egui::TextEdit::singleline(&mut v).desired_width(available_w),
                )
                .changed()
            {
                Some(PropValue::Str(v))
            } else {
                None
            }
        }
        PropValue::Bool(b) => {
            let v = *b;
            let (icon, label_text, fg_color, bg_color, stroke_color) = if v {
                (
                    regular::CHECK,
                    "On",
                    crate::theme::ACCENT,
                    egui::Color32::from_rgb(18, 48, 44),
                    egui::Stroke::new(1.0, crate::theme::ACCENT),
                )
            } else {
                (
                    regular::X,
                    "Off",
                    crate::theme::FAINT,
                    egui::Color32::from_rgb(22, 26, 32),
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 49, 56)),
                )
            };
            let btn = egui::Button::new(
                egui::RichText::new(format!("{icon} {label_text}"))
                    .size(12.0)
                    .color(fg_color),
            )
            .fill(bg_color)
            .stroke(stroke_color);
            if ui.add_sized(egui::vec2(available_w, 28.0), btn).clicked() {
                Some(PropValue::Bool(!v))
            } else {
                None
            }
        }
        PropValue::Vec2(x, y) => {
            let mut v = (*x, *y);
            if render_vec2_editor(ui, &mut v, 1.0, available_w) {
                Some(PropValue::Vec2(v.0, v.1))
            } else {
                None
            }
        }
        PropValue::Color(r, g, b, a) => {
            // Custom prop that happened to parse as a hex color on load.
            // Use the same picker as schema Color props.
            let mut c = egui::Color32::from_rgba_unmultiplied(*r, *g, *b, *a);
            let mut changed = false;
            let (btn_rect, btn_resp) =
                ui.allocate_exact_size(egui::vec2(available_w, 28.0), egui::Sense::click());
            if ui.is_rect_visible(btn_rect) {
                ui.painter().rect_filled(btn_rect, 3.0, c);
                ui.painter().rect_stroke(
                    btn_rect,
                    3.0,
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 60, 70)),
                    egui::StrokeKind::Inside,
                );
                let [rr, gg, bb, _] = c.to_srgba_unmultiplied();
                let hex_label = format!("#{:02x}{:02x}{:02x}", rr, gg, bb);
                ui.painter().text(
                    btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    hex_label,
                    egui::FontId::new(11.0, egui::FontFamily::Monospace),
                    contrast_color(c),
                );
            }
            let popup_id = ui.make_persistent_id("custom_color_picker");
            egui::Popup::menu(&btn_resp)
                .id(popup_id)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    if render_color_picker_popup(ui, &mut c) {
                        changed = true;
                    }
                });
            if changed {
                let [r, g, b, a] = c.to_srgba_unmultiplied();
                Some(PropValue::Color(r, g, b, a))
            } else {
                None
            }
        }
    }
}

/// Schema default for a `[props]`-declared type. Used by the inspector's
/// add-UI to seed a fresh value before the user edits it. `None` for
/// unrecognized types (they'd already have failed manifest validation, so
/// this shouldn't happen in practice — the fallback is silent no-op).
fn default_for_custom_type(ty: &str) -> Option<PropValue> {
    match ty {
        "string" | "str" => Some(PropValue::Str(String::new())),
        "int" | "integer" => Some(PropValue::Int(0)),
        "number" | "num" | "float" => Some(PropValue::Num(0.0)),
        "bool" | "boolean" => Some(PropValue::Bool(false)),
        "vec2" => Some(PropValue::Vec2(0.0, 0.0)),
        "color" => Some(PropValue::Color(255, 255, 255, 255)),
        _ => None,
    }
}

fn render_vec2_editor(ui: &mut egui::Ui, v: &mut (f64, f64), speed: f64, available_w: f32) -> bool {
    let mut changed = false;
    let half_w = ((available_w - 4.0) / 2.0).max(30.0);
    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
    if ui
        .add_sized(
            egui::vec2(half_w, 28.0),
            egui::DragValue::new(&mut v.0).prefix("X: ").speed(speed),
        )
        .changed()
    {
        changed = true;
    }
    if ui
        .add_sized(
            egui::vec2(half_w, 28.0),
            egui::DragValue::new(&mut v.1).prefix("Y: ").speed(speed),
        )
        .changed()
    {
        changed = true;
    }
    changed
}

fn contrast_color(c: egui::Color32) -> egui::Color32 {
    let lum = 0.299 * (c.r() as f32) + 0.587 * (c.g() as f32) + 0.114 * (c.b() as f32);
    if lum > 140.0 {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    }
}

fn render_color_picker_popup(ui: &mut egui::Ui, c: &mut egui::Color32) -> bool {
    let mut changed = false;
    let mut hsva = egui::ecolor::Hsva::from_srgba_unmultiplied(c.to_array());

    let picker_w = 210.0;
    ui.set_width(picker_w);
    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);

    // 1. 2D Saturation / Value Canvas
    let (sv_rect, sv_resp) =
        ui.allocate_exact_size(egui::vec2(picker_w, 130.0), egui::Sense::click_and_drag());
    if let Some(mpos) = sv_resp.interact_pointer_pos() {
        hsva.s = ((mpos.x - sv_rect.left()) / sv_rect.width()).clamp(0.0, 1.0);
        hsva.v = (1.0 - (mpos.y - sv_rect.top()) / sv_rect.height()).clamp(0.0, 1.0);
        *c = egui::Color32::from(hsva);
        changed = true;
    }

    if ui.is_rect_visible(sv_rect) {
        let mut mesh = egui::Mesh::default();
        let c_pure = egui::Color32::from(egui::ecolor::Hsva::new(hsva.h, 1.0, 1.0, 1.0));
        mesh.colored_vertex(sv_rect.left_top(), egui::Color32::WHITE);
        mesh.colored_vertex(sv_rect.right_top(), c_pure);
        mesh.colored_vertex(sv_rect.left_bottom(), egui::Color32::BLACK);
        mesh.colored_vertex(sv_rect.right_bottom(), egui::Color32::BLACK);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(1, 3, 2);
        ui.painter().add(mesh);
        ui.painter().rect_stroke(
            sv_rect,
            2.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 60, 70)),
            egui::StrokeKind::Inside,
        );

        let handle_pos = egui::pos2(
            sv_rect.left() + hsva.s * sv_rect.width(),
            sv_rect.top() + (1.0 - hsva.v) * sv_rect.height(),
        );
        ui.painter().circle(
            handle_pos,
            4.0,
            *c,
            egui::Stroke::new(1.5, contrast_color(*c)),
        );
    }

    ui.add_space(2.0);

    // 2. 1D Hue Slider
    let (hue_rect, hue_resp) =
        ui.allocate_exact_size(egui::vec2(picker_w, 14.0), egui::Sense::click_and_drag());
    if let Some(mpos) = hue_resp.interact_pointer_pos() {
        hsva.h = ((mpos.x - hue_rect.left()) / hue_rect.width()).clamp(0.0, 1.0);
        *c = egui::Color32::from(hsva);
        changed = true;
    }

    if ui.is_rect_visible(hue_rect) {
        let mut hue_mesh = egui::Mesh::default();
        let stops = 6;
        for i in 0..=stops {
            let t = i as f32 / stops as f32;
            let x = hue_rect.left() + t * hue_rect.width();
            let col = egui::Color32::from(egui::ecolor::Hsva::new(t, 1.0, 1.0, 1.0));
            hue_mesh.colored_vertex(egui::pos2(x, hue_rect.top()), col);
            hue_mesh.colored_vertex(egui::pos2(x, hue_rect.bottom()), col);
            if i > 0 {
                let base = (i as u32 - 1) * 2;
                hue_mesh.add_triangle(base, base + 1, base + 2);
                hue_mesh.add_triangle(base + 1, base + 3, base + 2);
            }
        }
        ui.painter().add(hue_mesh);
        ui.painter().rect_stroke(
            hue_rect,
            2.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 60, 70)),
            egui::StrokeKind::Inside,
        );

        let indicator_x = hue_rect.left() + hsva.h * hue_rect.width();
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(indicator_x - 2.0, hue_rect.top() - 1.0),
                egui::vec2(4.0, hue_rect.height() + 2.0),
            ),
            1.0,
            egui::Color32::WHITE,
        );
    }

    ui.add_space(4.0);

    // 3. RGB values each in a row: 0-255 and 0-1 beside each other for each channel
    let [mut r, mut g, mut b, mut a] = c.to_srgba_unmultiplied();
    let channels = [
        ("R", &mut r, egui::Color32::from_rgb(235, 80, 80)),
        ("G", &mut g, egui::Color32::from_rgb(80, 205, 80)),
        ("B", &mut b, egui::Color32::from_rgb(80, 145, 235)),
        ("A", &mut a, egui::Color32::from_rgb(180, 180, 180)),
    ];

    for (name, val, color) in channels {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
            ui.add_sized(
                [14.0, 18.0],
                egui::Label::new(egui::RichText::new(name).color(color).strong().size(12.0)),
            );

            let mut int_val = *val as i64;
            if ui
                .add(
                    egui::DragValue::new(&mut int_val)
                        .range(0..=255)
                        .speed(1.0)
                        .min_size(egui::vec2(85.0, 18.0)),
                )
                .changed()
            {
                *val = int_val.clamp(0, 255) as u8;
                changed = true;
            }

            let mut float_val = (*val as f64) / 255.0;
            if ui
                .add(
                    egui::DragValue::new(&mut float_val)
                        .range(0.0..=1.0)
                        .speed(0.01)
                        .max_decimals(3)
                        .min_size(egui::vec2(95.0, 18.0)),
                )
                .changed()
            {
                *val = (float_val.clamp(0.0, 1.0) * 255.0).round() as u8;
                changed = true;
            }
        });
    }

    if changed {
        *c = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
    }

    ui.add_space(2.0);

    // 4. Color Hex Code Line Edit with select-on-focus
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
        ui.add_sized(
            [30.0, 18.0],
            egui::Label::new(
                egui::RichText::new("Hex")
                    .color(crate::theme::FAINT)
                    .size(12.0),
            ),
        );
        let mut hex_str = format_hex_color(*c);
        let hex_resp = ui.add(egui::TextEdit::singleline(&mut hex_str).desired_width(125.0));
        if hex_resp.gained_focus() {
            let mut state =
                egui::text_edit::TextEditState::load(ui.ctx(), hex_resp.id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text_selection::CCursorRange::select_all_str(
                    &hex_str,
                )));
            state.store(ui.ctx(), hex_resp.id);
        }
        if hex_resp.changed() {
            if let Some(parsed) = parse_hex_color(&hex_str) {
                *c = parsed;
                changed = true;
            }
        }

        let copy_btn = egui::Button::new(
            egui::RichText::new(regular::COPY)
                .size(12.0)
                .color(crate::theme::FAINT),
        )
        .frame(false);
        if ui.add(copy_btn).on_hover_text("Copy Hex").clicked() {
            ui.ctx().copy_text(hex_str);
        }
    });

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_hex_parse_and_format() {
        assert_eq!(
            parse_hex_color("#0b9387"),
            Some(egui::Color32::from_rgb(11, 147, 135))
        );
        assert_eq!(
            parse_hex_color("#0b9387ff"),
            Some(egui::Color32::from_rgb(11, 147, 135))
        );
        assert_eq!(
            format_hex_color(egui::Color32::from_rgb(11, 147, 135)),
            "#0b9387ff"
        );
        assert_eq!(parse_hex_color("invalid"), None);
        // 8-digit round-trip preserves alpha.
        let semi = parse_hex_color("#ff000080").unwrap();
        assert_eq!(format_hex_color(semi), "#ff000080");
    }
}
