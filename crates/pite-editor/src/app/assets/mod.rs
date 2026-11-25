// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::{Path, PathBuf};

use eframe::egui;

pub use super::assets_fs::{
    copy_recursive, list_asset_dir, move_asset_rel, rename_asset_rel, resolve_duplicate_path,
    resolve_unique_path, show_in_file_manager, AssetClipboardEntry, AssetDndPayload, AssetModal,
    NewFileKind,
};
mod browser;
mod modals;

use super::{EditorApp, Tab};

impl EditorApp {
    pub(super) fn asset_files(&self) -> Vec<PathBuf> {
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
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
                    if out.len() < 500 {
                        stack.push(path);
                    }
                } else if out.len() < 500 {
                    out.push(path);
                }
            }
        }
        out.sort();
        out
    }

    pub(super) fn open_new_folder_modal(&mut self, parent_rel: &str) {
        self.asset_modal = Some(AssetModal::NewFolder {
            parent_rel: parent_rel.to_string(),
            name: "new_folder".to_string(),
            error: None,
        });
    }

    pub(super) fn open_new_file_modal(&mut self, parent_rel: &str, kind: NewFileKind) {
        let default_name = match kind {
            NewFileKind::Empty => "new_file.txt",
            NewFileKind::Script => "script.py",
            NewFileKind::Scene => "scene.ptscn",
        };
        self.asset_modal = Some(AssetModal::NewFile {
            parent_rel: parent_rel.to_string(),
            name: default_name.to_string(),
            kind,
            error: None,
        });
    }

    pub(super) fn open_rename_modal(&mut self, rel_path: &str) {
        let name = Path::new(rel_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(rel_path);
        self.asset_modal = Some(AssetModal::Rename {
            rel_path: rel_path.to_string(),
            new_name: name.to_string(),
            error: None,
        });
    }

    pub(super) fn open_delete_modal(&mut self, rel_path: &str, is_dir: bool) {
        self.asset_modal = Some(AssetModal::ConfirmDelete {
            rel_path: rel_path.to_string(),
            is_dir,
        });
    }

    pub(super) fn execute_asset_move(&mut self, src_rel: &str, dst_dir_rel: &str) {
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        match move_asset_rel(&root, src_rel, dst_dir_rel) {
            Ok(renames) => {
                self.apply_asset_renames(&renames);
                self.log(format!("Moved {src_rel} into {dst_dir_rel}"));
            }
            Err(e) => {
                self.log(format!("Move failed: {e:#}"));
            }
        }
    }

    pub(super) fn execute_asset_duplicate(&mut self, rel_path: &str) {
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        let src_fs = root.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !src_fs.exists() {
            return;
        }
        let dup_path = resolve_duplicate_path(&src_fs);
        match copy_recursive(&src_fs, &dup_path) {
            Ok(()) => {
                let _ = pite_assets::reimport_project(&root);
                if let Ok(dup_rel) = dup_path.strip_prefix(&root) {
                    self.log(format!(
                        "Duplicated {rel_path} -> {}",
                        dup_rel.to_string_lossy()
                    ));
                }
            }
            Err(e) => self.log(format!("Duplicate failed: {e:#}")),
        }
    }

    pub(super) fn execute_asset_delete(&mut self, rel_path: &str, is_dir: bool) {
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        let target = root.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let res = if is_dir {
            std::fs::remove_dir_all(&target)
        } else {
            std::fs::remove_file(&target)
        };
        match res {
            Ok(()) => {
                if let Some(code) = &self.code_file {
                    if code == &target || code.starts_with(&target) {
                        self.code_file = None;
                        self.code_text.clear();
                    }
                }
                if self.selected_asset.as_deref() == Some(rel_path) {
                    self.selected_asset = None;
                }
                let _ = pite_assets::reimport_project(&root);
                self.log(format!("Deleted {rel_path}"));
            }
            Err(e) => self.log(format!("Delete failed: {e:#}")),
        }
    }

    pub(super) fn execute_asset_paste(&mut self, target_dir_rel: &str) {
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        let target_dir_fs = if target_dir_rel.is_empty() {
            root.clone()
        } else {
            root.join(target_dir_rel.replace('/', std::path::MAIN_SEPARATOR_STR))
        };

        if let Some(clip) = self.asset_clipboard.take() {
            if clip.is_cut {
                self.execute_asset_move(&clip.rel_path, target_dir_rel);
            } else {
                let src_fs = root.join(clip.rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
                if src_fs.exists() {
                    let name = src_fs
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file");
                    let dst_fs = resolve_unique_path(&target_dir_fs, name);
                    if let Err(e) = copy_recursive(&src_fs, &dst_fs) {
                        self.log(format!("Paste copy failed: {e:#}"));
                    } else {
                        self.log(format!("Pasted copy of res://{}", clip.rel_path));
                        let _ = pite_assets::reimport_project(&root);
                    }
                }
            }
            return;
        }

        // Check if system clipboard has valid filesystem paths
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                let mut count = 0;
                for line in text.lines() {
                    let trimmed = line.trim().trim_matches('"');
                    let p = Path::new(trimmed);
                    if p.exists() {
                        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                        let dst_fs = resolve_unique_path(&target_dir_fs, name);
                        if copy_recursive(p, &dst_fs).is_ok() {
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    self.log(format!("Pasted {count} file(s) from clipboard"));
                    let _ = pite_assets::reimport_project(&root);
                }
            }
        }
    }

    pub(super) fn handle_os_drop(&mut self, ctx: &egui::Context, root: &Path) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }
        let target_dir_rel = self.hovered_asset_folder.clone().unwrap_or_default();
        let target_dir_fs = if target_dir_rel.is_empty() {
            root.to_path_buf()
        } else {
            root.join(target_dir_rel.replace('/', std::path::MAIN_SEPARATOR_STR))
        };
        let mut count = 0;
        for file in dropped {
            let src_path = file.path();
            if !src_path.as_os_str().is_empty() && src_path.exists() {
                let name = src_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file");
                let dst_path = resolve_unique_path(&target_dir_fs, name);
                if let Err(e) = copy_recursive(src_path, &dst_path) {
                    self.log(format!("Failed to copy {}: {e:#}", src_path.display()));
                } else {
                    count += 1;
                }
            }
        }
        if count > 0 {
            let target_desc = if target_dir_rel.is_empty() {
                "res://"
            } else {
                &target_dir_rel
            };
            self.log(format!("Imported {count} item(s) into {target_desc}"));
            let _ = pite_assets::reimport_project(root);
        }
    }

    pub(super) fn apply_asset_renames(&mut self, renames: &[(String, String)]) {
        if renames.is_empty() {
            return;
        }
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        match pite_assets::apply_renames(&root, renames) {
            Ok(touched) => {
                if !touched.is_empty() {
                    self.log(format!(
                        "Updated references in {} scene/script file(s).",
                        touched.len()
                    ));
                }
            }
            Err(e) => {
                self.log(format!("Reference rewrite error: {e:#}"));
            }
        }
        if let Err(e) = pite_assets::reimport_project(&root) {
            self.log(format!("Asset reimport error: {e:#}"));
        }
        // Update code_file if the open file was renamed
        if let Some(code_path) = &self.code_file {
            let rel = code_path
                .strip_prefix(&root)
                .ok()
                .map(|p| format!("res://{}", p.to_string_lossy().replace('\\', "/")));
            if let Some(res) = rel {
                for (old, new) in renames {
                    if res == *old {
                        if let Some(new_rel) = new.strip_prefix("res://") {
                            self.code_file = Some(
                                root.join(new_rel.replace('/', std::path::MAIN_SEPARATOR_STR)),
                            );
                        }
                    }
                }
            }
        }
    }
    pub(super) fn resolve_res(&self, res_path: &str) -> Option<PathBuf> {
        let base = self.project_dir.as_deref().unwrap_or(Path::new("."));
        pite_core::resolve_project_ref(self.project_dir.as_deref(), base, res_path).ok()
    }

    pub(super) fn open_asset(&mut self, file: &Path) {
        if Self::editor_text_file(file) {
            if self.code_file.as_deref() == Some(file) {
                self.tab = Tab::Code;
                return;
            }
            if self.code_is_dirty() {
                self.save_code();
            }
            match std::fs::read_to_string(file) {
                Ok(text) => {
                    self.code_file = Some(file.to_path_buf());
                    self.code_text = text;
                    self.tab = Tab::Code;
                    self.close_completion();
                    self.lsp_hint.clear();
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
