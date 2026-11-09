// SPDX-License-Identifier: MIT OR Apache-2.0
//! Node lifecycle dispatch trait (`_ready`, `_process`).

use crate::node::NodeId;

/// Lifecycle dispatch interface through which engine systems invoke script lifecycle hooks.
///
/// Missing lifecycle callbacks are treated as no-ops rather than errors.
pub trait LifecycleSink {
    /// Invoked once when a node and its children have entered the active scene tree.
    fn on_ready(&mut self, _node: &NodeId) {}

    /// Invoked every frame during the main update tick with the elapsed frame delta time in seconds.
    fn on_process(&mut self, _node: &NodeId, _delta: f32) {}
}

/// No-op lifecycle sink used in headless simulation, tests, or before a scripting host is attached.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct NoopSink;

impl LifecycleSink for NoopSink {}
