use crate::math::Rect;
use crate::node::NodeId;

/// Physics seam (excluded in M1). Single stub; real physics later
/// replaces the body, callers don't change (§12).
pub fn query_overlap(_rect: Rect) -> Vec<NodeId> {
    Vec::new()
}
