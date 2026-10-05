// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::math::Rect;
use crate::node::NodeId;

/// Physics seam. Single stub; real physics later replaces the body and
/// callers do not change.
pub fn query_overlap(_rect: Rect) -> Vec<NodeId> {
    Vec::new()
}
