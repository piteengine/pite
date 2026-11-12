// SPDX-License-Identifier: MIT OR Apache-2.0
//! AnimatedSprite2D: plays 2D flipbook animations from a spritesheet atlas.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "AnimatedSprite2D",
    description: "Plays 2D flipbook animations from a spritesheet atlas.",
    inherits: Some("Node2D"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[
    PropSchema {
        key: "atlas",
        label: "Atlas",
        doc: "Path to a spritesheet sidecar (`.atlas.json`).",
        default_value: DefaultPropVal::Str(""),
        hint: PropHint::Asset {
            filter: &["atlas.json", "json"],
        },
        read_only: false,
    },
    PropSchema {
        key: "animation",
        label: "Animation",
        doc: "Named animation sequence inside the atlas.",
        default_value: DefaultPropVal::Str(""),
        hint: PropHint::Text,
        read_only: false,
    },
    PropSchema {
        key: "frame",
        label: "Frame",
        doc: "Current frame index of the playing animation.",
        default_value: DefaultPropVal::Int(0),
        hint: PropHint::Int {
            speed: 1.0,
            min: 0,
            max: 9999,
        },
        read_only: false,
    },
    PropSchema {
        key: "speed_scale",
        label: "Speed Scale",
        doc: "Playback speed multiplier for the animation.",
        default_value: DefaultPropVal::Num(1.0),
        hint: PropHint::Num {
            speed: 0.1,
            min: -100.0,
            max: 100.0,
        },
        read_only: false,
    },
    PropSchema {
        key: "playing",
        label: "Playing",
        doc: "Whether the animation is currently playing.",
        default_value: DefaultPropVal::Bool(false),
        hint: PropHint::Bool,
        read_only: false,
    },
    PropSchema {
        key: "autoplay",
        label: "Autoplay",
        doc: "Animation name to begin playing automatically on ready.",
        default_value: DefaultPropVal::Str(""),
        hint: PropHint::Text,
        read_only: false,
    },
];
