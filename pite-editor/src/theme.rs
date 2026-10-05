// SPDX-License-Identifier: MIT OR Apache-2.0
use eframe::egui;

const ACCENT: egui::Color32 = egui::Color32::from_rgb(11, 147, 135);
pub(crate) const BG: egui::Color32 = egui::Color32::from_rgb(11, 14, 17);
const PANEL: egui::Color32 = egui::Color32::from_rgb(20, 24, 29);
const PANEL_STROKE: egui::Color32 = egui::Color32::from_rgb(42, 49, 56);
pub(crate) const TEXT: egui::Color32 = egui::Color32::from_rgb(230, 233, 236);
pub(crate) const FAINT: egui::Color32 = egui::Color32::from_rgb(154, 163, 173);
const INACTIVE: egui::Color32 = egui::Color32::from_rgb(23, 28, 34);
const HOVERED: egui::Color32 = egui::Color32::from_rgb(31, 38, 46);
const ACTIVE: egui::Color32 = egui::Color32::from_rgb(30, 58, 53);

pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_visuals(visuals());
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.indent = 20.0;
        style.spacing.scroll.bar_width = 10.0;
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(15.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(15.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(20.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(14.0, egui::FontFamily::Monospace),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::new(12.0, egui::FontFamily::Proportional),
        );
    });
}

fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".to_string(),
        egui::FontData::from_static(include_bytes!("../../fonts/Inter-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "JetBrainsMono".to_string(),
        egui::FontData::from_static(include_bytes!("../../fonts/JetBrainsMono-Regular.ttf")).into(),
    );
    fonts
        .families
        .insert(egui::FontFamily::Proportional, vec!["Inter".to_string()]);
    fonts.families.insert(
        egui::FontFamily::Monospace,
        vec!["JetBrainsMono".to_string()],
    );
    fonts
}

fn widget(fill: egui::Color32) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: egui::Stroke::new(1.0_f32, PANEL_STROKE),
        fg_stroke: egui::Stroke::new(1.0_f32, TEXT),
        corner_radius: 6.0.into(),
        expansion: 0.0,
    }
}

fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.dark_mode = true;
    visuals.override_text_color = Some(TEXT);
    visuals.weak_text_color = Some(FAINT);
    visuals.faint_bg_color = PANEL;
    visuals.extreme_bg_color = BG;
    visuals.code_bg_color = BG;
    visuals.window_fill = PANEL;
    visuals.panel_fill = PANEL;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, PANEL_STROKE);
    visuals.hyperlink_color = ACCENT;
    // Selection = palette `dark` fill with near-white glyphs (simulated 7.9;
    // the allow-listed `dark -> white` pair). Solid `dark` beats the old
    // 20%-tint wash for region visibility while staying quieter than solid
    // primary; egui paints selected glyphs with `selection.stroke.color`.
    visuals.selection.bg_fill = egui::Color32::from_rgb(18, 77, 73);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, TEXT);
    visuals.widgets.noninteractive = widget(PANEL);
    visuals.widgets.inactive = widget(INACTIVE);
    visuals.widgets.hovered = widget(HOVERED);
    visuals.widgets.active = widget(ACTIVE);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.open = widget(INACTIVE);
    visuals.window_corner_radius = 8.0.into();
    visuals.menu_corner_radius = 6.0.into();
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_applies_cleanly() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let style = ctx.global_style();
        assert!(style.visuals.dark_mode);
        assert_eq!(style.visuals.window_fill, PANEL);
        assert_eq!(style.text_styles[&egui::TextStyle::Body].size, 15.0);
        assert_eq!(style.visuals.selection.stroke.color, TEXT);
    }
}
