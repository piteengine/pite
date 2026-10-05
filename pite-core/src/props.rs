// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::HashMap;

/// Typed prop values. New props don't change the tree struct (§12).
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Str(String),
    Num(f64),
    Int(i64),
    Bool(bool),
    Vec2(f64, f64),
}

/// String→value prop map with typed accessors. Unknown props preserved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Props(pub HashMap<String, PropValue>);

impl Props {
    pub fn get(&self, key: &str) -> Option<&PropValue> {
        self.0.get(key)
    }

    pub fn insert(&mut self, key: impl Into<String>, value: PropValue) {
        self.0.insert(key.into(), value);
    }
}
