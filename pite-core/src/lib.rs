// SPDX-License-Identifier: MIT OR Apache-2.0
//! Core scene-graph primitives: node handles, the node tree, props, the node type
//! registry and lifecycle dispatch.
//!
//! Knows nothing about Python (`pite-script`), rendering (`pite-render`) or the
//! scene file format (`pite-scene`). Those plug in through the traits re-exported
//! here, which is where the crate boundaries are drawn.

#![forbid(unsafe_code)]

mod error;
mod lifecycle;
mod math;
mod node;
pub mod nodes;
mod props;
mod registry;
pub mod schema;
mod signals;
mod tree;

pub use error::{CoreError, Result};
pub use lifecycle::LifecycleSink;
pub use math::{global_position, Vec2};
pub use node::{Node, NodeDesc, NodeId, ScriptRef};
pub use props::{Color, PropValue, Props};
pub use registry::{NodeFactory, NodeTypeRegistry};
pub use schema::{
    inheritance_chain, node_type_schema, type_props, DefaultPropVal, NodeTypeSchema, PropHint,
    PropSchema, NODE_SCHEMAS,
};
pub use signals::{SignalDef, SignalRegistry};
pub use tree::NodeTree;
