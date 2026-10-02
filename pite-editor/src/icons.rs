// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hand-drawn vector icon buttons. Glyphs are painted with the egui painter
//! (never font glyphs) so they render identically with any bundled font.

use eframe::egui;

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Pause,
    Stop,
    Save,
    Add,
    Script,
}

/// A 26x22 click target with a painted glyph. Returns the response so callers
/// check `.clicked()`.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 22.0), egui::Sense::click());
    let resp = resp.on_hover_text(tooltip);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
        }
        let fg = if resp.hovered() {
            crate::theme::TEXT
        } else {
            crate::theme::FAINT
        };
        draw_icon(p, rect, icon, fg);
    }
    resp
}

pub fn draw_icon(p: &egui::Painter, rect: egui::Rect, icon: Icon, fg: egui::Color32) {
    let c = rect.center();
    match icon {
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::Pos2::new(c.x - 3.5, c.y - 5.5),
                    egui::Pos2::new(c.x - 3.5, c.y + 5.5),
                    egui::Pos2::new(c.x + 5.0, c.y),
                ],
                fg,
                egui::Stroke::NONE,
            ));
        }
        Icon::Pause => {
            p.rect_filled(
                egui::Rect::from_min_size(
                    egui::Pos2::new(c.x - 4.5, c.y - 5.0),
                    egui::vec2(3.2, 10.0),
                ),
                1.0,
                fg,
            );
            p.rect_filled(
                egui::Rect::from_min_size(
                    egui::Pos2::new(c.x + 1.3, c.y - 5.0),
                    egui::vec2(3.2, 10.0),
                ),
                1.0,
                fg,
            );
        }
        Icon::Stop => {
            p.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(9.5, 9.5)),
                1.5,
                fg,
            );
        }
        Icon::Save => {
            let body = egui::Rect::from_center_size(c, egui::vec2(12.0, 12.0));
            p.rect_stroke(
                body,
                1.5,
                egui::Stroke::new(1.6_f32, fg),
                egui::StrokeKind::Middle,
            );
            p.rect_filled(
                egui::Rect::from_min_size(
                    egui::Pos2::new(c.x - 2.5, c.y - 6.0),
                    egui::vec2(5.0, 4.0),
                ),
                0.5,
                fg,
            );
            p.rect_filled(
                egui::Rect::from_min_size(
                    egui::Pos2::new(c.x - 3.5, c.y + 1.0),
                    egui::vec2(7.0, 5.0),
                ),
                0.5,
                fg,
            );
        }
        Icon::Add => {
            const BAR: f32 = 1.9;
            const ARM: f32 = 5.5;
            p.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(ARM * 2.0, BAR)),
                0.0,
                fg,
            );
            p.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(BAR, ARM * 2.0)),
                0.0,
                fg,
            );
        }
        Icon::Script => {
            let page = egui::Rect::from_center_size(c, egui::vec2(8.0, 10.5));
            p.rect_stroke(
                page,
                1.2,
                egui::Stroke::new(1.2_f32, fg),
                egui::StrokeKind::Middle,
            );
            let inset = page.width() * 0.26;
            for row in [-0.8_f32, 0.8] {
                let y = c.y + row * page.height() * 0.2 + 0.2;
                p.line_segment(
                    [
                        egui::Pos2::new(page.left() + inset + 1.0, y),
                        egui::Pos2::new(page.right() - inset, y),
                    ],
                    egui::Stroke::new(1.1_f32, fg),
                );
            }
        }
    }
}
