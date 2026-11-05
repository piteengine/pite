// SPDX-License-Identifier: MIT OR Apache-2.0
mod assets;
mod code;
mod inspector;
mod scene;
mod settings;
#[cfg(test)]
mod tests;
mod tree;
mod viewport;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Instant;

use anyhow::Result;
use eframe::egui;
use pite_core::NodeId;
use pite_render::OffscreenRenderer;
use pite_runtime::GameSession;

use crate::icons::{self, Icon};
use crate::lsp::{self, LspClient, LspOutcome};
use crate::ops;

use settings::SettingsDraft;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Tab {
    Viewport,
    Code,
}

/// Helper for menus to ensure separators appear only between non-empty sections,
/// avoiding double separators or trailing separators.
pub(crate) struct MenuSeparatorTracker<'a> {
    pub ui: &'a mut egui::Ui,
    has_previous_items: bool,
    needs_separator: bool,
}

impl<'a> MenuSeparatorTracker<'a> {
    pub fn new(ui: &'a mut egui::Ui) -> Self {
        Self {
            ui,
            has_previous_items: false,
            needs_separator: false,
        }
    }

    pub fn start_section(&mut self) {
        if self.has_previous_items {
            self.needs_separator = true;
        }
    }

    fn ensure_separator(&mut self) {
        if self.needs_separator {
            self.ui.separator();
            self.needs_separator = false;
        }
    }

    pub fn button(&mut self, text: impl Into<egui::WidgetText>) -> egui::Response {
        self.ensure_separator();
        self.has_previous_items = true;
        self.ui.button(text)
    }

    pub fn add_enabled(&mut self, enabled: bool, widget: impl egui::Widget) -> egui::Response {
        self.ensure_separator();
        self.has_previous_items = true;
        self.ui.add_enabled(enabled, widget)
    }

    pub fn close(&mut self) {
        self.ui.close();
    }

    pub fn ctx(&self) -> &egui::Context {
        self.ui.ctx()
    }
}

pub fn launch(scene: &Path) -> Result<()> {
    let app = EditorApp::new(scene)?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_maximized(true)
            .with_icon(
                eframe::icon_data::from_png_bytes(pite_runtime::DEFAULT_ICON_PNG)
                    .expect("default window icon must be a valid PNG"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "Pite Editor",
        options,
        Box::new(|cc| {
            crate::theme::apply(&cc.egui_ctx);
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| anyhow::anyhow!("editor failed: {e}"))?;
    Ok(())
}

pub struct EditorApp {
    pub(super) session: GameSession,
    pub(super) scene_path: PathBuf,
    pub(super) project_dir: Option<PathBuf>,
    pub(super) selected: Option<String>,
    pub(super) console: Vec<String>,
    pub(super) seen_errors: HashSet<String>,
    pub(super) code_file: Option<PathBuf>,
    pub(super) code_text: String,
    pub(super) lsp: Option<LspClient>,
    pub(super) lsp_starter: Option<Receiver<LspOutcome>>,
    pub(super) lsp_status: String,
    pub(super) lsp_hint: String,
    pub(super) lsp_completions: Vec<lsp::CompletionItem>,
    pub(super) lsp_completion_open: bool,
    pub(super) lsp_completion_offset: usize,
    pub(super) lsp_completion_pending: Vec<(i64, usize)>,
    pub(super) lsp_completion_selected: usize,
    /// Previous frame's galley row tops (galley space) for the code editor:
    /// pixel-snapped, so the gutter tiles exactly instead of drifting on an
    /// assumed pitch. Empty until the first paint.
    pub(super) code_row_tops: Vec<f32>,
    /// `galley_pos.y - widget_top.y`: scroll-invariant content offset of the
    /// galley origin, paired with `code_row_tops`.
    pub(super) code_galley_dy: f32,
    pub(super) playing: bool,
    pub(super) last_frame: Instant,
    pub(super) add_node_modal: Option<tree::AddNodeModal>,
    pub(super) rename_node_modal: Option<tree::RenameNodeModal>,
    pub(super) instance_modal: Option<tree::InstanceSceneModal>,
    pub(super) hovered_node_target: Option<(String, tree::DropZone)>,
    pub(super) settings: Option<SettingsDraft>,
    pub(super) open_nodes: HashSet<String>,
    pub(super) open_seen: HashSet<String>,
    pub(super) viewport: Option<OffscreenRenderer>,
    pub(super) viewport_failed: bool,
    pub(super) viewport_tex: Option<egui::TextureHandle>,
    pub(super) viewport_dirty: bool,
    /// Pointer was over the viewport last frame: game input goes to the
    /// running scene only then, so editing text and shortcuts keep priority.
    pub(super) viewport_hovered: bool,
    pub(super) viewport_rect: Option<egui::Rect>,
    /// Bound to the project's window size: the viewport must frame the scene the
    /// way the game window does.
    pub(super) view_size: (u32, u32),
    pub(super) tab: Tab,
    pub(super) recent: Vec<PathBuf>,
    pub(super) about_open: bool,
    pub(super) selected_asset: Option<String>,
    pub(super) asset_modal: Option<assets::AssetModal>,
    pub(super) asset_clipboard: Option<assets::AssetClipboardEntry>,
    pub(super) hovered_asset_folder: Option<String>,
}

impl EditorApp {
    pub fn new(scene: &Path) -> Result<Self> {
        let mut session = GameSession::open(scene, false)?;
        session.set_pause_on_error(true);
        let project_dir = pite_project::find_project_root(scene);
        let view_size = project_dir
            .as_deref()
            .and_then(|dir| pite_project::load_manifest(dir).ok())
            .map_or_else(
                || {
                    let d = pite_project::ProjectMeta::default();
                    (d.window_width, d.window_height)
                },
                |m| (m.project.window_width, m.project.window_height),
            );
        Ok(Self {
            session,
            scene_path: scene.to_path_buf(),
            project_dir,
            selected: None,
            console: vec!["Pite editor ready.".to_string()],
            seen_errors: HashSet::new(),
            code_file: None,
            code_text: String::new(),
            lsp: None,
            lsp_starter: None,
            lsp_status: "lsp: off".to_string(),
            lsp_hint: String::new(),
            lsp_completions: Vec::new(),
            lsp_completion_open: false,
            lsp_completion_offset: 0,
            lsp_completion_pending: Vec::new(),
            lsp_completion_selected: 0,
            code_row_tops: Vec::new(),
            code_galley_dy: 0.0,
            playing: false,
            last_frame: Instant::now(),
            add_node_modal: None,
            rename_node_modal: None,
            instance_modal: None,
            hovered_node_target: None,
            settings: None,
            open_nodes: HashSet::new(),
            open_seen: HashSet::new(),
            viewport: None,
            viewport_failed: false,
            viewport_tex: None,
            viewport_hovered: false,
            viewport_rect: None,
            view_size,
            viewport_dirty: true,
            tab: Tab::Viewport,
            recent: Self::load_recent(),
            about_open: false,
            selected_asset: None,
            asset_modal: None,
            asset_clipboard: None,
            hovered_asset_folder: None,
        })
    }

    pub(super) fn log(&mut self, line: impl Into<String>) {
        self.console.push(line.into());
        if self.console.len() > 500 {
            self.console.drain(..self.console.len() - 500);
        }
    }

    pub(super) fn drain_script_errors(&mut self) {
        if let Some(err) = self.session.take_last_error() {
            self.playing = false;
            let log_msg = if !err.traceback.is_empty() {
                format!("{}\n{}", err.message, err.traceback)
            } else {
                err.message.clone()
            };
            self.log(log_msg);
            if let Some(file) = &err.file {
                let p = Path::new(file);
                if p.exists() {
                    self.open_asset(p);
                }
            }
        }
        for err in self.session.errors() {
            if self.seen_errors.insert(err.clone()) {
                self.log(err);
            }
        }
    }

    pub(super) fn is_open(&mut self, id: &str) -> bool {
        if !self.open_seen.contains(id) {
            self.open_seen.insert(id.to_string());
            self.open_nodes.insert(id.to_string());
            return true;
        }
        self.open_nodes.contains(id)
    }

    pub(super) fn debug_info(&mut self, ctx: &egui::Context) {
        let lines = [
            format!("pite {}", env!("CARGO_PKG_VERSION")),
            format!("scene {}", self.scene_path.display()),
            format!(
                "project {}",
                self.project_dir
                    .as_deref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!("lsp {}", self.lsp_status),
        ];
        for line in &lines {
            self.log(line.clone());
        }
        ctx.copy_text(lines.join("\n"));
        self.log("debug info copied to clipboard.".to_string());
    }
}

impl eframe::App for EditorApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let save_shortcut = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S));
        if save_shortcut {
            self.save_scene();
        }
        let cmd = |key: egui::Key| {
            ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(key))
        };
        let cmd_shift = |key: egui::Key| {
            ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(key))
        };
        if cmd(egui::Key::N) {
            self.new_scene_via_dialog();
        }
        if cmd(egui::Key::O) {
            self.open_scene_via_dialog();
        }
        if cmd_shift(egui::Key::S) {
            self.save_scene_as_via_dialog();
        }
        if cmd(egui::Key::Q) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        self.show_asset_modals(ctx, &root);
        self.show_node_modals(ctx);
        self.handle_os_drop(ctx, &root);

        if self.tab != Tab::Code {
            if cmd(egui::Key::A) {
                let parent = self.selected.clone().map(NodeId::from);
                self.open_add_node_modal(parent);
            }
            if cmd_shift(egui::Key::A) {
                let parent = self.selected.clone().map(NodeId::from);
                self.open_instance_modal(parent);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
                if let Some(sel) = self.selected_asset.clone() {
                    self.open_rename_modal(&sel);
                } else if let Some(sel) = self.selected.clone().map(NodeId::from) {
                    self.open_rename_node_modal(sel);
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
                if let Some(sel) = self.selected_asset.clone() {
                    let is_dir = root
                        .join(sel.replace('/', std::path::MAIN_SEPARATOR_STR))
                        .is_dir();
                    self.open_delete_modal(&sel, is_dir);
                } else if let Some(sel) = self.selected.clone().map(NodeId::from) {
                    self.delete_node(&sel);
                }
            }
            if cmd(egui::Key::ArrowUp) {
                if let Some(sel) = self.selected.clone().map(NodeId::from) {
                    self.move_sibling(&sel, -1);
                }
            }
            if cmd(egui::Key::ArrowDown) {
                if let Some(sel) = self.selected.clone().map(NodeId::from) {
                    self.move_sibling(&sel, 1);
                }
            }
            if cmd(egui::Key::C) {
                if let Some(sel) = self.selected_asset.clone() {
                    self.asset_clipboard = Some(assets::AssetClipboardEntry {
                        rel_path: sel,
                        is_cut: false,
                    });
                }
            }
            if cmd(egui::Key::X) {
                if let Some(sel) = self.selected_asset.clone() {
                    self.asset_clipboard = Some(assets::AssetClipboardEntry {
                        rel_path: sel,
                        is_cut: true,
                    });
                }
            }
            if cmd(egui::Key::V) {
                let target_dir = match &self.selected_asset {
                    Some(sel) => {
                        let fs = root.join(sel.replace('/', std::path::MAIN_SEPARATOR_STR));
                        if fs.is_dir() {
                            sel.clone()
                        } else {
                            Path::new(sel)
                                .parent()
                                .and_then(|p| p.to_str())
                                .unwrap_or("")
                                .to_string()
                        }
                    }
                    None => String::new(),
                };
                self.execute_asset_paste(&target_dir);
            }
            if cmd(egui::Key::D) {
                if self.selected_asset.is_none() && self.selected.is_some() {
                    self.duplicate_selected_node();
                } else if let Some(sel) = self.selected_asset.clone() {
                    self.execute_asset_duplicate(&sel);
                }
            }
        }
        if self.playing {
            let delta = self.last_frame.elapsed().as_secs_f64().min(0.1);
            self.last_frame = Instant::now();
            self.pump_game_input(ctx);
            self.session.frame(delta);
            self.drain_script_errors();
            ctx.request_repaint();
        } else if self.session.poll_watch() {
            self.viewport_dirty = true;
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.hovered_asset_folder = None;
        self.hovered_node_target = None;

        egui::Panel::top("menu").show(ui, |ui| {
            // Everything lives inside the MenuBar scope: MenuBar claims the
            // full row width, so siblings after it in an outer horizontal
            // row get zero space and vanish.
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Scene", |ui| {
                    if ui.button("New Scene  Ctrl+N").clicked() {
                        self.new_scene_via_dialog();
                        ui.close();
                    }
                    if ui.button("Open Scene…  Ctrl+O").clicked() {
                        self.open_scene_via_dialog();
                        ui.close();
                    }
                    if self.recent.is_empty() {
                        ui.add_enabled(false, egui::Button::new("Open Recent"));
                    } else {
                        ui.menu_button("Open Recent", |ui| {
                            let recent = self.recent.clone();
                            for path in &recent {
                                if ui.button(path.display().to_string()).clicked() {
                                    let path = path.clone();
                                    self.open_scene_file(&path);
                                    ui.close();
                                }
                            }
                        });
                    }
                    ui.separator();
                    if ui.button("Save Scene  Ctrl+S").clicked() {
                        self.save_scene();
                        ui.close();
                    }
                    if ui.button("Save Scene As…  Ctrl+Shift+S").clicked() {
                        self.save_scene_as_via_dialog();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quit  Ctrl+Q").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Project", |ui| {
                    if ui.button("Project Settings…").clicked() {
                        self.open_settings();
                        ui.close();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Repository").clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab("https://github.com/piteengine/pite"));
                    }
                    if ui.button("Issues").clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(
                            "https://github.com/piteengine/pite/issues",
                        ));
                    }
                    ui.separator();
                    if ui.button("About Pite").clicked() {
                        self.about_open = true;
                        ui.close();
                    }
                    if ui.button("Copy Debug Info").clicked() {
                        self.debug_info(ui.ctx());
                        ui.close();
                    }
                });
                // Transport controls pinned to the right end, Godot-style:
                // the same actions as the Run menu, always visible.
                // Right-to-left places the first widget rightmost, so Stop
                // comes first to keep the Play, Stop order.
                let (play_icon, play_tip) = if self.playing {
                    (Icon::Pause, "Pause the scene")
                } else {
                    (Icon::Play, "Run the scene")
                };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::icon_button(ui, Icon::Stop, "Stop and reset the scene").clicked() {
                        self.stop();
                    }
                    if icons::icon_button(ui, play_icon, play_tip).clicked() {
                        self.toggle_play();
                    }
                });
            });
        });

        if self.about_open {
            egui::Window::new("About Pite")
                .open(&mut self.about_open)
                .show(ui, |ui| {
                    ui.label(format!("Pite game engine {}", env!("CARGO_PKG_VERSION")));
                    ui.label("Scene-tree 2D engine: Rust core, embedded Python, egui editor.");
                    ui.horizontal(|ui| {
                        if ui.button("Repository").clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(
                                "https://github.com/piteengine/pite",
                            ));
                        }
                        if ui.button("Issues").clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(
                                "https://github.com/piteengine/pite/issues",
                            ));
                        }
                    });
                });
        }

        egui::Panel::left("left")
            .default_size(240.0)
            .min_size(240.0)
            .max_size(360.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Scene");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::icon_button(
                            ui,
                            Icon::Scene,
                            "Instantiate Child Scene… (Ctrl+Shift+A)",
                        )
                        .clicked()
                        {
                            let parent = self.selected.clone().map(NodeId::from);
                            self.open_instance_modal(parent);
                        }
                        if icons::icon_button(ui, Icon::Add, "Add Child Node… (Ctrl+A)").clicked()
                        {
                            let parent = self.selected.clone().map(NodeId::from);
                            self.open_add_node_modal(parent);
                        }
                    });
                });
                let tree_h = (ui.available_height() * 0.52).max(120.0);
                egui::ScrollArea::vertical()
                    .id_salt("tree")
                    .max_height(tree_h)
                    .show(ui, |ui| {
                        let root = self.session.host().with_tree(|t| t.root().cloned());
                        if let Some(root) = root {
                            self.show_node(ui, &root);
                        }
                        let avail = ui.available_size();
                        if avail.y > 8.0 {
                            let (rect, resp) = ui.allocate_exact_size(
                                egui::vec2(avail.x.max(1.0), avail.y),
                                egui::Sense::hover(),
                            );
                            let has_node_dnd = egui::DragAndDrop::has_payload_of_type::<
                                tree::NodeDndPayload,
                            >(ui.ctx());
                            if has_node_dnd && resp.contains_pointer() {
                                self.hovered_node_target =
                                    Some(("Root".to_string(), tree::DropZone::Under));
                                ui.painter().rect_filled(
                                    rect,
                                    2.0,
                                    egui::Color32::from_rgba_unmultiplied(11, 147, 135, 40),
                                );
                                ui.painter().rect_stroke(
                                    rect,
                                    2.0,
                                    egui::Stroke::new(
                                        1.5_f32,
                                        egui::Color32::from_rgb(11, 147, 135),
                                    ),
                                    egui::StrokeKind::Middle,
                                );
                            }
                            if let Some(dragged) =
                                resp.dnd_release_payload::<tree::NodeDndPayload>()
                            {
                                if let Some(root) =
                                    self.session.host().with_tree(|t| t.root().cloned())
                                {
                                    let dragged_id = NodeId::from(dragged.id.as_str());
                                    let result = self.session.host().with_tree_mut(|t| {
                                        let len = t.children_of(&root).len();
                                        ops::place_node(t, &dragged_id, &root, len)
                                    });
                                    match result {
                                        Ok(()) => {
                                            self.viewport_dirty = true;
                                            self.log(format!("moved {dragged_id}."));
                                        }
                                        Err(e) => self.log(format!("move failed: {e:#}")),
                                    }
                                }
                            }
                        }
                    });
                ui.separator();
                let root = self
                    .project_dir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("."));
                ui.horizontal(|ui| {
                    ui.heading("Assets");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::icon_button(ui, icons::Icon::FolderPlus, "New Folder…").clicked()
                        {
                            let parent_rel = match &self.selected_asset {
                                Some(sel) => {
                                    let fs =
                                        root.join(sel.replace('/', std::path::MAIN_SEPARATOR_STR));
                                    if fs.is_dir() {
                                        sel.clone()
                                    } else {
                                        Path::new(sel)
                                            .parent()
                                            .and_then(|p| p.to_str())
                                            .unwrap_or("")
                                            .to_string()
                                    }
                                }
                                None => String::new(),
                            };
                            self.open_new_folder_modal(&parent_rel);
                        }
                        if icons::icon_button(ui, icons::Icon::FilePlus, "New File…").clicked() {
                            let parent_rel = match &self.selected_asset {
                                Some(sel) => {
                                    let fs =
                                        root.join(sel.replace('/', std::path::MAIN_SEPARATOR_STR));
                                    if fs.is_dir() {
                                        sel.clone()
                                    } else {
                                        Path::new(sel)
                                            .parent()
                                            .and_then(|p| p.to_str())
                                            .unwrap_or("")
                                            .to_string()
                                    }
                                }
                                None => String::new(),
                            };
                            self.open_new_file_modal(&parent_rel, assets::NewFileKind::Empty);
                        }
                    });
                });
                egui::ScrollArea::new([true, true])
                    .id_salt("assets")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.show_asset_dir(ui, &root, "");
                        let avail = ui.available_size();
                        if avail.y > 8.0 {
                            let (rect, resp) = ui.allocate_exact_size(
                                egui::vec2(avail.x.max(1.0), avail.y),
                                egui::Sense::hover(),
                            );
                            let is_drop_target_root = if let Some(dragged) =
                                egui::DragAndDrop::payload::<assets::AssetDndPayload>(ui.ctx())
                            {
                                let parent = Path::new(&dragged.rel_path)
                                    .parent()
                                    .unwrap_or(Path::new(""));
                                !parent.as_os_str().is_empty()
                            } else {
                                false
                            };
                            if egui::DragAndDrop::has_any_payload(ui.ctx())
                                && resp.contains_pointer()
                            {
                                if is_drop_target_root {
                                    self.hovered_asset_folder = Some(String::new());
                                    ui.painter().rect_filled(
                                        rect,
                                        2.0,
                                        egui::Color32::from_rgba_unmultiplied(11, 147, 135, 45),
                                    );
                                    ui.painter().rect_stroke(
                                        rect,
                                        2.0,
                                        egui::Stroke::new(
                                            1.5_f32,
                                            egui::Color32::from_rgb(11, 147, 135),
                                        ),
                                        egui::StrokeKind::Middle,
                                    );
                                } else {
                                    ui.painter().rect_filled(
                                        rect,
                                        2.0,
                                        egui::Color32::from_rgba_unmultiplied(220, 80, 80, 30),
                                    );
                                    ui.painter().rect_stroke(
                                        rect,
                                        2.0,
                                        egui::Stroke::new(
                                            1.5_f32,
                                            egui::Color32::from_rgb(220, 80, 80),
                                        ),
                                        egui::StrokeKind::Middle,
                                    );
                                }
                            }
                            if let Some(dragged) =
                                resp.dnd_release_payload::<assets::AssetDndPayload>()
                            {
                                let parent = Path::new(&dragged.rel_path)
                                    .parent()
                                    .unwrap_or(Path::new(""));
                                if !parent.as_os_str().is_empty() {
                                    self.execute_asset_move(&dragged.rel_path, "");
                                }
                            }
                        }
                    });
            });

        egui::Panel::right("inspector")
            .default_size(240.0)
            .min_size(240.0)
            .max_size(360.0)
            .resizable(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("inspector")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.heading("Inspector");
                        match self.selected.clone() {
                            Some(id) => self.show_inspector(ui, &id),
                            None => {
                                ui.label("Select a node.");
                            }
                        }
                    })
            });

        egui::Panel::bottom("console")
            .default_size(120.0)
            .min_size(120.0)
            .max_size(240.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.heading("Console");
                egui::ScrollArea::vertical()
                    .id_salt("console")
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.console {
                            ui.monospace(line);
                        }
                    });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Viewport, "Viewport");
                ui.selectable_value(&mut self.tab, Tab::Code, "Code");
            });
            ui.separator();
            ui.horizontal(|ui| match self.tab {
                Tab::Viewport => {
                    ui.label(self.scene_path.to_string_lossy().as_ref());
                    if icons::icon_button(ui, Icon::Save, "Save scene (Ctrl+S)").clicked() {
                        self.save_scene();
                    }
                }
                Tab::Code => {
                    if let Some(file) = self.code_file.clone() {
                        ui.label(file.to_string_lossy().as_ref());
                        if icons::icon_button(ui, Icon::Save, "Save script (Ctrl+S)").clicked() {
                            self.save_code();
                        }
                    } else {
                        ui.label("Open a .py file from Assets.");
                    }
                }
            });
            ui.separator();
            match self.tab {
                Tab::Viewport => self.show_viewport(ui),
                Tab::Code => self.show_code(ui),
            }
        });

        self.show_settings(&ctx);
        self.show_asset_drag_preview(&ctx);
        self.show_node_drag_preview(&ctx);
    }
}
