// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite check` payload types and human rendering.
//!
//! The data is built once during the check; the human output and the JSON
//! payload are two renderings of the same structs.

use serde::{Deserialize, Serialize};

use crate::diagnostics::Finding;
use crate::output::SCHEMA_VERSION;
use crate::ui::{BarTone, Colors};

/// Project-relative cache entry for one scene load.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheEntry {
    pub status: String,
    pub file: String,
}

impl CacheEntry {
    pub fn new(status: impl Into<String>, file: impl Into<String>) -> Self {
        Self {
            status: status.into(),
            file: file.into(),
        }
    }
}

/// One renamed asset and how many references were rewritten for it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenameSummary {
    pub old: String,
    pub new: String,
    pub references: usize,
}

/// Asset reimport summary, present only with `--reimport-assets`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReimportSummary {
    pub scanned: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renamed: Vec<RenameSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs_updated: Vec<String>,
}

impl ReimportSummary {
    /// Asset actions only: reference rewrites follow their rename.
    pub fn action_count(&self) -> usize {
        self.renamed.len() + self.added.len() + self.changed.len() + self.removed.len()
    }
}

/// Counters for one check run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckSummary {
    pub errors: usize,
    pub warnings: usize,
    pub scenes_checked: usize,
    pub assets_scanned: usize,
    pub elapsed_ms: u64,
}

/// Everything `pite check` reports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckData {
    pub schema_version: u32,
    pub ok: bool,
    pub summary: CheckSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reimport: Option<ReimportSummary>,
    pub diagnostics: Vec<Finding>,
    pub cache: Vec<CacheEntry>,
}

impl CheckData {
    pub fn new(
        ok: bool,
        summary: CheckSummary,
        reimport: Option<ReimportSummary>,
        diagnostics: Vec<Finding>,
        cache: Vec<CacheEntry>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            ok,
            summary,
            reimport,
            diagnostics,
            cache,
        }
    }
}

/// A summary bar: label left, stats right, tinted by the verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryBar {
    pub label: String,
    pub right: String,
    pub tone: BarTone,
}

/// `Reimporting assets:` plus one row per action.
///
/// The heading always prints so the block is legible even when nothing
/// changed; with no actions a dim `Nothing to change.` row follows it.
pub fn format_reimport(summary: &ReimportSummary, c: &Colors) -> String {
    let mut lines = vec![format!(
        "{cyan}Reimporting assets:{reset}",
        cyan = c.bold_cyan,
        reset = c.reset
    )];

    for rename in &summary.renamed {
        let references = match rename.references {
            0 => "no references updated".to_string(),
            1 => "1 reference updated".to_string(),
            count => format!("{count} references updated"),
        };
        lines.push(format!(
            "  {dim}{label:<10}{reset}{bold}{old}{reset} {dim}→{reset} {bold}{new}{reset} {faded}({references}){reset}",
            dim = c.dim,
            bold = c.bold,
            reset = c.reset,
            label = "Renamed:",
            old = rename.old,
            new = rename.new,
            faded = c.dim,
            references = references,
        ));
    }
    for path in &summary.added {
        lines.push(format!(
            "  {green}{label:<10}{reset}{bold}{path}{reset}",
            green = c.bold_green,
            bold = c.bold,
            reset = c.reset,
            label = "Added:",
            path = path,
        ));
    }
    for path in &summary.changed {
        lines.push(format!(
            "  {yellow}{label:<10}{reset}{bold}{path}{reset}",
            yellow = c.bold_yellow,
            bold = c.bold,
            reset = c.reset,
            label = "Update:",
            path = path,
        ));
    }
    for path in &summary.removed {
        lines.push(format!(
            "  {red}{label:<10}{reset}{bold}{path}{reset}",
            red = c.bold_red,
            bold = c.bold,
            reset = c.reset,
            label = "Removed:",
            path = path,
        ));
    }

    if summary.action_count() == 0 {
        lines.push(format!(
            "  {dim}Nothing to change.{reset}",
            dim = c.dim,
            reset = c.reset
        ));
    }

    lines.join("\n")
}

/// The reimport verdict bar; faded when no asset changed.
pub fn reimport_bar(summary: &ReimportSummary) -> SummaryBar {
    let changes = summary.action_count();
    SummaryBar {
        label: "Reimported assets".to_string(),
        right: if changes == 0 {
            format!("{} scanned · no changes", summary.scanned)
        } else {
            format!(
                "{} scanned · {changes} {}",
                summary.scanned,
                if changes == 1 { "change" } else { "changes" }
            )
        },
        tone: if changes == 0 {
            BarTone::Faded
        } else {
            BarTone::Success
        },
    }
}

/// Verbose scene-cache block: cyan heading, one row per scene load.
pub fn format_cache(entries: &[CacheEntry], c: &Colors) -> String {
    let mut lines = vec![format!(
        "{cyan}Checking scene caches:{reset}",
        cyan = c.bold_cyan,
        reset = c.reset
    )];
    for entry in entries {
        let (label, color) = match entry.status.as_str() {
            "hit" => ("Hit", c.dim),
            "miss" => ("Miss", c.bold_yellow),
            _ => ("Cached", c.dim),
        };
        lines.push(format!(
            "  {color}{label:<5}{reset} {faded}{file}{reset}",
            color = color,
            reset = c.reset,
            label = label,
            faded = c.dim,
            file = entry.file,
        ));
    }
    lines.join("\n")
}

/// Always-faded stats bar for the scene cache.
pub fn cache_bar(entries: &[CacheEntry]) -> SummaryBar {
    let hits = entries
        .iter()
        .filter(|entry| entry.status == "hit" || entry.status == "cached")
        .count();
    let misses = entries.len() - hits;
    SummaryBar {
        label: "Scene cache".to_string(),
        right: format!(
            "{} scene{} · {hits} hit{} · {misses} miss{}",
            entries.len(),
            if entries.len() == 1 { "" } else { "s" },
            if hits == 1 { "" } else { "s" },
            if misses == 1 { "" } else { "es" },
        ),
        tone: BarTone::Faded,
    }
}

/// Final verdict bar.
pub fn summary_bar(data: &CheckData, project: &str, elapsed: &str, strict: bool) -> SummaryBar {
    let summary = &data.summary;
    let errors = summary.errors;
    let warnings = summary.warnings;
    let stats = format!(
        "{errors} error{} · {warnings} warning{} · {elapsed}",
        if errors == 1 { "" } else { "s" },
        if warnings == 1 { "" } else { "s" },
    );

    if !data.ok {
        SummaryBar {
            label: if strict && errors == 0 {
                "Check failed (strict)".to_string()
            } else {
                "Check failed".to_string()
            },
            right: stats,
            tone: BarTone::Error,
        }
    } else if warnings > 0 {
        SummaryBar {
            label: "Check passed with warnings".to_string(),
            right: stats,
            tone: BarTone::Warning,
        }
    } else {
        SummaryBar {
            label: format!("Checked {project}"),
            right: format!(
                "{} scene{} · {} asset{} · {elapsed}",
                summary.scenes_checked,
                if summary.scenes_checked == 1 { "" } else { "s" },
                summary.assets_scanned,
                if summary.assets_scanned == 1 { "" } else { "s" },
            ),
            tone: BarTone::Success,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{COLOR_DISABLED, COLOR_ENABLED};

    fn summary() -> CheckSummary {
        CheckSummary {
            errors: 0,
            warnings: 0,
            scenes_checked: 2,
            assets_scanned: 5,
            elapsed_ms: 12,
        }
    }

    fn data(ok: bool, warnings: usize) -> CheckData {
        CheckData::new(
            ok,
            CheckSummary {
                warnings,
                ..summary()
            },
            None,
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn success_bar_uses_checked_label() {
        let bar = summary_bar(&data(true, 0), "demo", "12ms", false);
        assert_eq!(bar.label, "Checked demo");
        assert_eq!(bar.right, "2 scenes · 5 assets · 12ms");
        assert_eq!(bar.tone, BarTone::Success);
    }

    #[test]
    fn warning_bar_uses_warning_tone() {
        let bar = summary_bar(&data(true, 2), "demo", "12ms", false);
        assert_eq!(bar.label, "Check passed with warnings");
        assert_eq!(bar.right, "0 errors · 2 warnings · 12ms");
        assert_eq!(bar.tone, BarTone::Warning);
    }

    #[test]
    fn failed_bar_notes_strict_mode() {
        let bar = summary_bar(&data(false, 1), "demo", "12ms", true);
        assert_eq!(bar.label, "Check failed (strict)");
        assert_eq!(bar.tone, BarTone::Error);
    }

    #[test]
    fn reimport_bar_is_faded_without_changes() {
        let summary = ReimportSummary {
            scanned: 9,
            added: Vec::new(),
            renamed: Vec::new(),
            removed: Vec::new(),
            changed: Vec::new(),
            refs_updated: Vec::new(),
        };
        let bar = reimport_bar(&summary);
        assert_eq!(bar.right, "9 scanned · no changes");
        assert_eq!(bar.tone, BarTone::Faded);
    }

    #[test]
    fn reimport_rows_count_references_and_rename_update_label() {
        let summary = ReimportSummary {
            scanned: 9,
            added: vec!["assets/new.png".to_string()],
            renamed: vec![RenameSummary {
                old: "assets/old.png".to_string(),
                new: "assets/moved.png".to_string(),
                references: 3,
            }],
            removed: vec!["assets/gone.png".to_string()],
            changed: vec!["assets/edited.png".to_string()],
            refs_updated: vec!["scenes/main.ptscn".to_string()],
        };
        let rendered = format_reimport(&summary, &COLOR_DISABLED);
        assert!(rendered.starts_with("Reimporting assets:"), "{rendered}");
        assert!(rendered.contains("(3 references updated)"), "{rendered}");
        assert!(rendered.contains("Update:"), "{rendered}");
        assert!(rendered.contains("Added:"), "{rendered}");
        assert!(!rendered.contains("Changed:"), "{rendered}");
        assert!(!rendered.contains("Refs updated"), "{rendered}");
    }

    #[test]
    fn reimport_heading_is_cyan_and_paths_bold() {
        let summary = ReimportSummary {
            scanned: 1,
            added: Vec::new(),
            renamed: Vec::new(),
            removed: Vec::new(),
            changed: vec!["assets/x.png".to_string()],
            refs_updated: Vec::new(),
        };
        let rendered = format_reimport(&summary, &COLOR_ENABLED);
        assert!(rendered.starts_with("\u{1b}[1;36mReimporting assets:\u{1b}[0m"));
        assert!(rendered.contains("\u{1b}[1massets/x.png\u{1b}[0m"));
    }

    #[test]
    fn reimport_rows_say_nothing_to_change_when_idle() {
        let summary = ReimportSummary {
            scanned: 9,
            added: Vec::new(),
            renamed: Vec::new(),
            removed: Vec::new(),
            changed: Vec::new(),
            refs_updated: Vec::new(),
        };
        let rendered = format_reimport(&summary, &COLOR_DISABLED);
        assert_eq!(rendered, "Reimporting assets:\n  Nothing to change.");
    }

    #[test]
    fn cache_rows_fade_hits_and_paths() {
        let entries = vec![
            CacheEntry::new("hit", "scenes/main.ptscn"),
            CacheEntry::new("miss", "scenes/level2.ptscn"),
        ];
        let rendered = format_cache(&entries, &COLOR_ENABLED);
        assert!(rendered.starts_with("\u{1b}[1;36mChecking scene caches:\u{1b}[0m"));
        assert!(rendered.contains("\u{1b}[2mHit  \u{1b}[0m \u{1b}[2mscenes/main.ptscn\u{1b}[0m"));
        assert!(bar_counts(&entries).right.contains("1 hit"));
    }

    fn bar_counts(entries: &[CacheEntry]) -> SummaryBar {
        cache_bar(entries)
    }

    #[test]
    fn cache_bar_is_always_faded() {
        let entries = vec![CacheEntry::new("hit", "a.ptscn")];
        let bar = cache_bar(&entries);
        assert_eq!(bar.tone, BarTone::Faded);
        assert_eq!(bar.right, "1 scene · 1 hit · 0 misses");
    }

    #[test]
    fn summary_snapshot() {
        let bar = summary_bar(&data(true, 0), "demo", "12ms", false);
        insta::assert_snapshot!("summary_success", format!("{} {}", bar.label, bar.right));
    }

    #[test]
    fn failed_summary_snapshot() {
        let bar = summary_bar(&data(false, 2), "demo", "1.20s", true);
        insta::assert_snapshot!(
            "summary_failed_strict",
            format!("{} {}", bar.label, bar.right)
        );
    }

    #[test]
    fn reimport_snapshot() {
        let summary = ReimportSummary {
            scanned: 9,
            added: vec!["assets/new.png".to_string()],
            renamed: vec![RenameSummary {
                old: "assets/old.png".to_string(),
                new: "assets/moved.png".to_string(),
                references: 2,
            }],
            removed: vec!["assets/gone.png".to_string()],
            changed: vec!["assets/edited.png".to_string()],
            refs_updated: vec!["scenes/main.ptscn".to_string()],
        };
        insta::assert_snapshot!(
            "reimport_actions",
            format_reimport(&summary, &COLOR_DISABLED)
        );
    }
}
