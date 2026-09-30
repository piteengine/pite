//! Core scene-graph primitives: node handles, the node tree, props, the node type
//! registry and lifecycle dispatch.
//!
//! Knows nothing about Python (`pite-script`), rendering (`pite-render`) or the
//! scene file format (`pite-scene`). Those plug in through the traits re-exported
//! here (see SCAFFOLD-SPEC §12 seams).

#![forbid(unsafe_code)]

mod error;
mod lifecycle;
mod math;
mod node;
mod physics;
mod props;
mod registry;
mod tree;

pub use error::{CoreError, Result};
pub use lifecycle::LifecycleSink;
pub use math::{Rect, Vec2};
pub use node::{Node, NodeDesc, NodeId, ScriptRef};
pub use physics::query_overlap;
pub use props::{PropValue, Props};
pub use registry::{NodeFactory, NodeTypeRegistry};
pub use tree::NodeTree;