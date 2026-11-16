// SPDX-License-Identifier: MIT OR Apache-2.0
//! Node2D: 2D transform (position, rotation).

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Node2D",
    description: "2D game object with position and rotation.",
    inherits: Some("Node"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[
    PropSchema {
        key: "position",
        label: "Position",
        doc: "Local position in world units, relative to the parent. Global position is this plus every ancestor's.",
        default_value: DefaultPropVal::Vec2(0.0, 0.0),
        hint: PropHint::Vec2 { speed: 1.0 },
        read_only: false,
    },
    PropSchema {
        key: "rotation",
        label: "Rotation",
        doc: "Rotation around the node's origin. Stored in radians; the inspector edits degrees.",
        default_value: DefaultPropVal::Num(0.0),
        hint: PropHint::AngleDeg,
        read_only: false,
    },
    PropSchema {
        key: "z_index",
        label: "Z Index",
        doc: "Draw order index. Nodes with higher values are drawn on top. Ties fall back to tree order.",
        default_value: DefaultPropVal::Int(0),
        hint: PropHint::Int {
            speed: 1.0,
            min: -4096,
            max: 4096,
        },
        read_only: false,
    },
];
