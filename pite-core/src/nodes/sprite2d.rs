// SPDX-License-Identifier: MIT OR Apache-2.0
//! Sprite2D: draws a texture or one atlas frame at its global position.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Sprite2D",
    inherits: Some("Node2D"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[
    PropSchema {
        key: "texture",
        label: "Texture",
        default_value: DefaultPropVal::Str(""),
        hint: PropHint::Asset {
            filter: &["png", "jpg", "jpeg", "webp"],
        },
        read_only: false,
    },
    PropSchema {
        key: "atlas",
        label: "Atlas",
        default_value: DefaultPropVal::Str(""),
        hint: PropHint::Asset {
            filter: &["atlas.json", "json"],
        },
        read_only: false,
    },
    PropSchema {
        key: "frame",
        label: "Frame",
        default_value: DefaultPropVal::Str(""),
        hint: PropHint::Text,
        read_only: false,
    },
];
