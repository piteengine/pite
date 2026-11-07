// SPDX-License-Identifier: MIT OR Apache-2.0
//! One file per node type. Each module owns its `SCHEMA` and `PROPS`.
//!
//! Abstract classes (`Control`, `CollisionObject2D`, `PhysicsBody2D`) exist
//! so subclasses inherit shared props without redeclaring them; the engine
//! still registers them (for compatibility with existing scenes), but the
//! editor's add-node menu filters them out.

pub mod area2d;
pub mod button;
pub mod camera2d;
pub mod character_body2d;
pub mod collision_object2d;
pub mod collision_shape2d;
pub mod control;
pub mod label;
pub mod node;
pub mod node2d;
pub mod physics_body2d;
pub mod rigid_body2d;
pub mod sprite2d;
pub mod static_body2d;
pub mod timer;

use crate::schema::NodeTypeSchema;

/// Every node schema, concrete and abstract, in a stable order.
/// `pite-core::schema::NODE_SCHEMAS` re-exports this.
pub static ALL_NODES: &[&NodeTypeSchema] = &[
    // Concrete root
    &node::SCHEMA,
    // 2D hierarchy
    &node2d::SCHEMA,
    &control::SCHEMA,
    &collision_object2d::SCHEMA,
    &physics_body2d::SCHEMA,
    // Concrete 2D
    &sprite2d::SCHEMA,
    &camera2d::SCHEMA,
    &collision_shape2d::SCHEMA,
    &area2d::SCHEMA,
    &static_body2d::SCHEMA,
    &rigid_body2d::SCHEMA,
    &character_body2d::SCHEMA,
    // UI
    &label::SCHEMA,
    &button::SCHEMA,
    // Misc
    &timer::SCHEMA,
];
