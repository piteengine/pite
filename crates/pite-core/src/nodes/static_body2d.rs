// SPDX-License-Identifier: MIT OR Apache-2.0
//! StaticBody2D: immobile physics body. `collision_layer`/`collision_mask`
//! come from `CollisionObject2D`, `friction`/`restitution` from `PhysicsBody2D`.

use crate::schema::{NodeTypeSchema, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "StaticBody2D",
    description: "Immovable physics body: floors, walls, platforms. Never responds to impulses.",
    inherits: Some("PhysicsBody2D"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[];
