// SPDX-License-Identifier: MIT OR Apache-2.0
//! Camera2D: frames the view on its global position.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Camera2D",
    description: "Frames the view: the screen centers on this node's global position.",
    inherits: Some("Node2D"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[PropSchema {
    key: "zoom",
    label: "Zoom",
    doc: "View scale; larger values show less of the world. Multiplied by the window fit scale.",
    default_value: DefaultPropVal::Num(1.0),
    hint: PropHint::Num {
        speed: 0.05,
        min: 0.01,
        max: 100.0,
    },
    read_only: false,
}];
