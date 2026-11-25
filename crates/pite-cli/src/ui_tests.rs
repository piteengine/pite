// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use crate::diagnostics::{Finding, Kind, Location, Severity};

fn card() -> Finding {
    Finding {
        severity: Severity::Error,
        kind: Kind::Asset,
        location: Location {
            file: Some("scenes/main.ptscn".to_string()),
            node: Some("sprite".to_string()),
        },
        message: Spans::text("Referenced asset is missing."),
        why: Spans::new()
            .plain("This property points to ")
            .strong("res://assets/gone.png")
            .plain(", which does not exist on disk."),
        fix: Spans::text("Add the asset at that res:// path or update the property."),
    }
}

#[test]
fn color_choice_resolution() {
    assert!(ColorChoice::Always.is_enabled(false));
    assert!(!ColorChoice::Never.is_enabled(true));
    assert!(!ColorChoice::Auto.is_enabled(false));
}

#[test]
fn json_disables_color_even_when_forced() {
    let ui = Ui::stdio(ColorChoice::Always, true, false, false);
    assert!(!ui.color_enabled());
    assert_eq!(ui.colors().reset, "");
}

#[test]
fn forced_color_works_without_a_terminal() {
    let ui = Ui::stdio(ColorChoice::Always, false, false, false);
    assert!(ui.color_enabled());
    assert_eq!(ui.colors().reset, "\u{1b}[0m");
}

#[test]
fn silent_context_renders_nothing_marked_up() {
    let ui = Ui::silent();
    assert!(!ui.color_enabled());
    assert!(!ui.json());
    assert_eq!(ui.colors().bold_red, "");
    assert_eq!(ui.width, 80);
}

#[test]
fn card_is_plain_without_color() {
    let rendered = format_card(&card(), &COLOR_DISABLED);
    assert!(rendered.starts_with("Error: Referenced asset is missing."));
    assert!(rendered.contains("\n  At:    scenes/main.ptscn › node \"sprite\""));
    assert!(rendered.contains("\n  Why:   This property points to res://assets/gone.png"));
    assert!(rendered.contains("\n  Fix:   "));
    assert!(!rendered.contains('\u{1b}'));
}

#[test]
fn card_labels_are_colored() {
    let rendered = format_card(&card(), &COLOR_ENABLED);
    assert!(rendered.starts_with("\u{1b}[1;31mError:\u{1b}[0m "));
    assert!(rendered.contains("\u{1b}[1mscenes/main.ptscn"));
    assert!(rendered.contains("\u{1b}[1;36mFix:\u{1b}[0m"));
}

#[test]
fn warning_card_uses_warning_label() {
    let mut finding = card();
    finding.severity = Severity::Warning;
    let rendered = format_card(&finding, &COLOR_ENABLED);
    assert!(rendered.starts_with("\u{1b}[1;33mWarning:\u{1b}[0m "));
}

#[test]
fn bar_aligns_right_edge() {
    let rendered = format_bar(
        "Checked demo",
        "2 scenes · 5 assets · 12ms",
        BarTone::Success,
        60,
        &COLOR_DISABLED,
    );
    assert_eq!(rendered.chars().count(), 60);
    assert!(rendered.starts_with(" Checked demo"));
    assert!(rendered.ends_with("2 scenes · 5 assets · 12ms"));
}

#[test]
fn bar_uses_background_and_black_text() {
    let rendered = format_bar(
        "Check failed",
        "1 error",
        BarTone::Error,
        40,
        &COLOR_ENABLED,
    );
    assert!(rendered.starts_with("\u{1b}[41m\u{1b}[1;30m Check failed"));
    assert!(rendered.ends_with("\u{1b}[0m"));
}

#[test]
fn faded_bar_has_no_background() {
    let rendered = format_bar(
        "No asset changes",
        "9 scanned",
        BarTone::Faded,
        40,
        &COLOR_ENABLED,
    );
    assert!(rendered.starts_with("\u{1b}[2m No asset changes"));
    assert!(!rendered.contains("\u{1b}[4"));
}

#[test]
fn bar_grows_instead_of_truncating() {
    let label = "A very long label that will not fit";
    let right = "and a long right side";
    let rendered = format_bar(label, right, BarTone::Faded, 40, &COLOR_DISABLED);
    assert!(rendered.contains(label));
    assert!(rendered.contains(right));
}

#[test]
fn progress_styles_verb_and_file_name() {
    let rendered = render_progress(
        "downloading player binary for windows (pite-player-v0.4.0-windows.zip)...",
        &COLOR_ENABLED,
    );
    assert!(rendered.starts_with("\u{1b}[1;36mDownloading\u{1b}[0m player binary"));
    assert!(rendered.contains("\u{1b}[1m(pite-player-v0.4.0-windows.zip)\u{1b}[0m"));
}

#[test]
fn table_aligns_columns() {
    let table = Table::new(&["platform", "status", "path"])
        .row(vec![
            Cell::plain("Linux"),
            Cell::success("Cached"),
            Cell::strong("/cache/linux"),
        ])
        .row(vec![
            Cell::plain("Windows"),
            Cell::error("Missing"),
            Cell::plain("cpython.zip"),
        ]);
    let rendered = format_table(&table, &COLOR_DISABLED);
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(
        lines[0].split_whitespace().collect::<Vec<_>>(),
        ["PLATFORM", "STATUS", "PATH"]
    );
    assert_eq!(
        lines[1].split_whitespace().collect::<Vec<_>>(),
        ["Linux", "Cached", "/cache/linux"]
    );
    assert_eq!(
        lines[2].split_whitespace().collect::<Vec<_>>(),
        ["Windows", "Missing", "cpython.zip"]
    );
}

#[test]
fn card_snapshot() {
    insta::assert_snapshot!("card_plain", format_card(&card(), &COLOR_DISABLED));
}

#[test]
fn table_snapshot() {
    let table = Table::new(&["platform", "status", "path"])
        .row(vec![
            Cell::plain("Linux"),
            Cell::success("Cached"),
            Cell::strong("/cache/linux"),
        ])
        .row(vec![
            Cell::plain("Windows"),
            Cell::warning("Missing"),
            Cell::plain("cpython-3.12.9.zip"),
        ])
        .row(vec![
            Cell::plain("macOS"),
            Cell::error("Mismatch"),
            Cell::plain("/cache/macos"),
        ]);
    insta::assert_snapshot!("table_plain", format_table(&table, &COLOR_DISABLED));
}

#[test]
fn bar_snapshot() {
    insta::assert_snapshot!(
        "bar_plain",
        format_bar(
            "Checked demo",
            "2 scenes · 5 assets · 12ms",
            BarTone::Success,
            72,
            &COLOR_DISABLED
        )
    );
}

/// Captures stderr writes so line handling can be asserted directly.
#[derive(Clone)]
struct Shared(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl Shared {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn capturing_ui(inline_capable: bool) -> (Ui, Shared) {
    let buffer = Shared(std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
    let ui = Ui {
        colors: COLOR_DISABLED,
        color_enabled: false,
        json: false,
        quiet: false,
        verbose: true,
        width: 60,
        out: Box::new(std::io::sink()),
        err: Box::new(buffer.clone()),
        inline_capable,
        inline_open: false,
        inline_width: 0,
        wrote_any: false,
        last_blank: false,
    };
    (ui, buffer)
}

#[test]
fn terminal_progress_rewrites_the_line_in_place() {
    let (mut ui, out) = capturing_ui(true);
    ui.start_progress("downloading thing...");
    ui.finish_progress("downloaded thing");
    assert_eq!(
        out.text(),
        "  Downloading thing...\r  Downloaded thing    \n"
    );
}

#[test]
fn piped_progress_keeps_both_lines() {
    let (mut ui, out) = capturing_ui(false);
    ui.start_progress("downloading thing...");
    ui.finish_progress("downloaded thing");
    assert_eq!(out.text(), "  Downloading thing...\n  Downloaded thing\n");
}

#[test]
fn blank_lines_never_lead_or_double() {
    let (mut ui, out) = capturing_ui(false);
    ui.blank();
    ui.line_err("checked");
    ui.blank();
    ui.blank();
    assert_eq!(out.text(), "checked\n\n");
}
