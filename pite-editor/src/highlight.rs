// SPDX-License-Identifier: MIT OR Apache-2.0
//! Single-pass syntax highlighting for the code pane.
//!
//! The `TextEdit` layouter calls [`layout`]; tokenizers are plain byte
//! scans (cuts only land on ASCII bytes, so multibyte text is never split).
//! Colors come from `palette.json` via [`crate::theme`]: keywords are
//! `light`, strings `veryLight`, numbers `primary`, comments `FAINT`.

use std::sync::Arc;

use eframe::egui;

const KEYWORD: egui::Color32 = egui::Color32::from_rgb(0x2b, 0xd3, 0xbf);
const STRING: egui::Color32 = egui::Color32::from_rgb(0xcb, 0xfa, 0xf0);
const NUMBER: egui::Color32 = egui::Color32::from_rgb(0x0b, 0x93, 0x87);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Plain,
    Keyword,
    Str,
    Comment,
    Number,
}

struct Span {
    start: usize,
    end: usize,
    kind: Kind,
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Python keywords plus the receivers every pite script uses.
fn is_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"False"
            | b"None"
            | b"True"
            | b"and"
            | b"as"
            | b"assert"
            | b"async"
            | b"await"
            | b"break"
            | b"case"
            | b"class"
            | b"cls"
            | b"continue"
            | b"def"
            | b"del"
            | b"elif"
            | b"else"
            | b"except"
            | b"finally"
            | b"for"
            | b"from"
            | b"global"
            | b"if"
            | b"import"
            | b"in"
            | b"is"
            | b"lambda"
            | b"match"
            | b"nonlocal"
            | b"not"
            | b"or"
            | b"pass"
            | b"raise"
            | b"return"
            | b"self"
            | b"try"
            | b"while"
            | b"with"
            | b"yield"
    )
}

/// `f`/`r`/`b` combos (any case) directly before a quote start a string.
fn is_string_prefix(word: &[u8]) -> bool {
    matches!(
        word,
        b"f" | b"F"
            | b"r"
            | b"R"
            | b"b"
            | b"B"
            | b"br"
            | b"bR"
            | b"Br"
            | b"BR"
            | b"rb"
            | b"rB"
            | b"Rb"
            | b"RB"
            | b"fr"
            | b"fR"
            | b"Fr"
            | b"FR"
            | b"rf"
            | b"rF"
            | b"Rf"
            | b"RF"
    )
}

/// Scan a quoted string from the opening quote at `i`; returns the end.
/// Backslash escapes are honored unless the string is raw.
fn scan_string(text: &[u8], i: usize, raw: bool) -> usize {
    let quote = text[i];
    let triple = text.get(i + 1) == Some(&quote) && text.get(i + 2) == Some(&quote);
    let mut j = i + if triple { 3 } else { 1 };
    while j < text.len() {
        if !raw && text[j] == b'\\' {
            j += 2;
            continue;
        }
        if text[j] == quote {
            if triple {
                if text.get(j + 1) == Some(&quote) && text.get(j + 2) == Some(&quote) {
                    return j + 3;
                }
            } else {
                return j + 1;
            }
        }
        if !triple && text[j] == b'\n' {
            return j;
        }
        j += 1;
    }
    j
}

fn scan_number(text: &[u8], mut j: usize) -> usize {
    while j < text.len() {
        let c = text[j];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' {
            j += 1;
        } else if (c == b'+' || c == b'-')
            && matches!(text.get(j.wrapping_sub(1)), Some(b'e' | b'E'))
        {
            j += 1;
        } else {
            break;
        }
    }
    j
}

fn flush(spans: &mut Vec<Span>, plain: &mut usize, end: usize) {
    if *plain < end {
        spans.push(Span {
            start: *plain,
            end,
            kind: Kind::Plain,
        });
    }
    *plain = end;
}

fn tokenize_python(text: &[u8]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut i = 0;
    let mut plain = 0;
    while i < text.len() {
        let c = text[i];
        if c == b'#' {
            flush(&mut spans, &mut plain, i);
            let mut j = i;
            while j < text.len() && text[j] != b'\n' {
                j += 1;
            }
            spans.push(Span {
                start: i,
                end: j,
                kind: Kind::Comment,
            });
            i = j;
            plain = j;
        } else if c == b'"' || c == b'\'' {
            flush(&mut spans, &mut plain, i);
            let end = scan_string(text, i, false);
            spans.push(Span {
                start: i,
                end,
                kind: Kind::Str,
            });
            i = end;
            plain = end;
        } else if c.is_ascii_digit()
            || (c == b'.' && text.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            flush(&mut spans, &mut plain, i);
            let end = scan_number(text, i);
            spans.push(Span {
                start: i,
                end,
                kind: Kind::Number,
            });
            i = end;
            plain = end;
        } else if is_word(c) {
            let mut j = i;
            while j < text.len() && is_word(text[j]) {
                j += 1;
            }
            let word = &text[i..j];
            let next = text.get(j);
            if next.is_some_and(|&q| q == b'"' || q == b'\'') && is_string_prefix(word) {
                flush(&mut spans, &mut plain, i);
                let raw = word.iter().any(|&b| b == b'r' || b == b'R');
                let end = scan_string(text, j, raw);
                spans.push(Span {
                    start: i,
                    end,
                    kind: Kind::Str,
                });
                i = end;
                plain = end;
            } else {
                flush(&mut spans, &mut plain, i);
                spans.push(Span {
                    start: i,
                    end: j,
                    kind: if is_keyword(word) {
                        Kind::Keyword
                    } else {
                        Kind::Plain
                    },
                });
                i = j;
                plain = j;
            }
        } else {
            i += 1;
        }
    }
    flush(&mut spans, &mut plain, text.len());
    spans
}

/// Config formats (`toml`/`cfg`/`ini`/`yaml`/`yml`/`json`): strings,
/// comments, numbers, and `[...]` section headers read as keywords.
fn tokenize_config(text: &[u8]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut i = 0;
    let mut plain = 0;
    while i < text.len() {
        let c = text[i];
        if c == b'#' || c == b';' {
            flush(&mut spans, &mut plain, i);
            let mut j = i;
            while j < text.len() && text[j] != b'\n' {
                j += 1;
            }
            spans.push(Span {
                start: i,
                end: j,
                kind: Kind::Comment,
            });
            i = j;
            plain = j;
        } else if c == b'"' || c == b'\'' {
            flush(&mut spans, &mut plain, i);
            let end = scan_string(text, i, false);
            spans.push(Span {
                start: i,
                end,
                kind: Kind::Str,
            });
            i = end;
            plain = end;
        } else if c.is_ascii_digit() {
            flush(&mut spans, &mut plain, i);
            let end = scan_number(text, i);
            spans.push(Span {
                start: i,
                end,
                kind: Kind::Number,
            });
            i = end;
            plain = end;
        } else if c == b'[' {
            let mut j = i + 1;
            while j < text.len() && text[j] != b'\n' && text[j] != b']' {
                j += 1;
            }
            if j < text.len() && text[j] == b']' {
                flush(&mut spans, &mut plain, i);
                spans.push(Span {
                    start: i,
                    end: j + 1,
                    kind: Kind::Keyword,
                });
                i = j + 1;
                plain = j + 1;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    flush(&mut spans, &mut plain, text.len());
    spans
}

/// Build the laid-out galley for editor text: no wrapping (one text row
/// per line keeps the gutter exact), colored by file extension.
pub(crate) fn layout(ui: &egui::Ui, text: &str, ext: &str) -> Arc<egui::Galley> {
    let fid = egui::TextStyle::Monospace.resolve(ui.style());
    let base = ui.visuals().text_color();
    let mut job = egui::text::LayoutJob::simple(String::new(), fid.clone(), base, f32::INFINITY);
    job.sections.clear();
    let spans = match ext {
        "py" => tokenize_python(text.as_bytes()),
        "toml" | "cfg" | "ini" | "yaml" | "yml" | "json" => tokenize_config(text.as_bytes()),
        _ => Vec::new(),
    };
    if spans.is_empty() {
        job.append(text, 0.0, egui::text::TextFormat::simple(fid, base));
    } else {
        for span in &spans {
            let color = match span.kind {
                Kind::Plain => base,
                Kind::Keyword => KEYWORD,
                Kind::Str => STRING,
                Kind::Comment => crate::theme::FAINT,
                Kind::Number => NUMBER,
            };
            job.append(
                &text[span.start..span.end],
                0.0,
                egui::text::TextFormat::simple(fid.clone(), color),
            );
        }
    }
    job.halign = egui::Align::LEFT;
    job.keep_trailing_whitespace = true;
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(lang: fn(&[u8]) -> Vec<Span>, text: &str) -> Vec<(&str, Kind)> {
        lang(text.as_bytes())
            .iter()
            .map(|s| (&text[s.start..s.end], s.kind))
            .collect()
    }

    #[test]
    fn python_keywords_strings_comments_numbers() {
        let got = kinds(
            tokenize_python,
            "def f():\n    # comment\n    return \"s\" + 42\n",
        );
        assert!(got.contains(&("def", Kind::Keyword)));
        assert!(got.contains(&("# comment", Kind::Comment)));
        assert!(got.contains(&("return", Kind::Keyword)));
        assert!(got.contains(&("\"s\"", Kind::Str)));
        assert!(got.contains(&("42", Kind::Number)));
    }

    #[test]
    fn python_hash_inside_string_is_not_a_comment() {
        let got = kinds(tokenize_python, "x = \"a#b\"\n");
        assert!(got.contains(&("\"a#b\"", Kind::Str)));
        assert!(!got.iter().any(|(_, k)| *k == Kind::Comment));
    }

    #[test]
    fn python_triple_quoted_and_prefixed_strings() {
        let got = kinds(
            tokenize_python,
            "a = \"\"\"x\ny\"\"\"\nb = f\"v={v}\"\nc = r\"\\d\"\n",
        );
        assert!(got
            .iter()
            .any(|(s, k)| *k == Kind::Str && s.starts_with("\"\"\"")));
        assert!(got
            .iter()
            .any(|(s, k)| *k == Kind::Str && s.starts_with("f\"")));
        assert!(got
            .iter()
            .any(|(s, k)| *k == Kind::Str && s.starts_with("r\"")));
    }

    #[test]
    fn python_unfinished_string_runs_to_end() {
        let got = kinds(tokenize_python, "x = \"abc");
        assert_eq!(got.last(), Some(&("\"abc", Kind::Str)));
    }

    #[test]
    fn python_floats_and_self() {
        let got = kinds(tokenize_python, "self.x = 1_000.5e-3\n");
        assert!(got.contains(&("self", Kind::Keyword)));
        assert!(got.contains(&("1_000.5e-3", Kind::Number)));
    }

    #[test]
    fn config_sections_strings_comments_numbers() {
        let got = kinds(
            tokenize_config,
            "[project]\nname = \"demo\" # trailing\nversion = 2\n",
        );
        assert!(got.contains(&("[project]", Kind::Keyword)));
        assert!(got.contains(&("\"demo\"", Kind::Str)));
        assert!(got.contains(&("# trailing", Kind::Comment)));
        assert!(got.contains(&("2", Kind::Number)));
    }

    #[test]
    fn highlighter_never_splits_multibyte_text() {
        let text = "x = \"日本語\" # コメント\n";
        let spans = tokenize_python(text.as_bytes());
        let mut pos = 0;
        for s in &spans {
            assert_eq!(s.start, pos);
            assert!(text.is_char_boundary(s.start) && text.is_char_boundary(s.end));
            pos = s.end;
        }
        assert_eq!(pos, text.len());
    }
}
