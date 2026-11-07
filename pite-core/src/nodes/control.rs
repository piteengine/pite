// SPDX-License-Identifier: MIT OR Apache-2.0
//! Abstract base for UI elements: position, size, color, font size.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Control",
    inherits: Some("Node"),
    abstract_class: true,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[
    PropSchema {
        key: "position",
        label: "Position",
        default_value: DefaultPropVal::Vec2(0.0, 0.0),
        hint: PropHint::Vec2 { speed: 1.0 },
        read_only: false,
    },
    PropSchema {
        key: "size",
        label: "Size",
        default_value: DefaultPropVal::Vec2(100.0, 40.0),
        hint: PropHint::Vec2 { speed: 1.0 },
        read_only: false,
    },
    PropSchema {
        key: "color",
        label: "Color",
        default_value: DefaultPropVal::Color([255, 255, 255, 255]),
        hint: PropHint::Color,
        read_only: false,
    },
    PropSchema {
        key: "font_size",
        label: "Font Size",
        default_value: DefaultPropVal::Num(16.0),
        hint: PropHint::Num {
            speed: 1.0,
            min: 6.0,
            max: 128.0,
        },
        read_only: false,
    },
];
