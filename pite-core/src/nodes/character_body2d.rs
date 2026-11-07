// SPDX-License-Identifier: MIT OR Apache-2.0
//! CharacterBody2D: kinematic physics body driven by `move_and_slide()`.
//! `collision_layer`/`collision_mask` and `friction`/`restitution` are
//! inherited from `PhysicsBody2D`.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "CharacterBody2D",
    inherits: Some("PhysicsBody2D"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[PropSchema {
    key: "velocity",
    label: "Velocity",
    default_value: DefaultPropVal::Vec2(0.0, 0.0),
    hint: PropHint::Vec2 { speed: 1.0 },
    read_only: false,
}];
