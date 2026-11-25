// SPDX-License-Identifier: MIT OR Apache-2.0
//! JSON-RPC framing, URI/offset math, and LSP message builders/parsers.

use std::io::BufRead;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

/// Best-effort `file://` URI for a path. Falls back to the raw path text
/// when the current dir is unknown; servers only need it as a document key.
pub fn path_to_uri(path: &Path) -> String {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    uri_from_path_text(&abs.to_string_lossy())
}

/// Build a `file://` URI from an absolute path *string*, on any platform.
/// Windows needs `file:///D:/dir/file.py` — forward slashes and a leading
/// slash before the drive letter. Emitting `file://D:\dir\file.py` (naive
/// concatenation) makes the server report the directory as nonexistent, so
/// the conversion is explicit rather than string-pasted.
pub fn uri_from_path_text(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let mut out = String::with_capacity(normalized.len() + 8);
    out.push_str("file://");
    if !normalized.starts_with('/') {
        out.push('/');
    }
    for c in normalized.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            '%' => out.push_str("%25"),
            other => out.push(other),
        }
    }
    out
}

/// Byte offset of a char offset (egui cursors count chars).
pub fn byte_offset_of_char(text: &str, char_offset: usize) -> usize {
    text.char_indices()
        .map(|(b, _)| b)
        .nth(char_offset)
        .unwrap_or(text.len())
}

/// `(line, character)` for an LSP position. Lines are 0-based;
/// characters count UTF-16 code units per the protocol.
pub fn offset_to_position(text: &str, byte_offset: usize) -> (u32, u32) {
    let at = byte_offset.min(text.len());
    let mut line = 0u32;
    let mut units = 0u32;
    for (b, c) in text.char_indices() {
        if b >= at {
            break;
        }
        if c == '\n' {
            line += 1;
            units = 0;
        } else {
            units += c.len_utf16() as u32;
        }
    }
    (line, units)
}

/// Byte offset of the start of 1-based `line` (clamped to end of text).
pub fn line_start_byte(text: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut seen = 1usize;
    for (b, c) in text.char_indices() {
        if c == '\n' {
            seen += 1;
            if seen >= line {
                return b + 1;
            }
        }
    }
    text.len()
}

/// Start byte offset of the identifier ending at `byte_offset`
/// (letters, digits, `_`); completion replaces from here.
pub fn word_start_before(text: &str, byte_offset: usize) -> usize {
    let mut start = byte_offset.min(text.len());
    while start > 0 {
        match text[..start].chars().next_back() {
            Some(c) if c.is_alphanumeric() || c == '_' => start -= c.len_utf8(),
            _ => break,
        }
    }
    start
}

/// End byte offset of the identifier starting at `byte_offset`
/// (letters, digits, `_`); accepting mid-word replaces up to here.
pub fn word_end_after(text: &str, byte_offset: usize) -> usize {
    let mut end = byte_offset.min(text.len());
    for c in text[end..].chars() {
        if c.is_alphanumeric() || c == '_' {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    end
}

/// Frame a JSON-RPC value with `Content-Length` headers.
pub fn encode_message(body: &Value) -> Vec<u8> {
    let text = serde_json::to_string(body).expect("LSP body serializes");
    let mut out = format!("Content-Length: {}\r\n\r\n", text.len()).into_bytes();
    out.extend_from_slice(text.as_bytes());
    out
}

/// Parse `Content-Length`-framed messages from any buffered reader.
pub struct FramingReader<R: BufRead> {
    inner: R,
}

impl<R: BufRead> FramingReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    pub fn read_message(&mut self) -> Result<Value> {
        let mut len: Option<usize> = None;
        loop {
            let mut line = String::new();
            let n = self
                .inner
                .read_line(&mut line)
                .context("server closed stdio")?;
            if n == 0 {
                anyhow::bail!("server closed stdio");
            }
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                len = Some(rest.trim().parse().context("bad Content-Length")?);
            }
        }
        let len = len.context("message without Content-Length")?;
        let mut buf = vec![0u8; len];
        self.inner
            .read_exact(&mut buf)
            .context("server closed stdio")?;
        serde_json::from_slice(&buf).context("server sent invalid JSON")
    }
}

fn request(id: i64, method: &str, params: Value) -> Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn notification(method: &str, params: Value) -> Value {
    serde_json::json!({"jsonrpc": "2.0", "method": method, "params": params})
}

/// `initialize` carrying the project root. Settings travel separately via
/// `didChangeConfiguration` (and `workspace/configuration` answers).
///
/// `workspaceFolders` is mandatory, not decorative: a server that only sees
/// `rootUri` falls back to a synthetic `<default workspace root>`, finds no
/// files there, and then never re-analyzes an edited document — which is
/// exactly what pyright logs when the root is missing.
pub fn initialize_request(id: i64, root_uri: Option<&str>) -> Value {
    let folders: Value = root_uri
        .map(|uri| serde_json::json!([{"uri": uri, "name": "pite"}]))
        .unwrap_or_else(|| serde_json::json!([]));
    request(
        id,
        "initialize",
        serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "workspaceFolders": folders,
            "capabilities": {
                "workspace": {"configuration": true},
                "textDocument": {"publishDiagnostics": {}},
            },
        }),
    )
}

pub fn initialized_notification() -> Value {
    notification("initialized", serde_json::json!({}))
}

fn python_analysis_settings(stubs_dir: Option<&Path>) -> Value {
    let extra: Vec<String> = stubs_dir
        .map(|d| vec![d.to_string_lossy().into_owned()])
        .unwrap_or_default();
    serde_json::json!({"analysis": {
        "extraPaths": extra,
        "typeCheckingMode": "basic",
    }})
}

/// `extraPaths` + basic type-checking mode so the shipped stubs resolve.
pub fn did_change_configuration(stubs_dir: Option<&Path>) -> Value {
    notification(
        "workspace/didChangeConfiguration",
        serde_json::json!({"settings": {"python": python_analysis_settings(stubs_dir)}}),
    )
}

/// Answer a server `workspace/configuration` pull. The answer is the
/// requested SECTION VALUE (`result[0]` pairs with `items[0]`): for section
/// `"python"` that is the `{"analysis": ...}` object itself — NOT wrapped in
/// another `"python"` layer, which the server would read as a section with
/// no `analysis` and silently drop (that exact shape starved `import pite`
/// of its stubs). Anything else gets an empty object (defaults hold).
pub fn configuration_response(id: i64, section: &str, stubs_dir: Option<&Path>) -> Value {
    let result = if section == "python" {
        python_analysis_settings(stubs_dir)
    } else {
        serde_json::json!({})
    };
    serde_json::json!({"jsonrpc": "2.0", "id": id, "result": [result]})
}

pub fn did_open_notification(uri: &str, text: &str) -> Value {
    notification(
        "textDocument/didOpen",
        serde_json::json!({"textDocument": {
            "uri": uri, "languageId": "python", "version": 1, "text": text,
        }}),
    )
}

pub fn did_change_notification(uri: &str, version: i32, text: &str) -> Value {
    notification(
        "textDocument/didChange",
        serde_json::json!({"textDocument": {"uri": uri, "version": version},
            "contentChanges": [{"text": text}]}),
    )
}

fn position_params(uri: &str, line: u32, character: u32) -> Value {
    serde_json::json!({"textDocument": {"uri": uri},
        "position": {"line": line, "character": character}})
}

pub fn completion_request(id: i64, uri: &str, line: u32, character: u32) -> Value {
    request(
        id,
        "textDocument/completion",
        position_params(uri, line, character),
    )
}

pub fn hover_request(id: i64, uri: &str, line: u32, character: u32) -> Value {
    request(
        id,
        "textDocument/hover",
        position_params(uri, line, character),
    )
}

pub fn signature_request(id: i64, uri: &str, line: u32, character: u32) -> Value {
    request(
        id,
        "textDocument/signatureHelp",
        position_params(uri, line, character),
    )
}

/// One server diagnostic (0-based line, protocol severity 1=error..4=hint).
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub line: u32,
    pub character: u32,
    pub severity: u32,
    pub message: String,
}

/// One completion candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionItem {
    pub label: String,
    pub detail: String,
}

/// Parse `publishDiagnostics` params into diagnostics (empty when clean).
pub fn parse_publish_diagnostics(params: &Value) -> Vec<Diagnostic> {
    let Some(items) = params.get("diagnostics").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .map(|d| Diagnostic {
            line: d
                .pointer("/range/start/line")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32,
            character: d
                .pointer("/range/start/character")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32,
            severity: d.get("severity").and_then(Value::as_u64).unwrap_or(1) as u32,
            message: d
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
        .collect()
}

/// Parse a completion result (array or `{items}`; null means no candidates).
pub fn parse_completion_items(result: &Value) -> Vec<CompletionItem> {
    let items = match result {
        Value::Array(items) => items.clone(),
        Value::Object(_) => result
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    items
        .iter()
        .map(|i| CompletionItem {
            label: i
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            detail: i
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
        .filter(|i| !i.label.is_empty())
        .collect()
}

/// Parse a hover result into plain text (markdown fenced content included).
pub fn parse_hover(result: &Value) -> String {
    let contents = match result {
        Value::Null => return String::new(),
        Value::Object(_) => result.get("contents").unwrap_or(&Value::Null),
        other => other,
    };
    match contents {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| {
                p.get("value")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| p.as_str().map(str::to_string))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(_) => contents
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

/// Parse signature help into one line per overload.
pub fn parse_signature_help(result: &Value) -> Vec<String> {
    result
        .get("signatures")
        .and_then(Value::as_array)
        .map(|sigs| {
            sigs.iter()
                .filter_map(|s| s.get("label").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}
