// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use eframe::egui;

use crate::icons;

use super::{EditorApp, Tab};

impl EditorApp {
    pub(super) fn asset_files(&self) -> Vec<PathBuf> {
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

    pub(super) fn asset_rel_paths(&self) -> Vec<String> {
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

    pub(super) fn show_asset_dir(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        dir: &str,
        paths: &[&str],
    ) {
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
                "ptscn" => icons::Icon::Scene,
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

    pub(super) fn resolve_res(&self, res_path: &str) -> PathBuf {
        if let Some(rel) = res_path.strip_prefix("res://") {
            self.project_dir
                .as_deref()
                .unwrap_or(Path::new("."))
                .join(rel)
        } else {
            PathBuf::from(res_path)
        }
    }

    pub(super) fn open_asset(&mut self, file: &Path) {
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
        } else if file.extension().and_then(|e| e.to_str()) == Some("ptscn") {
            self.open_scene_file(file);
        } else {
            self.log(format!("no opener for {}", file.display()));
        }
    }
}
