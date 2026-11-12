// SPDX-License-Identifier: MIT OR Apache-2.0
//! RigidBody2D: dynamic physics body.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "RigidBody2D",
    description: "Dynamic physics body simulated by gravity, impulses, and forces.",
    inherits: Some("PhysicsBody2D"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[
    PropSchema {
        key: "mass",
        label: "Mass",
        doc: "Mass in arbitrary units. Affects how impulses and forces change velocity.",
        default_value: DefaultPropVal::Num(1.0),
        hint: PropHint::Num {
            speed: 0.1,
            min: 0.01,
            max: 1000.0,
        },
        read_only: false,
    },
    PropSchema {
        key: "gravity_scale",
        label: "Gravity Scale",
        doc: "Multiplier on global gravity; negative values fall upward.",
        default_value: DefaultPropVal::Num(1.0),
        hint: PropHint::Num {
            speed: 0.1,
            min: -10.0,
            max: 10.0,
        },
        read_only: false,
    },
    PropSchema {
        key: "lock_rotation",
        label: "Lock Rotation",
        doc: "When true the body cannot rotate in response to contacts.",
        default_value: DefaultPropVal::Bool(false),
        hint: PropHint::Bool,
        read_only: false,
    },
];
