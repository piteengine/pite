// SPDX-License-Identifier: MIT OR Apache-2.0
//! Core scene-graph primitives for the Pite game engine: node handles, the
//! hierarchical scene tree, properties, the node-type registry, signals, and
//! lifecycle dispatch.
//!
//! Defines foundational abstractions of the scene model with no direct dependencies
//! on Python scripting (`pite-script`), rendering (`pite-render`), or scene file
//! serialization (`pite-scene`). External subsystems plug in through the traits
//! re-exported here to maintain modular architectural boundaries.

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
pub use props::{Color, PropValue, Props, CUSTOM_PROP_TYPES};
pub use registry::{NodeFactory, NodeTypeRegistry};
pub use schema::{
    all_props, inheritance_chain, is_valid_custom_prop_type, node_type_schema, type_props,
    validate_custom_prop_type, DefaultPropVal, NodeTypeSchema, PropHint, PropSchema, NODE_SCHEMAS,
};
pub use signals::{SignalDef, SignalRegistry};
pub use tree::NodeTree;
