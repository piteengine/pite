// SPDX-License-Identifier: MIT OR Apache-2.0
//! Abstract base for solid bodies: friction, restitution.
//! `Area2D` inherits `CollisionObject2D` directly (sensors have no friction).

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "PhysicsBody2D",
    description:
        "Abstract base for solid bodies: friction and restitution. Sensors (Area2D) skip it.",
    inherits: Some("CollisionObject2D"),
    abstract_class: true,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[
    PropSchema {
        key: "friction",
        label: "Friction",
        doc: "Coulomb friction coefficient applied to contacts. 0 = ice, 0.5 = wood, 1+ = sticky.",
        default_value: DefaultPropVal::Num(0.5),
        hint: PropHint::Num {
            speed: 0.05,
            min: 0.0,
            max: 2.0,
        },
        read_only: false,
    },
    PropSchema {
        key: "restitution",
        label: "Restitution",
        doc: "Bounciness of collisions: 0 absorbs the impact, 1 bounces back with equal speed.",
        default_value: DefaultPropVal::Num(0.0),
        hint: PropHint::Num {
            speed: 0.05,
            min: 0.0,
            max: 1.0,
        },
        read_only: false,
    },
];
