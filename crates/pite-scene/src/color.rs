// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hex color encoding for the scene file format.
//!
//! Accepted input shapes: `#RGB`, `#RRGGBB`, `#RRGGBBAA` (case-insensitive,
//! `#` optional). Canonical output is lowercase `#rrggbbaa` (always 8 digits).
//!
//! The `Color` type itself is bare (`pite-core::Color`); this module owns
//! the scene-file representation, sitting alongside `toml_to_prop` and
//! `prop_to_toml`.

use pite_core::Color;

/// Parse `#RGB`, `#RRGGBB`, or `#RRGGBBAA` (case-insensitive, `#` optional).
/// Returns `None` for other lengths or non-hex characters.
pub fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    let nibble = |i: usize| u8::from_str_radix(hex.get(i..i + 1)?, 16).ok();
    match hex.len() {
        3 => Some(Color::new(
            nibble(0)? * 17,
            nibble(1)? * 17,
            nibble(2)? * 17,
            255,
        )),
        6 => Some(Color::new(byte(0)?, byte(2)?, byte(4)?, 255)),
        8 => Some(Color::new(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

/// Canonical lowercase `#rrggbbaa` (always 8 digits, alpha included).
pub fn format_hex_color(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        assert_eq!(parse_hex_color("#ff0000"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(parse_hex_color("#f00"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(parse_hex_color("#00000000"), Some(Color::TRANSPARENT));
        assert_eq!(parse_hex_color("junk"), None);
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(format_hex_color(Color::rgb(255, 0, 0)), "#ff0000ff");
        assert_eq!(format_hex_color(Color::TRANSPARENT), "#00000000");
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(parse_hex_color("#FF00AA"), Some(Color::rgb(255, 0, 170)));
        assert_eq!(parse_hex_color("#ff00aa"), Some(Color::rgb(255, 0, 170)));
    }
}
