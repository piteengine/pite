// SPDX-License-Identifier: MIT OR Apache-2.0
//! External Python language server over stdio (pyright-family).
//!
//! `pite-editor` never embeds a server: it spawns `pyright-langserver
//! --stdio` (or `PITE_LSP_CMD`), syncs the code pane with
//! `didOpen`/`didChange`, and surfaces completion, hover, signature
//! help, and `publishDiagnostics`. The shipped `pite.pyi` stubs go on
//! the server's path so `import pite` resolves. No DAP, no bundled binary.
//!
//! The protocol core is transport-agnostic: [`Bridge`] speaks JSON-RPC
//! over any [`RpcTransport`]. Production uses [`StdioTransport`] (real
//! child stdio); tests drive the same [`Bridge`] with a mock server.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::Value;

/// Documented prerequisite; keep in sync with `docs/lsp.md`.
pub const PINNED_PYRIGHT: &str = "1.1.411";
/// Default server command; [`ENV_CMD_OVERRIDE`] replaces it.
pub const SERVER_CMD: &str = "pyright-langserver";
/// Args for the default command.
pub const SERVER_ARGS: &[&str] = &["--stdio"];
/// Env override for the server command (a binary name or path).
pub const ENV_CMD_OVERRIDE: &str = "PITE_LSP_CMD";
/// Shipped stubs file; its parent dir goes on the server's `extraPaths`.
pub const STUBS_FILE: &str = "pite.pyi";
/// How long the background starter waits for `initialize`.
const INIT_TIMEOUT: Duration = Duration::from_secs(20);

/// Loud message when no server is installed. Editing continues without LSP.
pub fn missing_server_message(cmd: &str) -> String {
    format!(
        "LSP unavailable: `{cmd}` not found on PATH. Install the pinned server \
         (`pip install \"pyright=={PINNED_PYRIGHT}\"` ships `{SERVER_CMD}`) or point \
         `{ENV_CMD_OVERRIDE}` at one. Editing continues without completion, hover, \
         signature help, or server diagnostics."
    )
}

/// Best-effort `file://` URI for a path. Falls back to the raw path text
/// when the current dir is unknown; servers only need it as a document key.
pub fn path_to_uri(path: &Path) -> String {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path)
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

/// First `major.minor.patch` in free-form `--version` output.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut digits = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() || c == '.' {
            digits.push(c);
        } else if !digits.is_empty() {
            break;
        }
    }
    let mut parts = digits.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ))
}

/// Numeric triple comparison against the pin.
pub fn version_at_least(have: (u64, u64, u64), want: (u64, u64, u64)) -> bool {
    have >= want
}

fn pin_triple() -> (u64, u64, u64) {
    let mut parts = PINNED_PYRIGHT.split('.');
    (
        parts.next().and_then(|p| p.parse().ok()).unwrap_or(0),
        parts.next().and_then(|p| p.parse().ok()).unwrap_or(0),
        parts.next().and_then(|p| p.parse().ok()).unwrap_or(0),
    )
}

fn version_output(cmd: &str) -> Option<String> {
    let out = Command::new(cmd).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `pyright-langserver` speaks only stdio, so ask its `pyright` sibling.
fn sibling_version(cmd: &str) -> Option<String> {
    if Path::new(cmd).file_name().and_then(|s| s.to_str()) != Some(SERVER_CMD) {
        return None;
    }
    let sibling = match Path::new(cmd).parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            parent.join("pyright").to_string_lossy().into_owned()
        }
        _ => "pyright".to_string(),
    };
    version_output(&sibling)
}

/// Best-effort version probe: `(version, warning)`. Never fails; a missing
/// binary surfaces later at spawn with the loud message.
pub fn probe_server_version(cmd: &str) -> (Option<String>, Option<String>) {
    let text = version_output(cmd).or_else(|| sibling_version(cmd));
    let Some(text) = text else {
        return (
            None,
            Some(format!("LSP: `{cmd} --version` failed; proceeding without a version check.")),
        );
    };
    match parse_version(&text) {
        Some(have) => {
            let label = format!("{}.{}.{}", have.0, have.1, have.2);
            if version_at_least(have, pin_triple()) {
                (Some(label), None)
            } else {
                (
                    Some(label.clone()),
                    Some(format!(
                        "LSP: server is {label} but {PINNED_PYRIGHT} is pinned; \
                         consider upgrading (`pip install \"pyright=={PINNED_PYRIGHT}\"`)."
                    )),
                )
            }
        }
        None => (
            None,
            Some(format!("LSP: could not parse server version from {text:?}; proceeding.")),
        ),
    }
}

/// Walk up from `start` looking for `python/pite/pite.pyi` (or a bare
/// `pite.pyi`); return the dir containing it — what goes on `extraPaths`
/// so `import pite` resolves to the shipped stubs.
pub fn find_stubs_dir(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let nested = dir.join("python").join("pite").join(STUBS_FILE);
        if nested.is_file() {
            return Some(dir.join("python").join("pite"));
        }
        if dir.join(STUBS_FILE).is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
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
            let n = self.inner.read_line(&mut line).context("server closed stdio")?;
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
        self.inner.read_exact(&mut buf).context("server closed stdio")?;
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

/// `extraPaths` + basic type-checking mode so the shipped stubs resolve.
pub fn did_change_configuration(stubs_dir: Option<&Path>) -> Value {
    let extra: Vec<String> = stubs_dir
        .map(|d| vec![d.to_string_lossy().into_owned()])
        .unwrap_or_default();
    notification(
        "workspace/didChangeConfiguration",
        serde_json::json!({"settings": {
            "python": {"analysis": {
                "extraPaths": extra,
                "typeCheckingMode": "basic",
            }},
        }}),
    )
}

/// Answer a server `workspace/configuration` request with the same settings.
pub fn configuration_response(id: i64, stubs_dir: Option<&Path>) -> Value {
    let extra: Vec<String> = stubs_dir
        .map(|d| vec![d.to_string_lossy().into_owned()])
        .unwrap_or_default();
    serde_json::json!({"jsonrpc": "2.0", "id": id, "result": [
        {"python": {"analysis": {"extraPaths": extra, "typeCheckingMode": "basic"}}},
    ]})
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
    request(id, "textDocument/completion", position_params(uri, line, character))
}

pub fn hover_request(id: i64, uri: &str, line: u32, character: u32) -> Value {
    request(id, "textDocument/hover", position_params(uri, line, character))
}

pub fn signature_request(id: i64, uri: &str, line: u32, character: u32) -> Value {
    request(id, "textDocument/signatureHelp", position_params(uri, line, character))
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
            label: i.get("label").and_then(Value::as_str).unwrap_or("").to_string(),
            detail: i.get("detail").and_then(Value::as_str).unwrap_or("").to_string(),
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum PendingKind {
    Initialize,
    Completion,
    Hover,
    Signature,
}

/// Byte transport for JSON-RPC values. Production speaks over child
/// stdio; tests substitute a mock server speaking the same messages.
pub trait RpcTransport: Send {
    fn send(&mut self, body: &Value) -> Result<()>;
    fn drain(&mut self) -> Vec<Value>;
    fn alive(&mut self) -> bool;
}

/// Real child-stdio transport: writes go straight to the server's
/// stdin, a reader thread forwards framed replies to a channel.
pub struct StdioTransport {
    stdin: std::process::ChildStdin,
    inbox: Receiver<Value>,
    dead: bool,
}

impl StdioTransport {
    pub fn spawn(cmd: &str, args: &[&str]) -> Result<(Self, Child)> {
        let mut child = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    anyhow::anyhow!(missing_server_message(cmd))
                } else {
                    anyhow::anyhow!("cannot start `{cmd}`: {e:#}")
                }
            })?;
        let stdin = child.stdin.take().context("server stdin unavailable")?;
        let stdout = child.stdout.take().context("server stdout unavailable")?;
        let (tx, inbox) = mpsc::channel();
        std::thread::spawn(move || {
            let mut framing = FramingReader::new(BufReader::new(stdout));
            while let Ok(msg) = framing.read_message() {
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        Ok((Self { stdin, inbox, dead: false }, child))
    }
}

impl RpcTransport for StdioTransport {
    fn send(&mut self, body: &Value) -> Result<()> {
        let bytes = encode_message(body);
        self.stdin.write_all(&bytes).context("server stdin closed")
    }

    fn drain(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        loop {
            match self.inbox.try_recv() {
                Ok(msg) => out.push(msg),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.dead = true;
                    break;
                }
            }
        }
        out
    }

    fn alive(&mut self) -> bool {
        !self.dead
    }
}

/// Protocol state above a transport: request ids, pending kinds, typed
/// results, and per-document diagnostics.
pub struct Bridge {
    next_id: i64,
    pending: HashMap<i64, PendingKind>,
    responses: HashMap<i64, Value>,
    versions: HashMap<String, i32>,
    completions: VecDeque<(i64, Vec<CompletionItem>)>,
    hovers: VecDeque<(i64, String)>,
    signatures: VecDeque<(i64, Vec<String>)>,
    notices: VecDeque<String>,
    pub diagnostics: HashMap<String, Vec<Diagnostic>>,
}

impl Bridge {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            pending: HashMap::new(),
            responses: HashMap::new(),
            versions: HashMap::new(),
            completions: VecDeque::new(),
            hovers: VecDeque::new(),
            signatures: VecDeque::new(),
            notices: VecDeque::new(),
            diagnostics: HashMap::new(),
        }
    }

    fn call(&mut self, tx: &mut dyn RpcTransport, kind: PendingKind, body: Value) -> Result<i64> {
        let id = self.next_id;
        self.next_id += 1;
        tx.send(&body)?;
        self.pending.insert(id, kind);
        Ok(id)
    }

    pub fn send_initialize(&mut self, tx: &mut dyn RpcTransport, root_uri: Option<&str>) -> Result<i64> {
        let id = self.next_id;
        let body = initialize_request(id, root_uri);
        self.call(tx, PendingKind::Initialize, body)
    }

    pub fn send_initialized(&mut self, tx: &mut dyn RpcTransport) -> Result<()> {
        tx.send(&initialized_notification())
    }

    pub fn send_configuration(&mut self, tx: &mut dyn RpcTransport, stubs: Option<&Path>) -> Result<()> {
        tx.send(&did_change_configuration(stubs))
    }

    pub fn did_open(&mut self, tx: &mut dyn RpcTransport, uri: &str, text: &str) -> Result<()> {
        self.versions.insert(uri.to_string(), 1);
        tx.send(&did_open_notification(uri, text))
    }

    pub fn did_change(&mut self, tx: &mut dyn RpcTransport, uri: &str, text: &str) -> Result<()> {
        let version = self.versions.entry(uri.to_string()).or_insert(0);
        *version += 1;
        tx.send(&did_change_notification(uri, *version, text))
    }

    pub fn request_completion(
        &mut self,
        tx: &mut dyn RpcTransport,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<i64> {
        let id = self.next_id;
        let body = completion_request(id, uri, line, character);
        self.call(tx, PendingKind::Completion, body)
    }

    pub fn request_hover(
        &mut self,
        tx: &mut dyn RpcTransport,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<i64> {
        let id = self.next_id;
        let body = hover_request(id, uri, line, character);
        self.call(tx, PendingKind::Hover, body)
    }

    pub fn request_signature(
        &mut self,
        tx: &mut dyn RpcTransport,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<i64> {
        let id = self.next_id;
        let body = signature_request(id, uri, line, character);
        self.call(tx, PendingKind::Signature, body)
    }

    /// Route one incoming message: match responses to pending requests,
    /// answer server requests, file diagnostics notifications.
    pub fn ingest(&mut self, msg: Value, tx: &mut dyn RpcTransport, stubs: Option<&Path>) {
        if let Some(id) = msg.get("id").and_then(Value::as_i64) {
            if msg.get("method").is_some() {
                self.answer_server_request(id, &msg, tx, stubs);
            } else {
                let result = msg.get("result").cloned().unwrap_or(Value::Null);
                match self.pending.remove(&id) {
                    Some(PendingKind::Completion) => {
                        self.completions.push_back((id, parse_completion_items(&result)));
                    }
                    Some(PendingKind::Hover) => {
                        self.hovers.push_back((id, parse_hover(&result)));
                    }
                    Some(PendingKind::Signature) => {
                        self.signatures.push_back((id, parse_signature_help(&result)));
                    }
                    Some(PendingKind::Initialize) => {
                        self.responses.insert(id, result);
                    }
                    None => {}
                }
            }
            return;
        }
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            return;
        };
        match method {
            "textDocument/publishDiagnostics" => {
                let params = msg.get("params").unwrap_or(&Value::Null);
                let uri = params.get("uri").and_then(Value::as_str).unwrap_or("");
                self.diagnostics
                    .insert(uri.to_string(), parse_publish_diagnostics(params));
            }
            "window/showMessage" | "window/logMessage" => {
                if let Some(text) = msg
                    .get("params")
                    .and_then(|p| p.get("message"))
                    .and_then(Value::as_str)
                {
                    self.notices.push_back(text.to_string());
                }
            }
            _ => {}
        }
    }

    fn answer_server_request(
        &mut self,
        id: i64,
        msg: &Value,
        tx: &mut dyn RpcTransport,
        stubs: Option<&Path>,
    ) {
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let reply = match method {
            "workspace/configuration" => configuration_response(id, stubs),
            _ => serde_json::json!({"jsonrpc": "2.0", "id": id, "result": null}),
        };
        let _ = tx.send(&reply);
    }

    /// Drain the transport into protocol state. Returns `false` once the
    /// server is gone (caller reports loudly and drops the client).
    pub fn poll(&mut self, tx: &mut dyn RpcTransport, stubs: Option<&Path>) -> bool {
        for msg in tx.drain() {
            self.ingest(msg, tx, stubs);
        }
        tx.alive()
    }

    /// Block (via transport polls) until a response id arrives or timeout.
    /// Used only by the background starter for `initialize`.
    pub fn await_response(
        &mut self,
        tx: &mut dyn RpcTransport,
        id: i64,
        stubs: Option<&Path>,
        timeout: Duration,
    ) -> Result<Value> {
        let start = Instant::now();
        loop {
            for msg in tx.drain() {
                self.ingest(msg, tx, stubs);
            }
            if let Some(result) = self.responses.remove(&id) {
                return Ok(result);
            }
            if !tx.alive() {
                anyhow::bail!("server exited during initialize");
            }
            if start.elapsed() > timeout {
                anyhow::bail!("server did not answer initialize in time");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn take_completion(&mut self) -> Option<(i64, Vec<CompletionItem>)> {
        self.completions.pop_front()
    }

    pub fn take_hover(&mut self) -> Option<(i64, String)> {
        self.hovers.pop_front()
    }

    pub fn take_signature(&mut self) -> Option<(i64, Vec<String>)> {
        self.signatures.pop_front()
    }

    pub fn take_notice(&mut self) -> Option<String> {
        self.notices.pop_front()
    }
}

impl Default for Bridge {
    fn default() -> Self {
        Self::new()
    }
}

/// Owns the child process; killing on drop so no server outlives the editor.
pub struct ChildGuard(Child);

impl ChildGuard {
    pub fn new(child: Child) -> Self {
        Self(child)
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A ready-to-use server connection for one editor session.
pub struct LspClient {
    bridge: Bridge,
    transport: Box<dyn RpcTransport>,
    _child: Option<ChildGuard>,
    stubs_dir: Option<PathBuf>,
    dead_reported: bool,
    pub server_version: Option<String>,
}

impl LspClient {
    fn new(
        transport: Box<dyn RpcTransport>,
        child: Option<ChildGuard>,
        stubs_dir: Option<PathBuf>,
        server_version: Option<String>,
    ) -> Self {
        Self {
            bridge: Bridge::new(),
            transport,
            _child: child,
            stubs_dir,
            dead_reported: false,
            server_version,
        }
    }

    /// Poll the server; returns an error string once when it dies.
    pub fn poll(&mut self) -> Option<String> {
        let stubs = self.stubs_dir.clone();
        if !self.bridge.poll(&mut *self.transport, stubs.as_deref()) && !self.dead_reported {
            self.dead_reported = true;
            return Some("LSP: language server exited; editing continues without it.".to_string());
        }
        None
    }

    pub fn did_open(&mut self, uri: &str, text: &str) -> Result<()> {
        self.bridge.did_open(&mut *self.transport, uri, text)
    }

    pub fn did_change(&mut self, uri: &str, text: &str) -> Result<()> {
        self.bridge.did_change(&mut *self.transport, uri, text)
    }

    pub fn request_completion(&mut self, uri: &str, line: u32, character: u32) -> Result<i64> {
        self.bridge.request_completion(&mut *self.transport, uri, line, character)
    }

    pub fn request_hover(&mut self, uri: &str, line: u32, character: u32) -> Result<i64> {
        self.bridge.request_hover(&mut *self.transport, uri, line, character)
    }

    pub fn request_signature(&mut self, uri: &str, line: u32, character: u32) -> Result<i64> {
        self.bridge.request_signature(&mut *self.transport, uri, line, character)
    }

    pub fn take_completion(&mut self) -> Option<(i64, Vec<CompletionItem>)> {
        self.bridge.take_completion()
    }

    pub fn take_hover(&mut self) -> Option<(i64, String)> {
        self.bridge.take_hover()
    }

    pub fn take_signature(&mut self) -> Option<(i64, Vec<String>)> {
        self.bridge.take_signature()
    }

    pub fn take_notice(&mut self) -> Option<String> {
        self.bridge.take_notice()
    }

    pub fn diagnostics_for_uri(&self, uri: &str) -> &[Diagnostic] {
        self.bridge.diagnostics.get(uri).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn diagnostic_count(&self) -> usize {
        self.bridge.diagnostics.values().map(Vec::len).sum()
    }
}

/// Starter outcome delivered to the UI thread.
pub enum LspOutcome {
    Ready { client: LspClient, warning: Option<String> },
    Failed(String),
}

/// Spawn the server in the background and run the `initialize` handshake
/// there (cold starts take seconds). The receiver gets exactly one outcome;
/// a missing binary fails fast and loudly, never silently.
pub fn spawn_lsp(root_dir: Option<PathBuf>, script: &Path) -> Receiver<LspOutcome> {
    let (tx, rx) = mpsc::channel();
    let script = script.to_path_buf();
    std::thread::spawn(move || {
        let outcome = start_blocking(root_dir.as_deref(), &script);
        let _ = tx.send(outcome);
    });
    rx
}

fn start_blocking(root_dir: Option<&Path>, script: &Path) -> LspOutcome {
    let cmd = std::env::var(ENV_CMD_OVERRIDE).unwrap_or_else(|_| SERVER_CMD.to_string());
    let (server_version, mut warning) = probe_server_version(&cmd);
    let stubs_dir = root_dir
        .and_then(find_stubs_dir)
        .or_else(|| find_stubs_dir(script))
        .or_else(|| find_stubs_dir(Path::new(".")));
    if stubs_dir.is_none() {
        let w = "LSP: shipped pite.pyi stubs not found; `import pite` may not resolve.".to_string();
        warning = Some(match warning {
            Some(prev) => format!("{prev} {w}"),
            None => w,
        });
    }
    let (mut transport, child) = match StdioTransport::spawn(&cmd, SERVER_ARGS) {
        Ok(pair) => pair,
        Err(e) => return LspOutcome::Failed(format!("LSP unavailable: {e:#}")),
    };
    let root_uri = root_dir.map(path_to_uri);
    let mut bridge = Bridge::new();
    let init_id = match bridge.send_initialize(&mut transport, root_uri.as_deref()) {
        Ok(id) => id,
        Err(e) => return LspOutcome::Failed(format!("LSP: cannot talk to server: {e:#}")),
    };
    let stubs = stubs_dir.clone();
    match bridge.await_response(&mut transport, init_id, stubs.as_deref(), INIT_TIMEOUT) {
        Ok(_) => {}
        Err(e) => return LspOutcome::Failed(format!("LSP: initialize failed ({e:#}); editing continues without it.")),
    }
    let stubs_ref = stubs_dir.clone();
    if bridge.send_initialized(&mut transport).is_err()
        || bridge
            .send_configuration(&mut transport, stubs_ref.as_deref())
            .is_err()
    {
        return LspOutcome::Failed("LSP: server went quiet after initialize.".to_string());
    }
    let mut client = LspClient::new(
        Box::new(transport),
        Some(ChildGuard::new(child)),
        stubs_dir,
        server_version,
    );
    client.bridge = bridge;
    LspOutcome::Ready { client, warning }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// In-test mock server: canned answers + diagnostics on open/change.
    struct MockServer {
        opened: HashMap<String, String>,
    }

    impl MockServer {
        fn new() -> Self {
            Self { opened: HashMap::new() }
        }

        fn handle(&mut self, msg: &Value) -> Vec<Value> {
            let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
            if msg.get("id").is_some() {
                let id = msg.get("id").cloned().unwrap();
                let result = match method {
                    "initialize" => serde_json::json!({"capabilities": {}}),
                    "textDocument/completion" => serde_json::json!([
                        {"label": "position", "detail": "tuple[float, float]"},
                        {"label": "pressed", "detail": "def pressed"},
                    ]),
                    "textDocument/hover" => serde_json::json!({"contents": {
                        "kind": "markdown", "value": "**position**: tuple"}}),
                    "textDocument/signatureHelp" => serde_json::json!({"signatures": [
                        {"label": "play(path, volume)"},
                    ]}),
                    "shutdown" => Value::Null,
                    _ => Value::Null,
                };
                return vec![serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result})];
            }
            match method {
                "textDocument/didOpen" | "textDocument/didChange" => {
                    let doc = msg.get("params").and_then(|p| p.get("textDocument"));
                    let uri = doc.and_then(|d| d.get("uri")).and_then(Value::as_str).unwrap_or("");
                    let text = if method.ends_with("didOpen") {
                        msg.pointer("/params/textDocument/text").and_then(Value::as_str).unwrap_or("")
                    } else {
                        msg.pointer("/params/contentChanges/0/text").and_then(Value::as_str).unwrap_or("")
                    };
                    self.opened.insert(uri.to_string(), text.to_string());
                    let diagnostics = if text.contains("oops") {
                        serde_json::json!([{"range": {"start": {"line": 0, "character": 0},
                            "end": {"line": 0, "character": 4}},
                            "severity": 1, "message": "mock: undefined name"}])
                    } else {
                        serde_json::json!([])
                    };
                    vec![serde_json::json!({"jsonrpc": "2.0",
                        "method": "textDocument/publishDiagnostics",
                        "params": {"uri": uri, "diagnostics": diagnostics}})]
                }
                _ => Vec::new(),
            }
        }
    }

    struct MockTransport {
        pub outbox: Vec<Value>,
        inbox: VecDeque<Value>,
        server: MockServer,
    }

    impl MockTransport {
        fn new() -> Self {
            Self { outbox: Vec::new(), inbox: VecDeque::new(), server: MockServer::new() }
        }
    }

    impl RpcTransport for MockTransport {
        fn send(&mut self, body: &Value) -> Result<()> {
            self.outbox.push(body.clone());
            self.inbox.extend(self.server.handle(body));
            Ok(())
        }

        fn drain(&mut self) -> Vec<Value> {
            self.inbox.drain(..).collect()
        }

        fn alive(&mut self) -> bool {
            true
        }
    }

    #[test]
    fn framing_round_trip() {
        let body = serde_json::json!({"jsonrpc": "2.0", "id": 7, "method": "x", "params": {}});
        let bytes = encode_message(&body);
        assert!(bytes.starts_with(b"Content-Length: "));
        let mut reader = FramingReader::new(Cursor::new(bytes));
        assert_eq!(reader.read_message().unwrap(), body);
    }

    #[test]
    fn framing_rejects_garbage_loudly() {
        let mut reader = FramingReader::new(Cursor::new(b"not a frame".to_vec()));
        assert!(reader.read_message().is_err());
        let mut reader = FramingReader::new(Cursor::new(b"Content-Length: 5\r\n\r\n###".to_vec()));
        assert!(reader.read_message().is_err());
    }

    #[test]
    fn builders_carry_params() {
        let open = did_open_notification("file:///a.py", "import pite\n");
        assert_eq!(open.pointer("/params/textDocument/uri").unwrap(), "file:///a.py");
        let change = did_change_notification("file:///a.py", 7, "x = 1\n");
        assert_eq!(
            change.pointer("/params/contentChanges/0/text").unwrap(),
            "x = 1\n"
        );
        assert_eq!(change.pointer("/params/textDocument/version").unwrap(), 7);
        let req = completion_request(3, "file:///a.py", 4, 9);
        assert_eq!(req.get("id").unwrap(), 3);
        assert_eq!(req.pointer("/params/position/line").unwrap(), 4);
        let init = initialize_request(1, Some("file:///proj"));
        assert_eq!(init.pointer("/params/rootUri").unwrap(), "file:///proj");
        assert_eq!(
            init.pointer("/params/workspaceFolders/0/uri").unwrap(),
            "file:///proj"
        );
        assert_eq!(init.pointer("/params/capabilities/workspace/configuration"), Some(&Value::Bool(true)));
        let bare = initialize_request(1, None);
        assert_eq!(bare.pointer("/params/workspaceFolders").unwrap(), &serde_json::json!([]));
        let cfg = did_change_configuration(Some(Path::new("/stubs")));
        assert_eq!(
            cfg.pointer("/params/settings/python/analysis/extraPaths/0").unwrap(),
            "/stubs"
        );
    }

    #[test]
    fn mock_round_trip_open_change_completion_diagnostics() {
        let mut bridge = Bridge::new();
        let mut tx = MockTransport::new();
        let uri = "file:///proj/scripts/player.py";

        bridge.did_open(&mut tx, uri, "import pite\noops\n").unwrap();
        assert!(bridge.poll(&mut tx, None));
        let diags = bridge.diagnostics.get(uri).cloned().unwrap_or_default();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 0);
        assert!(diags[0].message.contains("undefined name"));

        bridge.did_change(&mut tx, uri, "import pite\n").unwrap();
        assert!(bridge.poll(&mut tx, None));
        assert!(bridge.diagnostics.get(uri).cloned().unwrap_or_default().is_empty());
        let versions: Vec<i64> = tx
            .outbox
            .iter()
            .filter(|m| m.get("method").and_then(Value::as_str) == Some("textDocument/didChange"))
            .filter_map(|m| m.pointer("/params/textDocument/version").and_then(Value::as_i64))
            .collect();
        assert_eq!(versions, vec![2]);

        let id = bridge.request_completion(&mut tx, uri, 1, 2).unwrap();
        assert!(bridge.poll(&mut tx, None));
        let (got_id, items) = bridge.take_completion().expect("completion arrives");
        assert_eq!(got_id, id);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label, "position");

        bridge.request_hover(&mut tx, uri, 1, 2).unwrap();
        assert!(bridge.poll(&mut tx, None));
        let (_, hover) = bridge.take_hover().expect("hover arrives");
        assert!(hover.contains("position"));

        bridge.request_signature(&mut tx, uri, 1, 2).unwrap();
        assert!(bridge.poll(&mut tx, None));
        let (_, sigs) = bridge.take_signature().expect("signature arrives");
        assert_eq!(sigs, vec!["play(path, volume)"]);

        // Every request carried a JSON-RPC id; notifications did not.
        assert!(tx.outbox.iter().any(|m| m.get("method").unwrap() == "textDocument/didOpen"));
    }

    #[test]
    fn server_requests_get_answers() {
        let mut bridge = Bridge::new();
        let mut tx = MockTransport::new();
        bridge.ingest(
            serde_json::json!({"jsonrpc": "2.0", "id": 9,
                "method": "workspace/configuration", "params": {"items": [{}]}}),
            &mut tx,
            Some(Path::new("/stubs")),
        );
        let reply = tx.outbox.last().expect("server request answered");
        assert_eq!(reply.get("id").unwrap(), 9);
        assert_eq!(
            reply.pointer("/result/0/python/analysis/extraPaths/0").unwrap(),
            "/stubs"
        );
    }

    #[test]
    fn missing_server_fails_loudly() {
        let err = StdioTransport::spawn("definitely-not-a-pite-lsp-server", SERVER_ARGS)
            .err()
            .expect("missing server must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("not found on PATH"), "got: {msg}");
        assert!(msg.contains(PINNED_PYRIGHT), "got: {msg}");
        assert!(msg.contains(ENV_CMD_OVERRIDE), "got: {msg}");
    }

    #[test]
    fn version_parse_and_compare() {
        assert_eq!(parse_version("pyright 1.1.411"), Some((1, 1, 411)));
        assert_eq!(parse_version("1.1.398"), Some((1, 1, 398)));
        assert_eq!(parse_version("no version here"), None);
        assert!(version_at_least((1, 1, 411), (1, 1, 411)));
        assert!(version_at_least((1, 1, 412), (1, 1, 411)));
        assert!(!version_at_least((1, 1, 400), (1, 1, 411)));
    }

    #[test]
    fn stubs_found_beside_layout() {
        let dir = std::env::temp_dir().join(format!("pite-lsp-stubs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let stubs = dir.join("python").join("pite");
        std::fs::create_dir_all(&stubs).unwrap();
        std::fs::write(stubs.join(STUBS_FILE), "x: int\n").unwrap();
        let script = dir.join("scripts").join("a.py");
        assert_eq!(find_stubs_dir(&script), Some(stubs));
        assert!(find_stubs_dir(Path::new("/definitely/not/here")).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn uris_are_well_formed_on_every_platform() {
        assert_eq!(
            uri_from_path_text("/home/dev/pite/scripts/a.py"),
            "file:///home/dev/pite/scripts/a.py"
        );
        // Windows: backslashes and drive letter need normalizing, else the
        // server reports the project directory as nonexistent.
        assert_eq!(
            uri_from_path_text(r"D:\Projects\Pite\pite\examples\minimal-2d"),
            "file:///D:/Projects/Pite/pite/examples/minimal-2d"
        );
        assert_eq!(
            uri_from_path_text("D:/Projects/Pite/pite/scripts/a.py"),
            "file:///D:/Projects/Pite/pite/scripts/a.py"
        );
        assert_eq!(
            uri_from_path_text("/home/dev/my game/a b.py"),
            "file:///home/dev/my%20game/a%20b.py"
        );
        assert_eq!(path_to_uri(Path::new("/tmp/a.py")), "file:///tmp/a.py");
    }

    #[test]
    fn positions_and_word_starts() {
        let text = "ab\ncλd\n";
        assert_eq!(offset_to_position(text, 0), (0, 0));
        assert_eq!(offset_to_position(text, 3), (1, 0));
        // λ is one char but two UTF-16 units? No: U+03BB is one UTF-16 unit.
        assert_eq!(offset_to_position(text, 4), (1, 1));
        assert_eq!(byte_offset_of_char(text, 0), 0);
        assert_eq!(byte_offset_of_char(text, 4), 4);
        assert_eq!(word_start_before("self.position", 13), 5);
        assert_eq!(word_start_before("self.position", 4), 0);
        assert_eq!(word_start_before("x = 1", 4), 4);
        assert_eq!(word_start_before("x = 1", 5), 4);
    }

    #[test]
    fn null_results_are_empty() {
        assert!(parse_completion_items(&Value::Null).is_empty());
        assert!(parse_hover(&Value::Null).is_empty());
        assert!(parse_signature_help(&Value::Null).is_empty());
        assert!(parse_publish_diagnostics(&Value::Null).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn framing_survives_real_fds() {
        use std::os::unix::net::UnixStream;
        let (a, b) = UnixStream::pair().unwrap();
        let body = serde_json::json!({"jsonrpc": "2.0", "method": "ping"});
        let bytes = encode_message(&body);
        let mut writer = a;
        writer.write_all(&bytes).unwrap();
        let mut reader = FramingReader::new(BufReader::new(b));
        assert_eq!(reader.read_message().unwrap(), body);
    }

    /// Live-server smoke: real pyright over stdio on the dogfood script
    /// (open → completion, change → session still serving). Runs only with
    /// `PITE_LSP_SMOKE=1` (CI sets it after installing the pin); otherwise
    /// it skips so plain `cargo test` needs no server.
    #[test]
    fn live_pyright_smoke_open_completion_change() {
        if std::env::var("PITE_LSP_SMOKE").is_err() {
            eprintln!("skipped: set PITE_LSP_SMOKE=1 for the live pyright smoke");
            return;
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("examples")
            .join("minimal-2d");
        let script = root.join("scripts").join("player.py");
        let text = std::fs::read_to_string(&script).expect("dogfood script exists");
        let rx = spawn_lsp(Some(root), &script);
        let mut client = match rx.recv_timeout(Duration::from_secs(180)).expect("starter answers") {
            LspOutcome::Ready { client, warning } => {
                if let Some(w) = warning {
                    eprintln!("smoke warning: {w}");
                }
                assert_eq!(client.server_version.as_deref(), Some(PINNED_PYRIGHT));
                client
            }
            LspOutcome::Failed(msg) => panic!("smoke server failed: {msg}"),
        };
        let uri = path_to_uri(&script);
        client.did_open(&uri, &text).expect("didOpen sends");

        // The server must adopt our project root: without `workspaceFolders`
        // it logs a synthetic `<default workspace root>`, finds no files, and
        // silently analyzes nothing.
        while let Some(notice) = client.take_notice() {
            assert!(
                !notice.contains("<default workspace root>"),
                "smoke: server ignored the project root: {notice}"
            );
        }

        // Completion inside `self.` — proves the open + completion round-trip.
        // `x, y = self.position` is line 11 (0-based); the dot sits at column 20.
        let found = smoke_await_completion(&mut client, &uri, 11, 20);
        assert!(!found.is_empty(), "smoke: completion came back empty");
        assert!(
            found.iter().any(|i| i.label == "position"),
            "smoke: expected `position`, got {:?}",
            found.iter().map(|i| &i.label).collect::<Vec<_>>()
        );

        // Change, then prove the session still serves: `didChange` syncs the
        // server buffer only; the dogfood file on disk is never touched.
        let broken = format!("{text}\nundefined_smoke_name_xyz\n");
        client.did_change(&uri, &broken).expect("didChange sends");
        let after = smoke_await_completion(&mut client, &uri, 11, 20);
        assert!(
            after.iter().any(|i| i.label == "position"),
            "smoke: completion broke after didChange, got {:?}",
            after.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }

    /// Poll until the server answers a completion request, failing loudly if
    /// it dies or the request goes unanswered. Returns the labels' items.
    fn smoke_await_completion(
        client: &mut LspClient,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Vec<CompletionItem> {
        client.request_completion(uri, line, character).expect("completion sends");
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            if let Some(msg) = client.poll() {
                panic!("smoke: server died ({msg})");
            }
            if let Some((_, items)) = client.take_completion() {
                return items;
            }
            if Instant::now() > deadline {
                panic!("smoke: no completion arrived");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
