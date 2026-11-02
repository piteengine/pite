// SPDX-License-Identifier: MIT OR Apache-2.0
use pite_core::NodeId;

#[derive(Debug, Clone, PartialEq)]
pub struct RaycastHit {
    pub point: (f32, f32),
    pub normal: (f32, f32),
    pub distance: f32,
    pub collider_node: NodeId,
    pub body_node: Option<NodeId>,
}
