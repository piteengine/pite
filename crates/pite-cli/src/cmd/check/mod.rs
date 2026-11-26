// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite check`: validate manifests, scenes, assets, audio, and scripts.
//!
//! The check builds [`CheckData`] as it runs; human text and the JSON payload
//! are both rendered from that one struct.

pub mod report;
#[cfg(test)]
mod tests;
mod walk;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use anyhow::Result;

use crate::diagnostics::{format_path_rel, Finding, Severity};
use crate::output::Outcome;
use crate::text::Spans;
use crate::ui::Ui;

use report::{
    cache_bar, format_cache, format_reimport, reimport_bar, summary_bar, CacheEntry, CheckData,
    CheckSummary, ReimportSummary, RenameSummary, SummaryBar,
};
use walk::Walker;

/// Run the check and return the structured payload plus the exit status.
pub fn run(
    ui: &mut Ui,
    path: Option<&str>,
    reimport_assets: bool,
    strict: bool,
) -> Result<Outcome> {
    let started = Instant::now();
    let start = crate::resolve_start_dir(path)?;

    let mut findings: Vec<Finding> = Vec::new();
    let mut reimport_summary: Option<ReimportSummary> = None;
    let mut cache: Vec<CacheEntry> = Vec::new();
    let mut scenes_checked = 0;
    let mut assets_scanned = 0;

    let root = pite_project::find_project_root(&start);
    if root.is_none() {
        findings.push(Finding::missing_project_root(&start));
    }

    if let Some(root) = &root {
        let has_errors = findings.iter().any(|f| f.severity == Severity::Error);

        if reimport_assets && !has_errors {
            let (summary, failures) = reimport(root)?;
            if !ui.quiet() {
                ui.line_err(&format_reimport(&summary, ui.colors()));
                ui.blank();
            }
            for asset in failures {
                findings.push(Finding::reimport_failed(&format_path_rel(root, &asset)));
            }
            reimport_summary = Some(summary);
        }

        let manifest = if findings.iter().any(|f| f.severity == Severity::Error) {
            None
        } else {
            match pite_project::load_manifest(root) {
                Ok(manifest) => {
                    let (errors, warnings) = pite_project::validate_manifest_structured(&manifest);
                    for issue in errors {
                        findings.push(Finding::manifest_issue(Severity::Error, &issue));
                    }
                    for issue in warnings {
                        findings.push(Finding::manifest_issue(Severity::Warning, &issue));
                    }
                    if manifest.project.icon.starts_with("res://") {
                        let missing = pite_core::resolve_res(root, &manifest.project.icon)
                            .map(|p| !p.is_file())
                            .unwrap_or(true);
                        if missing {
                            findings.push(Finding::icon_missing(&manifest.project.icon));
                        }
                    }
                    Some(manifest)
                }
                Err(error) => {
                    findings.push(Finding::manifest_load_failed(&error));
                    None
                }
            }
        };

        if let Some(manifest) = &manifest {
            let scanned = match pite_assets::scan_files(root) {
                Ok(files) => {
                    assets_scanned = files.len();
                    files
                }
                Err(error) => {
                    findings.push(Finding::asset_scan_failed(&format!("{error:#}")));
                    Vec::new()
                }
            };
            let by_path: HashMap<&str, &pite_assets::ScannedFile> = scanned
                .iter()
                .map(|file| (file.res_path.as_str(), file))
                .collect();
            let by_hash: HashMap<&str, &str> = scanned
                .iter()
                .map(|file| (file.hash.as_str(), file.res_path.as_str()))
                .collect();
            let manifest_path = root.join(pite_assets::MANIFEST_FILE);
            let manifest_exists = manifest_path.is_file();
            let registry = pite_assets::load_registry(root).unwrap_or_default();

            if !reimport_assets {
                if !manifest_exists && !scanned.is_empty() {
                    findings.push(Finding::assets_manifest_missing());
                } else if manifest_exists {
                    let (_, scan_report) = pite_assets::reconcile(&registry, &scanned);
                    if !scan_report.added.is_empty()
                        || !scan_report.removed.is_empty()
                        || !scan_report.renamed.is_empty()
                        || !scan_report.changed.is_empty()
                    {
                        findings.push(Finding::assets_manifest_stale());
                    }
                }
            }

            let loaded_plugins = pite_project::load_enabled_plugins(root, manifest);
            for err in &loaded_plugins.errors {
                let id = match err {
                    pite_project::PluginLoadError::InvalidId { id }
                    | pite_project::PluginLoadError::ManifestIo { id, .. }
                    | pite_project::PluginLoadError::ManifestParse { id, .. }
                    | pite_project::PluginLoadError::IdMismatch { id, .. }
                    | pite_project::PluginLoadError::IncompatibleEngineVersion { id, .. }
                    | pite_project::PluginLoadError::InvalidNodeProp { id, .. }
                    | pite_project::PluginLoadError::Schema { id, .. }
                    | pite_project::PluginLoadError::DuplicateAutoload { id, .. }
                    | pite_project::PluginLoadError::DuplicateCustomProp { id, .. } => id.as_str(),
                };
                findings.push(Finding::plugin_load_error(id, &err.to_string()));
            }
            let vendor_dir = root.join("vendor");
            let mut plugin_ids = HashSet::new();
            for plug in &loaded_plugins.plugins {
                plugin_ids.insert(plug.id.clone());
                for (pkg, spec) in &plug.manifest.dependencies {
                    if !crate::cmd::vendor::vendor_provides(&vendor_dir, pkg) {
                        findings.push(Finding::plugin_missing_dependency(&plug.id, pkg, spec));
                    }
                }
            }

            let mut walker = Walker::new(root, &loaded_plugins.custom_props, &registry, &by_hash)
                .with_plugins(loaded_plugins.schemas, plugin_ids);
            if manifest.project.icon.starts_with("res://") {
                walker.referenced.insert(manifest.project.icon.clone());
            }
            // Mark any res:// references inside enabled plugin directories as referenced.
            for plug in &loaded_plugins.plugins {
                collect_plugin_asset_refs(root, &plug.dir, &mut walker.referenced);
            }
            walker.walk(&manifest.project.main_scene);
            scenes_checked = walker.visited_len();
            cache = std::mem::take(&mut walker.cache);

            let (atlas_errors, atlas_warnings) =
                pite_assets::atlas_report_structured(root, &walker.atlas_uses, &walker.anim_uses);
            for issue in atlas_errors {
                findings.push(Finding::atlas_issue(root, Severity::Error, &issue));
            }
            for issue in atlas_warnings {
                findings.push(Finding::atlas_issue(root, Severity::Warning, &issue));
            }
            for legacy in pite_assets::find_legacy_atlases(root) {
                findings.push(Finding::legacy_atlas_sidecar(&legacy));
            }

            // Sheets are referenced through their sidecar, never directly: mark
            // each used sidecar's sheet so packed PNGs don't read as orphans.
            let mut referenced = walker.referenced;
            let mut sheets = Vec::new();
            for reference in &referenced {
                if reference.ends_with(".ptatlas") || reference.ends_with(".atlas.json") {
                    if let Ok(sheet) = pite_assets::sheet_path(root, reference) {
                        if let Ok(rel) = sheet.strip_prefix(root) {
                            let joined = rel
                                .components()
                                .map(|component| {
                                    component.as_os_str().to_string_lossy().into_owned()
                                })
                                .collect::<Vec<_>>()
                                .join("/");
                            sheets.push(format!("res://{joined}"));
                        }
                    }
                }
            }
            referenced.extend(sheets);
            findings.extend(orphan_findings(&by_path, &referenced));
            findings.extend(walker.findings);
        }
    }

    let elapsed = started.elapsed();
    let elapsed_str = format_elapsed(elapsed);
    let ordered = order_findings(findings);
    let errors = ordered
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
        .count();
    let warnings = ordered.len() - errors;
    let failed = errors > 0 || (strict && warnings > 0);
    let data = CheckData::new(
        !failed,
        CheckSummary {
            errors,
            warnings,
            scenes_checked,
            assets_scanned,
            elapsed_ms: elapsed.as_millis() as u64,
        },
        reimport_summary,
        ordered,
        cache,
    );

    let project_name = root
        .as_ref()
        .and_then(|root| root.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_string();
    render(ui, &data, &project_name, &elapsed_str, strict);

    if failed {
        Outcome::failed(data)
    } else {
        Outcome::completed(data)
    }
}

fn reimport(root: &Path) -> Result<(ReimportSummary, Vec<String>)> {
    let old = pite_assets::load_registry(root).unwrap_or_default();
    let files = pite_assets::scan_files(root).unwrap_or_default();
    let (_next, scan) = pite_assets::reconcile(&old, &files);
    let (_registry, report) = pite_assets::reimport_project(root)?;
    let renamed = report
        .rewrites
        .iter()
        .map(|rewrite| RenameSummary {
            old: rewrite.old.clone(),
            new: rewrite.new.clone(),
            references: rewrite.references,
        })
        .collect();
    let summary = ReimportSummary {
        scanned: scan.scanned,
        added: scan.added,
        renamed,
        removed: scan.removed,
        changed: scan.changed,
        refs_updated: report.refs_updated,
    };
    Ok((summary, report.failed))
}

fn order_findings(findings: Vec<Finding>) -> Vec<Finding> {
    let (errors, warnings): (Vec<_>, Vec<_>) = findings
        .into_iter()
        .partition(|finding| finding.severity == Severity::Error);
    errors.into_iter().chain(warnings).collect()
}

fn format_elapsed(elapsed: std::time::Duration) -> String {
    let millis = elapsed.as_millis() as u64;
    if millis == 0 {
        "<1ms".to_string()
    } else if elapsed.as_secs() > 0 {
        format!("{:.2}s", elapsed.as_secs_f32())
    } else {
        format!("{millis}ms")
    }
}

fn collect_plugin_asset_refs(root: &Path, dir: &Path, referenced: &mut HashSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_plugin_asset_refs(root, &path, referenced);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("py") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                for asset in pite_assets::played_assets(&text) {
                    referenced.insert(asset.to_string());
                }
            }
        }
        if let Some(rel) = pite_project::to_res_path(root, &path) {
            if rel.ends_with(".png")
                || rel.ends_with(".ptatlas")
                || rel.ends_with(".wav")
                || rel.ends_with(".ogg")
                || rel.ends_with(".mp3")
                || rel.ends_with(".flac")
            {
                referenced.insert(rel);
            }
        }
    }
}

/// Scanned files no reachable scene references. Only `res://` strings from
/// reachable scenes feed `referenced`, so anything unvisited (uninstantiated
/// scenes, loose scripts) keeps its assets silent — reachable-only by design.
fn orphan_findings(
    by_path: &HashMap<&str, &pite_assets::ScannedFile>,
    referenced: &HashSet<String>,
) -> Vec<Finding> {
    let mut orphans: Vec<&str> = by_path
        .keys()
        .copied()
        .filter(|path| !referenced.contains(*path))
        .collect();
    orphans.sort_unstable();
    orphans.into_iter().map(Finding::orphan_asset).collect()
}

fn render(ui: &mut Ui, data: &CheckData, project: &str, elapsed: &str, strict: bool) {
    if ui.verbose() && !ui.quiet() && !data.cache.is_empty() {
        ui.line_err(&format_cache(&data.cache, ui.colors()));
        ui.blank();
    }
    for finding in &data.diagnostics {
        ui.card(finding);
    }

    // Point at the one command that clears the reference-rewrite findings,
    // separated from both the cards and the summary block.
    let fixable = data
        .diagnostics
        .iter()
        .filter(|finding| finding.fixed_by_reimport())
        .count();
    if fixable > 0 && !ui.quiet() {
        ui.blank();
        ui.info(
            Spans::new()
                .info("Hint:")
                .plain(" Run ")
                .info("pite check --reimport-assets")
                .plain(format!(
                    " to fix {fixable} {}.",
                    if fixable == 1 { "issue" } else { "issues" }
                )),
        );
        ui.blank();
    }

    // Every verdict and stat line lives in one block: a single blank line
    // before it, and no blank lines between the bars.
    let mut bars: Vec<SummaryBar> = Vec::new();
    if ui.verbose() && !ui.quiet() && !data.cache.is_empty() {
        bars.push(cache_bar(&data.cache));
    }
    if !ui.quiet() {
        if let Some(reimport) = &data.reimport {
            bars.push(reimport_bar(reimport));
        }
    }
    // Quiet keeps problems and a failing verdict, but drops a pure success line.
    if !ui.quiet() || !data.ok || data.summary.warnings > 0 {
        bars.push(summary_bar(data, project, elapsed, strict));
    }
    if bars.is_empty() {
        return;
    }
    ui.blank();
    for bar in bars {
        ui.bar(&bar.label, &bar.right, bar.tone);
    }
}
