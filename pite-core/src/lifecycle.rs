use crate::node::NodeId;

/// Lifecycle dispatch seam (`_ready`, `_process`, `_draw` in M1b).
///
/// Systems call into scripts through this sink; missing methods are
/// no-ops, never errors.
pub trait LifecycleSink {
    fn on_ready(&mut self, _node: &NodeId) {}
    fn on_process(&mut self, _node: &NodeId, _delta: f32) {}
}

/// No-op sink for headless use and tests (M1b wires the real backend).
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct NoopSink;

impl LifecycleSink for NoopSink {}
