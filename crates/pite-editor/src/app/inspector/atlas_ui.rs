// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;
use egui_phosphor::regular;
use pite_render::atlas::{sheet_for_sidecar, AtlasAnimation, AtlasFrame};

use super::super::EditorApp;
use super::atlas::{AtlasEditorState, CanvasDrag};

fn clamp_frame(frame: &mut AtlasFrame, sheet: (u32, u32)) {
    let (sw, sh) = (sheet.0.max(1), sheet.1.max(1));
    frame.w = frame.w.clamp(1, sw);
    frame.h = frame.h.clamp(1, sh);
    frame.x = frame.x.min(sw - frame.w);
    frame.y = frame.y.min(sh - frame.h);
}

fn draw_checkerboard(painter: &egui::Painter, rect: egui::Rect) {
    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(24, 28, 33));
    let cell = 8.0;
    let mut y = rect.top();
    let mut row = 0usize;
    while y < rect.bottom() {
        let mut x = rect.left();
        let mut col = row % 2;
        while x < rect.right() {
            if col.is_multiple_of(2) {
                let c_rect = egui::Rect::from_min_max(
                    egui::pos2(x, y),
                    egui::pos2((x + cell).min(rect.right()), (y + cell).min(rect.bottom())),
                );
                painter.rect_filled(c_rect, 0.0, egui::Color32::from_rgb(31, 36, 42));
            }
            x += cell;
            col += 1;
        }
        y += cell;
        row += 1;
    }
}

impl EditorApp {
    /// Open or switch the inline Inspector Atlas Editor to `ptatlas_path`
    /// without discarding unsaved edits if the same file is already loaded.
    pub(in crate::app) fn open_atlas_in_inspector(
        &mut self,
        ptatlas_path: PathBuf,
        res_path: String,
    ) {
        if self
            .atlas_editor
            .as_ref()
            .is_some_and(|ed| ed.path == ptatlas_path)
        {
            return;
        }
        match AtlasEditorState::from_path(ptatlas_path.clone(), res_path) {
            Ok(state) => self.atlas_editor = Some(state),
            Err(e) => self.log(format!(
                "Cannot open atlas {}: {e:#}",
                ptatlas_path.display()
            )),
        }
    }

    pub(in crate::app) fn save_atlas_editor(&mut self) {
        let Some(ed) = self.atlas_editor.as_mut() else {
            return;
        };
        let path = ed.path.clone();
        let sheet = sheet_for_sidecar(&path);
        match ed.save() {
            Ok(()) => {
                self.session.dirty_assets.insert(path.clone());
                self.session.dirty_assets.insert(sheet);
                self.viewport_dirty = true;
                self.log(format!("Saved atlas {}", path.display()));
            }
            Err(e) => {
                if let Some(ed) = self.atlas_editor.as_mut() {
                    ed.last_error = Some(format!("{e:#}"));
                }
                self.log(format!("Save atlas failed: {e:#}"));
            }
        }
    }

    pub(in crate::app) fn show_atlas_editor(&mut self, ui: &mut egui::Ui, standalone: bool) {
        let mut save_requested = false;
        let mut close_requested = false;
        let mut edit_toml_path: Option<PathBuf> = None;

        let Some(ed) = self.atlas_editor.as_mut() else {
            return;
        };
        ed.ensure_sheet_texture(ui.ctx());

        // Header bar
        ui.horizontal(|ui| {
            let label = if ed.dirty {
                format!("Atlas: {}*", ed.res_path)
            } else {
                format!("Atlas: {}", ed.res_path)
            };
            ui.label(
                egui::RichText::new(label)
                    .strong()
                    .color(crate::theme::TEXT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if standalone
                    && ui
                        .small_button("✕")
                        .on_hover_text("Close atlas editor")
                        .clicked()
                {
                    close_requested = true;
                }
                if ui
                    .small_button("TOML")
                    .on_hover_text("Edit raw .ptatlas in Code tab")
                    .clicked()
                {
                    edit_toml_path = Some(ed.path.clone());
                }
                let save_label = egui::RichText::new("Save").color(if ed.dirty {
                    crate::theme::ACCENT
                } else {
                    crate::theme::FAINT
                });
                if ui.small_button(save_label).clicked() {
                    save_requested = true;
                }
            });
        });

        ui.label(
            egui::RichText::new(format!(
                "Sheet: {} ({}x{})  •  {} frames  •  {} anims",
                ed.atlas.texture,
                ed.atlas.size.0,
                ed.atlas.size.1,
                ed.atlas.frames.len(),
                ed.atlas.animations.len()
            ))
            .size(11.0)
            .color(crate::theme::FAINT),
        );

        if let Some(err) = &ed.last_error {
            ui.colored_label(egui::Color32::from_rgb(230, 80, 80), err);
        }

        // Interactive sheet canvas
        show_sheet_canvas(ui, ed);

        ui.add_space(4.0);
        show_grid_slicer(ui, ed);

        ui.add_space(4.0);
        show_frames_section(ui, ed);

        ui.add_space(4.0);
        show_animations_section(ui, ed);

        if save_requested {
            self.save_atlas_editor();
        }
        if let Some(path) = edit_toml_path {
            self.open_text_file(&path);
        }
        if close_requested {
            self.atlas_editor = None;
        }
    }
}

fn show_sheet_canvas(ui: &mut egui::Ui, ed: &mut AtlasEditorState) {
    let (sw, sh) = (ed.atlas.size.0.max(1) as f32, ed.atlas.size.1.max(1) as f32);
    let avail_w = ui.available_width().max(160.0);
    if ed.zoom <= 0.0 {
        ed.zoom = (avail_w / sw).clamp(0.25, 4.0);
    }

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("Zoom: {:.0}%", ed.zoom * 100.0))
                .size(11.0)
                .color(crate::theme::FAINT),
        );
        if ui.small_button("-").clicked() {
            ed.zoom = (ed.zoom * 0.8).clamp(0.25, 8.0);
        }
        if ui.small_button("+").clicked() {
            ed.zoom = (ed.zoom * 1.25).clamp(0.25, 8.0);
        }
        if ui.small_button("Fit").clicked() {
            ed.zoom = (avail_w / sw).clamp(0.25, 8.0);
            ed.pan = egui::Vec2::ZERO;
        }
        if ui.small_button("1x").clicked() {
            ed.zoom = 1.0;
            ed.pan = egui::Vec2::ZERO;
        }
    });

    let canvas_h = (sh * ed.zoom).clamp(130.0, 250.0);
    let (canvas_rect, resp) =
        ui.allocate_exact_size(egui::vec2(avail_w, canvas_h), egui::Sense::click_and_drag());

    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.1 {
            let factor = if scroll > 0.0 { 1.12 } else { 0.89 };
            ed.zoom = (ed.zoom * factor).clamp(0.25, 8.0);
        }
    }

    let img_size = egui::vec2(sw * ed.zoom, sh * ed.zoom);
    let origin = egui::pos2(
        canvas_rect.center().x - img_size.x * 0.5 + ed.pan.x,
        canvas_rect.center().y - img_size.y * 0.5 + ed.pan.y,
    );
    let img_rect = egui::Rect::from_min_size(origin, img_size);

    let painter = ui.painter_at(canvas_rect);
    painter.rect_filled(canvas_rect, 3.0, egui::Color32::from_rgb(16, 19, 24));
    draw_checkerboard(&painter, img_rect);

    if let Some(tex) = &ed.sheet_tex {
        painter.image(
            tex.id(),
            img_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }

    // Draw frame boxes
    for (name, frame) in &ed.atlas.frames {
        let is_selected = ed.selected_frame.as_deref() == Some(name.as_str());
        let f_rect = egui::Rect::from_min_size(
            egui::pos2(
                img_rect.left() + frame.x as f32 * ed.zoom,
                img_rect.top() + frame.y as f32 * ed.zoom,
            ),
            egui::vec2(frame.w as f32 * ed.zoom, frame.h as f32 * ed.zoom),
        );
        let stroke = if is_selected {
            egui::Stroke::new(2.0, crate::theme::ACCENT)
        } else {
            egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(200, 210, 220, 110),
            )
        };
        if is_selected {
            painter.rect_filled(
                f_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(11, 147, 135, 36),
            );
        }
        painter.rect_stroke(f_rect, 0.0, stroke, egui::StrokeKind::Inside);

        if is_selected {
            let handle = egui::Rect::from_center_size(f_rect.right_bottom(), egui::vec2(8.0, 8.0));
            painter.rect_filled(handle, 1.0, crate::theme::ACCENT);
        }
    }

    painter.rect_stroke(
        canvas_rect,
        3.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 49, 56)),
        egui::StrokeKind::Inside,
    );

    let zoom = ed.zoom;
    let to_sheet_px = move |pos: egui::Pos2| -> (i32, i32) {
        let px = ((pos.x - img_rect.left()) / zoom).floor() as i32;
        let py = ((pos.y - img_rect.top()) / zoom).floor() as i32;
        (px, py)
    };

    // Click selects frame under cursor
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let (px, py) = to_sheet_px(pos);
            let hit = ed
                .atlas
                .frames
                .iter()
                .find(|(_, f)| {
                    px >= f.x as i32
                        && py >= f.y as i32
                        && px < (f.x + f.w) as i32
                        && py < (f.y + f.h) as i32
                })
                .map(|(n, _)| n.clone());
            if hit.is_some() {
                ed.select_frame(hit);
            }
        }
    }

    // Drag start: middle/Shift pan, resize selected corner, move selected, or draw new frame
    if resp.drag_started() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let middle_or_shift = ui.input(|i| i.pointer.middle_down() || i.modifiers.shift);
            if middle_or_shift {
                ed.drag_mode = Some(CanvasDrag::Pan { last_pos: pos });
            } else if let Some(sel) = ed.selected_frame.clone() {
                if let Some(orig) = ed.atlas.frames.get(&sel).copied() {
                    let f_rect = egui::Rect::from_min_size(
                        egui::pos2(
                            img_rect.left() + orig.x as f32 * ed.zoom,
                            img_rect.top() + orig.y as f32 * ed.zoom,
                        ),
                        egui::vec2(orig.w as f32 * ed.zoom, orig.h as f32 * ed.zoom),
                    );
                    let handle =
                        egui::Rect::from_center_size(f_rect.right_bottom(), egui::vec2(12.0, 12.0));
                    let (px, py) = to_sheet_px(pos);
                    if handle.contains(pos) {
                        ed.drag_mode = Some(CanvasDrag::ResizeFrame { name: sel, orig });
                    } else if f_rect.contains(pos) {
                        ed.drag_mode = Some(CanvasDrag::MoveFrame {
                            name: sel,
                            orig,
                            anchor_px: (px, py),
                        });
                    } else {
                        start_draw_drag(ed, pos, img_rect);
                    }
                } else {
                    start_draw_drag(ed, pos, img_rect);
                }
            } else {
                start_draw_drag(ed, pos, img_rect);
            }
        }
    }

    if resp.dragged() {
        if let (Some(pos), Some(mode)) = (resp.interact_pointer_pos(), ed.drag_mode.clone()) {
            let sheet = ed.atlas.size;
            let sw_u = sheet.0.max(1);
            let sh_u = sheet.1.max(1);
            match mode {
                CanvasDrag::Pan { last_pos } => {
                    ed.pan += pos - last_pos;
                    ed.drag_mode = Some(CanvasDrag::Pan { last_pos: pos });
                }
                CanvasDrag::DrawNew { start, frame_name } => {
                    let (px, py) = to_sheet_px(pos);
                    let cur_x = (px.max(0) as u32).min(sw_u - 1);
                    let cur_y = (py.max(0) as u32).min(sh_u - 1);
                    let x0 = start.0.min(cur_x);
                    let y0 = start.1.min(cur_y);
                    let w = start.0.abs_diff(cur_x).max(1);
                    let h = start.1.abs_diff(cur_y).max(1);
                    let mut next = AtlasFrame { x: x0, y: y0, w, h };
                    clamp_frame(&mut next, sheet);
                    ed.atlas.frames.insert(frame_name, next);
                    ed.dirty = true;
                }
                CanvasDrag::MoveFrame {
                    name,
                    orig,
                    anchor_px,
                } => {
                    let (px, py) = to_sheet_px(pos);
                    let dx = px - anchor_px.0;
                    let dy = py - anchor_px.1;
                    let mut moved = orig;
                    moved.x = (orig.x as i32 + dx).max(0) as u32;
                    moved.y = (orig.y as i32 + dy).max(0) as u32;
                    clamp_frame(&mut moved, sheet);
                    if let Some(slot) = ed.atlas.frames.get_mut(&name) {
                        *slot = moved;
                        ed.dirty = true;
                    }
                }
                CanvasDrag::ResizeFrame { name, orig } => {
                    let (px, py) = to_sheet_px(pos);
                    let w = (px - orig.x as i32 + 1).max(1) as u32;
                    let h = (py - orig.y as i32 + 1).max(1) as u32;
                    let mut resized = AtlasFrame {
                        x: orig.x,
                        y: orig.y,
                        w,
                        h,
                    };
                    clamp_frame(&mut resized, sheet);
                    if let Some(slot) = ed.atlas.frames.get_mut(&name) {
                        *slot = resized;
                        ed.dirty = true;
                    }
                }
            }
        }
    }

    if resp.drag_stopped() {
        ed.drag_mode = None;
    }

    ui.label(
        egui::RichText::new(
            "Click: select • Drag: draw/move • Corner: resize • Wheel/Shift+drag: zoom/pan",
        )
        .size(10.0)
        .color(crate::theme::FAINT),
    );
}

fn start_draw_drag(ed: &mut AtlasEditorState, pos: egui::Pos2, img_rect: egui::Rect) {
    let sw = ed.atlas.size.0.max(1);
    let sh = ed.atlas.size.1.max(1);
    let px = (((pos.x - img_rect.left()) / ed.zoom).floor() as i32).clamp(0, sw as i32 - 1) as u32;
    let py = (((pos.y - img_rect.top()) / ed.zoom).floor() as i32).clamp(0, sh as i32 - 1) as u32;
    let frame_name = ed.next_frame_name("frame_");
    ed.atlas.frames.insert(
        frame_name.clone(),
        AtlasFrame {
            x: px,
            y: py,
            w: 1,
            h: 1,
        },
    );
    ed.select_frame(Some(frame_name.clone()));
    ed.dirty = true;
    ed.drag_mode = Some(CanvasDrag::DrawNew {
        start: (px, py),
        frame_name,
    });
}

fn show_grid_slicer(ui: &mut egui::Ui, ed: &mut AtlasEditorState) {
    ui.collapsing("Grid Slicer", |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut ed.slice_by_cols, false, "Tile Size");
            ui.selectable_value(&mut ed.slice_by_cols, true, "Cols × Rows");
        });
        ui.horizontal(|ui| {
            if ed.slice_by_cols {
                ui.label("Cols:");
                ui.add(egui::DragValue::new(&mut ed.slice_cols).range(1..=256));
                ui.label("Rows:");
                ui.add(egui::DragValue::new(&mut ed.slice_rows).range(1..=256));
            } else {
                ui.label("W:");
                ui.add(
                    egui::DragValue::new(&mut ed.slice_tile_w).range(1..=ed.atlas.size.0.max(1)),
                );
                ui.label("H:");
                ui.add(
                    egui::DragValue::new(&mut ed.slice_tile_h).range(1..=ed.atlas.size.1.max(1)),
                );
            }
        });
        ui.horizontal(|ui| {
            ui.label("Offset:");
            ui.add(egui::DragValue::new(&mut ed.slice_offset_x).range(0..=512));
            ui.add(egui::DragValue::new(&mut ed.slice_offset_y).range(0..=512));
            ui.label("Pad:");
            ui.add(egui::DragValue::new(&mut ed.slice_spacing_x).range(0..=256));
            ui.add(egui::DragValue::new(&mut ed.slice_spacing_y).range(0..=256));
        });
        ui.horizontal(|ui| {
            ui.label("Prefix:");
            ui.add(egui::TextEdit::singleline(&mut ed.slice_prefix).desired_width(90.0));
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut ed.slice_skip_transparent, "Skip empty");
            ui.checkbox(&mut ed.slice_replace, "Replace existing");
        });
        if ui.button("Slice Grid").clicked() {
            match ed.apply_grid_slice() {
                Ok(_) => ed.last_error = None,
                Err(e) => ed.last_error = Some(format!("{e:#}")),
            }
        }
    });
}

fn show_frames_section(ui: &mut egui::Ui, ed: &mut AtlasEditorState) {
    ui.collapsing("Frames", |ui| {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut ed.frame_filter)
                    .hint_text("Filter frames…")
                    .desired_width(120.0),
            );
            if ui
                .small_button("+ Full")
                .on_hover_text("Add full-sheet frame")
                .clicked()
            {
                ed.add_full_image_frame();
            }
            if let Some(sel) = ed.selected_frame.clone() {
                if ui
                    .small_button("Dup")
                    .on_hover_text("Duplicate selected frame")
                    .clicked()
                {
                    ed.duplicate_frame(&sel);
                }
                if ui
                    .small_button("Del")
                    .on_hover_text("Delete selected frame")
                    .clicked()
                {
                    ed.delete_frame(&sel);
                }
            }
        });

        let filter = ed.frame_filter.to_ascii_lowercase();
        let names: Vec<String> = ed
            .atlas
            .frames
            .keys()
            .filter(|n| filter.is_empty() || n.to_ascii_lowercase().contains(&filter))
            .cloned()
            .collect();

        let shift_held = ui.input(|i| i.modifiers.shift);
        let mut clicked_frame: Option<String> = None;

        egui::ScrollArea::vertical()
            .id_salt("atlas_frames_list")
            .max_height(110.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for name in &names {
                        let sel = ed.selected_frame.as_deref() == Some(name.as_str());
                        if ui.selectable_label(sel, name).clicked() {
                            clicked_frame = Some(name.clone());
                        }
                    }
                });
            });

        if let Some(name) = clicked_frame {
            if shift_held {
                if let (Some(anim), Some(anchor)) =
                    (ed.selected_anim.clone(), ed.range_anchor.clone())
                {
                    ed.append_frame_range_to_anim(&anim, &anchor, &name);
                }
            } else {
                ed.range_anchor = Some(name.clone());
            }
            ed.select_frame(Some(name));
        }

        if let Some(sel) = ed.selected_frame.clone() {
            if let Some(mut frame) = ed.atlas.frames.get(&sel).copied() {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut ed.frame_rename_buf).desired_width(130.0),
                    );
                    let enter = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (resp.lost_focus() || enter) && ed.frame_rename_buf.trim() != sel {
                        let target = ed.frame_rename_buf.clone();
                        if !ed.rename_frame(&sel, &target) {
                            ed.frame_rename_buf = sel.clone();
                        }
                    }
                });

                let (sw, sh) = ed.atlas.size;
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("X:");
                    changed |= ui
                        .add(egui::DragValue::new(&mut frame.x).range(0..=sw.saturating_sub(1)))
                        .changed();
                    ui.label("Y:");
                    changed |= ui
                        .add(egui::DragValue::new(&mut frame.y).range(0..=sh.saturating_sub(1)))
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("W:");
                    changed |= ui
                        .add(egui::DragValue::new(&mut frame.w).range(1..=sw.max(1)))
                        .changed();
                    ui.label("H:");
                    changed |= ui
                        .add(egui::DragValue::new(&mut frame.h).range(1..=sh.max(1)))
                        .changed();
                });
                if changed {
                    clamp_frame(&mut frame, ed.atlas.size);
                    let active_key = ed.selected_frame.clone().unwrap_or(sel);
                    if let Some(slot) = ed.atlas.frames.get_mut(&active_key) {
                        *slot = frame;
                        ed.dirty = true;
                    }
                }
            }
        }
    });
}

fn show_animations_section(ui: &mut egui::Ui, ed: &mut AtlasEditorState) {
    ui.collapsing("Animations", |ui| {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut ed.anim_new_name)
                    .hint_text("new_anim")
                    .desired_width(110.0),
            );
            if ui.small_button("+ Add").clicked() {
                let name = ed.anim_new_name.trim().to_string();
                if !name.is_empty() && !ed.atlas.animations.contains_key(&name) {
                    let seed = ed
                        .selected_frame
                        .clone()
                        .or_else(|| ed.atlas.frames.keys().next().cloned())
                        .into_iter()
                        .collect();
                    ed.atlas.animations.insert(
                        name.clone(),
                        AtlasAnimation {
                            frames: seed,
                            fps: 10.0,
                            looping: true,
                        },
                    );
                    ed.selected_anim = Some(name.clone());
                    ed.anim_rename_buf = name;
                    ed.anim_new_name.clear();
                    ed.anim_frame_idx = 0;
                    ed.anim_elapsed = 0.0;
                    ed.dirty = true;
                }
            }
        });

        let anim_names: Vec<String> = ed.atlas.animations.keys().cloned().collect();
        if anim_names.is_empty() {
            ui.label(
                egui::RichText::new("No animations yet. Add one above.")
                    .size(11.0)
                    .color(crate::theme::FAINT),
            );
            return;
        }

        if ed
            .selected_anim
            .as_ref()
            .is_none_or(|a| !ed.atlas.animations.contains_key(a))
        {
            ed.selected_anim = anim_names.first().cloned();
            ed.anim_rename_buf = ed.selected_anim.clone().unwrap_or_default();
        }

        let Some(active_name) = ed.selected_anim.clone() else {
            return;
        };

        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("atlas_anim_select")
                .width(140.0)
                .selected_text(&active_name)
                .show_ui(ui, |ui| {
                    for name in &anim_names {
                        if ui
                            .selectable_value(&mut ed.selected_anim, Some(name.clone()), name)
                            .changed()
                        {
                            ed.anim_rename_buf = name.clone();
                            ed.anim_frame_idx = 0;
                            ed.anim_elapsed = 0.0;
                        }
                    }
                });
            if ui.small_button("Del").clicked() {
                ed.atlas.animations.remove(&active_name);
                ed.selected_anim = ed.atlas.animations.keys().next().cloned();
                ed.anim_rename_buf = ed.selected_anim.clone().unwrap_or_default();
                ed.dirty = true;
            }
        });

        let Some(active_name) = ed.selected_anim.clone() else {
            return;
        };

        // Rename animation
        ui.horizontal(|ui| {
            ui.label("Rename:");
            let resp =
                ui.add(egui::TextEdit::singleline(&mut ed.anim_rename_buf).desired_width(110.0));
            let enter = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (resp.lost_focus() || enter) && ed.anim_rename_buf.trim() != active_name {
                let target = ed.anim_rename_buf.clone();
                if !ed.rename_animation(&active_name, &target) {
                    ed.anim_rename_buf = active_name.clone();
                }
            }
        });

        let Some(active_name) = ed.selected_anim.clone() else {
            return;
        };

        // Advance playback timer
        let now = Instant::now();
        let dt = ed
            .last_tick
            .map(|prev| (now - prev).as_secs_f64().min(0.1))
            .unwrap_or(0.0);
        ed.last_tick = Some(now);

        if let Some(anim) = ed.atlas.animations.get_mut(&active_name) {
            ui.horizontal(|ui| {
                ui.label("FPS:");
                if ui
                    .add(
                        egui::DragValue::new(&mut anim.fps)
                            .range(0.5..=120.0)
                            .speed(0.5),
                    )
                    .changed()
                {
                    ed.dirty = true;
                }
                if ui.checkbox(&mut anim.looping, "Loop").changed() {
                    ed.dirty = true;
                }
            });

            if !anim.frames.is_empty() {
                if ed.anim_frame_idx >= anim.frames.len() {
                    ed.anim_frame_idx = 0;
                }
                if ed.anim_playing && anim.fps > 0.0 {
                    let step = 1.0 / anim.fps as f64;
                    ed.anim_elapsed += dt;
                    while ed.anim_elapsed >= step {
                        ed.anim_elapsed -= step;
                        if ed.anim_frame_idx + 1 < anim.frames.len() {
                            ed.anim_frame_idx += 1;
                        } else if anim.looping {
                            ed.anim_frame_idx = 0;
                        } else {
                            ed.anim_playing = false;
                            break;
                        }
                    }
                    ui.ctx().request_repaint();
                }
            }
        }

        // Preview thumbnail + playback controls
        let preview_frame = ed.atlas.animations.get(&active_name).and_then(|anim| {
            let fname = anim.frames.get(ed.anim_frame_idx)?;
            ed.atlas.frames.get(fname).copied()
        });
        let total_steps = ed
            .atlas
            .animations
            .get(&active_name)
            .map(|a| a.frames.len())
            .unwrap_or(0);

        ui.horizontal(|ui| {
            let (p_rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
            let painter = ui.painter_at(p_rect);
            draw_checkerboard(&painter, p_rect);
            if let (Some(tex), Some(fr)) = (&ed.sheet_tex, preview_frame) {
                let uv_arr = fr.uv(ed.atlas.size);
                let uv = egui::Rect::from_min_max(
                    egui::pos2(uv_arr[0], uv_arr[1]),
                    egui::pos2(uv_arr[2], uv_arr[3]),
                );
                let scale = (48.0 / fr.w.max(1) as f32).min(48.0 / fr.h.max(1) as f32);
                let dst = egui::Rect::from_center_size(
                    p_rect.center(),
                    egui::vec2(fr.w as f32 * scale, fr.h as f32 * scale),
                );
                painter.image(tex.id(), dst, uv, egui::Color32::WHITE);
            }
            painter.rect_stroke(
                p_rect,
                2.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 49, 56)),
                egui::StrokeKind::Inside,
            );

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let play_icon = if ed.anim_playing {
                        regular::PAUSE
                    } else {
                        regular::PLAY
                    };
                    if ui.small_button(play_icon).clicked() {
                        ed.anim_playing = !ed.anim_playing;
                    }
                    if ui.small_button("<").clicked() && total_steps > 0 {
                        ed.anim_playing = false;
                        ed.anim_frame_idx = if ed.anim_frame_idx == 0 {
                            total_steps - 1
                        } else {
                            ed.anim_frame_idx - 1
                        };
                    }
                    if ui.small_button(">").clicked() && total_steps > 0 {
                        ed.anim_playing = false;
                        ed.anim_frame_idx = (ed.anim_frame_idx + 1) % total_steps;
                    }
                });
                ui.label(
                    egui::RichText::new(if total_steps == 0 {
                        "0 / 0".to_string()
                    } else {
                        format!("{} / {total_steps}", ed.anim_frame_idx + 1)
                    })
                    .size(11.0)
                    .color(crate::theme::FAINT),
                );
            });
        });

        // Sequence step editor + quick append
        let all_frames: Vec<String> = ed.atlas.frames.keys().cloned().collect();
        let mut add_frame_choice: Option<String> = None;
        if let Some(sel) = &ed.selected_frame {
            ui.horizontal(|ui| {
                if ui.small_button(format!("+ Append '{sel}'")).clicked() {
                    add_frame_choice = Some(sel.clone());
                }
                ui.label(
                    egui::RichText::new("(Shift+click Frames above for range)")
                        .size(10.0)
                        .color(crate::theme::FAINT),
                );
            });
        }
        if !all_frames.is_empty() {
            egui::ComboBox::from_id_salt("atlas_anim_add_frame")
                .width(160.0)
                .selected_text("+ Add frame step…")
                .show_ui(ui, |ui| {
                    for f in &all_frames {
                        if ui.selectable_label(false, f).clicked() {
                            add_frame_choice = Some(f.clone());
                        }
                    }
                });
        }
        if let Some(frame_name) = add_frame_choice {
            if let Some(anim) = ed.atlas.animations.get_mut(&active_name) {
                anim.frames.push(frame_name);
                ed.dirty = true;
            }
        }

        if let Some(anim) = ed.atlas.animations.get_mut(&active_name) {
            let mut swap: Option<(usize, usize)> = None;
            let mut dup_idx: Option<usize> = None;
            let mut rm_idx: Option<usize> = None;
            let len = anim.frames.len();
            egui::ScrollArea::vertical()
                .id_salt("atlas_anim_steps")
                .max_height(120.0)
                .show(ui, |ui| {
                    for (idx, fname) in anim.frames.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let cur = idx == ed.anim_frame_idx;
                            ui.label(egui::RichText::new(format!("{}. {fname}", idx + 1)).color(
                                if cur {
                                    crate::theme::ACCENT
                                } else {
                                    crate::theme::TEXT
                                },
                            ));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("✕").clicked() {
                                        rm_idx = Some(idx);
                                    }
                                    if ui
                                        .small_button("+")
                                        .on_hover_text("Duplicate step")
                                        .clicked()
                                    {
                                        dup_idx = Some(idx);
                                    }
                                    if idx + 1 < len && ui.small_button("↓").clicked() {
                                        swap = Some((idx, idx + 1));
                                    }
                                    if idx > 0 && ui.small_button("↑").clicked() {
                                        swap = Some((idx, idx - 1));
                                    }
                                },
                            );
                        });
                    }
                });
            if let Some((a, b)) = swap {
                anim.frames.swap(a, b);
                ed.dirty = true;
            }
            if let Some(i) = dup_idx {
                let copy = anim.frames[i].clone();
                anim.frames.insert(i + 1, copy);
                ed.dirty = true;
            }
            if let Some(i) = rm_idx {
                anim.frames.remove(i);
                ed.dirty = true;
            }
        }
    });
}
