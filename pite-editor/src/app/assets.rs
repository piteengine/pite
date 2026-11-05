// SPDX-License-Identifier: MIT OR Apache-2.0
use std::path::{Path, PathBuf};

use eframe::egui;
use egui_phosphor::regular;

use crate::icons;

use super::{EditorApp, Tab};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewFileKind {
    Empty,
    Script,
    Scene,
}

#[derive(Debug, Clone)]
pub enum AssetModal {
    NewFolder {
        parent_rel: String,
        name: String,
        error: Option<String>,
    },
    NewFile {
        parent_rel: String,
        name: String,
        kind: NewFileKind,
        error: Option<String>,
    },
    Rename {
        rel_path: String,
        new_name: String,
        error: Option<String>,
    },
    ConfirmDelete {
        rel_path: String,
        is_dir: bool,
    },
}

#[derive(Debug, Clone)]
pub struct AssetClipboardEntry {
    pub rel_path: String,
    pub is_cut: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetDndPayload {
    pub rel_path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub rel_path: String,
    #[allow(dead_code)]
    pub is_dir: bool,
}

/// List children of `rel_dir` inside `root`. Returns `(folders, files)`
/// both sorted alphabetically by lowercase name. Empty folders are preserved.
pub fn list_asset_dir(root: &Path, rel_dir: &str) -> (Vec<DirEntry>, Vec<DirEntry>) {
    let fs_dir = if rel_dir.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel_dir.replace('/', std::path::MAIN_SEPARATOR_STR))
    };
    let Ok(entries) = std::fs::read_dir(&fs_dir) else {
        return (Vec::new(), Vec::new());
    };
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if name == "target"
                || name == "dist"
                || name == ".git"
                || name == pite_scene::cache::CACHE_DIR_NAME
            {
                continue;
            }
            let child_rel = if rel_dir.is_empty() {
                name.clone()
            } else {
                format!("{rel_dir}/{name}")
            };
            folders.push(DirEntry {
                name,
                rel_path: child_rel,
                is_dir: true,
            });
        } else {
            let child_rel = if rel_dir.is_empty() {
                name.clone()
            } else {
                format!("{rel_dir}/{name}")
            };
            files.push(DirEntry {
                name,
                rel_path: child_rel,
                is_dir: false,
            });
        }
    }
    folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    (folders, files)
}

/// Resolves a non-colliding path in `parent`: if `name` already exists,
/// suffixes it with `stem (1).ext`, `stem (2).ext`, etc.
pub fn resolve_unique_path(parent: &Path, name: &str) -> PathBuf {
    let target = parent.join(name);
    if !target.exists() {
        return target;
    }
    let p = Path::new(name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let mut count = 1;
    loop {
        let candidate = format!("{stem} ({count}){ext}");
        let cand_path = parent.join(&candidate);
        if !cand_path.exists() {
            return cand_path;
        }
        count += 1;
    }
}

/// Resolves a duplicate name for `path`: `stem (copy).ext`, `stem (copy 2).ext`.
pub fn resolve_duplicate_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let p = Path::new(name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let mut count = 1;
    loop {
        let candidate = if count == 1 {
            format!("{stem} (copy){ext}")
        } else {
            format!("{stem} (copy {count}){ext}")
        };
        let cand_path = parent.join(&candidate);
        if !cand_path.exists() {
            return cand_path;
        }
        count += 1;
    }
}

/// Recursively copies a file or directory tree.
pub fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let sub_src = entry.path();
            let sub_dst = dst.join(entry.file_name());
            copy_recursive(&sub_src, &sub_dst)?;
        }
        Ok(())
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        Ok(())
    }
}

/// Recursively collects rename pairs for all files within a directory tree.
fn collect_dir_renames(
    root: &Path,
    dir: &Path,
    old_base_rel: &str,
    new_base_rel: &str,
    renames: &mut Vec<(String, String)>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_dir_renames(root, &path, old_base_rel, new_base_rel, renames)?;
        } else {
            let rel = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            if let Some(sub) = rel.strip_prefix(old_base_rel) {
                let new_rel = format!("{new_base_rel}{sub}");
                renames.push((format!("res://{rel}"), format!("res://{new_rel}")));
            }
        }
    }
    Ok(())
}

/// Moves an asset on disk and returns the list of `(old_res, new_res)` reference pairs.
/// Enforces directory cycle checks.
pub fn move_asset_rel(
    root: &Path,
    src_rel: &str,
    dst_dir_rel: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let src_fs = root.join(src_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    if !src_fs.exists() {
        anyhow::bail!("Source path does not exist: {src_rel}");
    }
    let filename = Path::new(src_rel)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid source path: {src_rel}"))?;

    // Cycle check: cannot move a folder into itself or a descendant
    if src_fs.is_dir() {
        let prefix = format!("{src_rel}/");
        if dst_dir_rel == src_rel || dst_dir_rel.starts_with(&prefix) {
            anyhow::bail!("Cannot move folder into itself or its subdirectory");
        }
    }

    let dst_rel = if dst_dir_rel.is_empty() {
        filename.to_string()
    } else {
        format!("{dst_dir_rel}/{filename}")
    };
    if dst_rel == src_rel {
        return Ok(Vec::new());
    }

    let dst_fs = root.join(dst_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    if dst_fs.exists() {
        anyhow::bail!("Destination already exists: {dst_rel}");
    }

    let mut renames = Vec::new();
    if src_fs.is_dir() {
        collect_dir_renames(root, &src_fs, src_rel, &dst_rel, &mut renames)?;
    } else {
        renames.push((format!("res://{src_rel}"), format!("res://{dst_rel}")));
    }

    std::fs::rename(&src_fs, &dst_fs)?;
    Ok(renames)
}

/// Renames an asset on disk and returns the list of `(old_res, new_res)` reference pairs.
pub fn rename_asset_rel(
    root: &Path,
    old_rel: &str,
    new_name: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let old_fs = root.join(old_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    if !old_fs.exists() {
        anyhow::bail!("Path does not exist: {old_rel}");
    }
    let parent = Path::new(old_rel).parent().unwrap_or(Path::new(""));
    let new_rel = if parent.as_os_str().is_empty() {
        new_name.to_string()
    } else {
        format!("{}/{new_name}", parent.to_string_lossy().replace('\\', "/"))
    };
    if new_rel == old_rel {
        return Ok(Vec::new());
    }
    let new_fs = root.join(new_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    if new_fs.exists() {
        anyhow::bail!("An item named '{new_name}' already exists");
    }

    let mut renames = Vec::new();
    if old_fs.is_dir() {
        collect_dir_renames(root, &old_fs, old_rel, &new_rel, &mut renames)?;
    } else {
        renames.push((format!("res://{old_rel}"), format!("res://{new_rel}")));
    }

    std::fs::rename(&old_fs, &new_fs)?;
    Ok(renames)
}

/// Opens the operating system's file manager at the specified path.
pub fn show_in_file_manager(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg("/select,")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let dir = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(Path::new("."))
        };
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

impl EditorApp {
    pub(super) fn show_asset_dir(&mut self, ui: &mut egui::Ui, root: &Path, dir: &str) {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 1.0);
        ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
        ui.spacing_mut().indent = 14.0;

        let has_dnd = egui::DragAndDrop::has_any_payload(ui.ctx());

        // Root entry shown once at the top of the tree
        if dir.is_empty() {
            let root_selected = self.selected_asset.as_deref() == Some("");
            let is_drop_target_root =
                if let Some(dragged) = egui::DragAndDrop::payload::<AssetDndPayload>(ui.ctx()) {
                    let parent = Path::new(&dragged.rel_path)
                        .parent()
                        .unwrap_or(Path::new(""));
                    !parent.as_os_str().is_empty()
                } else {
                    false
                };

            let row_width = ui.available_width().max(160.0);
            let (root_rect, root_resp) =
                ui.allocate_exact_size(egui::vec2(row_width, 20.0), egui::Sense::click());

            let is_hovered = root_resp.hovered();

            if is_hovered && is_drop_target_root && has_dnd {
                self.hovered_asset_folder = Some(String::new());
                ui.painter().rect_stroke(
                    root_rect,
                    2.0,
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(11, 147, 135)),
                    egui::StrokeKind::Middle,
                );
            } else if root_selected {
                ui.painter()
                    .rect_filled(root_rect, 2.0, ui.visuals().selection.bg_fill);
            } else if is_hovered && !has_dnd {
                ui.painter()
                    .rect_filled(root_rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
            }

            if root_resp.clicked() {
                self.selected_asset = Some(String::new());
            }

            if let Some(dragged) = root_resp.dnd_release_payload::<AssetDndPayload>() {
                let parent = Path::new(&dragged.rel_path)
                    .parent()
                    .unwrap_or(Path::new(""));
                if !parent.as_os_str().is_empty() {
                    self.execute_asset_move(&dragged.rel_path, "");
                }
            }

            let p = ui.painter();
            let fg = if root_selected {
                crate::theme::TEXT
            } else {
                crate::theme::FAINT
            };
            let icon_rect = egui::Rect::from_min_size(
                egui::Pos2::new(root_rect.left() + 4.0, root_rect.center().y - 8.0),
                egui::vec2(16.0, 16.0),
            );
            icons::draw_icon(p, icon_rect, icons::Icon::FolderOpen, fg);

            let text_pos = egui::Pos2::new(root_rect.left() + 24.0, root_rect.center().y);
            p.text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                "res:// (project root)",
                egui::FontId::new(13.0, egui::FontFamily::Proportional),
                crate::theme::TEXT,
            );

            root_resp.context_menu(|ui| {
                if ui.button("New File…").clicked() {
                    self.open_new_file_modal("", NewFileKind::Empty);
                    ui.close();
                }
                if ui.button("New Script (.py)").clicked() {
                    self.open_new_file_modal("", NewFileKind::Script);
                    ui.close();
                }
                if ui.button("New Scene (.ptscn)").clicked() {
                    self.open_new_file_modal("", NewFileKind::Scene);
                    ui.close();
                }
                if ui.button("New Folder…").clicked() {
                    self.open_new_folder_modal("");
                    ui.close();
                }
                ui.separator();
                let can_paste = self.asset_clipboard.is_some();
                if ui
                    .add_enabled(can_paste, egui::Button::new("Paste"))
                    .clicked()
                {
                    self.execute_asset_paste("");
                    ui.close();
                }
                ui.separator();
                if ui.button("Show in File Manager").clicked() {
                    show_in_file_manager(root);
                    ui.close();
                }
            });
        }

        let (folders, files) = list_asset_dir(root, dir);

        // 1. Folders first (alphabetical)
        for folder in folders {
            let full = folder.rel_path.clone();
            let key = format!("assets:{full}");
            let open = self.open_nodes.contains(&key);
            let selected = self.selected_asset.as_deref() == Some(&full);

            let row_width = ui.available_width().max(160.0);
            let (rect, row_resp) =
                ui.allocate_exact_size(egui::vec2(row_width, 20.0), egui::Sense::click_and_drag());

            let is_valid_drop_target =
                if let Some(dragged) = egui::DragAndDrop::payload::<AssetDndPayload>(ui.ctx()) {
                    let dragged_parent = Path::new(&dragged.rel_path)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or("")
                        .replace('\\', "/");
                    dragged.rel_path != full
                        && dragged_parent != full
                        && !full.starts_with(&format!("{}/", dragged.rel_path))
                } else {
                    false
                };

            // Start drag only when threshold is passed
            if row_resp.drag_started() {
                egui::DragAndDrop::set_payload(
                    ui.ctx(),
                    AssetDndPayload {
                        rel_path: full.clone(),
                        is_dir: true,
                    },
                );
            }

            let mut toggled = false;
            if row_resp.clicked() {
                // If clicked on the left arrow region, toggle open/close
                if let Some(pos) = row_resp.interact_pointer_pos() {
                    if pos.x < rect.left() + 16.0 {
                        toggled = true;
                    } else {
                        self.selected_asset = Some(full.clone());
                    }
                } else {
                    self.selected_asset = Some(full.clone());
                }
            }
            if row_resp.double_clicked() {
                toggled = true;
            }

            if let Some(dragged) = row_resp.dnd_release_payload::<AssetDndPayload>() {
                let dragged_parent = Path::new(&dragged.rel_path)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("")
                    .replace('\\', "/");
                if dragged.rel_path != full
                    && dragged_parent != full
                    && !full.starts_with(&format!("{}/", dragged.rel_path))
                {
                    self.execute_asset_move(&dragged.rel_path, &full);
                }
            }

            let is_hovered = row_resp.hovered();
            if is_hovered && is_valid_drop_target && has_dnd {
                self.hovered_asset_folder = Some(full.clone());
                ui.painter().rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(11, 147, 135)),
                    egui::StrokeKind::Middle,
                );
            } else if selected {
                ui.painter()
                    .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
            } else if is_hovered && !has_dnd {
                ui.painter()
                    .rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
            }

            let p = ui.painter();
            let fg = if selected {
                crate::theme::TEXT
            } else {
                crate::theme::FAINT
            };

            // Caret expand/collapse arrow
            let caret_icon = if open {
                icons::Icon::CaretDown
            } else {
                icons::Icon::CaretRight
            };
            let caret_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.left() + 2.0, rect.center().y - 6.0),
                egui::vec2(12.0, 12.0),
            );
            icons::draw_icon(p, caret_rect, caret_icon, fg);

            // Folder icon
            let folder_icon = if open {
                icons::Icon::FolderOpen
            } else {
                icons::Icon::Folder
            };
            let f_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.left() + 16.0, rect.center().y - 8.0),
                egui::vec2(16.0, 16.0),
            );
            icons::draw_icon(p, f_rect, folder_icon, fg);

            // Folder label
            let text_pos = egui::Pos2::new(rect.left() + 36.0, rect.center().y);
            p.text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                &folder.name,
                egui::FontId::new(13.0, egui::FontFamily::Proportional),
                crate::theme::TEXT,
            );

            // Context menu for folder
            let folder_rel = full.clone();
            row_resp.context_menu(|ui| {
                if ui.button("New File…").clicked() {
                    self.open_new_file_modal(&folder_rel, NewFileKind::Empty);
                    ui.close();
                }
                if ui.button("New Script (.py)").clicked() {
                    self.open_new_file_modal(&folder_rel, NewFileKind::Script);
                    ui.close();
                }
                if ui.button("New Scene (.ptscn)").clicked() {
                    self.open_new_file_modal(&folder_rel, NewFileKind::Scene);
                    ui.close();
                }
                if ui.button("New Folder…").clicked() {
                    self.open_new_folder_modal(&folder_rel);
                    ui.close();
                }
                ui.separator();
                if ui.button("Cut").clicked() {
                    self.asset_clipboard = Some(AssetClipboardEntry {
                        rel_path: folder_rel.clone(),
                        is_cut: true,
                    });
                    ui.close();
                }
                if ui.button("Copy").clicked() {
                    self.asset_clipboard = Some(AssetClipboardEntry {
                        rel_path: folder_rel.clone(),
                        is_cut: false,
                    });
                    ui.close();
                }
                let can_paste = self.asset_clipboard.is_some();
                if ui
                    .add_enabled(can_paste, egui::Button::new("Paste"))
                    .clicked()
                {
                    self.execute_asset_paste(&folder_rel);
                    ui.close();
                }
                if ui.button("Rename…").clicked() {
                    self.open_rename_modal(&folder_rel);
                    ui.close();
                }
                if ui.button("Delete…").clicked() {
                    self.open_delete_modal(&folder_rel, true);
                    ui.close();
                }
                ui.separator();
                if ui.button("Copy Relative Path (res://…)").clicked() {
                    ui.ctx().copy_text(format!("res://{folder_rel}"));
                    self.log(format!("Copied res://{folder_rel}"));
                    ui.close();
                }
                if ui.button("Copy Absolute Path").clicked() {
                    let abs = root.join(folder_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    ui.ctx().copy_text(abs.to_string_lossy().to_string());
                    ui.close();
                }
                if ui.button("Show in File Manager").clicked() {
                    let abs = root.join(folder_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    show_in_file_manager(&abs);
                    ui.close();
                }
            });

            if toggled {
                if open {
                    self.open_nodes.remove(&key);
                } else {
                    self.open_nodes.insert(key.clone());
                }
            }
            if open {
                ui.indent(format!("assets-indent:{full}"), |ui| {
                    self.show_asset_dir(ui, root, &full);
                });
            }
        }

        // 2. Files next (alphabetical)
        for file in files {
            let full = file.rel_path.clone();
            let selected = self.selected_asset.as_deref() == Some(&full);

            let kind = match file
                .name
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

            let row_width = ui.available_width().max(160.0);
            let (rect, row_resp) =
                ui.allocate_exact_size(egui::vec2(row_width, 20.0), egui::Sense::click_and_drag());

            // Dropping onto a file targets this file's parent folder `dir`!
            let is_valid_drop_target =
                if let Some(dragged) = egui::DragAndDrop::payload::<AssetDndPayload>(ui.ctx()) {
                    let dragged_parent = Path::new(&dragged.rel_path)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or("")
                        .replace('\\', "/");
                    dragged.rel_path != dir
                        && dragged_parent != dir
                        && !dir.starts_with(&format!("{}/", dragged.rel_path))
                } else {
                    false
                };

            // Start drag only when threshold is passed
            if row_resp.drag_started() {
                egui::DragAndDrop::set_payload(
                    ui.ctx(),
                    AssetDndPayload {
                        rel_path: full.clone(),
                        is_dir: false,
                    },
                );
            }

            if row_resp.clicked() {
                self.selected_asset = Some(full.clone());
            }
            if row_resp.double_clicked() {
                let abs = root.join(full.replace('/', std::path::MAIN_SEPARATOR_STR));
                self.open_asset(&abs);
            }

            // Dropping onto a file moves into parent folder `dir`
            if let Some(dragged) = row_resp.dnd_release_payload::<AssetDndPayload>() {
                let dragged_parent = Path::new(&dragged.rel_path)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("")
                    .replace('\\', "/");
                if dragged.rel_path != dir
                    && dragged_parent != dir
                    && !dir.starts_with(&format!("{}/", dragged.rel_path))
                {
                    self.execute_asset_move(&dragged.rel_path, dir);
                }
            }

            let is_hovered = row_resp.hovered();
            if is_hovered && is_valid_drop_target && has_dnd {
                self.hovered_asset_folder = Some(dir.to_string());
                ui.painter().rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(11, 147, 135)),
                    egui::StrokeKind::Middle,
                );
            } else if selected {
                ui.painter()
                    .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
            } else if is_hovered && !has_dnd {
                ui.painter()
                    .rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
            }

            let p = ui.painter();
            let fg = if selected {
                crate::theme::TEXT
            } else {
                crate::theme::FAINT
            };

            let f_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.left() + 16.0, rect.center().y - 8.0),
                egui::vec2(16.0, 16.0),
            );
            icons::draw_icon(p, f_rect, kind, fg);

            let text_pos = egui::Pos2::new(rect.left() + 36.0, rect.center().y);
            p.text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                &file.name,
                egui::FontId::new(13.0, egui::FontFamily::Proportional),
                crate::theme::TEXT,
            );

            // Context menu for file
            let file_rel = full.clone();
            row_resp.context_menu(|ui| {
                if ui.button("Open").clicked() {
                    let abs = root.join(file_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    self.open_asset(&abs);
                    ui.close();
                }
                ui.separator();
                if ui.button("Cut").clicked() {
                    self.asset_clipboard = Some(AssetClipboardEntry {
                        rel_path: file_rel.clone(),
                        is_cut: true,
                    });
                    ui.close();
                }
                if ui.button("Copy").clicked() {
                    self.asset_clipboard = Some(AssetClipboardEntry {
                        rel_path: file_rel.clone(),
                        is_cut: false,
                    });
                    ui.close();
                }
                if ui.button("Duplicate").clicked() {
                    self.execute_asset_duplicate(&file_rel);
                    ui.close();
                }
                if ui.button("Rename…").clicked() {
                    self.open_rename_modal(&file_rel);
                    ui.close();
                }
                if ui.button("Delete…").clicked() {
                    self.open_delete_modal(&file_rel, false);
                    ui.close();
                }
                ui.separator();
                if ui.button("Copy Relative Path (res://…)").clicked() {
                    ui.ctx().copy_text(format!("res://{file_rel}"));
                    self.log(format!("Copied res://{file_rel}"));
                    ui.close();
                }
                if ui.button("Copy Absolute Path").clicked() {
                    let abs = root.join(file_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    ui.ctx().copy_text(abs.to_string_lossy().to_string());
                    ui.close();
                }
                if ui.button("Show in File Manager").clicked() {
                    let abs = root.join(file_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                    show_in_file_manager(&abs);
                    ui.close();
                }
            });
        }

        // Floating drag preview under cursor
        if dir.is_empty() {
            if let Some(payload) = egui::DragAndDrop::payload::<AssetDndPayload>(ui.ctx()) {
                if let Some(pos) = ui.ctx().pointer_latest_pos() {
                    egui::Area::new(egui::Id::unique("asset_drag_tooltip"))
                        .fixed_pos(pos + egui::vec2(16.0, 16.0))
                        .order(egui::Order::Tooltip)
                        .interactable(false)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                let icon_str = if payload.is_dir {
                                    regular::FOLDER
                                } else {
                                    regular::FILE
                                };
                                ui.horizontal(|ui| {
                                    ui.label(icon_str);
                                    ui.label(&payload.rel_path);
                                });
                            });
                        });
                }
            }
        }
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

    pub(super) fn show_asset_modals(&mut self, ctx: &egui::Context, root: &Path) {
        let Some(modal) = self.asset_modal.clone() else {
            return;
        };
        let mut close = false;

        enum Action {
            CreateFolder {
                parent_rel: String,
                name: String,
            },
            CreateFile {
                parent_rel: String,
                name: String,
                kind: NewFileKind,
            },
            Rename {
                rel_path: String,
                new_name: String,
            },
            Delete {
                rel_path: String,
                is_dir: bool,
            },
        }
        let mut action: Option<Action> = None;

        match modal {
            AssetModal::NewFolder {
                parent_rel,
                mut name,
                error,
            } => {
                egui::Window::new("New Folder")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        let dest_desc = if parent_rel.is_empty() {
                            "res:// (project root)".to_string()
                        } else {
                            format!("res://{parent_rel}")
                        };
                        ui.label(format!("Create inside: {dest_desc}"));
                        ui.add_space(4.0);

                        let text_edit = ui.add(
                            egui::TextEdit::singleline(&mut name)
                                .hint_text("folder_name")
                                .desired_width(240.0),
                        );
                        if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            action = Some(Action::CreateFolder {
                                parent_rel: parent_rel.clone(),
                                name: name.clone(),
                            });
                        }

                        if let Some(err) = &error {
                            ui.colored_label(egui::Color32::from_rgb(230, 80, 80), err);
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Create").clicked() {
                                action = Some(Action::CreateFolder {
                                    parent_rel: parent_rel.clone(),
                                    name: name.clone(),
                                });
                            }
                            if ui.button("Cancel").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                close = true;
                            }
                        });
                        self.asset_modal = Some(AssetModal::NewFolder {
                            parent_rel,
                            name,
                            error,
                        });
                    });
            }
            AssetModal::NewFile {
                parent_rel,
                mut name,
                kind,
                error,
            } => {
                let title = match kind {
                    NewFileKind::Empty => "New File",
                    NewFileKind::Script => "New Script",
                    NewFileKind::Scene => "New Scene",
                };
                egui::Window::new(title)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        let dest_desc = if parent_rel.is_empty() {
                            "res:// (project root)".to_string()
                        } else {
                            format!("res://{parent_rel}")
                        };
                        ui.label(format!("Create inside: {dest_desc}"));
                        ui.add_space(4.0);

                        let hint = match kind {
                            NewFileKind::Empty => "filename.ext",
                            NewFileKind::Script => "script_name.py",
                            NewFileKind::Scene => "scene_name.ptscn",
                        };
                        let text_edit = ui.add(
                            egui::TextEdit::singleline(&mut name)
                                .hint_text(hint)
                                .desired_width(240.0),
                        );
                        if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            action = Some(Action::CreateFile {
                                parent_rel: parent_rel.clone(),
                                name: name.clone(),
                                kind: kind.clone(),
                            });
                        }

                        if let Some(err) = &error {
                            ui.colored_label(egui::Color32::from_rgb(230, 80, 80), err);
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Create").clicked() {
                                action = Some(Action::CreateFile {
                                    parent_rel: parent_rel.clone(),
                                    name: name.clone(),
                                    kind: kind.clone(),
                                });
                            }
                            if ui.button("Cancel").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                close = true;
                            }
                        });
                        self.asset_modal = Some(AssetModal::NewFile {
                            parent_rel,
                            name,
                            kind,
                            error,
                        });
                    });
            }
            AssetModal::Rename {
                rel_path,
                mut new_name,
                error,
            } => {
                egui::Window::new("Rename Asset")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Rename: res://{rel_path}"));
                        ui.add_space(4.0);

                        let text_edit =
                            ui.add(egui::TextEdit::singleline(&mut new_name).desired_width(240.0));
                        if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            action = Some(Action::Rename {
                                rel_path: rel_path.clone(),
                                new_name: new_name.clone(),
                            });
                        }

                        if let Some(err) = &error {
                            ui.colored_label(egui::Color32::from_rgb(230, 80, 80), err);
                        }

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Rename").clicked() {
                                action = Some(Action::Rename {
                                    rel_path: rel_path.clone(),
                                    new_name: new_name.clone(),
                                });
                            }
                            if ui.button("Cancel").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                close = true;
                            }
                        });
                        self.asset_modal = Some(AssetModal::Rename {
                            rel_path,
                            new_name,
                            error,
                        });
                    });
            }
            AssetModal::ConfirmDelete { rel_path, is_dir } => {
                egui::Window::new("Delete Asset")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        let kind_str = if is_dir {
                            "folder and all its contents"
                        } else {
                            "file"
                        };
                        ui.label(format!("Are you sure you want to delete this {kind_str}?"));
                        ui.colored_label(
                            egui::Color32::from_rgb(230, 80, 80),
                            format!("res://{rel_path}"),
                        );
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            if ui.button("Delete").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                action = Some(Action::Delete {
                                    rel_path: rel_path.clone(),
                                    is_dir,
                                });
                            }
                            if ui.button("Cancel").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                close = true;
                            }
                        });
                    });
            }
        }

        if close {
            self.asset_modal = None;
        }

        if let Some(act) = action {
            match act {
                Action::CreateFolder { parent_rel, name } => {
                    let trimmed = name.trim();
                    if trimmed.is_empty() {
                        self.asset_modal = Some(AssetModal::NewFolder {
                            parent_rel,
                            name,
                            error: Some("Folder name cannot be empty".to_string()),
                        });
                        return;
                    }
                    if trimmed.contains('/') || trimmed.contains('\\') {
                        self.asset_modal = Some(AssetModal::NewFolder {
                            parent_rel,
                            name,
                            error: Some("Folder name cannot contain slashes".to_string()),
                        });
                        return;
                    }
                    let target_fs = if parent_rel.is_empty() {
                        root.join(trimmed)
                    } else {
                        root.join(parent_rel.replace('/', std::path::MAIN_SEPARATOR_STR))
                            .join(trimmed)
                    };
                    if target_fs.exists() {
                        self.asset_modal = Some(AssetModal::NewFolder {
                            parent_rel,
                            name,
                            error: Some("A folder with this name already exists".to_string()),
                        });
                        return;
                    }
                    if let Err(e) = std::fs::create_dir_all(&target_fs) {
                        self.asset_modal = Some(AssetModal::NewFolder {
                            parent_rel,
                            name,
                            error: Some(format!("Failed to create folder: {e:#}")),
                        });
                        return;
                    }
                    if !parent_rel.is_empty() {
                        self.open_nodes.insert(format!("assets:{parent_rel}"));
                    }
                    self.asset_modal = None;
                    self.log(format!("Created folder res://{parent_rel}/{trimmed}"));
                }
                Action::CreateFile {
                    parent_rel,
                    name,
                    kind,
                } => {
                    let trimmed = name.trim();
                    if trimmed.is_empty() {
                        self.asset_modal = Some(AssetModal::NewFile {
                            parent_rel,
                            name,
                            kind,
                            error: Some("File name cannot be empty".to_string()),
                        });
                        return;
                    }
                    if trimmed.contains('/') || trimmed.contains('\\') {
                        self.asset_modal = Some(AssetModal::NewFile {
                            parent_rel,
                            name,
                            kind,
                            error: Some("File name cannot contain slashes".to_string()),
                        });
                        return;
                    }
                    let full_name = match kind {
                        NewFileKind::Script if !trimmed.ends_with(".py") => {
                            format!("{trimmed}.py")
                        }
                        NewFileKind::Scene if !trimmed.ends_with(".ptscn") => {
                            format!("{trimmed}.ptscn")
                        }
                        _ => trimmed.to_string(),
                    };
                    let target_fs = if parent_rel.is_empty() {
                        root.join(&full_name)
                    } else {
                        root.join(parent_rel.replace('/', std::path::MAIN_SEPARATOR_STR))
                            .join(&full_name)
                    };
                    if target_fs.exists() {
                        self.asset_modal = Some(AssetModal::NewFile {
                            parent_rel,
                            name: full_name,
                            kind,
                            error: Some("A file with this name already exists".to_string()),
                        });
                        return;
                    }
                    let content = match kind {
                        NewFileKind::Script => {
                            let class_name = Path::new(&full_name)
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("MyScript");
                            let mut c_chars = class_name.chars();
                            let pascal_case = match c_chars.next() {
                                Some(first) => {
                                    first.to_uppercase().collect::<String>() + c_chars.as_str()
                                }
                                None => "MyScript".to_string(),
                            };
                            format!(
                                "import pite\n\nclass {pascal_case}(pite.Node):\n    def _ready(self):\n        pass\n\n    def _process(self, delta):\n        pass\n"
                            )
                        }
                        NewFileKind::Scene => {
                            "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n".to_string()
                        }
                        NewFileKind::Empty => String::new(),
                    };
                    if let Some(parent) = target_fs.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(&target_fs, content) {
                        self.asset_modal = Some(AssetModal::NewFile {
                            parent_rel,
                            name: full_name,
                            kind,
                            error: Some(format!("Failed to write file: {e:#}")),
                        });
                        return;
                    }
                    if !parent_rel.is_empty() {
                        self.open_nodes.insert(format!("assets:{parent_rel}"));
                    }
                    let _ = pite_assets::reimport_project(root);
                    self.asset_modal = None;
                    self.open_asset(&target_fs);
                    self.log(format!("Created file res://{parent_rel}/{full_name}"));
                }
                Action::Rename { rel_path, new_name } => {
                    let trimmed = new_name.trim();
                    if trimmed.is_empty() {
                        self.asset_modal = Some(AssetModal::Rename {
                            rel_path,
                            new_name,
                            error: Some("Name cannot be empty".to_string()),
                        });
                        return;
                    }
                    if trimmed.contains('/') || trimmed.contains('\\') {
                        self.asset_modal = Some(AssetModal::Rename {
                            rel_path,
                            new_name,
                            error: Some("Name cannot contain slashes".to_string()),
                        });
                        return;
                    }
                    match rename_asset_rel(root, &rel_path, trimmed) {
                        Ok(renames) => {
                            self.apply_asset_renames(&renames);
                            if self.selected_asset.as_deref() == Some(&rel_path) {
                                let parent = Path::new(&rel_path).parent().unwrap_or(Path::new(""));
                                let updated_rel = if parent.as_os_str().is_empty() {
                                    trimmed.to_string()
                                } else {
                                    format!(
                                        "{}/{trimmed}",
                                        parent.to_string_lossy().replace('\\', "/")
                                    )
                                };
                                self.selected_asset = Some(updated_rel);
                            }
                            self.asset_modal = None;
                            self.log(format!("Renamed res://{rel_path} -> {trimmed}"));
                        }
                        Err(e) => {
                            self.asset_modal = Some(AssetModal::Rename {
                                rel_path,
                                new_name,
                                error: Some(format!("{e:#}")),
                            });
                        }
                    }
                }
                Action::Delete { rel_path, is_dir } => {
                    self.execute_asset_delete(&rel_path, is_dir);
                    self.asset_modal = None;
                }
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
