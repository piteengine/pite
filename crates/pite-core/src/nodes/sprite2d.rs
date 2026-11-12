// SPDX-License-Identifier: MIT OR Apache-2.0
//! Sprite2D: draws a texture or one atlas frame at its global position.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Sprite2D",
    description: "Displays a 2D texture or one frame from a spritesheet atlas.",
    inherits: Some("Node2D"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[
    PropSchema {
        key: "texture",
        label: "Texture",
        doc: "Path to a png/jpg/webp image to draw. Mutually exclusive with `atlas`.",
        default_value: DefaultPropVal::Str(""),
        hint: PropHint::Asset {
            filter: &["png", "jpg", "jpeg", "webp"],
        },
        read_only: false,
    },
    PropSchema {
        key: "atlas",
        label: "Atlas",
        doc: "Path to a spritesheet sidecar (`.atlas.json`). Requires `frame`.",
        default_value: DefaultPropVal::Str(""),
        hint: PropHint::Asset {
            filter: &["atlas.json", "json"],
        },
        read_only: false,
    },
    PropSchema {
        key: "frame",
        label: "Frame",
        doc: "Named frame inside the atlas. Ignored without `atlas`.",
        default_value: DefaultPropVal::Str(""),
        hint: PropHint::Text,
        read_only: false,
    },
];
