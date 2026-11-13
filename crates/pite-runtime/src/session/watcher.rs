// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

impl GameSession {
    pub(super) fn start_watcher(&mut self) -> Result<()> {
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| self.scene_dir.clone());
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(tx, notify::Config::default())?;
        watcher.watch(&root, RecursiveMode::Recursive)?;
        self._watcher = Some(watcher);
        self.watch_rx = Some(rx);
        tracing::info!(watch = %root.display(), "hot reload watching");
        Ok(())
    }

    pub fn poll_watch(&mut self) -> bool {
        if self.no_reload {
            return false;
        }
        let mut script_touched: HashSet<PathBuf> = HashSet::new();
        let mut scene_touched = false;
        let mut assets_touched: HashSet<PathBuf> = HashSet::new();
        if let Some(rx) = &self.watch_rx {
            while let Ok(result) = rx.try_recv() {
                let Ok(event) = result else { continue };
                if !matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    continue;
                }
                for path in event.paths {
                    match path.extension().and_then(|e| e.to_str()) {
                        Some("py") => {
                            script_touched.insert(path);
                        }
                        Some("ptscn") => {
                            if self.is_scene_affected(&path) {
                                scene_touched = true;
                            }
                        }
                        Some("ptmap") => {
                            scene_touched = true;
                        }
                        Some("png") | Some("json") => {
                            assets_touched.insert(path);
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut changed = false;
        for path in script_touched {
            if self.reload_script_path(&path) {
                changed = true;
            }
        }
        if !assets_touched.is_empty() {
            self.dirty_assets.extend(assets_touched);
            changed = true;
        }
        if scene_touched {
            match self.rebuild() {
                Ok(()) => {
                    changed = true;
                    tracing::info!("scene reloaded");
                }
                Err(e) => tracing::error!("scene reload failed, keeping old tree: {e:#}"),
            }
        }
        changed
    }

    pub fn reload_script_path(&mut self, path: &Path) -> bool {
        let mut reloaded = false;
        for slot in self
            .instances
            .iter_mut()
            .filter(|s| s.file.as_ref().is_some_and(|f| same_file(f, path)))
        {
            match slot.backend.reload() {
                Ok(()) => {
                    slot.errored = false;
                    slot.has_ready = slot.backend.has_ready();
                    slot.has_process = slot.backend.has_process();
                    slot.has_physics_process = slot.backend.has_physics_process();
                    self.host.invalidate_proxy_cache(slot.id.as_str());
                    reloaded = true;
                    tracing::info!(node = %slot.id, "script reloaded");
                }
                Err(e) => {
                    tracing::error!(node = %slot.id, "reload failed, keeping last good state: {e:#}")
                }
            }
        }
        reloaded
    }

    pub fn is_scene_affected(&self, path: &Path) -> bool {
        if same_file(path, &self.scene) {
            return true;
        }
        if let Ok(doc) = pite_scene::load_scene_cached(&self.scene) {
            for inst in &doc.instance {
                if let Ok(inst_path) = pite_scene::resolve_scene_ref(
                    &inst.scene,
                    &self.scene_dir,
                    self.project_dir.as_deref(),
                ) {
                    if same_file(path, &inst_path) {
                        return true;
                    }
                }
            }
        }
        false
    }
}
