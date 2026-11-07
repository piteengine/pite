// SPDX-License-Identifier: MIT OR Apache-2.0
//! Timer: fires its engine-owned `timeout` signal every `wait_time` seconds
//! (looping; non-positive never fires).

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Timer",
    inherits: Some("Node"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[PropSchema {
    key: "wait_time",
    label: "Wait Time",
    default_value: DefaultPropVal::Num(1.0),
    hint: PropHint::Num {
        speed: 0.1,
        min: 0.01,
        max: 3600.0,
    },
    read_only: false,
}];
