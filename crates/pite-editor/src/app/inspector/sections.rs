// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::HashSet;

use eframe::egui;
use egui_phosphor::regular;
use pite_core::schema;
use pite_core::{Node, NodeId, PropValue};

use crate::icons;

use super::super::assets::AssetDndPayload;
use super::super::EditorApp;
use super::props::{default_for_custom_type, render_custom_value_editor};

fn draw_section_header(
    ui: &mut egui::Ui,
    section_id: egui::Id,
    title: &str,
    icon: &str,
    icon_color: egui::Color32,
) -> bool {
    let mut is_open = ui.data_mut(|d| d.get_temp::<bool>(section_id).unwrap_or(true));
    let header_w = ui.available_width();
    let (header_rect, header_resp) =
        ui.allocate_exact_size(egui::vec2(header_w, 22.0), egui::Sense::click());
    if header_resp.clicked() {
        is_open = !is_open;
        ui.data_mut(|d| d.insert_temp(section_id, is_open));
    }
    let bg_color = if header_resp.hovered() {
        egui::Color32::from_rgb(28, 34, 42)
    } else {
        egui::Color32::from_rgb(18, 22, 27)
    };
    ui.painter().rect_filled(header_rect, 3.0, bg_color);
    ui.painter().text(
        egui::Pos2::new(header_rect.left() + 6.0, header_rect.center().y),
        egui::Align2::LEFT_CENTER,
        icon,
        egui::FontId::new(13.0, egui::FontFamily::Proportional),
        icon_color,
    );
    ui.painter().text(
        egui::Pos2::new(header_rect.left() + 24.0, header_rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
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
    is_open
}

impl EditorApp {
    pub(super) fn pick_asset_dialog(&self, filter: &[&str]) -> Option<String> {
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

    pub(super) fn show_script_card(&mut self, ui: &mut egui::Ui, node_id: &NodeId, node: &Node) {
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
                                    if let Some(n) = t.get_mut(node_id) {
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
                        if let Some(resolved) = self.resolve_res(&script.path) {
                            self.open_asset(&resolved);
                        }
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
                        self.attach_or_create_script_dialog(node_id, &node.name, &node.type_name);
                    }
                }
                if let Some(payload) = btn_resp.dnd_release_payload::<AssetDndPayload>() {
                    if payload.rel_path.ends_with(".py") {
                        let class_name = std::path::Path::new(&payload.rel_path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Script")
                            .to_string();
                        let res_path = format!("res://{}", payload.rel_path);
                        self.session.host().with_tree_mut(|t| {
                            if let Some(n) = t.get_mut(node_id) {
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
    }

    pub(super) fn show_inherited_props(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        node_id: &NodeId,
        node: &Node,
    ) -> HashSet<&'static str> {
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
            let sec_icon = icons::node_type_icon(section_name);
            let sec_color = icons::node_type_color(section_name);
            let is_open =
                draw_section_header(ui, section_id, section_name, sec_icon.as_str(), sec_color);

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
                                && live_val.is_some_and(|v| !def.default_value.matches(v));

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

                                    let atlas_ref = match node.props.get("atlas") {
                                        Some(PropValue::Str(s)) if !s.is_empty() => {
                                            Some(s.as_str())
                                        }
                                        _ => None,
                                    };
                                    let is_atlas_picker = (section_name == "Sprite2D"
                                        && def.key == "frame")
                                        || (section_name == "AnimatedSprite2D"
                                            && def.key == "animation");
                                    let handled_by_picker = if is_atlas_picker {
                                        if let Some(a_ref) = atlas_ref {
                                            self.render_atlas_member_picker(
                                                ui,
                                                node_id,
                                                a_ref,
                                                def.key,
                                                &current_val,
                                                val_w,
                                            )
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    };
                                    if !handled_by_picker {
                                        if let Some(new_val) = self.render_prop_editor(
                                            ui,
                                            &current_val,
                                            &def.hint,
                                            val_w,
                                        ) {
                                            self.set_prop(node_id, def.key, new_val);
                                        }
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
                                            self.remove_prop(node_id, def.key);
                                        }
                                    }
                                },
                            );

                            ui.end_row();
                        }
                    });
                ui.add_space(2.0);
                if section_name == "Sprite2D" || section_name == "AnimatedSprite2D" {
                    self.show_sprite_atlas_actions(ui, id, node_id, node);
                }
            }
        }
        handled_keys
    }

    fn resolve_loaded_atlas(&self, atlas_ref: &str) -> Option<pite_render::atlas::Atlas> {
        if let Some(ed) = &self.atlas_editor {
            if ed.res_path == atlas_ref {
                return Some(ed.atlas.clone());
            }
        }
        let sidecar = self.resolve_res(atlas_ref)?;
        pite_render::atlas::load_atlas(&sidecar).ok()
    }

    fn render_atlas_member_picker(
        &mut self,
        ui: &mut egui::Ui,
        node_id: &NodeId,
        atlas_ref: &str,
        key: &str,
        current: &PropValue,
        available_w: f32,
    ) -> bool {
        let Some(atlas) = self.resolve_loaded_atlas(atlas_ref) else {
            return false;
        };
        let mut current_str = match current {
            PropValue::Str(s) => s.clone(),
            _ => String::new(),
        };
        let options: Vec<(String, String)> = if key == "frame" {
            atlas
                .frames
                .iter()
                .map(|(n, f)| (n.clone(), format!("{n} ({}×{})", f.w, f.h)))
                .collect()
        } else {
            atlas
                .animations
                .iter()
                .map(|(n, a)| (n.clone(), format!("{n} ({:.0} fps)", a.fps)))
                .collect()
        };
        if options.is_empty() {
            return false;
        }
        let mut changed = false;
        egui::ComboBox::from_id_salt(ui.scope_id().with(("atlas_member", key)))
            .width((available_w - 8.0).max(40.0))
            .selected_text(if current_str.is_empty() {
                format!("(select {key})")
            } else {
                current_str.clone()
            })
            .show_ui(ui, |ui| {
                for (name, label) in &options {
                    if ui
                        .selectable_value(&mut current_str, name.clone(), label)
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        if changed {
            self.set_prop(node_id, key, PropValue::Str(current_str));
        }
        true
    }

    fn show_sprite_atlas_actions(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        node_id: &NodeId,
        node: &Node,
    ) {
        let atlas_ref = match node.props.get("atlas") {
            Some(PropValue::Str(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };
        let tex_ref = match node.props.get("texture") {
            Some(PropValue::Str(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };

        if let Some(a_ref) = atlas_ref {
            let inline_id = ui.make_persistent_id(format!("inline_atlas_{id}"));
            let mut inline_open = ui.data_mut(|d| d.get_temp::<bool>(inline_id).unwrap_or(false));
            let is_loaded_here = self
                .atlas_editor
                .as_ref()
                .is_some_and(|e| e.res_path == a_ref);

            ui.horizontal(|ui| {
                let btn_label = if inline_open && is_loaded_here {
                    "Hide Atlas Editor"
                } else {
                    "Edit Atlas…"
                };
                if ui.small_button(btn_label).clicked() {
                    if inline_open && is_loaded_here {
                        inline_open = false;
                    } else if let Some(abs) = self.resolve_res(&a_ref) {
                        self.open_atlas_in_inspector(abs, a_ref.clone());
                        inline_open = true;
                    }
                    ui.data_mut(|d| d.insert_temp(inline_id, inline_open));
                }
            });

            if inline_open {
                if !is_loaded_here {
                    if let Some(abs) = self.resolve_res(&a_ref) {
                        self.open_atlas_in_inspector(abs, a_ref);
                    }
                }
                if self.atlas_editor.is_some() {
                    ui.separator();
                    self.show_atlas_editor(ui, false);
                }
            }
        } else if let Some(t_ref) = tex_ref {
            if let Some(rel) = t_ref.strip_prefix("res://") {
                if rel.to_ascii_lowercase().ends_with(".png")
                    && ui.small_button("Create Atlas (.ptatlas)…").clicked()
                {
                    self.open_new_atlas_modal(rel, Some(node_id.to_string()));
                }
            }
        }
    }

    pub(super) fn show_custom_props(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        node_id: &NodeId,
        node: &Node,
        handled_keys: &HashSet<&'static str>,
    ) {
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
        let custom_is_open = draw_section_header(
            ui,
            custom_sec_id,
            "Custom Properties",
            regular::GEAR,
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
                                        self.set_prop(node_id, key, new_val);
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
                                        self.remove_prop(node_id, key);
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
                                            self.set_prop(node_id, &choice, default);
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

    pub(super) fn show_tilemap_inspector(&mut self, ui: &mut egui::Ui, node_id: &NodeId) {
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
        let tilemap_atlas_id = ui.make_persistent_id(format!("inline_tilemap_atlas_{node_id}"));
        let mut tilemap_atlas_open =
            ui.data_mut(|d| d.get_temp::<bool>(tilemap_atlas_id).unwrap_or(false));
        let is_tilemap_atlas_loaded = self
            .atlas_editor
            .as_ref()
            .is_some_and(|e| e.res_path == doc.atlas);

        ui.horizontal(|ui| {
            ui.label("Tile Palette:");
            if self.resolve_res(&doc.atlas).is_some() {
                let lbl = if tilemap_atlas_open && is_tilemap_atlas_loaded {
                    "Hide Atlas"
                } else {
                    "Edit Atlas"
                };
                if ui.small_button(lbl).clicked() {
                    if tilemap_atlas_open && is_tilemap_atlas_loaded {
                        tilemap_atlas_open = false;
                    } else if let Some(abs) = self.resolve_res(&doc.atlas) {
                        self.open_atlas_in_inspector(abs, doc.atlas.clone());
                        tilemap_atlas_open = true;
                    }
                    ui.data_mut(|d| d.insert_temp(tilemap_atlas_id, tilemap_atlas_open));
                }
            }
        });

        let loaded_atlas = self.resolve_loaded_atlas(&doc.atlas);
        let sheet_tex = self
            .atlas_editor
            .as_ref()
            .filter(|e| e.res_path == doc.atlas)
            .and_then(|e| e.sheet_tex.clone());

        if let Some(atlas) = loaded_atlas {
            egui::ScrollArea::vertical()
                .id_salt("tile_palette")
                .max_height(140.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for name in atlas.names() {
                            let selected = !self.tilemap_eraser
                                && self.tilemap_selected_frame.as_deref() == Some(name);
                            let resp = if let (Some(tex), Ok(fr)) = (&sheet_tex, atlas.frame(name))
                            {
                                let uv_arr = fr.uv(atlas.size);
                                let uv = egui::Rect::from_min_max(
                                    egui::pos2(uv_arr[0], uv_arr[1]),
                                    egui::pos2(uv_arr[2], uv_arr[3]),
                                );
                                ui.add(
                                    egui::Button::image(
                                        egui::Image::new((tex.id(), egui::vec2(24.0, 24.0))).uv(uv),
                                    )
                                    .selected(selected),
                                )
                                .on_hover_text(name)
                            } else {
                                ui.selectable_label(selected, name)
                            };
                            if resp.clicked() {
                                self.tilemap_selected_frame = Some(name.to_string());
                                self.tilemap_eraser = false;
                            }
                        }
                    });
                });
        } else {
            ui.label(format!("Cannot load atlas: {}", doc.atlas));
        }

        if tilemap_atlas_open {
            if !is_tilemap_atlas_loaded {
                if let Some(abs) = self.resolve_res(&doc.atlas) {
                    self.open_atlas_in_inspector(abs, doc.atlas.clone());
                }
            }
            if self.atlas_editor.is_some() {
                ui.separator();
                self.show_atlas_editor(ui, false);
            }
        }

        if changed {
            let node_path = self.session.host().with_tree(|t| {
                t.get(node_id).and_then(|n| match n.props.get("tilemap") {
                    Some(pite_core::PropValue::Str(s)) if !s.is_empty() => Some(s.clone()),
                    _ => None,
                })
            });
            if let Some(map_ref) = node_path {
                if let Some(fs_path) = self.resolve_res(&map_ref) {
                    let _ = pite_scene::save_tilemap(&fs_path, &doc);
                }
            }
            self.session.host().set_tilemap_doc(node_id.as_str(), doc);
        }
    }
}
