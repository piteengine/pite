// SPDX-License-Identifier: MIT OR Apache-2.0
//! Node: base class for every scene object. No transform, no props.

use crate::schema::{NodeTypeSchema, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Node",
    inherits: None,
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[];
