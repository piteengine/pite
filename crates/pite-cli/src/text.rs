// SPDX-License-Identifier: MIT OR Apache-2.0
//! Styled text for human-facing output.
//!
//! A message is a sequence of [`Span`]s: plain prose plus the few pieces that
//! should stand out (paths bold, identifiers cyan, secondary context dim).
//! Styling is stored next to the text, never encoded in it, so the same message
//! renders to ANSI on a terminal and to plain text in JSON.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ui::Colors;

/// How one span is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Plain,
    /// Bold, for paths and commands.
    Strong,
    /// Cyan, for identifiers such as flags, keys, and file names.
    Info,
    /// Dim, for parenthetical or secondary context.
    Faded,
}

/// One run of text with a single style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub style: Style,
    pub text: String,
}

/// A message built from styled spans.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spans(Vec<Span>);

impl Spans {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Convenience for an unstyled message.
    pub fn text(text: impl Into<String>) -> Self {
        Self::new().push(Style::Plain, text)
    }

    fn push(mut self, style: Style, text: impl Into<String>) -> Self {
        let text = text.into();
        if !text.is_empty() {
            self.0.push(Span { style, text });
        }
        self
    }

    pub fn plain(self, text: impl Into<String>) -> Self {
        self.push(Style::Plain, text)
    }

    pub fn strong(self, text: impl Into<String>) -> Self {
        self.push(Style::Strong, text)
    }

    pub fn info(self, text: impl Into<String>) -> Self {
        self.push(Style::Info, text)
    }

    pub fn faded(self, text: impl Into<String>) -> Self {
        self.push(Style::Faded, text)
    }

    /// ANSI rendering; empty strings when colors are disabled.
    pub fn render(&self, c: &Colors) -> String {
        let mut out = String::with_capacity(self.len() + 8);
        for span in &self.0 {
            let (open, close) = match span.style {
                Style::Plain => ("", ""),
                Style::Strong => (c.bold, c.reset),
                Style::Info => (c.bold_cyan, c.reset),
                Style::Faded => (c.dim, c.reset),
            };
            out.push_str(open);
            out.push_str(&span.text);
            out.push_str(close);
        }
        out
    }

    fn len(&self) -> usize {
        self.0.iter().map(|span| span.text.len()).sum()
    }
}

impl fmt::Display for Spans {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for span in &self.0 {
            f.write_str(&span.text)?;
        }
        Ok(())
    }
}

impl From<&str> for Spans {
    fn from(text: &str) -> Self {
        Self::text(text)
    }
}

impl From<String> for Spans {
    fn from(text: String) -> Self {
        Self::text(text)
    }
}

impl From<&String> for Spans {
    fn from(text: &String) -> Self {
        Self::text(text)
    }
}

impl From<&Spans> for Spans {
    fn from(spans: &Spans) -> Self {
        spans.clone()
    }
}

/// JSON carries the plain text only.
impl Serialize for Spans {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Spans {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::text)
    }
}

/// Uppercase the first character, leaving the rest untouched.
///
/// Engine messages are written as sentences in lowercase ("downloading …")
/// and capitalized where they become a standalone line.
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{COLOR_DISABLED, COLOR_ENABLED};

    #[test]
    fn capitalize_only_touches_the_first_character() {
        assert_eq!(capitalize("downloading files"), "Downloading files");
        assert_eq!(capitalize("Already"), "Already");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn plain_rendering_has_no_escapes() {
        let spans = Spans::new()
            .plain("Asset ")
            .strong("res://a.png")
            .plain(" is missing.");
        assert_eq!(spans.to_string(), "Asset res://a.png is missing.");
        assert_eq!(spans.render(&COLOR_DISABLED), spans.to_string());
    }

    #[test]
    fn styles_map_to_colors() {
        let spans = Spans::new()
            .strong("path")
            .plain(" ")
            .info("--flag")
            .plain(" ")
            .faded("note");
        let rendered = spans.render(&COLOR_ENABLED);
        assert!(rendered.contains("\u{1b}[1mpath\u{1b}[0m"));
        assert!(rendered.contains("\u{1b}[1;36m--flag\u{1b}[0m"));
        assert!(rendered.contains("\u{1b}[2mnote\u{1b}[0m"));
    }

    #[test]
    fn json_is_the_plain_sentence() {
        let spans = Spans::new().plain("Run ").info("pite new").plain(".");
        let json = serde_json::to_string(&spans).unwrap();
        assert_eq!(json, "\"Run pite new.\"");
        let back: Spans = serde_json::from_str(&json).unwrap();
        assert_eq!(back.to_string(), "Run pite new.");
    }

    #[test]
    fn empty_segments_are_dropped() {
        let spans = Spans::new().plain("a").plain("").strong("b");
        assert_eq!(spans.to_string(), "ab");
    }
}
