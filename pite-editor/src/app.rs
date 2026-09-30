use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use eframe::egui;
use pite_core::{NodeId, PropValue};
use pite_runtime::GameSession;

pub fn launch(scene: &Path) -> Result<()> {
    let app = EditorApp::new(scene)?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
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
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                ui.separator();
                ui.label(self.scene_path.to_string_lossy().as_ref());
            });
        });

        egui::SidePanel::left("tree").default_width(220.0).show(ctx, |ui| {
            ui.heading("Scene tree");
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
        if children.is_empty() {
            if ui.selectable_label(selected, format!("{indent}{name} ({type_name})")).clicked() {
                self.selected = Some(id.to_string());
            }
        } else {
            egui::CollapsingHeader::new(format!("{name} ({type_name})"))
                .default_open(true)
                .show(ui, |ui| {
                    if ui.selectable_label(selected, "select").clicked() {
                        self.selected = Some(id.to_string());
                    }
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
        let mut pos = match node.props.get("position") {
            Some(PropValue::Vec2(x, y)) => (*x, *y),
            _ => (0.0, 0.0),
        };
        ui.horizontal(|ui| {
            ui.label("position");
            if ui.add(egui::DragValue::new(&mut pos.0).speed(1.0)).changed()
                || ui.add(egui::DragValue::new(&mut pos.1).speed(1.0)).changed()
            {
                self.session.host().with_tree_mut(|t| {
                    if let Some(n) = t.get_mut(&node_id) {
                        n.props.insert("position".to_string(), PropValue::Vec2(pos.0, pos.1));
                    }
                });
            }
        });
        let mut texture = match node.props.get("texture") {
            Some(PropValue::Str(s)) => s.clone(),
            _ => String::new(),
        };
        ui.horizontal(|ui| {
            ui.label("texture");
            if ui.text_edit_singleline(&mut texture).changed() {
                self.session.host().with_tree_mut(|t| {
                    if let Some(n) = t.get_mut(&node_id) {
                        n.props.insert("texture".to_string(), PropValue::Str(texture.clone()));
                    }
                });
            }
        });
        if let Some(PropValue::Vec2(sw, sh)) = node.props.get("size") {
            let mut size = (*sw, *sh);
            ui.horizontal(|ui| {
                ui.label("size");
                if ui.add(egui::DragValue::new(&mut size.0).speed(1.0)).changed()
                    || ui.add(egui::DragValue::new(&mut size.1).speed(1.0)).changed()
                {
                    self.session.host().with_tree_mut(|t| {
                        if let Some(n) = t.get_mut(&node_id) {
                            n.props.insert("size".to_string(), PropValue::Vec2(size.0, size.1));
                        }
                    });
                }
            });
        }
        if let Some(PropValue::Str(current)) = node.props.get("text") {
            let mut text = current.clone();
            ui.horizontal(|ui| {
                ui.label("text");
                if ui.text_edit_singleline(&mut text).changed() {
                    self.session.host().with_tree_mut(|t| {
                        if let Some(n) = t.get_mut(&node_id) {
                            n.props.insert("text".to_string(), PropValue::Str(text.clone()));
                        }
                    });
                }
            });
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
        painter.rect_filled(resp.rect, 0.0, egui::Color32::from_gray(24));
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
