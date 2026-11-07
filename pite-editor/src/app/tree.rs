// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use eframe::egui;
use egui_phosphor::regular;
use pite_core::NodeId;
use pite_core::NodeTypeSchema;

use crate::icons::{self, Icon};
use crate::ops;

use super::EditorApp;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropZone {
    None,
    Before,
    Under,
    After,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDndPayload {
    pub id: String,
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone)]
pub struct AddNodeModal {
    pub parent: Option<NodeId>,
    pub selected_type: String,
    pub name: String,
    pub search: String,
}

#[derive(Debug, Clone)]
pub struct RenameNodeModal {
    pub id: NodeId,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct InstanceSceneModal {
    pub parent: Option<NodeId>,
}

pub struct NodeTypeInfo {
    pub type_name: &'static str,
    pub description: &'static str,
    pub children: Vec<NodeTypeInfo>,
}

/// Add-node tree derived from `pite_core::NODE_SCHEMAS`: every concrete
/// type sits under its nearest **non-abstract** ancestor. Abstract bases
/// (`Control`, `CollisionObject2D`, `PhysicsBody2D`) flatten out, so
/// `Label`/`Button` hang off `Node` and physics bodies off `Node2D` —
/// the same shape the old hardcoded tree had, now impossible to drift
/// from the schema.
pub fn node_hierarchy() -> &'static [NodeTypeInfo] {
    static HIERARCHY: OnceLock<Vec<NodeTypeInfo>> = OnceLock::new();
    HIERARCHY.get_or_init(build_node_hierarchy).as_slice()
}

fn build_node_hierarchy() -> Vec<NodeTypeInfo> {
    let mut by_parent: HashMap<&'static str, Vec<&'static NodeTypeSchema>> = HashMap::new();
    let mut roots: Vec<&'static NodeTypeSchema> = Vec::new();
    for &schema in pite_core::NODE_SCHEMAS {
        if schema.abstract_class {
            continue;
        }
        match nearest_visible_ancestor(schema.type_name) {
            Some(parent) => by_parent.entry(parent).or_default().push(schema),
            None => roots.push(schema),
        }
    }
    roots
        .into_iter()
        .map(|schema| build_branch(schema.type_name, &by_parent))
        .collect()
}

fn build_branch(
    type_name: &'static str,
    by_parent: &HashMap<&'static str, Vec<&'static NodeTypeSchema>>,
) -> NodeTypeInfo {
    let children = by_parent
        .get(type_name)
        .map(|kids| {
            kids.iter()
                .map(|s| build_branch(s.type_name, by_parent))
                .collect()
        })
        .unwrap_or_default();
    NodeTypeInfo {
        type_name,
        description: description_for(type_name),
        children,
    }
}

/// Walk `inherits` until a non-abstract ancestor; `None` for the root.
fn nearest_visible_ancestor(type_name: &'static str) -> Option<&'static str> {
    let mut current = pite_core::node_type_schema(type_name)?.inherits;
    while let Some(name) = current {
        let schema = pite_core::node_type_schema(name)?;
        if !schema.abstract_class {
            return Some(schema.type_name);
        }
        current = schema.inherits;
    }
    None
}

/// Prose only — commit 12 moves these into the schema. Structure above is
/// already fully schema-derived.
fn description_for(type_name: &str) -> &'static str {
    match type_name {
        "Node" => "Base class for all scene objects. Does not have a 2D transform.",
        "Node2D" => "2D game object with position, rotation, and scale.",
        "Sprite2D" => "Displays a 2D texture or spritesheet frame.",
        "Camera2D" => "2D viewport camera that can follow the player or stay fixed.",
        "CollisionShape2D" => "Defines a 2D collision boundary (box, circle, capsule).",
        "Area2D" => "2D detection area for triggers and overlap signals.",
        "StaticBody2D" => "Static 2D physics body that does not move.",
        "CharacterBody2D" => "2D physics body specialized for characters controlled by script.",
        "RigidBody2D" => "Dynamic 2D physics body simulated by the physics engine.",
        "Label" => "Displays formatted text on the screen.",
        "Button" => "Clickable UI button with hover/pressed states.",
        "Timer" => "Countdown timer that emits a timeout signal periodically or once.",
        _ => "Engine scene node.",
    }
}

fn find_description(hierarchy: &[NodeTypeInfo], target: &str) -> Option<&'static str> {
    for item in hierarchy {
        if item.type_name == target {
            return Some(item.description);
        }
        if let Some(desc) = find_description(&item.children, target) {
            return Some(desc);
        }
    }
    None
}

fn node_type_matches_search(info: &NodeTypeInfo, query: &str) -> bool {
    if info.type_name.to_lowercase().contains(query)
        || info.description.to_lowercase().contains(query)
    {
        return true;
    }
    info.children
        .iter()
        .any(|child| node_type_matches_search(child, query))
}

impl EditorApp {
    pub(super) fn delete_node(&mut self, id: &NodeId) {
        match self
            .session
            .host()
            .with_tree_mut(|t| ops::remove_node(t, id))
        {
            Ok(()) => {
                if self.selected.as_deref() == Some(id.as_str()) {
                    self.selected = None;
                }
                self.viewport_dirty = true;
                self.log(format!("deleted {id}."));
            }
            Err(e) => self.log(format!("delete failed: {e:#}")),
        }
    }

    pub(super) fn move_sibling(&mut self, id: &NodeId, delta: i32) {
        match self
            .session
            .host()
            .with_tree_mut(|t| ops::move_sibling(t, id, delta))
        {
            Ok(()) => {
                self.viewport_dirty = true;
                self.log(format!("moved {id}."));
            }
            Err(e) => self.log(format!("move failed: {e:#}")),
        }
    }

    pub(super) fn duplicate_selected_node(&mut self) {
        let Some(sel) = self.selected.clone().map(NodeId::from) else {
            return;
        };
        let is_root = self.session.host().with_tree(|t| t.root() == Some(&sel));
        if is_root {
            self.log("Cannot duplicate the root node.");
            return;
        }
        match self
            .session
            .host()
            .with_tree_mut(|t| ops::duplicate_node(t, &sel))
        {
            Ok(new_id) => {
                self.selected = Some(new_id.to_string());
                self.viewport_dirty = true;
                self.log(format!("Duplicated {sel} -> {new_id}"));
            }
            Err(e) => self.log(format!("Duplicate failed: {e:#}")),
        }
    }

    pub(super) fn open_add_node_modal(&mut self, parent: Option<NodeId>) {
        self.add_node_modal = Some(AddNodeModal {
            parent,
            selected_type: "Node2D".to_string(),
            name: "Node2D".to_string(),
            search: String::new(),
        });
    }

    pub(super) fn open_rename_node_modal(&mut self, id: NodeId) {
        let current_name = self
            .session
            .host()
            .with_tree(|t| t.get(&id).map(|n| n.name.clone()))
            .unwrap_or_else(|| id.to_string());
        self.rename_node_modal = Some(RenameNodeModal {
            id,
            name: current_name,
        });
    }

    pub(super) fn open_instance_modal(&mut self, parent: Option<NodeId>) {
        self.instance_modal = Some(InstanceSceneModal { parent });
    }

    pub(super) fn drop_node(&mut self, dragged: &str, target: &NodeId, zone: DropZone) {
        let dragged_id = NodeId::from(dragged);
        if dragged_id == *target || zone == DropZone::None {
            return;
        }
        let result = self.session.host().with_tree_mut(|t| match zone {
            DropZone::Under => ops::move_node(t, &dragged_id, Some(target.clone())),
            DropZone::Before => {
                let parent = t
                    .get(target)
                    .and_then(|n| n.parent.clone())
                    .ok_or_else(|| anyhow::anyhow!("cannot reorder at the top level"))?;
                let sibs = t.children_of(&parent);
                let pos = sibs
                    .iter()
                    .position(|s| s == target)
                    .ok_or_else(|| anyhow::anyhow!("target `{target}` not found"))?;
                ops::place_node(t, &dragged_id, &parent, pos)
            }
            DropZone::After => {
                let parent = t
                    .get(target)
                    .and_then(|n| n.parent.clone())
                    .ok_or_else(|| anyhow::anyhow!("cannot reorder at the top level"))?;
                let sibs = t.children_of(&parent);
                let pos = sibs
                    .iter()
                    .position(|s| s == target)
                    .ok_or_else(|| anyhow::anyhow!("target `{target}` not found"))?;
                ops::place_node(t, &dragged_id, &parent, pos + 1)
            }
            DropZone::None => Ok(()),
        });
        match result {
            Ok(()) => {
                self.viewport_dirty = true;
                self.log(format!("moved {dragged_id}."));
            }
            Err(e) => self.log(format!("move failed: {e:#}")),
        }
    }

    pub(super) fn show_node(&mut self, ui: &mut egui::Ui, id: &NodeId) {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 1.0);
        ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
        ui.spacing_mut().indent = 14.0;

        let (name, type_name, children, parent_id) = self
            .session
            .host()
            .with_tree(|t| {
                t.get(id).map(|n| {
                    (
                        n.name.clone(),
                        n.type_name.clone(),
                        t.children_of(id),
                        n.parent.clone(),
                    )
                })
            })
            .unwrap_or_else(|| ("?".to_string(), "?".to_string(), vec![], None));

        let script = self.session.host().with_tree(|t| {
            t.get(id).and_then(|n| {
                n.script
                    .as_ref()
                    .map(|s| (s.class_name.clone(), self.resolve_res(&s.path)))
            })
        });

        let selected = self.selected.as_ref() == Some(&id.to_string());
        let id_str = id.to_string();
        let target = id.clone();
        let is_root = self.session.host().with_tree(|t| t.root() == Some(id));
        let open = self.is_open(&id_str);

        // Check if this node is the first child of its parent branch
        let is_first_child = if let Some(parent) = &parent_id {
            self.session
                .host()
                .with_tree(|t| t.children_of(parent).first() == Some(id))
        } else {
            false
        };

        let row_width = ui.available_width().max(160.0);
        let (rect, row_resp) =
            ui.allocate_exact_size(egui::vec2(row_width, 20.0), egui::Sense::click_and_drag());

        // Start drag when threshold passed
        if !is_root && row_resp.drag_started() {
            egui::DragAndDrop::set_payload(
                ui.ctx(),
                NodeDndPayload {
                    id: id_str.clone(),
                    name: name.clone(),
                    type_name: type_name.clone(),
                },
            );
        }

        let has_node_dnd = egui::DragAndDrop::has_payload_of_type::<NodeDndPayload>(ui.ctx());
        let has_asset_dnd =
            egui::DragAndDrop::has_payload_of_type::<super::assets::AssetDndPayload>(ui.ctx());

        let is_source =
            if let Some(dragged) = egui::DragAndDrop::payload::<NodeDndPayload>(ui.ctx()) {
                dragged.id == id_str
            } else {
                false
            };

        // Check cycle: cannot drop into self or descendant
        let is_descendant_of_dragged =
            if let Some(dragged) = egui::DragAndDrop::payload::<NodeDndPayload>(ui.ctx()) {
                let dragged_id = NodeId::from(dragged.id.as_str());
                self.session.host().with_tree(|t| {
                    let mut curr = parent_id.clone();
                    while let Some(c) = curr {
                        if c == dragged_id {
                            return true;
                        }
                        curr = t.get(&c).and_then(|n| n.parent.clone());
                    }
                    false
                })
            } else {
                false
            };

        let is_node_target = has_node_dnd && !is_source && row_resp.contains_pointer();
        let is_valid_node_target = is_node_target && !is_descendant_of_dragged;
        let is_invalid_node_target = is_node_target && is_descendant_of_dragged;

        // Drop zones calculation (3-2 method requested by user):
        // Center: Child (DropZone::Under)
        // Bottom edge (> 0.75): Sibling After (DropZone::After)
        // Top edge (< 0.25, ONLY for the first child of a branch): Sibling Before (DropZone::Before)
        let zone = if is_valid_node_target {
            let pointer_pos = ui
                .ctx()
                .input(|i| i.pointer.latest_pos())
                .or_else(|| ui.ctx().pointer_interact_pos())
                .unwrap_or(rect.center());
            let frac = (pointer_pos.y - rect.top()) / rect.height().max(1.0);
            if is_root {
                DropZone::Under
            } else if is_first_child && frac < 0.25 {
                DropZone::Before
            } else if frac > 0.75 {
                DropZone::After
            } else {
                DropZone::Under
            }
        } else {
            DropZone::None
        };

        if is_valid_node_target && zone != DropZone::None {
            self.hovered_node_target = Some((name.clone(), zone));
        }

        // Asset drop target calculation
        let is_asset_target = has_asset_dnd && row_resp.contains_pointer();
        let is_valid_asset_target = if let Some(payload) =
            egui::DragAndDrop::payload::<super::assets::AssetDndPayload>(ui.ctx())
        {
            if payload.rel_path.ends_with(".py") {
                true
            } else if payload.rel_path.ends_with(".png")
                || payload.rel_path.ends_with(".atlas.json")
            {
                type_name == "Sprite2D"
            } else {
                false
            }
        } else {
            false
        };
        let is_invalid_asset_target = is_asset_target && !is_valid_asset_target;

        if is_asset_target && is_valid_asset_target {
            self.hovered_node_target = Some((format!("Node '{name}'"), DropZone::Under));
        }

        // Clicks
        let mut toggled = false;
        if row_resp.clicked() {
            if let Some(pos) = row_resp.interact_pointer_pos() {
                if pos.x < rect.left() + 16.0 && !children.is_empty() {
                    toggled = true;
                } else {
                    self.selected = Some(id_str.clone());
                }
            } else {
                self.selected = Some(id_str.clone());
            }
        }
        if row_resp.double_clicked() {
            if !children.is_empty() {
                toggled = true;
            } else {
                self.open_rename_node_modal(id.clone());
            }
        }

        // Drops from node dragging
        if let Some(dragged) = row_resp.dnd_release_payload::<NodeDndPayload>() {
            if is_valid_node_target {
                self.drop_node(&dragged.id, &target, zone);
            }
        }

        // Drops from asset panel (attaching script or assigning texture)
        if let Some(payload) = row_resp.dnd_release_payload::<super::assets::AssetDndPayload>() {
            if is_valid_asset_target {
                if payload.rel_path.ends_with(".py") {
                    let class_name = Path::new(&payload.rel_path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Script")
                        .to_string();
                    let res_path = format!("res://{}", payload.rel_path);
                    self.session.host().with_tree_mut(|t| {
                        if let Some(n) = t.get_mut(&target) {
                            n.script = Some(pite_core::ScriptRef {
                                path: res_path.clone(),
                                class_name: class_name.clone(),
                            });
                        }
                    });
                    self.viewport_dirty = true;
                    self.log(format!("Attached script {res_path} to {target}"));
                } else if payload.rel_path.ends_with(".png")
                    || payload.rel_path.ends_with(".atlas.json")
                {
                    let res_path = format!("res://{}", payload.rel_path);
                    self.session.host().with_tree_mut(|t| {
                        if let Some(n) = t.get_mut(&target) {
                            if n.type_name == "Sprite2D" {
                                if payload.rel_path.ends_with(".atlas.json") {
                                    n.props.insert(
                                        "atlas".to_string(),
                                        pite_core::PropValue::Str(res_path.clone()),
                                    );
                                } else {
                                    n.props.insert(
                                        "texture".to_string(),
                                        pite_core::PropValue::Str(res_path.clone()),
                                    );
                                }
                            }
                        }
                    });
                    self.viewport_dirty = true;
                    self.log(format!("Assigned {res_path} to {target}"));
                }
            }
        }

        // Background and Drop Target Indicators painting
        let p = ui.painter();
        if zone == DropZone::Under || (is_asset_target && is_valid_asset_target) {
            p.rect_filled(
                rect,
                2.0,
                egui::Color32::from_rgba_unmultiplied(11, 147, 135, 45),
            );
            p.rect_stroke(
                rect,
                2.0,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(11, 147, 135)),
                egui::StrokeKind::Middle,
            );
        } else if is_invalid_node_target || is_invalid_asset_target {
            p.rect_filled(
                rect,
                2.0,
                egui::Color32::from_rgba_unmultiplied(220, 80, 80, 30),
            );
            p.rect_stroke(
                rect,
                2.0,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(220, 80, 80)),
                egui::StrokeKind::Middle,
            );
        } else if is_source && has_node_dnd {
            p.rect_filled(
                rect,
                2.0,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 12),
            );
        } else if selected {
            p.rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
        } else if row_resp.hovered() && !has_node_dnd && !has_asset_dnd {
            p.rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
        }

        // Insertion line indicators for Before and After
        if zone == DropZone::Before {
            let y = rect.top();
            p.line_segment(
                [
                    egui::Pos2::new(rect.left(), y),
                    egui::Pos2::new(rect.right(), y),
                ],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(11, 147, 135)),
            );
            p.circle_filled(
                egui::Pos2::new(rect.left() + 4.0, y),
                3.0,
                egui::Color32::from_rgb(11, 147, 135),
            );
        } else if zone == DropZone::After {
            let y = rect.bottom();
            p.line_segment(
                [
                    egui::Pos2::new(rect.left(), y),
                    egui::Pos2::new(rect.right(), y),
                ],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(11, 147, 135)),
            );
            p.circle_filled(
                egui::Pos2::new(rect.left() + 4.0, y),
                3.0,
                egui::Color32::from_rgb(11, 147, 135),
            );
        }

        // Caret arrow
        if !children.is_empty() {
            let caret_icon = if open {
                Icon::CaretDown
            } else {
                Icon::CaretRight
            };
            let caret_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.left() + 2.0, rect.center().y - 6.0),
                egui::vec2(12.0, 12.0),
            );
            icons::draw_icon(p, caret_rect, caret_icon, crate::theme::FAINT);
        }

        // Dedicated Node Type Icon
        let node_icon = icons::node_type_icon(&type_name);
        let node_color = icons::node_type_color(&type_name);
        let icon_rect = egui::Rect::from_min_size(
            egui::Pos2::new(rect.left() + 16.0, rect.center().y - 8.0),
            egui::vec2(16.0, 16.0),
        );
        icons::draw_icon(p, icon_rect, node_icon, node_color);

        // Node name and type text
        let text_pos = egui::Pos2::new(rect.left() + 36.0, rect.center().y);
        let label_text = &name;
        p.text(
            text_pos,
            egui::Align2::LEFT_CENTER,
            label_text,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
            crate::theme::TEXT,
        );

        let hover_info = if let Some((class, _)) = &script {
            format!("Type: {type_name}\nScript: {class}")
        } else {
            format!("Type: {type_name}")
        };
        let row_resp = row_resp.on_hover_text(hover_info);

        // Script badge on right if script is attached
        if let Some((class, file)) = script.as_ref() {
            let badge_rect = egui::Rect::from_min_size(
                egui::Pos2::new(rect.right() - 22.0, rect.center().y - 8.0),
                egui::vec2(16.0, 16.0),
            );
            let badge_resp = ui.interact(
                badge_rect,
                egui::Id::unique(("script-badge", id_str.clone())),
                egui::Sense::click(),
            );
            let is_badge_hovered = badge_resp.hovered();
            let fg = if is_badge_hovered {
                crate::theme::TEXT
            } else {
                crate::theme::FAINT
            };
            if is_badge_hovered {
                p.rect_filled(badge_rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
            }
            icons::draw_icon(p, badge_rect, Icon::Script, fg);
            if badge_resp
                .on_hover_text(format!("Open script: {class}"))
                .clicked()
            {
                let file = file.clone();
                self.open_asset(&file);
            }
        }

        // Context Menu
        let menu_id = id.clone();
        let menu_kids = !children.is_empty();
        let menu_open = open;
        let has_script = script.is_some();
        row_resp.context_menu(|ui| {
            let mut menu = super::MenuSeparatorTracker::new(ui);
            if menu.button("Add Child Node…  Ctrl+A").clicked() {
                self.open_add_node_modal(Some(menu_id.clone()));
                menu.close();
            }
            if menu
                .button("Instantiate Child Scene…  Ctrl+Shift+A")
                .clicked()
            {
                self.open_instance_modal(Some(menu_id.clone()));
                menu.close();
            }

            menu.start_section();
            if !is_root && menu.button("Duplicate  Ctrl+D").clicked() {
                match self
                    .session
                    .host()
                    .with_tree_mut(|t| ops::duplicate_node(t, &menu_id))
                {
                    Ok(new_id) => {
                        self.selected = Some(new_id.to_string());
                        self.viewport_dirty = true;
                        self.log(format!("Duplicated {menu_id} -> {new_id}"));
                    }
                    Err(e) => self.log(format!("Duplicate failed: {e:#}")),
                }
                menu.close();
            }
            if menu.button("Rename…  F2").clicked() {
                self.open_rename_node_modal(menu_id.clone());
                menu.close();
            }
            if !is_root && menu.button("Delete  Delete").clicked() {
                self.delete_node(&menu_id);
                menu.close();
            }

            if !is_root {
                menu.start_section();
                if menu.button("Move Up  Ctrl+Up").clicked() {
                    self.move_sibling(&menu_id, -1);
                    menu.close();
                }
                if menu.button("Move Down  Ctrl+Down").clicked() {
                    self.move_sibling(&menu_id, 1);
                    menu.close();
                }
            }

            if has_script {
                menu.start_section();
                if menu.button("Open Script").clicked() {
                    if let Some((_, file)) = script.as_ref() {
                        let file = file.clone();
                        self.open_asset(&file);
                    }
                    menu.close();
                }
                if menu.button("Detach Script").clicked() {
                    self.session.host().with_tree_mut(|t| {
                        if let Some(n) = t.get_mut(&menu_id) {
                            n.script = None;
                        }
                    });
                    self.viewport_dirty = true;
                    self.log(format!("Detached script from {menu_id}"));
                    menu.close();
                }
            }

            if menu_kids {
                menu.start_section();
                let label = if menu_open { "Collapse" } else { "Expand" };
                if menu.button(label).clicked() {
                    if menu_open {
                        self.open_nodes.remove(&menu_id.to_string());
                    } else {
                        self.open_nodes.insert(menu_id.to_string());
                    }
                    menu.close();
                }
            }
        });

        if toggled {
            if open {
                self.open_nodes.remove(&id_str);
            } else {
                self.open_nodes.insert(id_str.clone());
            }
        }

        if open && !children.is_empty() {
            ui.indent(id_str, |ui| {
                for child in children {
                    self.show_node(ui, &child);
                }
            });
        }
    }

    pub(super) fn show_node_drag_preview(&self, ctx: &egui::Context) {
        let Some(payload) = egui::DragAndDrop::payload::<NodeDndPayload>(ctx) else {
            return;
        };
        let pointer_pos = ctx
            .input(|i| i.pointer.latest_pos())
            .or_else(|| ctx.pointer_interact_pos());
        let Some(pos) = pointer_pos else {
            return;
        };

        egui::Area::new(egui::Id::unique("node_drag_preview"))
            .fixed_pos(pos + egui::vec2(16.0, 16.0))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(egui::Color32::from_rgb(20, 24, 29))
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(11, 147, 135),
                    ))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        let icon = icons::node_type_icon(&payload.type_name);
                        let color = icons::node_type_color(&payload.type_name);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(icon.as_str()).color(color));
                            ui.label(egui::RichText::new(&payload.name).strong());
                            if let Some((target_name, zone)) = &self.hovered_node_target {
                                match zone {
                                    DropZone::Under => {
                                        ui.label(
                                            egui::RichText::new(format!("-> Inside {target_name}"))
                                                .color(egui::Color32::from_rgb(11, 147, 135)),
                                        );
                                    }
                                    DropZone::Before => {
                                        ui.label(
                                            egui::RichText::new(format!("-> Before {target_name}"))
                                                .color(egui::Color32::from_rgb(11, 147, 135)),
                                        );
                                    }
                                    DropZone::After => {
                                        ui.label(
                                            egui::RichText::new(format!("-> After {target_name}"))
                                                .color(egui::Color32::from_rgb(11, 147, 135)),
                                        );
                                    }
                                    DropZone::None => {}
                                }
                            }
                        });
                    });
            });
    }

    pub(super) fn show_node_modals(&mut self, ctx: &egui::Context) {
        self.show_add_node_modal(ctx);
        self.show_rename_node_modal(ctx);
        self.show_instance_scene_modal(ctx);
    }

    fn show_add_node_modal(&mut self, ctx: &egui::Context) {
        let Some(mut modal) = self.add_node_modal.clone() else {
            return;
        };
        let mut close = false;
        let mut create = false;

        let res = egui::Window::new("Create New Node")
            .id(egui::Id::unique("add_node_modal_window"))
            .default_size(egui::vec2(420.0, 480.0))
            .min_size(egui::vec2(340.0, 400.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(regular::MAGNIFYING_GLASS).color(crate::theme::FAINT),
                    );
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut modal.search)
                            .hint_text("Search node types…")
                            .desired_width(f32::INFINITY),
                    );
                    if modal.search.is_empty() && ui.ctx().input(|i| i.time < 0.1) {
                        resp.request_focus();
                    }
                });

                ui.separator();

                let search_lower = modal.search.to_lowercase();
                let tree_height = ui.available_height() - 140.0;
                egui::ScrollArea::vertical()
                    .max_height(tree_height)
                    .show(ui, |ui| {
                        render_node_hierarchy(
                            ui,
                            node_hierarchy(),
                            &search_lower,
                            &mut modal.selected_type,
                            &mut modal.name,
                            &mut create,
                        );
                    });

                ui.separator();

                // Description card
                let desc = find_description(node_hierarchy(), &modal.selected_type)
                    .unwrap_or("Engine scene node.");
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(16, 20, 24))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        let icon = icons::node_type_icon(&modal.selected_type);
                        let color = icons::node_type_color(&modal.selected_type);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(icon.as_str()).color(color).size(16.0));
                            ui.label(egui::RichText::new(&modal.selected_type).strong());
                        });
                        ui.label(egui::RichText::new(desc).weak().size(12.0));
                    });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Node Name:");
                    let resp = ui.text_edit_singleline(&mut modal.name);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        create = true;
                    }
                });

                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        create = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if res.is_none() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }

        if create && !modal.selected_type.is_empty() && !modal.name.is_empty() {
            let parent = modal
                .parent
                .clone()
                .or_else(|| self.selected.clone().map(NodeId::from))
                .or_else(|| self.session.host().with_tree(|t| t.root().cloned()));

            match self.session.host().with_tree_mut(|t| {
                ops::add_node(t, parent.clone(), &modal.selected_type, &modal.name)
            }) {
                Ok(new_id) => {
                    self.selected = Some(new_id.to_string());
                    if let Some(p) = &parent {
                        self.open_nodes.insert(p.to_string());
                    }
                    self.viewport_dirty = true;
                    self.log(format!("Added node {} ({})", new_id, modal.selected_type));
                    close = true;
                }
                Err(e) => self.log(format!("Failed to add node: {e:#}")),
            }
        }

        if close {
            self.add_node_modal = None;
        } else {
            self.add_node_modal = Some(modal);
        }
    }

    fn show_rename_node_modal(&mut self, ctx: &egui::Context) {
        let Some(mut modal) = self.rename_node_modal.clone() else {
            return;
        };
        let mut close = false;
        let mut rename = false;

        let res = egui::Window::new("Rename Node")
            .id(egui::Id::unique("rename_node_modal_window"))
            .default_size(egui::vec2(280.0, 120.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let resp = ui.text_edit_singleline(&mut modal.name);
                    resp.request_focus();
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        rename = true;
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        rename = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if res.is_none() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }

        if rename && !modal.name.trim().is_empty() {
            match self
                .session
                .host()
                .with_tree_mut(|t| ops::rename_node(t, &modal.id, &modal.name))
            {
                Ok(()) => {
                    self.viewport_dirty = true;
                    self.log(format!("Renamed node {} to {}", modal.id, modal.name));
                    close = true;
                }
                Err(e) => self.log(format!("Failed to rename node: {e:#}")),
            }
        }

        if close {
            self.rename_node_modal = None;
        } else {
            self.rename_node_modal = Some(modal);
        }
    }

    fn show_instance_scene_modal(&mut self, ctx: &egui::Context) {
        let Some(modal) = self.instance_modal.clone() else {
            return;
        };
        let mut close = false;
        let root = self
            .project_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));

        let mut scenes = Vec::new();
        for file in self.asset_files() {
            if file.extension().and_then(|e| e.to_str()) == Some("ptscn") {
                if let Ok(rel) = file.strip_prefix(&root) {
                    scenes.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }

        let res = egui::Window::new("Instantiate Child Scene")
            .id(egui::Id::unique("instance_scene_modal_window"))
            .default_size(egui::vec2(340.0, 360.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                if scenes.is_empty() {
                    ui.label("No .ptscn scene files found in project.");
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            for sc in scenes {
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 22.0),
                                    egui::Sense::click(),
                                );
                                if resp.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        2.0,
                                        ui.visuals().widgets.hovered.bg_fill,
                                    );
                                }
                                let p = ui.painter();
                                let icon_rect = egui::Rect::from_min_size(
                                    egui::Pos2::new(rect.left() + 4.0, rect.center().y - 8.0),
                                    egui::vec2(16.0, 16.0),
                                );
                                icons::draw_icon(p, icon_rect, Icon::Scene, crate::theme::TEXT);
                                p.text(
                                    egui::Pos2::new(rect.left() + 26.0, rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &sc,
                                    egui::FontId::new(13.0, egui::FontFamily::Proportional),
                                    crate::theme::TEXT,
                                );
                                if resp.double_clicked() || resp.clicked() {
                                    let fs_path =
                                        root.join(sc.replace('/', std::path::MAIN_SEPARATOR_STR));
                                    let parent = modal
                                        .parent
                                        .clone()
                                        .or_else(|| self.selected.clone().map(NodeId::from))
                                        .or_else(|| {
                                            self.session.host().with_tree(|t| t.root().cloned())
                                        });
                                    match self.session.host().with_tree_mut(|t| {
                                        ops::instantiate_scene_as_child(t, parent, &fs_path)
                                    }) {
                                        Ok(new_root) => {
                                            self.selected = Some(new_root.to_string());
                                            self.viewport_dirty = true;
                                            self.log(format!(
                                                "Instantiated scene res://{sc} as {new_root}"
                                            ));
                                            close = true;
                                        }
                                        Err(e) => {
                                            self.log(format!("Failed to instantiate scene: {e:#}"))
                                        }
                                    }
                                }
                            }
                        });
                }
                ui.add_space(8.0);
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });

        if res.is_none() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }

        if close {
            self.instance_modal = None;
        }
    }
}

fn render_node_hierarchy(
    ui: &mut egui::Ui,
    items: &[NodeTypeInfo],
    search: &str,
    selected_type: &mut String,
    name: &mut String,
    create: &mut bool,
) {
    for item in items {
        if !search.is_empty() && !node_type_matches_search(item, search) {
            continue;
        }

        let is_selected = selected_type == item.type_name;
        let has_children = !item.children.is_empty();

        let row_width = ui.available_width();
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(row_width, 22.0), egui::Sense::click());

        if is_selected {
            ui.painter()
                .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
        } else if resp.hovered() {
            ui.painter()
                .rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
        }

        let p = ui.painter();
        let icon = icons::node_type_icon(item.type_name);
        let color = icons::node_type_color(item.type_name);
        let icon_rect = egui::Rect::from_min_size(
            egui::Pos2::new(rect.left() + 4.0, rect.center().y - 8.0),
            egui::vec2(16.0, 16.0),
        );
        icons::draw_icon(p, icon_rect, icon, color);

        p.text(
            egui::Pos2::new(rect.left() + 24.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            item.type_name,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
            crate::theme::TEXT,
        );

        if resp.clicked() {
            *selected_type = item.type_name.to_string();
            *name = item.type_name.to_string();
        }
        if resp.double_clicked() {
            *selected_type = item.type_name.to_string();
            *name = item.type_name.to_string();
            *create = true;
        }

        if has_children {
            ui.indent(item.type_name, |ui| {
                render_node_hierarchy(ui, &item.children, search, selected_type, name, create);
            });
        }
    }
}
