// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::{HashMap, HashSet};
use std::path::Path;

use eframe::egui;

use crate::lsp::{self, LspOutcome};
use crate::ops;

use super::EditorApp;

impl EditorApp {
    pub(super) fn current_file_errors(&self) -> Vec<String> {
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

    pub(super) fn save_code(&mut self) {
        let Some(file) = self.code_file.clone() else {
            self.log("save: no script open.".to_string());
            return;
        };
        match std::fs::write(&file, &self.code_text) {
            Ok(()) => {
                if self.code_is_python() {
                    self.session.reload_script_path(&file);
                    self.viewport_dirty = true;
                }
                self.log(format!("saved {}", file.display()));
            }
            Err(e) => self.log(format!("save failed: {e:#}")),
        }
    }

    pub(super) fn show_code(&mut self, ui: &mut egui::Ui) {
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
                    let edit_id = egui::Id::unique("code-editor");
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

    pub(super) fn code_uri(&self) -> Option<String> {
        self.code_file
            .as_ref()
            .map(|p| lsp::path_to_uri(p.as_path()))
    }

    /// The open file is a Python script: the only kind the LSP serves.
    pub(super) fn code_is_python(&self) -> bool {
        self.code_file
            .as_ref()
            .and_then(|f| f.extension())
            .and_then(|e| e.to_str())
            == Some("py")
    }

    /// Plain-text files the code editor opens (same set the asset
    /// browser shows with a gear icon); LSP stays Python-only.
    pub(super) fn editor_text_file(file: &Path) -> bool {
        matches!(
            file.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("py" | "toml" | "cfg" | "ini" | "json" | "yaml" | "yml")
        )
    }

    pub(super) fn code_cursor_byte(
        text: &str,
        ctx: &egui::Context,
        id: &egui::Id,
    ) -> Option<usize> {
        let state = egui::widgets::text_edit::TextEditState::load(ctx, *id)?;
        let range = state.cursor.char_range()?;
        Some(lsp::byte_offset_of_char(text, range.primary.index.0))
    }

    /// Move the code editor's cursor to the start of 1-based `line` and focus
    /// it. Callers scroll the outer area to the line themselves: the TextEdit
    /// only scrolls on text/selection changes, never on a stored cursor.
    pub(super) fn jump_to_line(&mut self, ctx: &egui::Context, line: usize) {
        let edit_id = egui::Id::unique("code-editor");
        let byte = lsp::line_start_byte(&self.code_text, line);
        let index = self.code_text[..byte].chars().count();
        Self::store_code_cursor(ctx, index);
        ctx.memory_mut(|memory| memory.request_focus(edit_id));
    }

    pub(super) fn store_code_cursor(ctx: &egui::Context, char_index: usize) {
        let edit_id = egui::Id::unique("code-editor");
        if let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ctx, edit_id) {
            let cursor = egui::text::CCursor::new(char_index);
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
            state.store(ctx, edit_id);
        }
    }

    pub(super) fn ensure_lsp(&mut self) {
        if self.lsp.is_some() || self.lsp_starter.is_some() {
            return;
        }
        let Some(script) = self.code_file.clone() else {
            return;
        };
        self.lsp_status = "lsp: starting…".to_string();
        self.lsp_starter = Some(lsp::spawn_lsp(self.project_dir.clone(), &script));
    }

    pub(super) fn poll_lsp(&mut self) {
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

    pub(super) fn lsp_marks(&self) -> Vec<(usize, String)> {
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

    pub(super) fn code_changed(&mut self, paren_at: Option<usize>) {
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

    pub(super) fn request_completion_at(&mut self, cursor_byte: Option<usize>) {
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
    pub(super) fn close_completion(&mut self) {
        self.lsp_completion_open = false;
        self.lsp_completion_pending.clear();
        self.lsp_completion_selected = 0;
    }

    /// Indices into `lsp_completions` matching the text typed since the
    /// trigger offset (case-insensitive prefix): optimistic narrowing
    /// while a server refresh is still in flight.
    pub(super) fn completion_filtered(&self, cursor_byte: usize) -> Vec<usize> {
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
    pub(super) fn accept_completion(&mut self, ctx: &egui::Context, cursor_byte: Option<usize>) {
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

    pub(super) fn request_hover_at(&mut self, cursor_byte: Option<usize>) {
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

    pub(super) fn show_completion_popup(
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
}
