// SPDX-License-Identifier: MIT OR Apache-2.0
//! Timer: fires its engine-owned `timeout` signal every `wait_time` seconds
//! (looping; non-positive never fires). `current_elapsed` is the rolling
//! clock advanced by `PyTimer::_process`; exposed as a normal prop so
//! scripts can inspect or reset it.

use crate::schema::{DefaultPropVal, NodeTypeSchema, PropHint, PropSchema};

pub static SCHEMA: NodeTypeSchema = NodeTypeSchema {
    type_name: "Timer",
    description: "Fires the engine-owned `timeout` signal every `wait_time` seconds.",
    inherits: Some("Node"),
    abstract_class: false,
    props: PROPS,
};

pub static PROPS: &[PropSchema] = &[
    PropSchema {
        key: "wait_time",
        label: "Wait Time",
        doc: "Seconds between `timeout` emissions. Non-positive disables the timer.",
        default_value: DefaultPropVal::Num(1.0),
        hint: PropHint::Num {
            speed: 0.1,
            min: 0.01,
            max: 3600.0,
        },
        read_only: false,
    },
    PropSchema {
        key: "current_elapsed",
        label: "Current Elapsed",
        doc: "Seconds accumulated toward the next fire. Reset to 0 on fire; scripts can reset it manually.",
        default_value: DefaultPropVal::Num(0.0),
        hint: PropHint::Num {
            speed: 0.01,
            min: 0.0,
            max: 3600.0,
        },
        read_only: false,
    },
];
