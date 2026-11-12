// SPDX-License-Identifier: MIT OR Apache-2.0
//! TileMap2D: renders 2D tilemaps defined in an external .ptmap TOML sidecar.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "TileMap2D",
    description: "Renders 2D multi-layer grid tilemaps with automatic physics colliders.",
    inherits: Some("Node2D"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[PropSchema {
    key: "tilemap",
    label: "TileMap",
    doc: "Path to a tilemap sidecar (`.ptmap`).",
    default_value: DefaultPropVal::Str(""),
    hint: PropHint::Asset { filter: &["ptmap"] },
    read_only: false,
}];
