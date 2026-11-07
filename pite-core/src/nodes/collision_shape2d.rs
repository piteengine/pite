// SPDX-License-Identifier: MIT OR Apache-2.0
//! CollisionShape2D: geometric collider attached to a physics body or area.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "CollisionShape2D",
    inherits: Some("Node2D"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[
    PropSchema {
        key: "shape",
        label: "Shape",
        default_value: DefaultPropVal::Str("box"),
        hint: PropHint::Enum(&["box", "circle", "capsule"]),
        read_only: false,
    },
    PropSchema {
        key: "size",
        label: "Size",
        default_value: DefaultPropVal::Vec2(32.0, 32.0),
        hint: PropHint::Vec2 { speed: 1.0 },
        read_only: false,
    },
    PropSchema {
        key: "radius",
        label: "Radius",
        default_value: DefaultPropVal::Num(16.0),
        hint: PropHint::Num {
            speed: 1.0,
            min: 0.1,
            max: 10000.0,
        },
        read_only: false,
    },
    PropSchema {
        key: "height",
        label: "Height",
        default_value: DefaultPropVal::Num(32.0),
        hint: PropHint::Num {
            speed: 1.0,
            min: 0.1,
            max: 10000.0,
        },
        read_only: false,
    },
    PropSchema {
        key: "disabled",
        label: "Disabled",
        default_value: DefaultPropVal::Bool(false),
        hint: PropHint::Bool,
        read_only: false,
    },
];
