// SPDX-License-Identifier: MIT OR Apache-2.0
//! Abstract base for collision participants: collision_layer, collision_mask.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "CollisionObject2D",
    description: "Abstract base for collision participants: collision_layer and collision_mask.",
    inherits: Some("Node2D"),
    abstract_class: true,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[
    PropSchema {
        key: "collision_layer",
        label: "Layer",
        doc: "Bitmask of physics layers this body belongs to (1 = layer 1, 2 = layer 2, …).",
        default_value: DefaultPropVal::Int(1),
        hint: PropHint::Int {
            speed: 1.0,
            min: 1,
            max: 256,
        },
        read_only: false,
    },
    PropSchema {
        key: "collision_mask",
        label: "Mask",
        doc: "Bitmask of physics layers this body collides with.",
        default_value: DefaultPropVal::Int(1),
        hint: PropHint::Int {
            speed: 1.0,
            min: 1,
            max: 256,
        },
        read_only: false,
    },
];
