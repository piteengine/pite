// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::HashMap;

/// An 8-bit per channel sRGB color with straight (unmultiplied) alpha.
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

/// Dynamically-typed property values supported by the engine's scene and node model.
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    /// String text value.
    Str(String),
    /// Floating-point number.
    Num(f64),
    /// Signed integer value.
    Int(i64),
    /// Boolean flag.
    Bool(bool),
    /// 2D vector coordinate pair `(x, y)`.
    Vec2(f64, f64),
    /// RGBA color represented as 8-bit channels `(r, g, b, a)`.
    Color(u8, u8, u8, u8),
}

impl PropValue {
    /// Returns the value as [`Color`] if this is a [`PropValue::Color`] variant.
    pub fn as_color(&self) -> Option<Color> {
        match self {
            PropValue::Color(r, g, b, a) => Some(Color::new(*r, *g, *b, *a)),
            _ => None,
        }
    }

    /// Returns the canonical type name string (e.g. `"string"`, `"vec2"`), used in diagnostics.
    pub const fn type_name(&self) -> &'static str {
        match self {
            PropValue::Str(_) => "string",
            PropValue::Num(_) => "number",
            PropValue::Int(_) => "int",
            PropValue::Bool(_) => "bool",
            PropValue::Vec2(_, _) => "vec2",
            PropValue::Color(_, _, _, _) => "color",
        }
    }
}

/// Supported type declarations permitted in manifest `[props]` tables.
pub const CUSTOM_PROP_TYPES: &[&str] = &["string", "int", "number", "bool", "vec2", "color"];

/// Key-value property map associated with a [`Node`](crate::Node).
///
/// Unrecognized property keys are safely preserved during deserialization and serialization.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Props(pub HashMap<String, PropValue>);

impl Props {
    /// Retrieves a reference to a property value by key.
    pub fn get(&self, key: &str) -> Option<&PropValue> {
        self.0.get(key)
    }

    /// Inserts or updates a property value by key.
    pub fn insert(&mut self, key: impl Into<String>, value: PropValue) {
        self.0.insert(key.into(), value);
    }
}
