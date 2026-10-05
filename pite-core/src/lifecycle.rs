// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::node::NodeId;

/// Lifecycle dispatch seam (`_ready`, `_process`, `_draw`).
///
/// Systems call into scripts through this sink; missing methods are
/// no-ops, never errors.
pub trait LifecycleSink {
    fn on_ready(&mut self, _node: &NodeId) {}
    fn on_process(&mut self, _node: &NodeId, _delta: f32) {}
}

/// No-op sink for headless use and tests; the script backend replaces it.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct NoopSink;

impl LifecycleSink for NoopSink {}
