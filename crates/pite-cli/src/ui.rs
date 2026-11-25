// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared terminal output for every `pite` command.
//!
//! Commands never write escape sequences or choose streams themselves: they
//! receive a [`Ui`] built once in `main`, which carries the color policy, the
//! `--json`/`--quiet`/`--verbose` state, the terminal width, and the two output
//! streams.
//!
//! Human-facing lines (progress, summaries, cards, tables) go to **stderr**;
//! stdout is reserved for requested data and JSON payloads. `--json` therefore
//! never mixes ANSI output into a machine-readable stream.
//!
//! Messages are built from [`crate::text::Spans`]: plain prose plus the pieces
//! that should stand out (paths bold, identifiers cyan, secondary context dim).
//! Styling lives beside the text, so the same message renders to ANSI on a
//! terminal and to plain text in JSON.

use std::io::{IsTerminal, Write};

use clap::ValueEnum;
use terminal_size::{terminal_size, Width};

use crate::diagnostics::Finding;
use crate::text::{capitalize, Spans};

/// Fallback width when no terminal is attached (pipes, CI, tests).
const FALLBACK_WIDTH: usize = 80;
const MIN_WIDTH: usize = 40;
const MAX_WIDTH: usize = 200;

/// `--color <auto|always|never>` policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    /// Resolve the policy against terminal detection and `NO_COLOR`.
    pub fn is_enabled(self, is_terminal: bool) -> bool {
        match self {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => is_terminal && std::env::var_os("NO_COLOR").is_none(),
        }
    }
}

/// The escape sequences a rendered line may use.
pub struct Colors {
    pub bold_red: &'static str,
    pub bold_yellow: &'static str,
    pub bold_green: &'static str,
    pub bold_cyan: &'static str,
    pub bold: &'static str,
    pub dim: &'static str,
    pub reset: &'static str,
    /// Bold black, for text on a bright status background.
    pub on_bar: &'static str,
    pub bg_green: &'static str,
    pub bg_yellow: &'static str,
    pub bg_red: &'static str,
}

pub const COLOR_ENABLED: Colors = Colors {
    bold_red: "\x1b[1;31m",
    bold_yellow: "\x1b[1;33m",
    bold_green: "\x1b[1;32m",
    bold_cyan: "\x1b[1;36m",
    bold: "\x1b[1m",
    dim: "\x1b[2m",
    reset: "\x1b[0m",
    on_bar: "\x1b[1;30m",
    bg_green: "\x1b[42m",
    bg_yellow: "\x1b[43m",
    bg_red: "\x1b[41m",
};

pub const COLOR_DISABLED: Colors = Colors {
    bold_red: "",
    bold_yellow: "",
    bold_green: "",
    bold_cyan: "",
    bold: "",
    dim: "",
    reset: "",
    on_bar: "",
    bg_green: "",
    bg_yellow: "",
    bg_red: "",
};

/// How a table cell is tinted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Strong,
    /// Cyan, for identifiers such as package names and flags.
    Info,
    Success,
    Warning,
    Error,
}

impl Tone {
    fn color(self, c: &Colors) -> &'static str {
        match self {
            Tone::Plain => "",
            Tone::Strong => c.bold,
            Tone::Info => c.bold_cyan,
            Tone::Success => c.bold_green,
            Tone::Warning => c.bold_yellow,
            Tone::Error => c.bold_red,
        }
    }
}

/// One table cell with its tint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub tone: Tone,
}

impl Cell {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Plain,
        }
    }

    /// Bold, for paths and identifiers.
    pub fn strong(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Strong,
        }
    }

    /// Cyan, for identifiers such as package names.
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Info,
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Success,
        }
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Warning,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Error,
        }
    }
}

/// A simple auto-width table. Widths come from the longest cell per column;
/// nothing is truncated, so long paths widen the table instead of lying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

impl Table {
    pub fn new(headers: &[&str]) -> Self {
        Self {
            headers: headers.iter().map(|h| h.to_uppercase()).collect(),
            rows: Vec::new(),
        }
    }

    pub fn row(mut self, cells: Vec<Cell>) -> Self {
        self.rows.push(cells);
        self
    }
}

/// Color of a full-width summary bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarTone {
    Success,
    Warning,
    Error,
    /// No background; dim text for neutral or unchanged outcomes.
    Faded,
}

/// Rendering context shared by every command.
pub struct Ui {
    colors: Colors,
    color_enabled: bool,
    json: bool,
    quiet: bool,
    verbose: bool,
    width: usize,
    out: Box<dyn Write>,
    err: Box<dyn Write>,
    /// Stderr is a terminal, so an in-progress line can be rewritten in place.
    inline_capable: bool,
    /// An in-progress line is on screen without its trailing newline yet.
    inline_open: bool,
    /// Visible width of the in-progress line, so its replacement can pad.
    inline_width: usize,
    /// Something was already written, so a leading blank line is skipped.
    wrote_any: bool,
    /// The last write was a blank line; blank lines never double up.
    last_blank: bool,
}

impl Ui {
    /// Build the process-wide context from the global flags.
    pub fn stdio(choice: ColorChoice, json: bool, quiet: bool, verbose: bool) -> Self {
        let is_term = std::io::stdout().is_terminal() || std::io::stderr().is_terminal();
        let color_enabled = choice.is_enabled(is_term) && !json;
        Self {
            colors: if color_enabled {
                COLOR_ENABLED
            } else {
                COLOR_DISABLED
            },
            color_enabled,
            json,
            quiet,
            verbose,
            width: detect_width(),
            out: Box::new(std::io::stdout()),
            err: Box::new(std::io::stderr()),
            inline_capable: std::io::stderr().is_terminal(),
            inline_open: false,
            inline_width: 0,
            wrote_any: false,
            last_blank: false,
        }
    }

    /// Silent context for tests and internal call sites that do not render.
    #[cfg(test)]
    pub fn silent() -> Self {
        Self {
            colors: COLOR_DISABLED,
            color_enabled: false,
            json: false,
            quiet: true,
            verbose: false,
            width: FALLBACK_WIDTH,
            out: Box::new(std::io::sink()),
            err: Box::new(std::io::sink()),
            inline_capable: false,
            inline_open: false,
            inline_width: 0,
            wrote_any: false,
            last_blank: false,
        }
    }

    pub fn colors(&self) -> &Colors {
        &self.colors
    }

    pub fn color_enabled(&self) -> bool {
        self.color_enabled
    }

    pub fn json(&self) -> bool {
        self.json
    }

    pub fn quiet(&self) -> bool {
        self.quiet
    }

    pub fn verbose(&self) -> bool {
        self.verbose
    }

    /// Raw write to stdout (JSON payloads only).
    pub fn write_out(&mut self, text: &str) {
        let _ = write!(self.out, "{text}");
    }

    /// Close an in-progress line before a different kind of output starts.
    fn end_inline(&mut self) {
        if self.inline_open {
            let _ = writeln!(self.err);
            self.inline_open = false;
            self.wrote_any = true;
            self.last_blank = false;
        }
    }

    pub fn line_err(&mut self, text: &str) {
        self.end_inline();
        let _ = writeln!(self.err, "{text}");
        self.wrote_any = true;
        self.last_blank = false;
    }

    /// One blank line, never leading and never doubled.
    pub fn blank(&mut self) {
        self.end_inline();
        if !self.wrote_any || self.last_blank {
            return;
        }
        let _ = writeln!(self.err);
        self.last_blank = true;
    }

    /// `Success: …` — suppressed by `--quiet`.
    pub fn success(&mut self, message: impl Into<Spans>) {
        if self.quiet {
            return;
        }
        let c = &self.colors;
        let message = message.into();
        self.line_err(&format!(
            "{green}Success:{reset} {message}",
            green = c.bold_green,
            reset = c.reset,
            message = message.render(c)
        ));
    }

    /// `Error: …` — always printed.
    pub fn error(&mut self, message: impl Into<Spans>) {
        let c = &self.colors;
        let message = message.into();
        self.line_err(&format!(
            "{red}Error:{reset} {message}",
            red = c.bold_red,
            reset = c.reset,
            message = message.render(c)
        ));
    }

    /// Plain status line, no label — suppressed by `--quiet`.
    pub fn info(&mut self, message: impl Into<Spans>) {
        if self.quiet {
            return;
        }
        let rendered = message.into().render(&self.colors);
        self.line_err(&rendered);
    }

    /// `  Label:  value` — suppressed by `--quiet`.
    pub fn detail(&mut self, label: &str, value: impl Into<Spans>) {
        if self.quiet {
            return;
        }
        let c = &self.colors;
        let value = value.into();
        self.line_err(&format!(
            "  {dim}{label:<7}{reset} {value}",
            dim = c.dim,
            reset = c.reset,
            value = value.render(c)
        ));
    }

    /// A cyan section title with no leading blank line.
    pub fn heading(&mut self, title: &str) {
        if self.quiet {
            return;
        }
        let c = &self.colors;
        self.line_err(&format!(
            "{cyan}{title}{reset}",
            cyan = c.bold_cyan,
            reset = c.reset
        ));
    }

    /// A cyan section title preceded by a blank line.
    pub fn section(&mut self, title: &str) {
        if self.quiet {
            return;
        }
        self.blank();
        self.heading(title);
    }

    /// Dim secondary detail; only prints when `--verbose` is on.
    pub fn verbose_line(&mut self, message: &str) {
        if !self.verbose || self.quiet {
            return;
        }
        let c = &self.colors;
        self.line_err(&format!(
            "{dim}{message}{reset}",
            dim = c.dim,
            reset = c.reset
        ));
    }

    /// Full-weight detail behind `--verbose`, with its own styles.
    pub fn verbose_note(&mut self, message: impl Into<Spans>) {
        if !self.verbose || self.quiet {
            return;
        }
        let rendered = message.into().render(&self.colors);
        self.line_err(&rendered);
    }

    /// Render one diagnostic card followed by a blank line.
    pub fn card(&mut self, finding: &Finding) {
        let rendered = format_card(finding, &self.colors);
        self.line_err(&rendered);
        self.blank();
    }

    /// Render a table to stderr, indented under its section heading.
    pub fn table(&mut self, table: &Table) {
        if self.quiet {
            return;
        }
        let rendered = format_table(table, &self.colors);
        self.line_err(&indent(&rendered));
    }

    /// Full-width summary bar: label on the left, stats on the right.
    pub fn bar(&mut self, label: &str, right: &str, tone: BarTone) {
        let rendered = format_bar(label, right, tone, self.width, &self.colors);
        self.line_err(&rendered);
    }

    /// One indented progress line (section content).
    fn progress_line(&mut self, message: &str) {
        let rendered = render_progress(message, &self.colors);
        self.line_err(&format!("  {rendered}"));
    }

    /// Start an indented line that [`Self::finish_progress`] rewrites in place
    /// on a terminal, and that stays a normal line when stderr is piped.
    fn start_progress(&mut self, message: &str) {
        let rendered = render_progress(message, &self.colors);
        if !self.inline_capable {
            self.line_err(&format!("  {rendered}"));
            return;
        }
        self.end_inline();
        let _ = write!(self.err, "  {rendered}");
        let _ = self.err.flush();
        self.inline_open = true;
        self.inline_width = 2 + message.chars().count();
        self.wrote_any = true;
        self.last_blank = false;
    }

    /// Replace the in-progress line with a green completion line.
    fn finish_progress(&mut self, message: &str) {
        let c = &self.colors;
        let width = 2 + message.chars().count();
        let padding = " ".repeat(self.inline_width.saturating_sub(width));
        let completed = format!(
            "  {green}{}{reset}{padding}",
            capitalize(message),
            green = c.bold_green,
            reset = c.reset,
        );
        if self.inline_open {
            let _ = write!(self.err, "\r{completed}\n");
            let _ = self.err.flush();
            self.inline_open = false;
            self.wrote_any = true;
            self.last_blank = false;
        } else {
            self.line_err(&completed);
        }
    }
}

impl pite_export::Progress for Ui {
    fn info(&mut self, message: &str) {
        if !self.quiet {
            self.progress_line(message);
        }
    }

    fn step(&mut self, message: &str) {
        if !self.quiet {
            self.progress_line(message);
        }
    }

    fn success(&mut self, message: &str) {
        if !self.quiet {
            self.progress_line(message);
        }
    }

    fn warn(&mut self, message: &str) {
        let c = &self.colors;
        let rendered = format!(
            "  {yellow}Warning:{reset} {message}",
            yellow = c.bold_yellow,
            reset = c.reset
        );
        self.line_err(&rendered);
    }

    fn section(&mut self, title: &str) {
        Ui::section(self, title);
    }

    fn pending(&mut self, message: &str) {
        if !self.quiet {
            self.start_progress(message);
        }
    }

    fn complete(&mut self, message: &str) {
        if !self.quiet {
            self.finish_progress(message);
        }
    }

    fn detail(&mut self, message: &str) {
        if self.verbose && !self.quiet {
            let c = &self.colors;
            self.line_err(&format!(
                "  {dim}{message}{reset}",
                dim = c.dim,
                reset = c.reset
            ));
        }
    }
}

/// Prefix every line of `text` with two spaces, for section content.
fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Terminal width, clamped so bars stay readable and testable.
fn detect_width() -> usize {
    terminal_size()
        .map(|(Width(width), _)| width as usize)
        .unwrap_or(FALLBACK_WIDTH)
        .clamp(MIN_WIDTH, MAX_WIDTH)
}

/// Render a diagnostic card:
///
/// ```text
/// Error: Referenced asset is missing.
///   At:    scenes/main.ptscn › node "sprite"
///   Why:   This property points to "res://gone.png", which does not exist on disk.
///   Fix:   Add the asset at that res:// path or update the property.
/// ```
pub fn format_card(finding: &Finding, c: &Colors) -> String {
    let (label, label_color) = match finding.severity {
        crate::diagnostics::Severity::Error => ("Error:", c.bold_red),
        crate::diagnostics::Severity::Warning => ("Warning:", c.bold_yellow),
    };

    let mut out = format!(
        "{label_color}{label}{reset} {message}",
        reset = c.reset,
        message = finding.message.render(c)
    );

    if let Some(location) = finding.location.render() {
        out.push_str(&format!(
            "\n  {dim}At:{reset}    {bold}{location}{reset}",
            dim = c.dim,
            reset = c.reset,
            bold = c.bold
        ));
    }
    out.push_str(&format!(
        "\n  {dim}Why:{reset}   {why}",
        dim = c.dim,
        reset = c.reset,
        why = finding.why.render(c)
    ));
    out.push_str(&format!(
        "\n  {cyan}Fix:{reset}   {fix}",
        cyan = c.bold_cyan,
        reset = c.reset,
        fix = finding.fix.render(c)
    ));
    out
}

/// Full-width summary bar. Content longer than `width` grows the line instead
/// of truncating it.
pub fn format_bar(label: &str, right: &str, tone: BarTone, width: usize, c: &Colors) -> String {
    let left_len = label.chars().count();
    let right_len = right.chars().count();
    let width = width.max(left_len + right_len + 2);
    let gap = width - 1 - left_len - right_len;
    let fill = " ".repeat(gap);
    match tone {
        BarTone::Faded => format!(
            "{dim} {label}{fill}{right}{reset}",
            dim = c.dim,
            reset = c.reset
        ),
        BarTone::Success | BarTone::Warning | BarTone::Error => {
            let (bg, fg) = match tone {
                BarTone::Success => (c.bg_green, c.on_bar),
                BarTone::Warning => (c.bg_yellow, c.on_bar),
                BarTone::Error => (c.bg_red, c.on_bar),
                BarTone::Faded => unreachable!(),
            };
            format!(
                "{bg}{fg} {label}{fill}{right}{reset}",
                bg = bg,
                fg = fg,
                reset = c.reset
            )
        }
    }
}

/// Style one progress message: capitalized verb in cyan, file names bold.
pub fn render_progress(message: &str, c: &Colors) -> String {
    let capitalized = capitalize(message);
    let (verb, rest) = match capitalized.find(' ') {
        Some(index) => capitalized.split_at(index),
        None => (capitalized.as_str(), ""),
    };
    format!(
        "{cyan}{verb}{reset}{rest}",
        cyan = c.bold_cyan,
        reset = c.reset,
        rest = bold_parenthesized_paths(rest, c)
    )
}

/// Bold `(file.ext)` / `(dir/file)` groups inside progress detail text.
fn bold_parenthesized_paths(text: &str, c: &Colors) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let Some(close_rel) = rest[open..].find(')') else {
            break;
        };
        let close = open + close_rel;
        let inner = &rest[open + 1..close];
        let looks_like_file = inner.contains('.') || inner.contains('/') || inner.contains('\\');
        out.push_str(&rest[..open]);
        if looks_like_file {
            out.push_str(&format!(
                "{bold}({inner}){reset}",
                bold = c.bold,
                reset = c.reset
            ));
        } else {
            out.push('(');
            out.push_str(inner);
            out.push(')');
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Render an auto-width table with a dim, uppercase header row.
pub fn format_table(table: &Table, c: &Colors) -> String {
    let columns = table
        .headers
        .len()
        .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
    let mut widths = vec![0usize; columns];
    for (index, header) in table.headers.iter().enumerate() {
        widths[index] = header.chars().count();
    }
    for row in &table.rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.text.chars().count());
        }
    }

    let mut out = String::new();
    let header_cells: Vec<String> = table
        .headers
        .iter()
        .enumerate()
        .map(|(index, header)| pad(header, widths[index]))
        .collect();
    out.push_str(&format!(
        "{dim}{}{reset}",
        header_cells.join("  ").trim_end(),
        dim = c.dim,
        reset = c.reset
    ));

    for row in &table.rows {
        out.push('\n');
        let mut rendered: Vec<String> = Vec::with_capacity(columns);
        for (index, cell) in row.iter().enumerate() {
            let padded = pad(&cell.text, widths[index]);
            let color = cell.tone.color(c);
            if color.is_empty() {
                rendered.push(padded);
            } else {
                let trimmed = padded.trim_end();
                let spaces = padded.chars().count() - trimmed.chars().count();
                rendered.push(format!(
                    "{color}{trimmed}{reset}{}",
                    " ".repeat(spaces),
                    reset = c.reset
                ));
            }
        }
        out.push_str(rendered.join("  ").trim_end());
    }
    out
}

fn pad(text: &str, width: usize) -> String {
    let missing = width.saturating_sub(text.chars().count());
    let mut out = String::with_capacity(text.len() + missing);
    out.push_str(text);
    out.push_str(&" ".repeat(missing));
    out
}

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;
