use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use eframe::egui;
use pite_core::{NodeId, PropValue};
use pite_render::{OffscreenRenderer, Renderer2D};
use pite_runtime::GameSession;

use crate::ops;

const VIEW_W: u32 = 640;
const VIEW_H: u32 = 400;

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
}

impl EditorApp {
    pub fn new(scene: &Path) -> Result<Self> {
        let session = GameSession::open(scene, false)?;
        let project_dir = pite_project::find_project_root(scene);
        Ok(Self {
            session,
            scene_path: scene.to_path_buf(),
            project_dir,
            selected: None,
            console: vec!["Pite editor ready.".to_string()],
            seen_errors: HashSet::new(),
            code_file: None,
            code_text: String::new(),
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
            viewport_dirty: true,
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

    fn stop(&mut self) {
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
        let doc = self.session.host().with_tree(ops::build_doc);
        match pite_scene::save_scene(&doc) {
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

    fn delete_selected(&mut self) {
        let Some(sel) = self.selected.clone() else {
            self.log("delete: nothing selected.".to_string());
            return;
        };
        let id = NodeId::from(sel);
        match self.session.host().with_tree_mut(|t| ops::remove_node(t, &id)) {
            Ok(()) => {
                self.selected = None;
                self.viewport_dirty = true;
                self.log(format!("deleted {id}."));
            }
            Err(e) => self.log(format!("delete failed: {e:#}")),
        }
    }

    fn move_selected_out(&mut self) {
        let Some(sel) = self.selected.clone() else {
            self.log("move out: nothing selected.".to_string());
            return;
        };
        let id = NodeId::from(sel);
        let target = self.session.host().with_tree(|t| {
            let node = t.get(&id)?;
            let parent = node.parent.clone()?;
            let grandparent = t.get(&parent)?.parent.clone();
            if grandparent.is_none() {
                return None;
            }
            Some(grandparent)
        });
        match target {
            None => self.log("move out: already at top.".to_string()),
            Some(grandparent) => {
                match self.session.host().with_tree_mut(|t| {
                    ops::move_node(t, &id, grandparent.clone())
                }) {
                    Ok(()) => {
                        self.viewport_dirty = true;
                        self.log(format!("moved {id} out."));
                    }
                    Err(e) => self.log(format!("move failed: {e:#}")),
                }
            }
        }
    }

    fn move_selected_in(&mut self) {
        let Some(sel) = self.selected.clone() else {
            self.log("move in: nothing selected.".to_string());
            return;
        };
        let id = NodeId::from(sel);
        let target = self.session.host().with_tree(|t| {
            let node = t.get(&id)?;
            let parent = node.parent.clone()?;
            let siblings = t.children_of(&parent);
            let pos = siblings.iter().position(|s| s == &id)?;
            if pos == 0 {
                return None;
            }
            Some(Some(siblings[pos - 1].clone()))
        });
        match target {
            None => self.log("move in: no previous sibling.".to_string()),
            Some(new_parent) => {
                match self.session.host().with_tree_mut(|t| {
                    ops::move_node(t, &id, new_parent.clone())
                }) {
                    Ok(()) => {
                        self.viewport_dirty = true;
                        self.log(format!("moved {id} in."));
                    }
                    Err(e) => self.log(format!("move failed: {e:#}")),
                }
            }
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let save_shortcut =
            ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S));
        if save_shortcut {
            self.save_scene();
        }
        if self.playing {
            let delta = self.last_frame.elapsed().as_secs_f64().min(0.1);
            self.last_frame = Instant::now();
            self.session.poll_watch();
            self.session.update(delta);
            self.drain_script_errors();
            ctx.request_repaint();
        }

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
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
            });
        });

        egui::TopBottomPanel::top("transport").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let label = if self.playing { "Pause" } else { "Play" };
                if ui.button(label).on_hover_text("Run or pause the scene").clicked() {
                    self.playing = !self.playing;
                    self.last_frame = Instant::now();
                }
                if ui.button("Stop").on_hover_text("Stop and reset the scene").clicked() {
                    self.stop();
                }
                if ui.button("Save scene").on_hover_text("Save scene (Ctrl+S)").clicked() {
                    self.save_scene();
                }
                ui.separator();
                ui.label(self.scene_path.to_string_lossy().as_ref());
            });
        });

        egui::SidePanel::left("tree").default_width(220.0).resizable(true).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Scene tree");
                if ui.button("Add").on_hover_text("Add a child node under the selection").clicked() {
                    self.add_open = true;
                }
                if ui.button("Del").on_hover_text("Delete the selected node").clicked() {
                    self.delete_selected();
                }
                if ui.button("Out").on_hover_text("Move the selection to its grandparent").clicked() {
                    self.move_selected_out();
                }
                if ui.button("In").on_hover_text("Move the selection under its previous sibling").clicked() {
                    self.move_selected_in();
                }
            });
            if self.add_open {
                self.show_add_child(ui);
            }
            let root = self
                .session
                .host()
                .with_tree(|t| t.root().cloned());
            if let Some(root) = root {
                self.show_node(ui, &root, 0);
            }
        });

        egui::SidePanel::right("inspector").default_width(280.0).resizable(true).show(ctx, |ui| {
            ui.heading("Inspector");
            match self.selected.clone() {
                Some(id) => self.show_inspector(ui, &id),
                None => {
                    ui.label("Select a node.");
                }
            }
        });

        egui::TopBottomPanel::bottom("console").resizable(true).show(ctx, |ui| {
            ui.heading("Console");
            egui::ScrollArea::vertical().max_height(200.0).stick_to_bottom(true).show(ui, |ui| {
                for line in &self.console {
                    ui.monospace(line);
                }
            });
        });

        egui::SidePanel::left("assets").default_width(220.0).resizable(true).show(ctx, |ui| {
            ui.heading("Assets");
            let files = self.asset_files();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for file in files {
                    let label = file
                        .strip_prefix(self.project_dir.as_deref().unwrap_or(Path::new(".")))
                        .unwrap_or(&file)
                        .to_string_lossy()
                        .replace('\\', "/");
                    if ui.link(format!("res://{label}")).clicked() {
                        self.open_asset(&file);
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Viewport");
            self.show_viewport(ui);
            ui.separator();
            ui.horizontal(|ui| {
                ui.heading("Code");
                if let Some(file) = self.code_file.clone() {
                    ui.label(file.to_string_lossy().as_ref());
                    if ui.button("Save").clicked() {
                        match std::fs::write(&file, &self.code_text) {
                            Ok(()) => {
                                self.session.reload_script_path(&file);
                                self.log(format!("saved {}", file.display()));
                            }
                            Err(e) => self.log(format!("save failed: {e:#}")),
                        }
                    }
                } else {
                    ui.label("Open a .py file from Assets.");
                }
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                let marks = ops::gutter_marks(&self.current_file_errors());
                for (line, msg) in &marks {
                    let _ = ui.small_button(
                        egui::RichText::new(format!("line {line}: {msg}"))
                            .monospace()
                            .color(egui::Color32::YELLOW),
                    );
                }
                ui.add(
                    egui::TextEdit::multiline(&mut self.code_text)
                        .code_editor()
                        .desired_width(f32::INFINITY),
                );
            });
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
                let parent = self.selected.clone().map(NodeId::from).or_else(|| {
                    self.session.host().with_tree(|t| t.root().cloned())
                });
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

    fn show_node(&mut self, ui: &mut egui::Ui, id: &NodeId, depth: usize) {
        let _ = depth;
        let (name, type_name, children) = self.session.host().with_tree(|t| {
            t.get(id).map(|n| {
                (
                    n.name.clone(),
                    n.type_name.clone(),
                    t.children_of(id),
                )
            })
        }).unwrap_or_else(|| ("?".to_string(), "?".to_string(), vec![]));
        let selected = self.selected.as_ref() == Some(&id.to_string());
        let id_str = id.to_string();
        if children.is_empty() {
            if ui.selectable_label(selected, format!("{name} ({type_name})")).clicked() {
                self.selected = Some(id_str);
            }
            return;
        }
        let open = self.is_open(&id_str);
        ui.horizontal(|ui| {
            let toggle = if open { "[-]" } else { "[+]" };
            if ui.button(toggle).on_hover_text("Expand or collapse children").clicked() {
                if open {
                    self.open_nodes.remove(&id_str);
                } else {
                    self.open_nodes.insert(id_str.clone());
                }
            }
            if ui.selectable_label(selected, format!("{name} ({type_name})")).clicked() {
                self.selected = Some(id.to_string());
            }
        });
        if open {
            let indent_id = id.to_string();
            ui.indent(indent_id, |ui| {
                for child in children {
                    self.show_node(ui, &child, depth + 1);
                }
            });
        }
    }

    fn show_inspector(&mut self, ui: &mut egui::Ui, id: &str) {
        let node_id = NodeId::from(id.to_string());
        let snapshot = self
            .session
            .host()
            .with_tree(|t| t.get(&node_id).cloned());
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

    fn show_viewport(&mut self, ui: &mut egui::Ui) {
        let want_refresh = self.playing || self.viewport_dirty;
        if self.viewport.is_none() && !self.viewport_failed {
            match OffscreenRenderer::new_offscreen(VIEW_W, VIEW_H) {
                Ok(r) => {
                    self.viewport = Some(r);
                }
                Err(e) => {
                    self.viewport_failed = true;
                    self.console.push(format!(
                        "viewport GPU unavailable, using fallback: {e:#}"
                    ));
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
            let image = egui::ColorImage::from_rgba_unmultiplied([VIEW_W as usize, VIEW_H as usize], &rgba);
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
        let width = ui.available_width().max(1.0);
        let height = width * VIEW_H as f32 / VIEW_W as f32;
        let (resp, painter) =
            ui.allocate_painter(egui::Vec2::new(width, height), egui::Sense::hover());
        if textured {
            if let Some(handle) = self.viewport_tex.as_ref() {
                egui::Image::from_texture(handle).paint_at(ui, resp.rect);
            }
            let ((cam_x, cam_y), zoom) = self.session.camera_view();
            let captions: Vec<(f64, f64, String)> = self.session.host().with_tree(|t| {
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
                        let caption = if text.is_empty() {
                            n.id.to_string()
                        } else {
                            text
                        };
                        (pos.0, pos.1, caption)
                    })
                    .collect()
            });
            for (x, y, caption) in captions {
                let (sx, sy) = pite_render::world_to_screen(
                    (x, y),
                    (cam_x, cam_y),
                    zoom,
                    (VIEW_W, VIEW_H),
                );
                let p = egui::Pos2::new(
                    resp.rect.min.x + sx / VIEW_W as f32 * resp.rect.width(),
                    resp.rect.min.y + sy / VIEW_H as f32 * resp.rect.height(),
                );
                painter.text(
                    p + egui::Vec2::new(10.0, -10.0),
                    egui::Align2::LEFT_TOP,
                    caption,
                    egui::FontId::monospace(11.0),
                    egui::Color32::from_rgb(201, 209, 216),
                );
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
            let center = resp.rect.center();
            painter.rect_filled(resp.rect, 0.0, crate::theme::BG);
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
                        egui::Rect::from_center_size(
                            p,
                            egui::Vec2::new(w as f32, h as f32),
                        ),
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
        let root = self.project_dir.clone().unwrap_or_else(|| PathBuf::from("."));
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
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
                    self.log(format!("opened {}", file.display()));
                }
                Err(e) => self.log(format!("open failed: {e:#}")),
            }
        } else {
            self.log(format!("no opener for {}", file.display()));
        }
    }
}
