// SPDX-License-Identifier: MIT OR Apache-2.0
//! Label: draws text at its global position. `font_size` and `color` are
//! inherited from `Control`.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Label",
    description: "Draws text centered on its global position.",
    inherits: Some("Control"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[PropSchema {
    key: "text",
    label: "Text",
    doc: "The string to draw. Empty text draws nothing.",
    default_value: DefaultPropVal::Str(""),
    hint: PropHint::Text,
    read_only: false,
}];
