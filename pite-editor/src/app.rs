// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::{BTreeMap, HashMap, HashSet};
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

/// Unsaved Project Settings edits. `None` while the window is closed; `base`
/// carries the unknown keys so saving only swaps the known fields.
struct SettingsDraft {
    base: pite_project::PiteManifest,
    name: String,
    author: String,
    main_scene: String,
    python: String,
    icon: String,
    width: String,
    height: String,
    binary_name: String,
    include: String,
    error: String,
}

pub fn launch(scene: &Path) -> Result<()> {
    let app = EditorApp::new(scene)?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_maximized(true)
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
    lsp_completion_pending: Vec<(i64, usize)>,
    lsp_completion_selected: usize,
    /// Previous frame's galley row tops (galley space) for the code editor:
    /// pixel-snapped, so the gutter tiles exactly instead of drifting on an
    /// assumed pitch. Empty until the first paint.
    code_row_tops: Vec<f32>,
    /// `galley_pos.y - widget_top.y`: scroll-invariant content offset of the
    /// galley origin, paired with `code_row_tops`.
    code_galley_dy: f32,
    playing: bool,
    last_frame: Instant,
    add_open: bool,
    add_type: String,
    add_name: String,
    settings: Option<SettingsDraft>,
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
    recent: Vec<PathBuf>,
    about_open: bool,
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
            lsp_completion_pending: Vec::new(),
            lsp_completion_selected: 0,
            code_row_tops: Vec::new(),
            code_galley_dy: 0.0,
            playing: false,
            last_frame: Instant::now(),
            add_open: false,
            add_type: "Node2D".to_string(),
            add_name: "NewNode".to_string(),
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
            grip_hover: None,
            tab: Tab::Viewport,
            recent: Self::load_recent(),
            about_open: false,
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

    fn view_size_for(project_dir: Option<&Path>) -> (u32, u32) {
        project_dir
            .and_then(|dir| pite_project::load_manifest(dir).ok())
            .map_or_else(
                || {
                    let d = pite_project::ProjectMeta::default();
                    (d.window_width, d.window_height)
                },
                |m| (m.project.window_width, m.project.window_height),
            )
    }

    fn recent_file() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("pite").join("recent.json"))
    }

    fn read_recent_file(path: &Path) -> Vec<PathBuf> {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let files: Vec<String> = serde_json::from_str(&text).unwrap_or_default();
        files
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .collect()
    }

    fn write_recent_file(path: &Path, recent: &[PathBuf]) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let files: Vec<String> = recent
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        std::fs::write(path, serde_json::to_string_pretty(&files)?)?;
        Ok(())
    }

    fn load_recent() -> Vec<PathBuf> {
        Self::recent_file()
            .map(|p| Self::read_recent_file(&p))
            .unwrap_or_default()
    }

    fn push_recent(&mut self, file: &Path) {
        self.recent.retain(|p| p != file);
        self.recent.insert(0, file.to_path_buf());
        self.recent.truncate(10);
        if let Some(path) = Self::recent_file() {
            if let Err(e) = Self::write_recent_file(&path, &self.recent) {
                self.log(format!("recent list not saved: {e:#}"));
            }
        }
    }

    /// Root-only scene: `New Scene` starts blank, never from the example scaffold.
    fn blank_scene_text() -> String {
        format!(
            "format_version = {}\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n",
            pite_scene::FORMAT_VERSION
        )
    }

    fn open_scene_file(&mut self, file: &Path) {
        match GameSession::open(file, false) {
            Ok(session) => {
                self.session = session;
                self.scene_path = file.to_path_buf();
                self.project_dir = pite_project::find_project_root(file);
                self.view_size = Self::view_size_for(self.project_dir.as_deref());
                self.selected = None;
                self.viewport_dirty = true;
                self.tab = Tab::Viewport;
                self.push_recent(file);
                self.log(format!("opened {}", file.display()));
            }
            Err(e) => self.log(format!("open failed: {e:#}")),
        }
    }

    /// Native dialogs return the path as-is: enforce `.pitescene` so a bare
    /// name cannot scatter extensionless scenes through the project.
    fn with_scene_extension(mut path: PathBuf) -> PathBuf {
        if path.extension().and_then(|e| e.to_str()) != Some("pitescene") {
            path.set_extension("pitescene");
        }
        path
    }

    fn new_scene_via_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Pite scene", &["pitescene"])
            .set_file_name("main.pitescene")
            .save_file()
        else {
            return;
        };
        let path = Self::with_scene_extension(path);
        match std::fs::write(&path, Self::blank_scene_text()) {
            Ok(()) => self.open_scene_file(&path),
            Err(e) => self.log(format!("new scene failed: {e:#}")),
        }
    }

    fn open_scene_via_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Pite scene", &["pitescene"])
            .pick_file()
        else {
            return;
        };
        self.open_scene_file(&path);
    }

    fn save_scene_as_via_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Pite scene", &["pitescene"])
            .set_file_name(
                self.scene_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("main.pitescene"),
            )
            .save_file()
        else {
            return;
        };
        self.scene_path = Self::with_scene_extension(path);
        self.save_scene();
        let path = self.scene_path.clone();
        self.push_recent(&path);
    }

    fn debug_info(&mut self, ctx: &egui::Context) {
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

    fn open_settings(&mut self) {
        let dir = match self.project_dir.clone() {
            Some(dir) => dir,
            None => {
                self.log("project settings: no project open.");
                return;
            }
        };
        match pite_project::load_manifest(&dir) {
            Ok(base) => {
                let binary_name = base.export.binary_name.clone().unwrap_or_default();
                let include = base.export.include.join("\n");
                let project = &base.project;
                self.settings = Some(SettingsDraft {
                    name: project.name.clone(),
                    author: project.author.clone(),
                    main_scene: project.main_scene.clone(),
                    python: project.python.clone(),
                    icon: project.icon.clone(),
                    width: project.window_width.to_string(),
                    height: project.window_height.to_string(),
                    binary_name,
                    include,
                    error: String::new(),
                    base,
                });
            }
            Err(e) => self.log(format!("project settings: cannot load manifest: {e:#}")),
        }
    }

    fn save_settings(&mut self) {
        let (manifest, dir) = {
            let draft = match self.settings.as_mut() {
                Some(draft) => draft,
                None => return,
            };
            let width = match draft.width.trim().parse::<u32>() {
                Ok(width) => width,
                Err(_) => {
                    draft.error = format!("window width {:?} is not a number", draft.width);
                    return;
                }
            };
            let height = match draft.height.trim().parse::<u32>() {
                Ok(height) => height,
                Err(_) => {
                    draft.error = format!("window height {:?} is not a number", draft.height);
                    return;
                }
            };
            let mut manifest = draft.base.clone();
            let project = &mut manifest.project;
            project.name = draft.name.clone();
            project.author = draft.author.clone();
            project.main_scene = draft.main_scene.clone();
            project.python = draft.python.clone();
            project.icon = draft.icon.clone();
            project.window_width = width;
            project.window_height = height;
            let binary_name = draft.binary_name.trim().to_string();
            manifest.export.binary_name = if binary_name.is_empty() {
                None
            } else {
                Some(binary_name)
            };
            manifest.export.include = draft
                .include
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect();
            let (errors, _) = pite_project::validate_manifest(&manifest);
            if !errors.is_empty() {
                draft.error = errors.join("\n");
                return;
            }
            (
                manifest,
                self.project_dir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(".")),
            )
        };
        match pite_project::save_manifest(&dir, &manifest) {
            Ok(()) => {
                self.view_size = (
                    manifest.project.window_width,
                    manifest.project.window_height,
                );
                self.viewport_dirty = true;
                self.settings = None;
                self.log("saved project settings.");
            }
            Err(e) => match self.settings.as_mut() {
                Some(draft) => draft.error = format!("{e:#}"),
                None => self.log(format!("save project settings failed: {e:#}")),
            },
        }
    }

    fn show_settings(&mut self, ctx: &egui::Context) {
        let mut open = self.settings.is_some();
        egui::Window::new("Project Settings")
            .open(&mut open)
            .show(ctx, |ui| {
                let Some(draft) = self.settings.as_mut() else {
                    return;
                };
                ui.label(format!(
                    "engine version: {}",
                    draft.base.project.pite_version
                ));
                ui.separator();
                egui::Grid::new("settings-grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut draft.name);
                        ui.end_row();
                        ui.label("Author");
                        ui.text_edit_singleline(&mut draft.author);
                        ui.end_row();
                        ui.label("Main scene");
                        ui.text_edit_singleline(&mut draft.main_scene);
                        ui.end_row();
                        ui.label("Python");
                        ui.text_edit_singleline(&mut draft.python);
                        ui.end_row();
                        ui.label("Icon");
                        ui.text_edit_singleline(&mut draft.icon);
                        ui.end_row();
                        ui.label("Window width");
                        ui.text_edit_singleline(&mut draft.width);
                        ui.end_row();
                        ui.label("Window height");
                        ui.text_edit_singleline(&mut draft.height);
                        ui.end_row();
                    });
                ui.separator();
                ui.heading("Export");
                egui::Grid::new("settings-export-grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Binary name");
                        ui.text_edit_singleline(&mut draft.binary_name);
                        ui.end_row();
                    });
                ui.label("Extra staged files, one per line:");
                ui.text_edit_multiline(&mut draft.include);
                if !draft.error.is_empty() {
                    ui.label(format!("error: {}", draft.error));
                }
                if ui.button("Save").clicked() {
                    self.save_settings();
                }
            });
        if !open {
            self.settings = None;
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

        self.show_settings(&ctx);
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
                if self.code_is_python() {
                    self.session.reload_script_path(&file);
                }
                self.log(format!("saved {}", file.display()));
            }
            Err(e) => self.log(format!("save failed: {e:#}")),
        }
    }

    fn show_code(&mut self, ui: &mut egui::Ui) {
        self.poll_lsp();
        let diag_count = self.lsp.as_ref().map(|c| c.diagnostic_count()).unwrap_or(0);
        ui.horizontal(|ui| {
            if self.code_is_python() {
                ui.label(&self.lsp_status);
                if diag_count > 0 {
                    ui.label(format!("diagnostics: {diag_count}"));
                }
                if !self.lsp_hint.is_empty() {
                    ui.monospace(&self.lsp_hint);
                }
            } else {
                let name = self
                    .code_file
                    .as_ref()
                    .and_then(|f| f.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or("no file open");
                ui.label(format!("text: {name}"));
            }
        });
        let mut marks = ops::gutter_marks(&self.current_file_errors());
        marks.extend(self.lsp_marks());
        marks.sort();
        marks.dedup();
        let mut error_lines = HashSet::new();
        let mut messages = HashMap::new();
        for (line, msg) in marks {
            error_lines.insert(line);
            messages.insert(line, msg);
        }
        let line_count = self.code_text.split('\n').count().max(1);
        let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
        let font_id = egui::TextStyle::Monospace.resolve(ui.style());
        let digits = line_count.to_string().len().max(2);
        let strip_w = ui.fonts_mut(|fonts| {
            let sample =
                fonts.layout_no_wrap("8".repeat(digits), font_id.clone(), egui::Color32::WHITE);
            sample.size().x + 10.0
        });
        // Fill the panel when the file is short: rows for the viewport
        // height, so the editor keeps its old full-area look.
        let min_rows = (ui.available_size().y / row_h) as usize;
        egui::ScrollArea::vertical()
            .id_salt("code-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        // No gaps between rows: the placer would drift the
                        // numbers away from the text rows down the file.
                        ui.spacing_mut().item_spacing.y = 0.0;
                        // Measured row geometry from last frame's galley: exact,
                        // pixel-snapped tops, so long files cannot drift.
                        let measured = self.code_row_tops.len() == line_count;
                        if measured {
                            let top = self.code_galley_dy + self.code_row_tops[0];
                            if top > 0.0 {
                                ui.add_space(top);
                            }
                        } else {
                            // Match the editor frame's top inner margin so row 1
                            // sits level with the first text row.
                            ui.add_space(2.0);
                        }
                        for (idx, n) in (1..=line_count).enumerate() {
                            let h = if measured {
                                let tops = &self.code_row_tops;
                                if idx + 1 < tops.len() {
                                    tops[idx + 1] - tops[idx]
                                } else if tops.len() >= 2 {
                                    tops[tops.len() - 1] - tops[tops.len() - 2]
                                } else {
                                    row_h
                                }
                            } else {
                                row_h
                            };
                            let (rect, resp) = ui
                                .allocate_exact_size(egui::vec2(strip_w, h), egui::Sense::click());
                            let color = if error_lines.contains(&n) {
                                egui::Color32::YELLOW
                            } else {
                                crate::theme::FAINT
                            };
                            ui.painter().text(
                                egui::pos2(rect.right() - 4.0, rect.center().y),
                                egui::Align2::RIGHT_CENTER,
                                n.to_string(),
                                font_id.clone(),
                                color,
                            );
                            let hovered = resp.hovered();
                            let clicked = resp.clicked();
                            if let Some(msg) = messages.get(&n) {
                                resp.on_hover_text(msg.as_str());
                            }
                            if hovered {
                                ui.set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if clicked {
                                self.jump_to_line(ui.ctx(), n);
                                ui.scroll_to_rect(rect, None);
                            }
                        }
                    });
                    // No wrapping: one text row per line keeps the gutter exact.
                    // Highlighting rides the same layouter, colored by extension.
                    let ext = self
                        .code_file
                        .as_ref()
                        .and_then(|f| f.extension())
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    let mut layouter =
                        |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                            let _ = wrap_width;
                            crate::highlight::layout(ui, text.as_str(), &ext)
                        };
                    let edit_id = egui::Id::new("code-editor");
                    let before_len = self.code_text.len();
                    // Completion keys are consumed before the editor sees them:
                    // arrows move the selection and Enter accepts, so typing
                    // flows without the popup stealing or mangling keystrokes.
                    // Escape is NOT focus-gated: egui core drops editor focus
                    // on every plain Esc before widgets run, so a focus check
                    // here would never observe the press.
                    let mut completion_accept = false;
                    if self.lsp_completion_open {
                        if ui.ctx().memory(|m| m.focused()) == Some(edit_id) {
                            if let Some(pre) =
                                Self::code_cursor_byte(&self.code_text, ui.ctx(), &edit_id)
                            {
                                let matches = self.completion_filtered(pre).len();
                                ui.ctx().input_mut(|i| {
                                    let plain = i.modifiers.is_none();
                                    if plain && i.key_pressed(egui::Key::ArrowDown) {
                                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                                        if matches > 0 {
                                            self.lsp_completion_selected =
                                                (self.lsp_completion_selected + 1) % matches;
                                        }
                                    } else if plain && i.key_pressed(egui::Key::ArrowUp) {
                                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                                        if matches > 0 {
                                            self.lsp_completion_selected = self
                                                .lsp_completion_selected
                                                .checked_sub(1)
                                                .unwrap_or(matches - 1);
                                        }
                                    } else if plain
                                        && matches > 0
                                        && i.key_pressed(egui::Key::Enter)
                                    {
                                        i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                                        completion_accept = true;
                                    }
                                });
                            }
                        }
                        if ui.ctx().input_mut(|i| {
                            i.modifiers.is_none()
                                && i.key_pressed(egui::Key::Escape)
                                && i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                        }) {
                            self.close_completion();
                            // Core already defocused the editor: hand focus
                            // back so typing resumes without a click.
                            ui.ctx().memory_mut(|m| m.request_focus(edit_id));
                        }
                    }
                    // `show` (not `add`) exposes the galley so the popup can
                    // anchor at the cursor instead of a fixed offset.
                    let output = egui::TextEdit::multiline(&mut self.code_text)
                        .code_editor()
                        // Pinned id: gutter jumps address the editor across
                        // frames, so it must not depend on widget order.
                        .id(edit_id)
                        // Full content height (+1 slack row) so the outer
                        // scroll area owns scrolling and the gutter stays
                        // in sync; the editor never scrolls internally.
                        // At least the viewport height so short files still
                        // fill the panel.
                        .desired_rows((line_count + 1).max(min_rows))
                        // Whatever width the gutter column leaves: the row
                        // does not expand the editor on its own.
                        .desired_width(ui.available_size().x)
                        .layouter(&mut layouter)
                        .show(ui);
                    let resp = &output.response;
                    // Record this frame's row geometry for next frame's
                    // gutter: galley-space tops plus the scroll-invariant
                    // widget offset, so numbers tile the true rows.
                    if output.galley.rows.len() == line_count && !output.galley.rows.is_empty() {
                        self.code_row_tops.clear();
                        self.code_row_tops
                            .extend(output.galley.rows.iter().map(|r| r.pos.y));
                        self.code_galley_dy = output.galley_pos.y - output.response.rect.top();
                    }
                    let cursor_byte = Self::code_cursor_byte(&self.code_text, ui.ctx(), &resp.id);
                    if resp.changed() {
                        // Insertions only: backspace re-filters the live list
                        // and the anchor check dismisses what no longer fits.
                        let grew = self.code_text.len() > before_len;
                        let last =
                            cursor_byte.and_then(|off| self.code_text[..off].chars().next_back());
                        let is_word = matches!(last, Some(c) if c.is_alphanumeric() || c == '_');
                        let paren_at = cursor_byte
                            .filter(|&off| self.code_text[..off].chars().next_back() == Some('('));
                        self.code_changed(paren_at);
                        if grew && last == Some('.') {
                            self.request_completion_at(cursor_byte);
                        } else if grew && is_word {
                            self.request_completion_at(cursor_byte);
                        } else if grew && !is_word {
                            self.close_completion();
                        }
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
                    if completion_accept {
                        self.accept_completion(ui.ctx(), cursor_byte);
                    }
                    let anchor = output.cursor_range.map(|range| {
                        output.galley_pos
                            + output
                                .galley
                                .pos_from_cursor(range.primary)
                                .left_bottom()
                                .to_vec2()
                            + egui::vec2(0.0, 4.0)
                    });
                    self.show_completion_popup(ui, cursor_byte, anchor);
                });
            });
    }

    fn code_uri(&self) -> Option<String> {
        self.code_file
            .as_ref()
            .map(|p| lsp::path_to_uri(p.as_path()))
    }

    /// The open file is a Python script: the only kind the LSP serves.
    fn code_is_python(&self) -> bool {
        self.code_file
            .as_ref()
            .and_then(|f| f.extension())
            .and_then(|e| e.to_str())
            == Some("py")
    }

    /// Plain-text files the code editor opens (same set the asset
    /// browser shows with a gear icon); LSP stays Python-only.
    fn editor_text_file(file: &Path) -> bool {
        matches!(
            file.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("py" | "toml" | "cfg" | "ini" | "json" | "yaml" | "yml")
        )
    }

    fn code_cursor_byte(text: &str, ctx: &egui::Context, id: &egui::Id) -> Option<usize> {
        let state = egui::widgets::text_edit::TextEditState::load(ctx, *id)?;
        let range = state.cursor.char_range()?;
        Some(lsp::byte_offset_of_char(text, range.primary.index.0))
    }

    /// Move the code editor's cursor to the start of 1-based `line` and focus
    /// it. Callers scroll the outer area to the line themselves: the TextEdit
    /// only scrolls on text/selection changes, never on a stored cursor.
    fn jump_to_line(&mut self, ctx: &egui::Context, line: usize) {
        let edit_id = egui::Id::new("code-editor");
        let byte = lsp::line_start_byte(&self.code_text, line);
        let index = self.code_text[..byte].chars().count();
        Self::store_code_cursor(ctx, index);
        ctx.memory_mut(|memory| memory.request_focus(edit_id));
    }

    fn store_code_cursor(ctx: &egui::Context, char_index: usize) {
        let edit_id = egui::Id::new("code-editor");
        if let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ctx, edit_id) {
            let cursor = egui::text::CCursor::new(char_index);
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
            state.store(ctx, edit_id);
        }
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
                            if self.code_is_python() {
                                if let Err(e) = client.did_open(&uri, &text) {
                                    self.log(format!("lsp: {e:#}"));
                                }
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
        let mut hovers: Vec<String> = Vec::new();
        let mut signatures: Vec<String> = Vec::new();
        let mut notices: Vec<String> = Vec::new();
        let mut dead: Option<String> = None;
        if let Some(client) = self.lsp.as_mut() {
            if let Some(msg) = client.poll() {
                dead = Some(msg);
            }
            while let Some((id, items)) = client.take_completion() {
                let hit = self
                    .lsp_completion_pending
                    .iter()
                    .position(|(pending, _)| *pending == id);
                if let Some(i) = hit {
                    let (_, anchor) = self.lsp_completion_pending.remove(i);
                    // Out-of-order guard: an older response landing after a
                    // newer trigger must not resurrect a stale list.
                    if anchor >= self.lsp_completion_offset {
                        self.lsp_completion_offset = anchor;
                        self.lsp_completion_selected = 0;
                        self.lsp_completions = items;
                        self.lsp_completion_open = !self.lsp_completions.is_empty();
                    }
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
        if !self.code_is_python() {
            return;
        };
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
        if !self.code_is_python() {
            return;
        }
        let (Some(off), Some(uri)) = (cursor_byte, self.code_uri()) else {
            return;
        };
        let (line, ch) = lsp::offset_to_position(&self.code_text, off);
        match self.lsp.as_mut() {
            Some(client) => match client.request_completion(&uri, line, ch) {
                Ok(id) => {
                    // Provisional anchor: the render-time check needs a base
                    // before the response lands; the accepted response
                    // overwrites it with the same value.
                    self.lsp_completion_offset = off;
                    self.lsp_completion_pending.push((id, off));
                    if self.lsp_completion_pending.len() > 16 {
                        let excess = self.lsp_completion_pending.len() - 16;
                        self.lsp_completion_pending.drain(..excess);
                    }
                }
                Err(e) => self.log(format!("lsp: {e:#}")),
            },
            None => self.log("lsp: no server (see console for the prerequisite).".to_string()),
        }
    }

    /// Dismiss the popup and drop in-flight requests so late responses
    /// cannot reopen it.
    fn close_completion(&mut self) {
        self.lsp_completion_open = false;
        self.lsp_completion_pending.clear();
        self.lsp_completion_selected = 0;
    }

    /// Indices into `lsp_completions` matching the text typed since the
    /// trigger offset (case-insensitive prefix): optimistic narrowing
    /// while a server refresh is still in flight.
    fn completion_filtered(&self, cursor_byte: usize) -> Vec<usize> {
        let base = self.lsp_completion_offset.min(self.code_text.len());
        let cur = cursor_byte.min(self.code_text.len());
        let prefix = self.code_text[base.min(cur)..cur].to_lowercase();
        self.lsp_completions
            .iter()
            .enumerate()
            .filter(|(_, item)| item.label.to_lowercase().starts_with(&prefix))
            .map(|(i, _)| i)
            .collect()
    }

    /// Insert the selected candidate, replacing the whole identifier under
    /// the cursor (accepting mid-word must not duplicate its tail).
    fn accept_completion(&mut self, ctx: &egui::Context, cursor_byte: Option<usize>) {
        let Some(cur) = cursor_byte else { return };
        let cur = cur.min(self.code_text.len());
        let label = self
            .completion_filtered(cur)
            .get(self.lsp_completion_selected)
            .copied()
            .and_then(|i| self.lsp_completions.get(i))
            .map(|item| item.label.clone());
        let Some(label) = label else { return };
        let start = lsp::word_start_before(&self.code_text, cur);
        let end = lsp::word_end_after(&self.code_text, cur);
        self.code_text.replace_range(start..end, &label);
        let index = self.code_text[..start + label.len()].chars().count();
        Self::store_code_cursor(ctx, index);
        self.code_changed(None);
        self.close_completion();
    }

    fn request_hover_at(&mut self, cursor_byte: Option<usize>) {
        if !self.code_is_python() {
            return;
        }
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

    fn show_completion_popup(
        &mut self,
        ui: &mut egui::Ui,
        cursor_byte: Option<usize>,
        anchor: Option<egui::Pos2>,
    ) {
        if !self.lsp_completion_open || self.lsp_completions.is_empty() {
            return;
        }
        // The cursor must still sit in the word the request was made for;
        // anything else means the list belongs to another spot.
        let Some(cur) = cursor_byte else {
            self.close_completion();
            return;
        };
        let cur = cur.min(self.code_text.len());
        let word_start = lsp::word_start_before(&self.code_text, cur);
        if word_start > self.lsp_completion_offset || self.lsp_completion_offset > cur {
            self.close_completion();
            return;
        }
        let shown = self.completion_filtered(cur);
        if shown.is_empty() {
            return;
        }
        self.lsp_completion_selected = self.lsp_completion_selected.min(shown.len() - 1);
        // No cursor, no popup: falling back to a corner rect is how the
        // list ends up stranded at top-left after focus loss.
        let Some(anchor) = anchor else {
            self.close_completion();
            return;
        };
        let mut pick: Option<usize> = None;
        egui::Window::new("LSP completions")
            .fixed_pos(anchor)
            .constrain(true)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .show(ui, |ui| {
                ui.set_max_height(220.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (row, &i) in shown.iter().enumerate() {
                        let item = &self.lsp_completions[i];
                        let label = if item.detail.is_empty() {
                            item.label.clone()
                        } else {
                            format!("{} — {}", item.label, item.detail)
                        };
                        let resp = ui.selectable_label(row == self.lsp_completion_selected, &label);
                        if row == self.lsp_completion_selected {
                            ui.scroll_to_rect(resp.rect, None);
                        }
                        if resp.clicked() {
                            pick = Some(row);
                        }
                    }
                });
            });
        if let Some(row) = pick {
            self.lsp_completion_selected = row;
            self.accept_completion(ui.ctx(), cursor_byte);
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
        if Self::editor_text_file(file) {
            match std::fs::read_to_string(file) {
                Ok(text) => {
                    self.code_file = Some(file.to_path_buf());
                    self.code_text = text;
                    self.tab = Tab::Code;
                    self.close_completion();
                    self.lsp_hint.clear();
                    // LSP stays Python-only: text files open with no
                    // did_open, so pyright never sees them.
                    if self.code_is_python() {
                        self.ensure_lsp();
                        let uri = self.code_uri();
                        let text = self.code_text.clone();
                        if let (Some(client), Some(uri)) = (self.lsp.as_mut(), uri) {
                            if let Err(e) = client.did_open(&uri, &text) {
                                self.log(format!("lsp: {e:#}"));
                            }
                        }
                    }
                }
                Err(e) => self.log(format!("cannot open {}: {e:#}", file.display())),
            }
        } else if file.extension().and_then(|e| e.to_str()) == Some("pitescene") {
            self.open_scene_file(file);
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
        let view = (800, 600);
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
            assert!(
                f.x <= ax + 0.01 && f.y <= ay + 0.01,
                "{f:?} exceeds {ax}x{ay}"
            );
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

    #[test]
    fn editor_opens_gear_group_as_text() {
        for name in [
            "a.py", "p.toml", "c.cfg", "c.ini", "d.json", "e.yaml", "f.yml", "G.TOML",
        ] {
            assert!(EditorApp::editor_text_file(Path::new(name)), "{name}");
        }
        for name in ["a.png", "b.wav", "c.pitescene", "noext", "a.pyx"] {
            assert!(!EditorApp::editor_text_file(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn recent_list_round_trips_and_drops_missing() {
        let dir = std::env::temp_dir().join(format!("pite-recent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let present = dir.join("a.pitescene");
        std::fs::write(&present, "x").unwrap();
        let file = dir.join("recent.json");
        let recent = vec![present.clone(), dir.join("gone.pitescene")];
        EditorApp::write_recent_file(&file, &recent).unwrap();
        assert_eq!(EditorApp::read_recent_file(&file), vec![present]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_dialog_path_keeps_scene_extension() {
        assert_eq!(
            EditorApp::with_scene_extension(PathBuf::from("s/main")),
            PathBuf::from("s/main.pitescene")
        );
        assert_eq!(
            EditorApp::with_scene_extension(PathBuf::from("s/main.pitescene")),
            PathBuf::from("s/main.pitescene")
        );
    }

    #[test]
    fn blank_scene_tracks_format_version() {
        let text = EditorApp::blank_scene_text();
        assert!(
            text.starts_with(&format!(
                "format_version = {}\n",
                pite_scene::FORMAT_VERSION
            )),
            "{text}"
        );
        assert!(text.contains("id = \"root\""));
    }
}
