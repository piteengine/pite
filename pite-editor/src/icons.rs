// SPDX-License-Identifier: MIT OR Apache-2.0

//! Phosphor vector icon buttons and helpers.

use eframe::egui;
use egui_phosphor::regular;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Stop,
    Save,
    Add,
    Script,
    Folder,
    FolderOpen,
    FolderPlus,
    FilePlus,
    Scene,
    Gear,
    Image,
    Audio,
    File,
    Trash,
    Copy,
    Scissors,
    Pencil,
    CaretRight,
    CaretDown,
}

impl Icon {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Play => regular::PLAY,
            Self::Pause => regular::PAUSE,
            Self::Stop => regular::STOP,
            Self::Save => regular::FLOPPY_DISK,
            Self::Add => regular::PLUS,
            Self::Script => regular::FILE_CODE,
            Self::Folder => regular::FOLDER,
            Self::FolderOpen => regular::FOLDER_OPEN,
            Self::FolderPlus => regular::FOLDER_PLUS,
            Self::FilePlus => regular::FILE_PLUS,
            Self::Scene => regular::FILM_SLATE,
            Self::Gear => regular::GEAR,
            Self::Image => regular::IMAGE,
            Self::Audio => regular::SPEAKER_HIGH,
            Self::File => regular::FILE,
            Self::Trash => regular::TRASH,
            Self::Copy => regular::COPY,
            Self::Scissors => regular::SCISSORS,
            Self::Pencil => regular::PENCIL_SIMPLE,
            Self::CaretRight => regular::CARET_RIGHT,
            Self::CaretDown => regular::CARET_DOWN,
        }
    }
}

/// A 26x22 click target with an icon glyph. Returns the response so callers check `.clicked()`.
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
    let font_size = (rect.height() * 0.75).clamp(11.0, 16.0);
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon.as_str(),
        egui::FontId::new(font_size, egui::FontFamily::Proportional),
        fg,
    );
}
