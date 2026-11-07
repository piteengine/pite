// SPDX-License-Identifier: MIT OR Apache-2.0
//! Area2D: trigger zones for overlap detection. Inherits
//! collision_layer/collision_mask from CollisionObject2D.

use crate::schema::NodeTypeSchema;

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Area2D",
    inherits: Some("CollisionObject2D"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[crate::schema::PropSchema] = &[];
