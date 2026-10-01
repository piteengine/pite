use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use eframe::egui;
use pite_core::{NodeId, PropValue};
use pite_runtime::GameSession;

use crate::ops;

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
                self.log("stopped, scene reset.");
            }
            Err(e) => self.log(format!("stop failed: {e:#}")),
        }
    }

    fn save_scene(&mut self) {
        let doc = self.session.host().with_tree(ops::build_doc);
        match pite_scene::save_scene(&doc) {
            Ok(text) => match ops::save_text(&self.scene_path, &text) {
                Ok(()) => self.log(format!("saved scene {}", self.scene_path.display())),
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
                self.log(format!("deleted {id}."));
            }
            Err(e) => self.log(format!("delete failed: {e:#}")),
        }
    }

    fn set_prop(&mut self, id: &NodeId, key: &str, value: PropValue) {
        let result = self
            .session
            .host()
            .with_tree_mut(|t| ops::set_prop(t, id, key, value));
        if let Err(e) = result {
            self.log(format!("set prop failed: {e:#}"));
        }
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

        egui::TopBottomPanel::top("transport").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let label = if self.playing { "⏸ Pause" } else { "▶ Play" };
                if ui.button(label).clicked() {
                    self.playing = !self.playing;
                    self.last_frame = Instant::now();
                }
                if ui.button("⏹ Stop").clicked() {
                    self.stop();
                }
                if ui.button("💾 Save scene").clicked() {
                    self.save_scene();
                }
                ui.separator();
                ui.label(self.scene_path.to_string_lossy().as_ref());
            });
        });

        egui::SidePanel::left("tree").default_width(220.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Scene tree");
                if ui.small_button("+").clicked() {
                    self.add_open = true;
                }
                if ui.small_button("−").clicked() {
                    self.delete_selected();
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

        egui::SidePanel::right("inspector").default_width(280.0).show(ctx, |ui| {
            ui.heading("Inspector");
            match self.selected.clone() {
                Some(id) => self.show_inspector(ui, &id),
                None => {
                    ui.label("Select a node.");
                }
            }
        });

        egui::TopBottomPanel::bottom("console").show(ctx, |ui| {
            ui.heading("Console");
            egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                for line in &self.console {
                    ui.monospace(line);
                }
            });
        });

        egui::SidePanel::left("assets").default_width(220.0).show(ctx, |ui| {
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
        let indent = "  ".repeat(depth.min(8));
        let id_string = id.to_string();
        let target = id.clone();
        let (_, dropped) = ui.dnd_drop_zone::<String, _>(egui::Frame::default(), |ui| {
            ui.dnd_drag_source(
                egui::Id::new(("tree-node", id_string.clone())),
                id_string.clone(),
                |ui| {
                    if children.is_empty() {
                        if ui.selectable_label(selected, format!("{indent}{name} ({type_name})")).clicked() {
                            self.selected = Some(id_string.clone());
                        }
                    } else {
                        egui::CollapsingHeader::new(format!("{name} ({type_name})"))
                            .default_open(true)
                            .show(ui, |ui| {
                                if ui.selectable_label(selected, "select").clicked() {
                                    self.selected = Some(id_string.clone());
                                }
                                for child in children {
                                    self.show_node(ui, &child, depth + 1);
                                }
                            });
                    }
                },
            );
        });
        if let Some(dragged) = dropped {
            let dragged_id = NodeId::from(dragged.as_str());
            if dragged_id != target {
                match self.session.host().with_tree_mut(|t| {
                    ops::move_node(t, &dragged_id, Some(target.clone()))
                }) {
                    Ok(()) => self.log(format!("moved {dragged_id} under {target}.")),
                    Err(e) => self.log(format!("move failed: {e:#}")),
                }
            }
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
                _ => {
                    ui.label(format!("{key}: {value:?}"));
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
        let (resp, painter) =
            ui.allocate_painter(egui::Vec2::new(ui.available_width(), 240.0), egui::Sense::hover());
        let center = resp.rect.center();
        painter.rect_filled(resp.rect, 0.0, crate::theme::BG);
        for (id, type_name, (x, y), text, (w, h)) in nodes {
            let p = center + egui::Vec2::new(x as f32, y as f32);
            let color = if type_name == "Sprite2D" {
                egui::Color32::LIGHT_GREEN
            } else {
                egui::Color32::LIGHT_BLUE
            };
            painter.circle_filled(p, 6.0, color);
            if type_name == "Button" && w > 0.0 && h > 0.0 {
                painter.rect_stroke(
                    egui::Rect::from_center_size(
                        p,
                        egui::Vec2::new(w as f32, h as f32),
                    ),
                    4.0,
                    egui::Stroke::new(1.0, egui::Color32::LIGHT_GREEN),
                    egui::StrokeKind::Middle,
                );
            }
            let caption = if text.is_empty() { id } else { text };
            painter.text(
                p + egui::Vec2::new(10.0, -10.0),
                egui::Align2::LEFT_TOP,
                caption,
                egui::FontId::monospace(11.0),
                egui::Color32::WHITE,
            );
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
                    self.log(format!("opened {}", file.display()));
                }
                Err(e) => self.log(format!("open failed: {e:#}")),
            }
        } else {
            self.log(format!("no opener for {}", file.display()));
        }
    }
}
