// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

pub struct InstanceSlot {
    pub id: NodeId,
    type_name: String,
    pub file: Option<PathBuf>,
    pub backend: Box<dyn ScriptBackend>,
    pub errored: bool,
    pub has_ready: bool,
    pub has_process: bool,
    pub has_physics_process: bool,
}

fn slot_still_valid(
    slot: &InstanceSlot,
    type_name: &str,
    script: &Option<(PathBuf, String)>,
) -> bool {
    if slot.type_name != type_name {
        return false;
    }
    match (script, slot.file.as_ref()) {
        (Some((path, _class)), Some(existing)) => existing == path,
        (None, None) => true,
        _ => false,
    }
}

/// Engine-owned signals every node of that type declares before `_ready`
/// runs. Subclasses that re-declare `pressed`/`timeout` still work
/// (`declare_signals` is idempotent). Adding a new engine signal = one
/// row here, no code change in `rebuild`.
const ENGINE_SIGNALS: &[(&str, &[(&str, &[&str])])] = &[
    ("Button", &[("pressed", &[])]),
    ("Timer", &[("timeout", &[])]),
    (
        "AnimatedSprite2D",
        &[
            ("animation_finished", &["str"]),
            ("animation_looped", &["str"]),
        ],
    ),
    (
        "Area2D",
        &[
            ("body_entered", &["str"]),
            ("body_exited", &["str"]),
            ("area_entered", &["str"]),
            ("area_exited", &["str"]),
        ],
    ),
];

impl GameSession {
    pub(super) fn rebuild(&mut self) -> Result<()> {
        let doc = pite_scene::load_scene_cached(&self.scene)
            .with_context(|| format!("cannot load scene {}", self.scene.display()))?;
        let tree = pite_scene::build_tree(&doc, &self.scene_dir, self.project_dir.as_deref())?;
        self.host.with_tree_mut(|t| *t = tree);

        // Engine-owned signals come from one table; every matching node declares
        // its type's signals before `_ready` runs. Subclasses that re-declare
        // inherit through Python MRO as before.
        let nodes: Vec<(String, String)> = self.host.with_tree(|tree| {
            tree.iter()
                .map(|n| (n.type_name.clone(), n.id.to_string()))
                .collect()
        });
        for (type_name, id) in &nodes {
            let Some((_, signals)) = ENGINE_SIGNALS
                .iter()
                .find(|(t, _)| *t == type_name.as_str())
            else {
                continue;
            };
            let defs: Vec<(String, Vec<String>)> = signals
                .iter()
                .map(|(name, payload)| {
                    (
                        name.to_string(),
                        payload.iter().map(|s| s.to_string()).collect(),
                    )
                })
                .collect();
            if let Err(e) = self.host.declare_signals(id, defs) {
                tracing::error!(
                    node = %id,
                    ty = %type_name,
                    "cannot declare builtin signals: {e:#}"
                );
            }
        }

        let tilemaps: Vec<(String, String)> = self.host.with_tree(|tree| {
            tree.iter()
                .filter(|n| n.type_name == "TileMap2D")
                .filter_map(|n| {
                    let map_path = match n.props.get("tilemap") {
                        Some(PropValue::Str(s)) if !s.is_empty() => s.clone(),
                        _ => return None,
                    };
                    Some((n.id.to_string(), map_path))
                })
                .collect()
        });
        for (id, map_path) in tilemaps {
            let resolved = self.resolve_path(&map_path);
            match pite_scene::load_tilemap(&resolved) {
                Ok(doc) => {
                    self.host.set_tilemap_doc(&id, doc);
                }
                Err(e) => {
                    tracing::error!(node = %id, "cannot load tilemap {}: {e:#}", resolved.display());
                }
            }
        }

        let anim_sprites: Vec<(NodeId, String)> = self.host.with_tree(|tree| {
            tree.iter()
                .filter(|n| n.type_name == "AnimatedSprite2D")
                .filter_map(|n| {
                    if let Some(PropValue::Str(autoplay)) = n.props.get("autoplay") {
                        if !autoplay.is_empty() {
                            return Some((n.id.clone(), autoplay.clone()));
                        }
                    }
                    None
                })
                .collect()
        });
        self.host.with_tree_mut(|tree| {
            for (id, autoplay) in anim_sprites {
                if let Some(node) = tree.get_mut(&id) {
                    let anim_empty = match node.props.get("animation") {
                        Some(PropValue::Str(s)) => s.is_empty(),
                        _ => true,
                    };
                    if anim_empty {
                        node.props
                            .insert("animation".to_string(), PropValue::Str(autoplay.clone()));
                    }
                    node.props
                        .insert("playing".to_string(), PropValue::Bool(true));
                }
            }
        });

        self.rebuild_physics();

        let all_nodes: Vec<(NodeId, String, Option<(PathBuf, String)>)> =
            self.host.with_tree(|tree| {
                tree.iter()
                    .map(|node| {
                        let script = node.script.as_ref().and_then(|s| {
                            resolve_script(&s.path, &self.scene_dir, self.project_dir.as_deref())
                                .map(|p| (p, s.class_name.clone()))
                        });
                        (node.id.clone(), node.type_name.clone(), script)
                    })
                    .collect()
            });

        let mut olds: HashMap<String, InstanceSlot> = HashMap::new();
        for slot in self.instances.drain(..) {
            olds.insert(slot.id.to_string(), slot);
        }

        for (id, type_name, script_opt) in all_nodes {
            let id_str = id.to_string();
            if let Some(slot) = olds.remove(&id_str) {
                if slot_still_valid(&slot, &type_name, &script_opt) {
                    self.instances.push(slot);
                    continue;
                }
                self.host.unregister(&id_str);
                self.host.disconnect_node(&id_str);
            }

            match script_opt {
                Some((path, class)) => {
                    if !path.is_file() {
                        tracing::warn!(node = %id, script = %path.display(), "missing script, keeping placeholder node");
                        continue;
                    }
                    let mut backend = Box::new(Pyo3Backend::new(id_str.clone()));
                    backend.set_host(self.host.clone());
                    match backend.load(&path.to_string_lossy(), &class) {
                        Ok(()) => {
                            let has_ready = backend.has_ready();
                            let has_process = backend.has_process();
                            let has_physics_process = backend.has_physics_process();
                            let mut errored = false;
                            if has_ready {
                                if let Err(e) = backend.call_ready() {
                                    tracing::error!(node = %id, "{e:#}");
                                    errored = true;
                                }
                            }
                            self.instances.push(InstanceSlot {
                                id,
                                type_name,
                                file: Some(path),
                                backend,
                                errored,
                                has_ready,
                                has_process,
                                has_physics_process,
                            });
                        }
                        Err(e) => {
                            tracing::error!(node = %id, "cannot load script: {e:#}");
                        }
                    }
                }
                None => {
                    let mut backend = Box::new(Pyo3Backend::new(id_str.clone()));
                    backend.set_host(self.host.clone());
                    match backend.load_base(&type_name) {
                        Ok(()) => {
                            let has_ready = backend.has_ready();
                            let has_process = backend.has_process();
                            let has_physics_process = backend.has_physics_process();
                            let mut errored = false;
                            if has_ready {
                                if let Err(e) = backend.call_ready() {
                                    tracing::error!(node = %id, "{e:#}");
                                    errored = true;
                                }
                            }
                            self.instances.push(InstanceSlot {
                                id,
                                type_name,
                                file: None,
                                backend,
                                errored,
                                has_ready,
                                has_process,
                                has_physics_process,
                            });
                        }
                        Err(e) => {
                            tracing::error!(node = %id, "cannot instantiate base pyclass: {e:#}");
                        }
                    }
                }
            }
        }

        for (id_str, _) in olds {
            self.host.unregister(&id_str);
            self.host.disconnect_node(&id_str);
        }

        tracing::info!(
            nodes = self.tree_len(),
            instances = self.instances.len(),
            scripted = self.scripted_count(),
            "scene ready"
        );
        Ok(())
    }
}
