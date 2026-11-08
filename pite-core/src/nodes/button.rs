// SPDX-License-Identifier: MIT OR Apache-2.0
//! Button: clickable UI element. Overrides `Control`'s `color` (slate
//! background instead of white) and `size` (120×40 instead of 100×40);
//! adds `text` and `disabled`.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Button",
    description: "Clickable rectangle with a caption. Fires the engine-owned `pressed` signal.",
    inherits: Some("Control"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[
    PropSchema {
        key: "text",
        label: "Text",
        doc: "Caption drawn centered on the button.",
        default_value: DefaultPropVal::Str(""),
        hint: PropHint::Text,
        read_only: false,
    },
    PropSchema {
        key: "color",
        label: "Color",
        doc: "Fill color as sRGB hex with alpha, e.g. `#334155ff`.",
        default_value: DefaultPropVal::Color([51, 65, 85, 255]),
        hint: PropHint::Color,
        read_only: false,
    },
    PropSchema {
        key: "size",
        label: "Size",
        doc: "Hit rectangle size in world units; both rendering and click hit-testing use it.",
        default_value: DefaultPropVal::Vec2(120.0, 40.0),
        hint: PropHint::Vec2 { speed: 1.0 },
        read_only: false,
    },
    PropSchema {
        key: "disabled",
        label: "Disabled",
        doc: "When true the button draws dimmed and ignores clicks.",
        default_value: DefaultPropVal::Bool(false),
        hint: PropHint::Bool,
        read_only: false,
    },
];
