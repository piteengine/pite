use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Instant;

use anyhow::Result;
use eframe::egui;

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
fn viewport_cursor(rect: egui::Rect, view: (u32, u32), pointer: egui::Pos2) -> Option<(f64, f64)> {
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
fn fit_view(avail: egui::Vec2, view: (u32, u32)) -> egui::Vec2 {
    let aspect = (view.0 as f32 / view.1.max(1) as f32).max(0.0001);
    let avail = egui::Vec2::new(avail.x.max(1.0), avail.y.max(1.0));
    if avail.x / avail.y > aspect {
        egui::Vec2::new(avail.y * aspect, avail.y)
    } else {
        egui::Vec2::new(avail.x, avail.x / aspect)
    }
}
use pite_core::{NodeId, PropValue};
use pite_render::{OffscreenRenderer, Renderer2D};
use pite_runtime::GameSession;

use crate::icons::{self, Icon};
use crate::lsp::{self, LspClient, LspOutcome};
use crate::ops;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Viewport,
    Code,
}

#[derive(Clone, Copy, PartialEq)]
enum DropZone {
    None,
    Before,
    Under,
}

pub fn launch(scene: &Path) -> Result<()> {
    let app = EditorApp::new(scene)?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon.png"))
                    .expect("assets/icon.png must be a valid PNG"),
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
    session: GameSession,
    scene_path: PathBuf,
    project_dir: Option<PathBuf>,
    selected: Option<String>,
    console: Vec<String>,
    seen_errors: HashSet<String>,
    code_file: Option<PathBuf>,
    code_text: String,
    lsp: Option<LspClient>,
    lsp_starter: Option<Receiver<LspOutcome>>,
    lsp_status: String,
    lsp_hint: String,
    lsp_completions: Vec<lsp::CompletionItem>,
    lsp_completion_open: bool,
    lsp_completion_offset: usize,
    playing: bool,
    last_frame: Instant,
    add_open: bool,
    add_type: String,
    add_name: String,
    open_nodes: HashSet<String>,
    open_seen: HashSet<String>,
    viewport: Option<OffscreenRenderer>,
    viewport_failed: bool,
    viewport_tex: Option<egui::TextureHandle>,
    viewport_dirty: bool,
    /// Pointer was over the viewport last frame: game input goes to the
    /// running scene only then, so editing text and shortcuts keep priority.
    viewport_hovered: bool,
    viewport_rect: Option<egui::Rect>,
    /// Bound to the project's window size: the viewport must frame the scene the
    /// way the game window does.
    view_size: (u32, u32),
    grip_hover: Option<String>,
    tab: Tab,
}

impl EditorApp {
    pub fn new(scene: &Path) -> Result<Self> {
        let session = GameSession::open(scene, false)?;
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
            playing: false,
            last_frame: Instant::now(),
            add_open: false,
            add_type: "Node2D".to_string(),
            add_name: "NewNode".to_string(),
            open_nodes: HashSet::new(),
            open_seen: HashSet::new(),
            viewport: None,
            viewport_failed: false,
            viewport_tex: None,
            viewport_hovered: false,
            viewport_rect: None,
            view_size,
            viewport_dirty: true,
            grip_hover: None,
            tab: Tab::Viewport,
        })
    }

    fn log(&mut self, line: impl Into<String>) {
        self.console.push(line.into());
        if self.console.len() > 500 {
            self.console.drain(..self.console.len() - 500);
        }
    }

    fn drain_script_errors(&mut self) {
        for err in self.session.errors() {
            if self.seen_errors.insert(err.clone()) {
                self.log(err);
            }
        }
    }

    fn toggle_play(&mut self) {
        self.playing = !self.playing;
        self.last_frame = Instant::now();
        pite_script::input_clear();
    }

    /// Forward viewport input to the running scene's script input state.
    /// Reads level state (`key_down`) and lets `input_set_key` derive edges;
    /// runs before `session.frame()` so scripts consume this frame's state.
    fn pump_game_input(&mut self, ctx: &egui::Context) {
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

    fn stop(&mut self) {
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

    fn save_scene(&mut self) {
        let source = match std::fs::metadata(&self.scene_path) {
            Ok(_) => match pite_scene::load_scene(&self.scene_path) {
                Ok(doc) => Some(doc),
                Err(e) => {
                    self.log(format!(
                        "save aborted: cannot read existing scene {}: {e:#}",
                        self.scene_path.display()
                    ));
                    return;
                }
            },
            Err(_) => None,
        };
        let scene_dir = self
            .scene_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let refs = ops::InstanceRefs {
            scene_dir: &scene_dir,
            project_dir: self.project_dir.as_deref(),
        };
        let built = self
            .session
            .host()
            .with_tree(|tree| ops::build_doc(tree, source.as_ref(), Some(refs)));
        match built.and_then(|doc| pite_scene::save_scene(&doc).map_err(anyhow::Error::from)) {
            Ok(text) => match ops::save_text(&self.scene_path, &text) {
                Ok(()) => {
                    self.viewport_dirty = true;
                    self.log(format!("saved scene {}", self.scene_path.display()));
                }
                Err(e) => self.log(format!("save scene failed: {e:#}")),
            },
            Err(e) => self.log(format!("save scene failed: {e:#}")),
        }
    }

    fn delete_node(&mut self, id: &NodeId) {
        match self
            .session
            .host()
            .with_tree_mut(|t| ops::remove_node(t, id))
        {
            Ok(()) => {
                if self.selected.as_deref() == Some(id.as_str()) {
                    self.selected = None;
                }
                self.viewport_dirty = true;
                self.log(format!("deleted {id}."));
            }
            Err(e) => self.log(format!("delete failed: {e:#}")),
        }
    }

    fn move_sibling(&mut self, id: &NodeId, delta: i32) {
        match self
            .session
            .host()
            .with_tree_mut(|t| ops::move_sibling(t, id, delta))
        {
            Ok(()) => {
                self.viewport_dirty = true;
                self.log(format!("moved {id}."));
            }
            Err(e) => self.log(format!("move failed: {e:#}")),
        }
    }

    fn drop_node(&mut self, dragged: &str, target: &NodeId, zone: DropZone) {
        let dragged_id = NodeId::from(dragged);
        if dragged_id == *target || zone == DropZone::None {
            return;
        }
        let result = self.session.host().with_tree_mut(|t| match zone {
            DropZone::Under => ops::move_node(t, &dragged_id, Some(target.clone())),
            DropZone::Before => {
                let parent = t
                    .get(target)
                    .and_then(|n| n.parent.clone())
                    .ok_or_else(|| anyhow::anyhow!("cannot reorder at the top level"))?;
                let sibs = t.children_of(&parent);
                let pos = sibs
                    .iter()
                    .position(|s| s == target)
                    .ok_or_else(|| anyhow::anyhow!("target `{target}` not found"))?;
                ops::place_node(t, &dragged_id, &parent, pos)
            }
            DropZone::None => Ok(()),
        });
        match result {
            Ok(()) => {
                self.viewport_dirty = true;
                self.log(format!("moved {dragged_id}."));
            }
            Err(e) => self.log(format!("move failed: {e:#}")),
        }
    }

    fn set_prop(&mut self, id: &NodeId, key: &str, value: PropValue) {
        let result = self
            .session
            .host()
            .with_tree_mut(|t| ops::set_prop(t, id, key, value));
        match result {
            Ok(()) => self.viewport_dirty = true,
            Err(e) => self.log(format!("set prop failed: {e:#}")),
        }
    }

    fn is_open(&mut self, id: &str) -> bool {
        if !self.open_seen.contains(id) {
            self.open_seen.insert(id.to_string());
            self.open_nodes.insert(id.to_string());
            return true;
        }
        self.open_nodes.contains(id)
    }

    fn current_file_errors(&self) -> Vec<String> {
        let name = self
            .code_file
            .as_ref()
            .and_then(|f| f.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if name.is_empty() {
            return Vec::new();
        }
        self.session
            .errors()
            .into_iter()
            .filter(|e| e.contains(name))
            .collect()
    }
}

impl eframe::App for EditorApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let save_shortcut = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S));
        if save_shortcut {
            self.save_scene();
        }
        if self.playing {
            let delta = self.last_frame.elapsed().as_secs_f64().min(0.1);
            self.last_frame = Instant::now();
            self.pump_game_input(ctx);
            self.session.frame(delta);
            self.drain_script_errors();
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        egui::Panel::top("menu").show(ui, |ui| {
            // Everything lives inside the MenuBar scope: MenuBar claims the
            // full row width, so siblings after it in an outer horizontal
            // row get zero space and vanish.
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Save scene  Ctrl+S").clicked() {
                        self.save_scene();
                        ui.close();
                    }
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Run", |ui| {
                    let label = if self.playing { "Pause" } else { "Play" };
                    if ui.button(label).clicked() {
                        self.toggle_play();
                        ui.close();
                    }
                    if ui.button("Stop").clicked() {
                        self.stop();
                        ui.close();
                    }
                });
                // Transport controls next to the menus, Godot-style: the same
                // actions as the Run menu, always visible.
                let (play_icon, play_tip) = if self.playing {
                    (Icon::Pause, "Pause the scene")
                } else {
                    (Icon::Play, "Run the scene")
                };
                if icons::icon_button(ui, play_icon, play_tip).clicked() {
                    self.toggle_play();
                }
                if icons::icon_button(ui, Icon::Stop, "Stop and reset the scene").clicked() {
                    self.stop();
                }
            });
        });

        egui::Panel::left("left")
            .default_size(240.0)
            .min_size(240.0)
            .max_size(360.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Scene");
                    if icons::icon_button(ui, Icon::Add, "Add a child node under the selection")
                        .clicked()
                    {
                        self.add_open = true;
                    }
                });
                if self.add_open {
                    self.show_add_child(ui);
                }
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
                            if egui::DragAndDrop::has_any_payload(ui.ctx())
                                && resp.contains_pointer()
                            {
                                ui.painter().rect_stroke(
                                    rect,
                                    3.0,
                                    egui::Stroke::new(1.5_f32, crate::theme::FAINT),
                                    egui::StrokeKind::Middle,
                                );
                            }
                            if let Some(dragged) = resp.dnd_release_payload::<String>() {
                                if let Some(root) =
                                    self.session.host().with_tree(|t| t.root().cloned())
                                {
                                    let dragged_id = NodeId::from(dragged.as_str());
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
                ui.heading("Assets");
                let rels = self.asset_rel_paths();
                let refs: Vec<&str> = rels.iter().map(String::as_str).collect();
                let root = self
                    .project_dir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("."));
                egui::ScrollArea::new([true, true])
                    .id_salt("assets")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.show_asset_dir(ui, &root, "", &refs);
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
    }
}

impl EditorApp {
    fn show_add_child(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        let types = ops::registered_types();
        if !types.contains(&self.add_type) {
            self.add_type = types.first().cloned().unwrap_or_default();
        }
        egui::ComboBox::from_label("Type")
            .selected_text(&self.add_type)
            .show_ui(ui, |ui| {
                for t in &types {
                    ui.selectable_value(&mut self.add_type, t.clone(), t);
                }
            });
        ui.text_edit_singleline(&mut self.add_name);
        ui.horizontal(|ui| {
            if ui.button("Add").clicked() {
                let type_name = self.add_type.clone();
                let name = self.add_name.clone();
                let parent = self
                    .selected
                    .clone()
                    .map(NodeId::from)
                    .or_else(|| self.session.host().with_tree(|t| t.root().cloned()));
                match self
                    .session
                    .host()
                    .with_tree_mut(|t| ops::add_node(t, parent, &type_name, &name))
                {
                    Ok(id) => {
                        self.selected = Some(id.to_string());
                        self.add_open = false;
                        self.viewport_dirty = true;
                        self.log(format!("added {id} ({type_name})."));
                    }
                    Err(e) => self.log(format!("add node failed: {e:#}")),
                }
            }
            if ui.button("Cancel").clicked() {
                self.add_open = false;
            }
        });
    }

    fn show_node(&mut self, ui: &mut egui::Ui, id: &NodeId) {
        let (name, type_name, children) = self
            .session
            .host()
            .with_tree(|t| {
                t.get(id)
                    .map(|n| (n.name.clone(), n.type_name.clone(), t.children_of(id)))
            })
            .unwrap_or_else(|| ("?".to_string(), "?".to_string(), vec![]));
        let script = self.session.host().with_tree(|t| {
            t.get(id).and_then(|n| {
                n.script
                    .as_ref()
                    .map(|s| (s.class_name.clone(), self.resolve_res(&s.path)))
            })
        });
        let selected = self.selected.as_ref() == Some(&id.to_string());
        let id_str = id.to_string();
        let target = id.clone();
        let is_root = self.session.host().with_tree(|t| t.root() == Some(id));
        let open = self.is_open(&id_str);
        let show_grip = self.grip_hover.as_deref() == Some(id_str.as_str());
        let mut row_resp: Option<egui::Response> = None;
        let mut grip_rect: Option<egui::Rect> = None;
        let hresp = ui.horizontal(|ui| {
            if !is_root {
                let g = ui.dnd_drag_source(
                    egui::Id::new(("tree-grip", id_str.clone())),
                    id_str.clone(),
                    |ui| {
                        ui.allocate_exact_size(
                            egui::Vec2::new(12.0, ui.text_style_height(&egui::TextStyle::Body)),
                            egui::Sense::hover(),
                        )
                    },
                );
                grip_rect = Some(g.inner.0);
            }
            let r = ui.selectable_label(selected, format!("{name} ({type_name})"));
            if let Some((class, file)) = script.as_ref() {
                let (badge, resp) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
                if ui.is_rect_visible(badge) {
                    icons::draw_icon(ui.painter(), badge, Icon::Script, crate::theme::FAINT);
                }
                let resp = resp.on_hover_text(format!("open {class}"));
                if resp.clicked() {
                    let file = file.clone();
                    self.open_asset(&file);
                }
            }
            if r.clicked() {
                self.selected = Some(id_str.clone());
            }
            if !children.is_empty() && r.double_clicked() {
                if open {
                    self.open_nodes.remove(&id_str);
                } else {
                    self.open_nodes.insert(id_str.clone());
                }
            }
            let menu_id = id.clone();
            let menu_open = open;
            let menu_kids = !children.is_empty();
            let _ = r.context_menu(|ui| {
                if ui.button("Delete").clicked() {
                    self.delete_node(&menu_id);
                    ui.close();
                }
                if ui.button("Move up").clicked() {
                    self.move_sibling(&menu_id, -1);
                    ui.close();
                }
                if ui.button("Move down").clicked() {
                    self.move_sibling(&menu_id, 1);
                    ui.close();
                }
                if menu_kids {
                    let label = if menu_open { "Collapse" } else { "Expand" };
                    if ui.button(label).clicked() {
                        if menu_open {
                            self.open_nodes.remove(&menu_id.to_string());
                        } else {
                            self.open_nodes.insert(menu_id.to_string());
                        }
                        ui.close();
                    }
                }
            });
            row_resp = Some(r);
        });
        if hresp.response.contains_pointer() {
            self.grip_hover = Some(id_str.clone());
        } else if self.grip_hover.as_deref() == Some(id_str.as_str()) {
            self.grip_hover = None;
        }
        if show_grip {
            if let (Some(g), Some(r)) = (grip_rect, &row_resp) {
                let p = ui.painter();
                let x = g.center().x;
                let y = r.rect.center().y;
                for row in 0..3 {
                    for col in 0..2 {
                        p.circle_filled(
                            egui::Pos2::new(
                                x - 2.25 + col as f32 * 4.5,
                                y - 4.5 + row as f32 * 4.5,
                            ),
                            1.2,
                            crate::theme::FAINT,
                        );
                    }
                }
            }
        }
        if let Some(r) = &row_resp {
            let dragging = egui::DragAndDrop::has_any_payload(ui.ctx());
            let zone = if dragging && r.contains_pointer() {
                let pos = ui.ctx().pointer_hover_pos().unwrap_or(r.rect.center());
                let frac = (pos.y - r.rect.top()) / r.rect.height().max(1.0);
                if is_root {
                    DropZone::Under
                } else if frac < 0.35 || frac > 0.65 {
                    DropZone::Before
                } else {
                    DropZone::Under
                }
            } else {
                DropZone::None
            };
            let p = ui.painter();
            let edge = egui::Stroke::new(1.5_f32, crate::theme::FAINT);
            match zone {
                DropZone::Before => {
                    p.line_segment(
                        [
                            egui::Pos2::new(r.rect.left(), r.rect.top() + 1.0),
                            egui::Pos2::new(r.rect.right(), r.rect.top() + 1.0),
                        ],
                        edge,
                    );
                }
                DropZone::Under => {
                    p.rect_stroke(r.rect, 3.0, edge, egui::StrokeKind::Middle);
                }
                DropZone::None => {}
            }
            if let Some(dragged) = r.dnd_release_payload::<String>() {
                self.drop_node(dragged.as_str(), &target, zone);
            }
        }
        if open && !children.is_empty() {
            ui.indent(id_str, |ui| {
                for child in children {
                    self.show_node(ui, &child);
                }
            });
        }
    }

    fn show_inspector(&mut self, ui: &mut egui::Ui, id: &str) {
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

    fn save_code(&mut self) {
        let Some(file) = self.code_file.clone() else {
            self.log("save: no script open.".to_string());
            return;
        };
        match std::fs::write(&file, &self.code_text) {
            Ok(()) => {
                self.session.reload_script_path(&file);
                self.log(format!("saved {}", file.display()));
            }
            Err(e) => self.log(format!("save failed: {e:#}")),
        }
    }

    fn show_code(&mut self, ui: &mut egui::Ui) {
        self.poll_lsp();
        let diag_count = self.lsp.as_ref().map(|c| c.diagnostic_count()).unwrap_or(0);
        ui.horizontal(|ui| {
            ui.label(&self.lsp_status);
            if diag_count > 0 {
                ui.label(format!("diagnostics: {diag_count}"));
            }
            if !self.lsp_hint.is_empty() {
                ui.monospace(&self.lsp_hint);
            }
        });
        let mut marks = ops::gutter_marks(&self.current_file_errors());
        marks.extend(self.lsp_marks());
        marks.sort();
        for (line, msg) in &marks {
            let _ = ui.small_button(
                egui::RichText::new(format!("line {line}: {msg}"))
                    .monospace()
                    .color(egui::Color32::YELLOW),
            );
        }
        let resp = ui.add_sized(
            ui.available_size(),
            egui::TextEdit::multiline(&mut self.code_text).code_editor(),
        );
        let cursor_byte = Self::code_cursor_byte(&self.code_text, ui.ctx(), &resp.id);
        if resp.changed() {
            self.lsp_completion_open = false;
            let paren_at =
                cursor_byte.filter(|&off| self.code_text[..off].chars().next_back() == Some('('));
            self.code_changed(paren_at);
        }
        if resp.has_focus() {
            let pressed = |key| {
                ui.ctx()
                    .input(|i| i.modifiers.command && i.key_pressed(key))
            };
            if pressed(egui::Key::Space) {
                self.request_completion_at(cursor_byte);
            } else if pressed(egui::Key::H) {
                self.request_hover_at(cursor_byte);
            }
        }
        self.show_completion_popup(ui);
    }

    fn code_uri(&self) -> Option<String> {
        self.code_file
            .as_ref()
            .map(|p| lsp::path_to_uri(p.as_path()))
    }

    fn code_cursor_byte(text: &str, ctx: &egui::Context, id: &egui::Id) -> Option<usize> {
        let state = egui::widgets::text_edit::TextEditState::load(ctx, *id)?;
        let range = state.cursor.char_range()?;
        Some(lsp::byte_offset_of_char(text, range.primary.index.0))
    }

    fn ensure_lsp(&mut self) {
        if self.lsp.is_some() || self.lsp_starter.is_some() {
            return;
        }
        let Some(script) = self.code_file.clone() else {
            return;
        };
        self.lsp_status = "lsp: starting…".to_string();
        self.lsp_starter = Some(lsp::spawn_lsp(self.project_dir.clone(), &script));
    }

    fn poll_lsp(&mut self) {
        if let Some(rx) = &self.lsp_starter {
            if let Ok(outcome) = rx.try_recv() {
                self.lsp_starter = None;
                match outcome {
                    LspOutcome::Ready {
                        mut client,
                        warning,
                    } => {
                        if let Some(w) = warning {
                            self.log(w);
                        }
                        self.lsp_status = match &client.server_version {
                            Some(v) => format!("lsp: ready ({v})"),
                            None => "lsp: ready".to_string(),
                        };
                        if let Some(uri) = self.code_uri() {
                            let text = self.code_text.clone();
                            if let Err(e) = client.did_open(&uri, &text) {
                                self.log(format!("lsp: {e:#}"));
                            }
                        }
                        self.lsp = Some(client);
                    }
                    LspOutcome::Failed(msg) => {
                        self.log(msg);
                        self.lsp_status = "lsp: off (see console)".to_string();
                    }
                }
            }
        }
        let mut completions: Vec<lsp::CompletionItem> = Vec::new();
        let mut hovers: Vec<String> = Vec::new();
        let mut signatures: Vec<String> = Vec::new();
        let mut notices: Vec<String> = Vec::new();
        let mut dead: Option<String> = None;
        if let Some(client) = self.lsp.as_mut() {
            if let Some(msg) = client.poll() {
                dead = Some(msg);
            }
            while let Some((_, items)) = client.take_completion() {
                if !items.is_empty() {
                    completions = items;
                }
            }
            while let Some((_, text)) = client.take_hover() {
                if !text.is_empty() {
                    hovers.push(text);
                }
            }
            while let Some((_, sigs)) = client.take_signature() {
                signatures.extend(sigs);
            }
            while let Some(notice) = client.take_notice() {
                notices.push(notice);
            }
        }
        for notice in notices {
            self.log(format!("lsp: {notice}"));
        }
        if !completions.is_empty() {
            self.lsp_completions = completions;
            self.lsp_completion_open = true;
        }
        for hover in hovers {
            self.log(format!("hover: {hover}"));
        }
        if !signatures.is_empty() {
            self.lsp_hint = signatures.join(" | ");
        }
        if let Some(msg) = dead {
            self.log(msg);
            self.lsp = None;
            self.lsp_status = "lsp: off (see console)".to_string();
        }
    }

    fn lsp_marks(&self) -> Vec<(usize, String)> {
        let Some(uri) = self.code_uri() else {
            return Vec::new();
        };
        let Some(client) = self.lsp.as_ref() else {
            return Vec::new();
        };
        client
            .diagnostics_for_uri(&uri)
            .iter()
            .map(|d| {
                let first = d.message.lines().next().unwrap_or("").to_string();
                (d.line as usize + 1, format!("[lsp] {first}"))
            })
            .collect()
    }

    fn code_changed(&mut self, paren_at: Option<usize>) {
        let Some(uri) = self.code_uri() else {
            return;
        };
        let text = self.code_text.clone();
        let sig_pos = paren_at.map(|off| lsp::offset_to_position(&text, off));
        let Some(client) = self.lsp.as_mut() else {
            return;
        };
        if let Err(e) = client.did_change(&uri, &text) {
            self.log(format!("lsp: {e:#}"));
            return;
        }
        if let Some((line, ch)) = sig_pos {
            if let Err(e) = client.request_signature(&uri, line, ch) {
                self.log(format!("lsp: {e:#}"));
            }
        }
    }

    fn request_completion_at(&mut self, cursor_byte: Option<usize>) {
        let (Some(off), Some(uri)) = (cursor_byte, self.code_uri()) else {
            return;
        };
        let (line, ch) = lsp::offset_to_position(&self.code_text, off);
        match self.lsp.as_mut() {
            Some(client) => match client.request_completion(&uri, line, ch) {
                Ok(_) => self.lsp_completion_offset = off,
                Err(e) => self.log(format!("lsp: {e:#}")),
            },
            None => self.log("lsp: no server (see console for the prerequisite).".to_string()),
        }
    }

    fn request_hover_at(&mut self, cursor_byte: Option<usize>) {
        let (Some(off), Some(uri)) = (cursor_byte, self.code_uri()) else {
            return;
        };
        let (line, ch) = lsp::offset_to_position(&self.code_text, off);
        match self.lsp.as_mut() {
            Some(client) => {
                if let Err(e) = client.request_hover(&uri, line, ch) {
                    self.log(format!("lsp: {e:#}"));
                }
            }
            None => self.log("lsp: no server (see console for the prerequisite).".to_string()),
        }
    }

    fn show_completion_popup(&mut self, ui: &mut egui::Ui) {
        if !self.lsp_completion_open || self.lsp_completions.is_empty() {
            return;
        }
        let anchor = ui.min_rect().left_top() + egui::vec2(40.0, 40.0);
        let mut pick: Option<String> = None;
        egui::Window::new("LSP completions")
            .fixed_pos(anchor)
            .collapsible(false)
            .resizable(false)
            .show(ui, |ui| {
                ui.set_max_height(220.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for item in &self.lsp_completions[..self.lsp_completions.len().min(20)] {
                        let label = if item.detail.is_empty() {
                            item.label.clone()
                        } else {
                            format!("{} — {}", item.label, item.detail)
                        };
                        if ui.small_button(&label).clicked() {
                            pick = Some(item.label.clone());
                        }
                    }
                });
                if ui.small_button("Close").clicked() {
                    self.lsp_completion_open = false;
                }
            });
        if let Some(label) = pick {
            self.lsp_completion_open = false;
            let off = self.lsp_completion_offset.min(self.code_text.len());
            let start = lsp::word_start_before(&self.code_text, off);
            self.code_text.replace_range(start..off, &label);
            self.code_changed(None);
        }
    }

    fn show_viewport(&mut self, ui: &mut egui::Ui) {
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
        // Reserve a row for the caption below the viewport, so a viewport that
        // exactly fills the panel doesn't push the label off the bottom edge.
        let caption_h =
            ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
        let avail = egui::Vec2::new(
            ui.available_width(),
            (ui.available_height() - caption_h).max(1.0),
        );
        let fitted = fit_view(avail, (view_w, view_h));
        // Center the fitted rect in the space we measured: allocate the full
        // avail, then paint into a centered sub-rect, so the viewport reads as
        // framed content instead of content pinned to the top-left corner.
        let (outer, _outer_painter) = ui.allocate_painter(avail, egui::Sense::hover());
        let view_rect = egui::Rect::from_center_size(outer.rect.center(), fitted);
        let resp = ui.interact(view_rect, ui.id().with("viewport"), egui::Sense::hover());
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
        let selected = self.selected.clone().unwrap_or_default();
        ui.label(format!(
            "{} nodes{}",
            self.session.tree_len(),
            if selected.is_empty() {
                String::new()
            } else {
                format!(" — selected {selected}")
            }
        ));
    }

    fn asset_files(&self) -> Vec<PathBuf> {
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                // Mirror pite-assets::scan_files: never show build output,
                // VCS metadata, or hidden files in the Assets panel.
                if name.starts_with('.') {
                    continue;
                }
                if path.is_dir() {
                    if name == "target"
                        || name == "dist"
                        || name == ".git"
                        || name == pite_scene::cache::CACHE_DIR_NAME
                    {
                        continue;
                    }
                    if out.len() < 400 {
                        stack.push(path);
                    }
                } else if out.len() < 400 {
                    out.push(path);
                }
            }
        }
        out.sort();
        out
    }

    fn asset_rel_paths(&self) -> Vec<String> {
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        let mut out: Vec<String> = self
            .asset_files()
            .iter()
            .filter_map(|f| {
                f.strip_prefix(&root)
                    .ok()
                    .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            })
            .collect();
        out.sort();
        out
    }

    fn show_asset_dir(&mut self, ui: &mut egui::Ui, root: &Path, dir: &str, paths: &[&str]) {
        let mut subdirs: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut files: Vec<&str> = Vec::new();
        for p in paths {
            match p.split_once('/') {
                Some((head, rest)) => subdirs.entry(head).or_default().push(rest),
                None => files.push(p),
            }
        }
        for (name, rest) in &subdirs {
            let full = if dir.is_empty() {
                name.to_string()
            } else {
                format!("{dir}/{name}")
            };
            let key = format!("assets:{full}");
            let open = self.is_open(&key);
            let mut toggled = false;
            ui.horizontal(|ui| {
                let (rect, icon_resp) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
                if ui.is_rect_visible(rect) {
                    icons::draw_icon(ui.painter(), rect, icons::Icon::Folder, crate::theme::FAINT);
                }
                toggled = icon_resp.clicked() || ui.label(*name).clicked();
            });
            if toggled {
                if open {
                    self.open_nodes.remove(&key);
                } else {
                    self.open_nodes.insert(key.clone());
                }
            }
            if toggled != open {
                ui.indent(format!("assets-indent:{full}"), |ui| {
                    self.show_asset_dir(ui, root, &full, rest)
                });
            }
        }
        for name in &files {
            let rel = if dir.is_empty() {
                name.to_string()
            } else {
                format!("{dir}/{name}")
            };
            let kind = match name
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "py" => icons::Icon::Script,
                "pitescene" => icons::Icon::Scene,
                "toml" | "cfg" | "ini" | "json" | "yaml" | "yml" => icons::Icon::Gear,
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" => icons::Icon::Image,
                "wav" | "ogg" | "mp3" | "flac" => icons::Icon::Audio,
                _ => icons::Icon::File,
            };
            let mut opened = false;
            ui.horizontal(|ui| {
                let (rect, icon_resp) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
                if ui.is_rect_visible(rect) {
                    icons::draw_icon(ui.painter(), rect, kind, crate::theme::FAINT);
                }
                opened = icon_resp.clicked()
                    || ui
                        .add(egui::Label::new(*name).sense(egui::Sense::click()))
                        .clicked();
            });
            if opened {
                let abs = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                self.open_asset(&abs);
            }
        }
    }

    fn resolve_res(&self, res_path: &str) -> PathBuf {
        if let Some(rel) = res_path.strip_prefix("res://") {
            self.project_dir
                .as_deref()
                .unwrap_or(Path::new("."))
                .join(rel)
        } else {
            PathBuf::from(res_path)
        }
    }

    fn open_asset(&mut self, file: &Path) {
        if file.extension().and_then(|e| e.to_str()) == Some("py") {
            match std::fs::read_to_string(file) {
                Ok(text) => {
                    self.code_file = Some(file.to_path_buf());
                    self.code_text = text;
                    self.tab = Tab::Code;
                    self.ensure_lsp();
                    let uri = self.code_uri();
                    let text = self.code_text.clone();
                    if let (Some(client), Some(uri)) = (self.lsp.as_mut(), uri) {
                        if let Err(e) = client.did_open(&uri, &text) {
                            self.log(format!("lsp: {e:#}"));
                        }
                    }
                }
                Err(e) => self.log(format!("cannot open {}: {e:#}", file.display())),
            }
        } else if file.extension().and_then(|e| e.to_str()) == Some("pitescene") {
            match GameSession::open(file, false) {
                Ok(session) => {
                    self.session = session;
                    self.scene_path = file.to_path_buf();
                    self.selected = None;
                    self.viewport_dirty = true;
                    self.tab = Tab::Viewport;
                    self.log(format!("opened {}", file.display()));
                }
                Err(e) => self.log(format!("open failed: {e:#}")),
            }
        } else {
            self.log(format!("no opener for {}", file.display()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_maps_viewport_corners() {
        let rect =
            egui::Rect::from_min_size(egui::Pos2::new(10.0, 20.0), egui::Vec2::new(400.0, 300.0));
        assert_eq!(
            viewport_cursor(rect, (800, 600), egui::Pos2::new(10.0, 20.0)),
            Some((0.0, 0.0))
        );
        assert_eq!(
            viewport_cursor(rect, (800, 600), egui::Pos2::new(410.0, 320.0)),
            Some((800.0, 600.0))
        );
        assert_eq!(
            viewport_cursor(rect, (800, 600), egui::Pos2::new(210.0, 170.0)),
            Some((400.0, 300.0))
        );
    }

    #[test]
    fn cursor_rejects_degenerate_viewport() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(400.0, 300.0));
        assert_eq!(
            viewport_cursor(rect, (0, 600), egui::Pos2::new(10.0, 10.0)),
            None
        );
        let flat = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(0.0, 300.0));
        assert_eq!(
            viewport_cursor(flat, (800, 600), egui::Pos2::new(10.0, 10.0)),
            None
        );
    }

    #[test]
    fn viewport_fits_available_space_on_both_axes() {
        let view = (800, 600); // 4:3
        // Wide panel: height binds, width shrinks to match.
        let wide = fit_view(egui::Vec2::new(2000.0, 400.0), view);
        assert!((wide.y - 400.0).abs() < 0.01, "got {wide:?}");
        assert!((wide.x - 400.0 * 4.0 / 3.0).abs() < 0.01, "got {wide:?}");
        // Tall panel: width binds, height shrinks to match.
        let tall = fit_view(egui::Vec2::new(400.0, 2000.0), view);
        assert!((tall.x - 400.0).abs() < 0.01, "got {tall:?}");
        assert!((tall.y - 300.0).abs() < 0.01, "got {tall:?}");
        // Never exceeds the panel on either axis, at any panel shape.
        for (ax, ay) in [(2000.0, 400.0), (400.0, 2000.0), (800.0, 600.0), (1.0, 1.0)] {
            let f = fit_view(egui::Vec2::new(ax, ay), view);
            assert!(f.x <= ax + 0.01 && f.y <= ay + 0.01, "{f:?} exceeds {ax}x{ay}");
        }
        // Degenerate input stays positive.
        let tiny = fit_view(egui::Vec2::ZERO, view);
        assert!(tiny.x > 0.0 && tiny.y > 0.0);
    }

    #[test]
    fn viewport_is_centered_in_its_panel() {
        // 4:3 scene, panel wider than tall: fitted box is height-bound and must
        // sit centered horizontally, with equal margins on both sides.
        let avail = egui::Vec2::new(1000.0, 300.0);
        let fitted = fit_view(avail, (800, 600));
        let outer = egui::Rect::from_min_size(egui::Pos2::ZERO, avail);
        let view = egui::Rect::from_center_size(outer.center(), fitted);
        assert!((view.left() - (outer.right() - view.right())).abs() < 0.01);
        assert!(view.width() <= outer.width() + 0.01);
        assert!(view.height() <= outer.height() + 0.01);
    }
}
