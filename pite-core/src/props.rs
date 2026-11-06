// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::HashMap;

/// An sRGB color with 8-bit channels. Alpha is straight (unmultiplied).
///
/// Bare value struct — no serialization helpers. Hex parsing and formatting
/// live in `pite-scene` (the format layer, alongside TOML encode/decode).
/// Python gets a proxy type in `pite-script` with host-tied mutation, matching
/// how `Vec2` is handled (`pite-core::Vec2` here, `pite-script::Vec2` there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

impl From<[u8; 4]> for Color {
    fn from(v: [u8; 4]) -> Self {
        Self {
            r: v[0],
            g: v[1],
            b: v[2],
            a: v[3],
        }
    }
}

impl From<Color> for [u8; 4] {
    fn from(c: Color) -> Self {
        [c.r, c.g, c.b, c.a]
    }
}

/// Typed prop values. New props don't change the tree struct (§12).
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Str(String),
    Num(f64),
    Int(i64),
    Bool(bool),
    Vec2(f64, f64),
    Color(u8, u8, u8, u8),
}

impl PropValue {
    /// Typed accessor; `None` unless this is a `Color` variant.
    pub fn as_color(&self) -> Option<Color> {
        match self {
            PropValue::Color(r, g, b, a) => Some(Color::new(*r, *g, *b, *a)),
            _ => None,
        }
    }
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
