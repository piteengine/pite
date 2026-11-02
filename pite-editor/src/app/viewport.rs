// SPDX-License-Identifier: MIT OR Apache-2.0
use std::time::Instant;

use eframe::egui;
use pite_core::PropValue;
use pite_render::{OffscreenRenderer, Renderer2D};
use pite_runtime::GameSession;

use super::EditorApp;

/// egui keys forwarded to the running scene, with the `pite.held` /
/// `pite.pressed` names scripts use. Level-read each frame while playing.
const GAME_KEYS: &[(egui::Key, &str)] = &[
    (egui::Key::Space, "Space"),
    (egui::Key::Enter, "Enter"),
    (egui::Key::Escape, "Escape"),
    (egui::Key::Tab, "Tab"),
    (egui::Key::Backspace, "Backspace"),
    (egui::Key::ArrowLeft, "ArrowLeft"),
    (egui::Key::ArrowRight, "ArrowRight"),
    (egui::Key::ArrowUp, "ArrowUp"),
    (egui::Key::ArrowDown, "ArrowDown"),
    (egui::Key::A, "A"),
    (egui::Key::B, "B"),
    (egui::Key::C, "C"),
    (egui::Key::D, "D"),
    (egui::Key::E, "E"),
    (egui::Key::F, "F"),
    (egui::Key::G, "G"),
    (egui::Key::H, "H"),
    (egui::Key::I, "I"),
    (egui::Key::J, "J"),
    (egui::Key::K, "K"),
    (egui::Key::L, "L"),
    (egui::Key::M, "M"),
    (egui::Key::N, "N"),
    (egui::Key::O, "O"),
    (egui::Key::P, "P"),
    (egui::Key::Q, "Q"),
    (egui::Key::R, "R"),
    (egui::Key::S, "S"),
    (egui::Key::T, "T"),
    (egui::Key::U, "U"),
    (egui::Key::V, "V"),
    (egui::Key::W, "W"),
    (egui::Key::X, "X"),
    (egui::Key::Y, "Y"),
    (egui::Key::Z, "Z"),
    (egui::Key::Num0, "0"),
    (egui::Key::Num1, "1"),
    (egui::Key::Num2, "2"),
    (egui::Key::Num3, "3"),
    (egui::Key::Num4, "4"),
    (egui::Key::Num5, "5"),
    (egui::Key::Num6, "6"),
    (egui::Key::Num7, "7"),
    (egui::Key::Num8, "8"),
    (egui::Key::Num9, "9"),
];

/// Cursor in offscreen pixels from an egui pointer position. The texture
/// stretches uniformly into its rect, so this is a pure rescale.
pub(crate) fn viewport_cursor(
    rect: egui::Rect,
    view: (u32, u32),
    pointer: egui::Pos2,
) -> Option<(f64, f64)> {
    if view.0 == 0 || view.1 == 0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    let x = (pointer.x - rect.min.x) as f64 / rect.width() as f64 * view.0 as f64;
    let y = (pointer.y - rect.min.y) as f64 / rect.height() as f64 * view.1 as f64;
    Some((x, y))
}

/// Largest (width, height) with `view`'s aspect that fits inside `avail`.
/// "Contain", not "fill": whichever axis binds first decides the scale, so
/// the viewport grows and shrinks on both axes instead of blowing past the
/// panel on the height axis when the editor is wide.
pub(crate) fn fit_view(avail: egui::Vec2, view: (u32, u32)) -> egui::Vec2 {
    let aspect = (view.0 as f32 / view.1.max(1) as f32).max(0.0001);
    let avail = egui::Vec2::new(avail.x.max(1.0), avail.y.max(1.0));
    if avail.x / avail.y > aspect {
        egui::Vec2::new(avail.y * aspect, avail.y)
    } else {
        egui::Vec2::new(avail.x, avail.x / aspect)
    }
}

impl EditorApp {
    pub(super) fn toggle_play(&mut self) {
        self.playing = !self.playing;
        self.last_frame = Instant::now();
        pite_script::input_clear();
    }

    /// Forward viewport input to the running scene's script input state.
    /// Reads level state (`key_down`) and lets `input_set_key` derive edges;
    /// runs before `session.frame()` so scripts consume this frame's state.
    pub(super) fn pump_game_input(&mut self, ctx: &egui::Context) {
        let (view_w, view_h) = self.view_size;
        self.session.set_viewport(view_w, view_h);
        if !(self.playing && self.viewport_hovered) {
            return;
        }
        let rect = self.viewport_rect;
        ctx.input(|i| {
            for (key, name) in GAME_KEYS {
                pite_script::input_set_key(name, i.key_down(*key));
            }
            pite_script::input_set_key("Shift", i.modifiers.shift);
            pite_script::input_set_key("Control", i.modifiers.ctrl);
            pite_script::input_set_key("Alt", i.modifiers.alt);
            if let (Some(rect), Some(pos)) = (rect, i.pointer.hover_pos()) {
                if let Some((x, y)) = viewport_cursor(rect, (view_w, view_h), pos) {
                    pite_script::input_set_mouse(x, y);
                }
            }
            pite_script::input_set_key(
                "MouseLeft",
                i.pointer.button_down(egui::PointerButton::Primary),
            );
            pite_script::input_set_key(
                "MouseRight",
                i.pointer.button_down(egui::PointerButton::Secondary),
            );
            pite_script::input_set_key(
                "MouseMiddle",
                i.pointer.button_down(egui::PointerButton::Middle),
            );
        });
    }

    pub(super) fn stop(&mut self) {
        pite_script::input_clear();
        match GameSession::open(&self.scene_path, false) {
            Ok(session) => {
                self.session = session;
                self.playing = false;
                self.viewport_dirty = true;
                self.log("stopped, scene reset.");
            }
            Err(e) => self.log(format!("stop failed: {e:#}")),
        }
    }

    pub(super) fn show_viewport(&mut self, ui: &mut egui::Ui) {
        let (view_w, view_h) = self.view_size;
        let want_refresh = self.playing || self.viewport_dirty;
        if self.viewport.is_none() && !self.viewport_failed {
            match OffscreenRenderer::new_offscreen(view_w, view_h) {
                Ok(r) => {
                    self.viewport = Some(r);
                }
                Err(e) => {
                    self.viewport_failed = true;
                    self.console
                        .push(format!("viewport GPU unavailable, using fallback: {e:#}"));
                }
            }
        }
        let mut fresh_rgba: Option<Vec<u8>> = None;
        let mut render_err: Option<String> = None;
        if let Some(renderer) = self.viewport.as_mut() {
            if want_refresh {
                // The offscreen buffer is the design size: sync the session
                // viewport here, not only while playing, or a fresh or just-
                // stopped session still carries the (800, 600) init and the
                // preview draws shrunken.
                self.session.set_viewport(view_w, view_h);
                let draw = renderer.begin_frame().and_then(|()| {
                    self.session.draw_into(renderer)?;
                    renderer.render_to_rgba()
                });
                match draw {
                    Ok(rgba) => fresh_rgba = Some(rgba),
                    Err(e) => render_err = Some(format!("viewport render failed: {e:#}")),
                }
            }
        }
        if let Some(rgba) = fresh_rgba {
            let image =
                egui::ColorImage::from_rgba_unmultiplied([view_w as usize, view_h as usize], &rgba);
            match self.viewport_tex.as_mut() {
                Some(handle) => handle.set(image, egui::TextureOptions::NEAREST),
                None => {
                    let handle = ui.ctx().load_texture(
                        "pite-viewport",
                        image,
                        egui::TextureOptions::NEAREST,
                    );
                    self.viewport_tex = Some(handle);
                }
            }
            self.viewport_dirty = false;
        }
        if let Some(err) = render_err {
            self.log(err);
        }
        let textured = self.viewport.is_some() && self.viewport_tex.is_some();
        let avail = ui.available_size();
        let fitted = fit_view(avail, (view_w, view_h));
        // Center the fitted rect in the space we measured: allocate the full
        // avail, then paint into a centered sub-rect, so the viewport reads as
        // framed content instead of content pinned to the top-left corner.
        let (outer, _outer_painter) = ui.allocate_painter(avail, egui::Sense::hover());
        let view_rect = egui::Rect::from_center_size(outer.rect.center(), fitted);
        let resp = ui.interact(
            view_rect,
            ui.scope_id().with("viewport"),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(view_rect);
        self.viewport_hovered = resp.hovered();
        self.viewport_rect = Some(view_rect);
        if textured {
            if let Some(handle) = self.viewport_tex.as_ref() {
                egui::Image::from_texture(handle).paint_at(ui, view_rect);
            }
        } else {
            let nodes: Vec<(String, String, (f64, f64), String, (f64, f64))> =
                self.session.host().with_tree(|t| {
                    t.iter()
                        .map(|n| {
                            let pos = match n.props.get("position") {
                                Some(PropValue::Vec2(x, y)) => (*x, *y),
                                _ => (0.0, 0.0),
                            };
                            let text = match n.props.get("text") {
                                Some(PropValue::Str(s)) => s.clone(),
                                _ => String::new(),
                            };
                            let size = match n.props.get("size") {
                                Some(PropValue::Vec2(w, h)) => (*w, *h),
                                _ => (0.0, 0.0),
                            };
                            (n.id.to_string(), n.type_name.clone(), pos, text, size)
                        })
                        .collect()
                });
            let center = view_rect.center();
            painter.rect_filled(view_rect, 0.0, crate::theme::BG);
            for (id, type_name, (x, y), text, (w, h)) in nodes {
                let p = center + egui::Vec2::new(x as f32, y as f32);
                let color = if type_name == "Sprite2D" {
                    egui::Color32::from_rgb(111, 179, 167)
                } else {
                    egui::Color32::from_rgb(124, 155, 184)
                };
                painter.circle_filled(p, 6.0, color);
                if type_name == "Button" && w > 0.0 && h > 0.0 {
                    painter.rect_stroke(
                        egui::Rect::from_center_size(p, egui::Vec2::new(w as f32, h as f32)),
                        4.0,
                        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(111, 179, 167)),
                        egui::StrokeKind::Middle,
                    );
                }
                let caption = if text.is_empty() { id } else { text };
                painter.text(
                    p + egui::Vec2::new(10.0, -10.0),
                    egui::Align2::LEFT_TOP,
                    caption,
                    egui::FontId::monospace(11.0),
                    egui::Color32::from_rgb(201, 209, 216),
                );
            }
        }
        if !self.playing {
            draw_collision_overlays(&self.session, &painter, view_rect, (view_w, view_h));
        }
    }
}

struct CollisionGizmo {
    shape_type: String,
    center: (f64, f64),
    size: (f64, f64),
    radius: f64,
    height: f64,
    disabled: bool,
}

fn draw_collision_overlays(
    session: &GameSession,
    painter: &egui::Painter,
    view_rect: egui::Rect,
    view_size: (u32, u32),
) {
    let gizmos: Vec<CollisionGizmo> = session.host().with_tree(|tree| {
        tree.iter()
            .filter(|n| n.type_name == "CollisionShape2D")
            .map(|n| {
                let shape_type = match n.props.get("shape") {
                    Some(PropValue::Str(s)) => s.clone(),
                    _ => "box".to_string(),
                };
                let size = match n.props.get("size") {
                    Some(PropValue::Vec2(w, h)) => (*w, *h),
                    _ => (32.0, 32.0),
                };
                let radius = match n.props.get("radius") {
                    Some(PropValue::Num(r)) => *r,
                    _ => 16.0,
                };
                let height = match n.props.get("height") {
                    Some(PropValue::Num(h)) => *h,
                    _ => 32.0,
                };
                let disabled = match n.props.get("disabled") {
                    Some(PropValue::Bool(b)) => *b,
                    _ => false,
                };
                let center = pite_core::global_position(tree, &n.id);
                CollisionGizmo {
                    shape_type,
                    center,
                    size,
                    radius,
                    height,
                    disabled,
                }
            })
            .collect()
    });

    if gizmos.is_empty() {
        return;
    }

    let (cam, _) = session.camera_view();
    let zoom = session.effective_zoom();
    let (view_w, view_h) = view_size;
    let scale = (view_rect.width() / view_w as f32) * zoom as f32;

    for gizmo in gizmos {
        let (sx, sy) = pite_render::world_to_screen(gizmo.center, cam, zoom, (view_w, view_h));
        let vp_x = view_rect.min.x + (sx / view_w as f32) * view_rect.width();
        let vp_y = view_rect.min.y + (sy / view_h as f32) * view_rect.height();
        let center_pos = egui::pos2(vp_x, vp_y);

        let (fill_col, stroke_col) = if gizmo.disabled {
            (
                egui::Color32::from_rgba_unmultiplied(160, 160, 160, 35),
                egui::Color32::from_rgb(160, 160, 160),
            )
        } else {
            (
                egui::Color32::from_rgba_unmultiplied(72, 220, 140, 50),
                egui::Color32::from_rgb(72, 220, 140),
            )
        };
        let stroke = egui::Stroke::new(1.5, stroke_col);

        match gizmo.shape_type.as_str() {
            "circle" => {
                let r = (gizmo.radius as f32 * scale).max(1.0);
                painter.circle_filled(center_pos, r, fill_col);
                painter.circle_stroke(center_pos, r, stroke);
            }
            "capsule" => {
                let r = (gizmo.radius as f32 * scale).max(1.0);
                let half_h = (gizmo.height as f32 * 0.5 * scale).max(0.0);
                let rect = egui::Rect::from_center_size(
                    center_pos,
                    egui::vec2(r * 2.0, half_h * 2.0 + r * 2.0),
                );
                painter.rect_filled(rect, r, fill_col);
                painter.rect_stroke(rect, r, stroke, egui::StrokeKind::Middle);
            }
            _ => {
                let w = (gizmo.size.0 as f32 * scale).max(2.0);
                let h = (gizmo.size.1 as f32 * scale).max(2.0);
                let rect = egui::Rect::from_center_size(center_pos, egui::vec2(w, h));
                painter.rect_filled(rect, 0.0, fill_col);
                painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Middle);
            }
        }
    }
}
