// SPDX-License-Identifier: MIT OR Apache-2.0
//! Camera2D: frames the view on its global position.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Camera2D",
    inherits: Some("Node2D"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[PropSchema {
    key: "zoom",
    label: "Zoom",
    default_value: DefaultPropVal::Vec2(1.0, 1.0),
    hint: PropHint::Vec2 { speed: 0.05 },
    read_only: false,
}];
