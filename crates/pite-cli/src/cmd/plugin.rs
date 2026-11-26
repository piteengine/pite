// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pite plugin`: manage project plugins under `res://plugins/<id>/`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::Subcommand;
use pite_export::Progress;
use pite_project::{
    find_project_root, is_valid_plugin_id, load_manifest, load_plugin_manifest, plugin_dir,
    save_manifest, save_plugin_manifest, PluginEntry, PluginManifest, PLUGINS_DIR,
    PLUGIN_MANIFEST_FILE,
};
use serde::Serialize;

use crate::output::{Outcome, SCHEMA_VERSION};
use crate::text::Spans;
use crate::ui::{BarTone, Cell, Table, Ui};

#[derive(Debug, Subcommand)]
pub enum PluginAction {
    /// Download or copy a plugin into plugins/<id>/ (disabled until enabled).
    Add {
        /// Git URL, .zip URL/path, or local directory path.
        source: String,
        /// Optional git branch, tag, or commit revision.
        #[arg(long)]
        rev: Option<String>,
        /// Override the plugin ID (defaults to `id` in plugin.toml).
        #[arg(long)]
        name: Option<String>,
    },
    /// Enable an installed plugin in pite.toml.
    Enable {
        /// Plugin ID (snake_case).
        id: String,
    },
    /// Disable a plugin in pite.toml without deleting its files.
    Disable {
        /// Plugin ID (snake_case).
        id: String,
    },
    /// Remove a plugin from pite.toml and delete plugins/<id>/.
    Remove {
        /// Plugin ID (snake_case).
        id: String,
    },
    /// List installed and configured plugins.
    List,
    /// Scaffold a starter plugin under plugins/<id>/ and enable it.
    New {
        /// Plugin ID (snake_case Python identifier).
        id: String,
    },
}

#[derive(Debug, Serialize)]
struct PluginRow {
    id: String,
    version: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

#[derive(Debug, Serialize)]
struct ListData {
    schema_version: u32,
    plugins: Vec<PluginRow>,
}

#[derive(Debug, Serialize)]
struct AddData {
    schema_version: u32,
    id: String,
    version: String,
    enabled: bool,
    path: String,
}

#[derive(Debug, Serialize)]
struct ToggleData {
    schema_version: u32,
    id: String,
    enabled: bool,
}

#[derive(Debug, Serialize)]
struct RemoveData {
    schema_version: u32,
    removed: String,
}

#[derive(Debug, Serialize)]
struct NewPluginData {
    schema_version: u32,
    id: String,
    path: String,
}

struct TempStage {
    path: PathBuf,
}

impl TempStage {
    fn new(tag: &str) -> Result<Self> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("pite-plugin-{tag}-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path)
            .with_context(|| format!("cannot create staging directory {}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for TempStage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub fn run(ui: &mut Ui, action: PluginAction) -> Result<Outcome> {
    let cwd = std::env::current_dir()?;
    let root = find_project_root(&cwd).ok_or_else(|| {
        anyhow::anyhow!(
            "no pite.toml found in {} or any parent directory",
            cwd.display()
        )
    })?;
    match action {
        PluginAction::Add { source, rev, name } => {
            cmd_add(ui, &root, &source, rev.as_deref(), name.as_deref())
        }
        PluginAction::Enable { id } => cmd_set_enabled(ui, &root, &id, true),
        PluginAction::Disable { id } => cmd_set_enabled(ui, &root, &id, false),
        PluginAction::Remove { id } => cmd_remove(ui, &root, &id),
        PluginAction::List => cmd_list(ui, &root),
        PluginAction::New { id } => cmd_new(ui, &root, &id),
    }
}

fn copy_dir_clean(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if matches!(name_str.as_ref(), ".git" | "__pycache__" | "_cache") {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir_clean(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn extract_zip_to(zip_path: &Path, dst: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path)
        .with_context(|| format!("cannot open archive {}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out_path = dst.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    // Unwrap a single top-level directory (typical for GitHub/zipballs) when
    // `plugin.toml` lives inside that single child directory.
    if !dst.join(PLUGIN_MANIFEST_FILE).is_file() {
        let dirs: Vec<PathBuf> = std::fs::read_dir(dst)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        if dirs.len() == 1 && dirs[0].join(PLUGIN_MANIFEST_FILE).is_file() {
            let nested = &dirs[0];
            for entry in std::fs::read_dir(nested)? {
                let entry = entry?;
                let target = dst.join(entry.file_name());
                std::fs::rename(entry.path(), target)?;
            }
            let _ = std::fs::remove_dir_all(nested);
        }
    }
    Ok(())
}

fn fetch_source(source: &str, rev: Option<&str>, stage_dir: &Path) -> Result<()> {
    let local_path = Path::new(source);
    if local_path.is_dir() {
        return copy_dir_clean(local_path, stage_dir);
    }
    if source.ends_with(".zip") {
        if local_path.is_file() {
            return extract_zip_to(local_path, stage_dir);
        }
        if source.starts_with("http://") || source.starts_with("https://") {
            let zip_file = stage_dir.join("_download.zip");
            let status = Command::new("curl")
                .args(["-fsSL", "-o"])
                .arg(&zip_file)
                .arg(source)
                .status()
                .context("failed to invoke curl to download plugin archive")?;
            if !status.success() {
                anyhow::bail!("curl failed downloading plugin archive from {source}");
            }
            let unpack_dir = stage_dir.join("_unpack");
            extract_zip_to(&zip_file, &unpack_dir)?;
            let _ = std::fs::remove_file(&zip_file);
            copy_dir_clean(&unpack_dir, stage_dir)?;
            let _ = std::fs::remove_dir_all(&unpack_dir);
            return Ok(());
        }
    }

    let git_url = source.strip_prefix("git+").unwrap_or(source);
    let mut cmd = Command::new("git");
    cmd.args(["clone", "--depth", "1"]);
    if let Some(r) = rev {
        cmd.args(["--branch", r]);
    }
    let clone_dir = stage_dir.join("_clone");
    let status = cmd
        .arg(git_url)
        .arg(&clone_dir)
        .status()
        .context("failed to invoke git to clone plugin")?;
    if !status.success() {
        anyhow::bail!("git clone failed for {git_url}");
    }
    copy_dir_clean(&clone_dir, stage_dir)?;
    let _ = std::fs::remove_dir_all(&clone_dir);
    Ok(())
}

fn cmd_add(
    ui: &mut Ui,
    root: &Path,
    source: &str,
    rev: Option<&str>,
    name_override: Option<&str>,
) -> Result<Outcome> {
    let stage = TempStage::new("add")?;
    ui.step(&format!("fetching plugin from {source}"));
    fetch_source(source, rev, &stage.path)?;

    let plugin_manifest = load_plugin_manifest(&stage.path).with_context(|| {
        format!(
            "source `{source}` does not contain a valid {}",
            PLUGIN_MANIFEST_FILE
        )
    })?;
    let id = name_override
        .map(str::to_string)
        .unwrap_or_else(|| plugin_manifest.id.clone());
    if !is_valid_plugin_id(&id) {
        anyhow::bail!("plugin id `{id}` must be a valid snake_case Python identifier");
    }

    let dest = plugin_dir(root, &id);
    if dest.exists() {
        anyhow::bail!(
            "plugin `{id}` already exists at {}; run `pite plugin remove {id}` first",
            dest.display()
        );
    }

    copy_dir_clean(&stage.path, &dest)?;

    let mut manifest = load_manifest(root)?;
    manifest.plugins.insert(
        id.clone(),
        PluginEntry::Detailed {
            enabled: false,
            source: Some(source.to_string()),
            rev: rev.map(str::to_string),
        },
    );
    save_manifest(root, &manifest)?;

    ui.success(
        Spans::new()
            .plain("Installed plugin ")
            .strong(&id)
            .plain(" (")
            .info(&plugin_manifest.version)
            .plain(") into ")
            .strong(format!("{PLUGINS_DIR}/{id}"))
            .plain(" (disabled)."),
    );
    if !ui.quiet() {
        ui.info(
            Spans::new()
                .plain("  Enable it with ")
                .info(format!("pite plugin enable {id}"))
                .plain(" or set ")
                .info(format!("{id} = true"))
                .plain(" under [plugins] in pite.toml."),
        );
    }

    Outcome::completed(AddData {
        schema_version: SCHEMA_VERSION,
        id: id.clone(),
        version: plugin_manifest.version,
        enabled: false,
        path: format!("{PLUGINS_DIR}/{id}"),
    })
}

fn cmd_set_enabled(ui: &mut Ui, root: &Path, id: &str, enable: bool) -> Result<Outcome> {
    if !is_valid_plugin_id(id) {
        anyhow::bail!("plugin id `{id}` must be a valid snake_case Python identifier");
    }
    let dir = plugin_dir(root, id);
    if enable {
        let pm = load_plugin_manifest(&dir).with_context(|| {
            format!(
                "cannot enable `{id}`: missing or invalid {}",
                dir.join(PLUGIN_MANIFEST_FILE).display()
            )
        })?;
        if pm.id != id {
            anyhow::bail!("plugin manifest id `{}` does not match `{id}`", pm.id);
        }
        let vendor_dir = root.join("vendor");
        for (pkg, spec) in &pm.dependencies {
            if !crate::cmd::vendor::vendor_provides(&vendor_dir, pkg) {
                ui.info(
                    Spans::new()
                        .plain("Warning: plugin ")
                        .strong(id)
                        .plain(" depends on ")
                        .info(format!("{pkg}{spec}"))
                        .plain(" (not vendored). Run ")
                        .info(format!("pite vendor add {pkg}{spec}"))
                        .plain("."),
                );
            }
        }
    }

    let mut manifest = load_manifest(root)?;
    match manifest.plugins.get_mut(id) {
        Some(entry) => entry.set_enabled(enable),
        None => {
            manifest
                .plugins
                .insert(id.to_string(), PluginEntry::Enabled(enable));
        }
    }
    save_manifest(root, &manifest)?;

    let verb = if enable { "Enabled" } else { "Disabled" };
    ui.success(Spans::new().plain(format!("{verb} plugin ")).strong(id));
    Outcome::completed(ToggleData {
        schema_version: SCHEMA_VERSION,
        id: id.to_string(),
        enabled: enable,
    })
}

fn cmd_remove(ui: &mut Ui, root: &Path, id: &str) -> Result<Outcome> {
    let mut manifest = load_manifest(root)?;
    let had_entry = manifest.plugins.remove(id).is_some();
    if had_entry {
        save_manifest(root, &manifest)?;
    }
    let dir = plugin_dir(root, id);
    let had_dir = dir.exists();
    if had_dir {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("cannot remove {}", dir.display()))?;
    }
    if !had_entry && !had_dir {
        anyhow::bail!("plugin `{id}` is not installed or configured");
    }
    ui.success(Spans::new().plain("Removed plugin ").strong(id));
    Outcome::completed(RemoveData {
        schema_version: SCHEMA_VERSION,
        removed: id.to_string(),
    })
}

fn cmd_list(ui: &mut Ui, root: &Path) -> Result<Outcome> {
    let manifest = load_manifest(root)?;
    let mut ids: BTreeSet<String> = manifest.plugins.keys().cloned().collect();
    let plugins_root = root.join(PLUGINS_DIR);
    if let Ok(entries) = std::fs::read_dir(&plugins_root) {
        for entry in entries.filter_map(|e| e.ok()) {
            if entry.path().join(PLUGIN_MANIFEST_FILE).is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.insert(name.to_string());
                }
            }
        }
    }

    let mut rows = Vec::new();
    for id in ids {
        let entry = manifest.plugins.get(&id);
        let dir = plugin_dir(root, &id);
        let pm = load_plugin_manifest(&dir).ok();
        let status = match (entry.map(|e| e.is_enabled()), pm.is_some()) {
            (Some(true), true) => "enabled",
            (Some(true), false) => "missing",
            (Some(false), true) => "disabled",
            (Some(false), false) => "missing",
            (None, true) => "unconfigured",
            (None, false) => "missing",
        };
        let version = pm.map(|m| m.version).unwrap_or_else(|| "-".to_string());
        let source = entry.and_then(|e| e.source().map(str::to_string));
        rows.push(PluginRow {
            id,
            version,
            status,
            source,
        });
    }

    if rows.is_empty() {
        ui.info(Spans::new().faded("No plugins configured or installed."));
        ui.bar("Plugins", "0 total", BarTone::Faded);
    } else {
        let mut table = Table::new(&["ID", "VERSION", "STATUS", "SOURCE"]);
        for r in &rows {
            table = table.row(vec![
                Cell::plain(&r.id),
                Cell::plain(&r.version),
                Cell::plain(r.status),
                Cell::plain(r.source.as_deref().unwrap_or("local")),
            ]);
        }
        ui.table(&table);
        let enabled = rows.iter().filter(|r| r.status == "enabled").count();
        ui.bar(
            "Plugins",
            &format!("{enabled} enabled, {} total", rows.len()),
            BarTone::Faded,
        );
    }

    Outcome::completed(ListData {
        schema_version: SCHEMA_VERSION,
        plugins: rows,
    })
}

fn cmd_new(ui: &mut Ui, root: &Path, id: &str) -> Result<Outcome> {
    if !is_valid_plugin_id(id) {
        anyhow::bail!("plugin id `{id}` must be a valid snake_case Python identifier");
    }
    let dir = plugin_dir(root, id);
    if dir.exists() {
        anyhow::bail!("plugin `{id}` already exists at {}", dir.display());
    }

    let pm = PluginManifest {
        id: id.to_string(),
        name: id.to_string(),
        version: "0.1.0".to_string(),
        pite_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        description: format!("Starter plugin `{id}`."),
        runtime: Some("runtime:Plugin".to_string()),
        editor: Some("editor:EditorPlugin".to_string()),
        autoloads: BTreeMap::new(),
        nodes: Vec::new(),
        props: HashMap::new(),
        dependencies: BTreeMap::new(),
    };
    save_plugin_manifest(&dir, &pm)?;
    std::fs::write(dir.join("__init__.py"), "")?;
    std::fs::write(
        dir.join("runtime.py"),
        "import pite\n\n\nclass Plugin:\n    def on_ready(self):\n        pass\n",
    )?;
    std::fs::write(
        dir.join("editor.py"),
        "class EditorPlugin:\n    def on_enable(self, editor):\n        pass\n",
    )?;

    let mut manifest = load_manifest(root)?;
    manifest
        .plugins
        .insert(id.to_string(), PluginEntry::Enabled(true));
    save_manifest(root, &manifest)?;

    ui.success(
        Spans::new()
            .plain("Created plugin ")
            .strong(id)
            .plain(" at ")
            .strong(format!("{PLUGINS_DIR}/{id}")),
    );
    Outcome::completed(NewPluginData {
        schema_version: SCHEMA_VERSION,
        id: id.to_string(),
        path: format!("{PLUGINS_DIR}/{id}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_add_list_enable_disable_remove_round_trip() {
        let root = std::env::temp_dir().join(format!("pite-plug-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let proj = root.join("game");
        let src_plug = root.join("src_plug");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&src_plug).unwrap();
        std::fs::write(proj.join("pite.toml"), "[project]\nname = \"g\"\n").unwrap();
        std::fs::write(
            src_plug.join(PLUGIN_MANIFEST_FILE),
            "id = \"hello\"\nversion = \"0.2.0\"\n",
        )
        .unwrap();
        std::fs::write(src_plug.join("__init__.py"), "").unwrap();

        let mut ui = Ui::silent();
        cmd_add(&mut ui, &proj, &src_plug.to_string_lossy(), None, None).unwrap();
        let m = load_manifest(&proj).unwrap();
        assert!(!m.plugins["hello"].is_enabled());
        assert!(plugin_dir(&proj, "hello").join("__init__.py").is_file());

        cmd_set_enabled(&mut ui, &proj, "hello", true).unwrap();
        let m = load_manifest(&proj).unwrap();
        assert!(m.plugins["hello"].is_enabled());

        cmd_set_enabled(&mut ui, &proj, "hello", false).unwrap();
        let m = load_manifest(&proj).unwrap();
        assert!(!m.plugins["hello"].is_enabled());

        cmd_remove(&mut ui, &proj, "hello").unwrap();
        let m = load_manifest(&proj).unwrap();
        assert!(!m.plugins.contains_key("hello"));
        assert!(!plugin_dir(&proj, "hello").exists());

        std::fs::remove_dir_all(&root).ok();
    }
}
