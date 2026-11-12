// SPDX-License-Identifier: MIT OR Apache-2.0
//! Node: base class for every scene object. No transform, no props.

use crate::schema::{NodeTypeSchema, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Node",
    description: "Base class for all scene objects. Does not have a 2D transform.",
    inherits: None,
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[];
