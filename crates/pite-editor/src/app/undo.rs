// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed inverse-command undo/redo history for the editor.

use std::path::PathBuf;

use anyhow::Result;
use pite_core::{NodeDesc, NodeId, NodeTree, PropValue, Props, ScriptRef};
use pite_render::atlas::Atlas;

use crate::ops;

/// One node captured inside a [`SubtreeSnapshot`].
#[derive(Debug, Clone)]
pub(super) struct CapturedNode {
    pub(super) id: NodeId,
    pub(super) name: String,
    pub(super) type_name: String,
    pub(super) parent: Option<NodeId>,
    pub(super) props: Props,
    pub(super) script: Option<ScriptRef>,
}

/// Pre-order snapshot of a node subtree and its attachment slot under its parent.
#[derive(Debug, Clone)]
pub(super) struct SubtreeSnapshot {
    pub(super) root_id: NodeId,
    pub(super) parent: Option<NodeId>,
    pub(super) sibling_index: usize,
    pub(super) nodes: Vec<CapturedNode>,
}

/// Capture `root_id` and all of its descendants in pre-order (preserving child order).
pub(super) fn capture_subtree(tree: &NodeTree, root_id: &NodeId) -> Option<SubtreeSnapshot> {
    let root = tree.get(root_id)?;
    let parent = root.parent.clone();
    let sibling_index = parent
        .as_ref()
        .and_then(|p| tree.children_of(p).iter().position(|c| c == root_id))
        .unwrap_or(0);

    let mut nodes = Vec::new();
    let mut stack = vec![root_id.clone()];
    while let Some(id) = stack.pop() {
        let Some(node) = tree.get(&id) else {
            continue;
        };
        nodes.push(CapturedNode {
            id: node.id.clone(),
            name: node.name.clone(),
            type_name: node.type_name.clone(),
            parent: node.parent.clone(),
            props: node.props.clone(),
            script: node.script.clone(),
        });
        for child in node.children.iter().rev() {
            stack.push(child.clone());
        }
    }

    Some(SubtreeSnapshot {
        root_id: root_id.clone(),
        parent,
        sibling_index,
        nodes,
    })
}

/// Re-insert a previously captured subtree into `tree` at its original sibling index.
pub(super) fn restore_subtree(tree: &mut NodeTree, snap: &SubtreeSnapshot) -> Result<()> {
    for node in &snap.nodes {
        let desc = NodeDesc {
            id: node.id.clone(),
            name: node.name.clone(),
            type_name: node.type_name.clone(),
            parent: node.parent.clone(),
            props: node.props.clone(),
            script: node.script.clone(),
        };
        tree.insert(desc).map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    if let Some(parent) = &snap.parent {
        ops::place_node(tree, &snap.root_id, parent, snap.sibling_index)?;
    }
    Ok(())
}

/// Single cell change within a [`UndoCommand::TileMapStroke`].
#[derive(Debug, Clone)]
pub(super) struct TileCellChange {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) old: Option<String>,
    pub(super) new: Option<String>,
}

/// Typed inverse command for every non-code-editor mutation.
#[derive(Debug, Clone)]
pub(super) enum UndoCommand {
    SetProp {
        id: NodeId,
        key: String,
        old: Option<PropValue>,
        new: Option<PropValue>,
        coalesce_key: Option<String>,
    },
    RenameNode {
        id: NodeId,
        old_name: String,
        new_name: String,
    },
    MoveNode {
        id: NodeId,
        old_parent: Option<NodeId>,
        old_index: usize,
        new_parent: Option<NodeId>,
        new_index: usize,
    },
    InsertSubtree {
        snapshot: SubtreeSnapshot,
    },
    RemoveSubtree {
        snapshot: SubtreeSnapshot,
    },
    TileMapStroke {
        node_id: String,
        path: PathBuf,
        layer: usize,
        cells: Vec<TileCellChange>,
        coalesce_key: Option<String>,
    },
    AtlasEdit {
        path: PathBuf,
        old: Atlas,
        new: Atlas,
        coalesce_key: Option<String>,
    },
}

impl UndoCommand {
    /// Human-readable short label for the `Edit` menu.
    pub(super) fn label(&self) -> String {
        match self {
            Self::SetProp { key, .. } => format!("Edit {key}"),
            Self::RenameNode { new_name, .. } => format!("Rename to {new_name}"),
            Self::MoveNode { id, .. } => format!("Move {id}"),
            Self::InsertSubtree { snapshot } => format!("Add {}", snapshot.root_id),
            Self::RemoveSubtree { snapshot } => format!("Delete {}", snapshot.root_id),
            Self::TileMapStroke { node_id, .. } => format!("Paint {node_id}"),
            Self::AtlasEdit { .. } => "Edit Atlas".to_string(),
        }
    }

    fn coalesce_key(&self) -> Option<&str> {
        match self {
            Self::SetProp { coalesce_key, .. }
            | Self::TileMapStroke { coalesce_key, .. }
            | Self::AtlasEdit { coalesce_key, .. } => coalesce_key.as_deref(),
            _ => None,
        }
    }

    /// Try merging `incoming` into `self` when both share the same active coalesce key.
    fn try_merge(&mut self, incoming: &Self) -> bool {
        match (self, incoming) {
            (
                Self::SetProp {
                    id: id_a,
                    key: key_a,
                    new: new_a,
                    coalesce_key: Some(ca),
                    ..
                },
                Self::SetProp {
                    id: id_b,
                    key: key_b,
                    new: new_b,
                    coalesce_key: Some(cb),
                    ..
                },
            ) if ca == cb && id_a == id_b && key_a == key_b => {
                *new_a = new_b.clone();
                true
            }
            (
                Self::TileMapStroke {
                    node_id: na,
                    layer: la,
                    cells: cells_a,
                    coalesce_key: Some(ca),
                    ..
                },
                Self::TileMapStroke {
                    node_id: nb,
                    layer: lb,
                    cells: cells_b,
                    coalesce_key: Some(cb),
                    ..
                },
            ) if ca == cb && na == nb && la == lb => {
                for b in cells_b {
                    if let Some(existing) = cells_a.iter_mut().find(|c| c.x == b.x && c.y == b.y) {
                        existing.new = b.new.clone();
                    } else {
                        cells_a.push(b.clone());
                    }
                }
                true
            }
            (
                Self::AtlasEdit {
                    path: pa,
                    new: new_a,
                    coalesce_key: Some(ca),
                    ..
                },
                Self::AtlasEdit {
                    path: pb,
                    new: new_b,
                    coalesce_key: Some(cb),
                    ..
                },
            ) if ca == cb && pa == pb => {
                *new_a = new_b.clone();
                true
            }
            _ => false,
        }
    }
}

/// Bounded undo/redo stack with drag/stroke coalescing and save-point tracking.
#[derive(Debug, Default)]
pub(super) struct UndoHistory {
    undo_stack: Vec<UndoCommand>,
    redo_stack: Vec<UndoCommand>,
    saved_index: usize,
    active_coalesce: Option<String>,
}

impl UndoHistory {
    pub(super) fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.saved_index = 0;
        self.active_coalesce = None;
    }

    pub(super) fn mark_saved(&mut self) {
        self.saved_index = self.undo_stack.len();
    }

    #[cfg(test)]
    pub(super) fn is_dirty(&self) -> bool {
        self.undo_stack.len() != self.saved_index
    }

    pub(super) fn begin_coalesce(&mut self, key: impl Into<String>) {
        self.active_coalesce = Some(key.into());
    }

    pub(super) fn end_coalesce(&mut self) {
        self.active_coalesce = None;
    }

    pub(super) fn push(&mut self, cmd: UndoCommand) {
        self.redo_stack.clear();
        if let (Some(active), Some(incoming_key)) = (&self.active_coalesce, cmd.coalesce_key()) {
            if active == incoming_key {
                if let Some(top) = self.undo_stack.last_mut() {
                    if top.try_merge(&cmd) {
                        return;
                    }
                }
            }
        }
        if self.saved_index > self.undo_stack.len() {
            self.saved_index = usize::MAX;
        }
        self.undo_stack.push(cmd);
    }

    pub(super) fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub(super) fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub(super) fn undo_label(&self) -> Option<String> {
        self.undo_stack.last().map(UndoCommand::label)
    }

    pub(super) fn redo_label(&self) -> Option<String> {
        self.redo_stack.last().map(UndoCommand::label)
    }

    pub(super) fn pop_undo(&mut self) -> Option<UndoCommand> {
        self.active_coalesce = None;
        self.undo_stack.pop()
    }

    pub(super) fn push_redo(&mut self, cmd: UndoCommand) {
        self.redo_stack.push(cmd);
    }

    pub(super) fn pop_redo(&mut self) -> Option<UndoCommand> {
        self.active_coalesce = None;
        self.redo_stack.pop()
    }

    pub(super) fn push_undo_without_clearing_redo(&mut self, cmd: UndoCommand) {
        self.undo_stack.push(cmd);
    }
}
