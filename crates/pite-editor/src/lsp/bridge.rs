// SPDX-License-Identifier: MIT OR Apache-2.0
//! Transport abstraction, stdio child process management, and protocol state bridge.

use std::collections::{HashMap, VecDeque};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::Value;

use super::missing_server_message;
use super::protocol::{
    completion_request, configuration_response_with_paths, did_change_configuration_with_paths,
    did_change_notification, did_open_notification, encode_message, hover_request,
    initialize_request, initialized_notification, parse_completion_items, parse_hover,
    parse_publish_diagnostics, parse_signature_help, signature_request, CompletionItem, Diagnostic,
    FramingReader,
};

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
        Ok((
            Self {
                stdin,
                inbox,
                dead: false,
            },
            child,
        ))
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
    extra_paths: Vec<PathBuf>,
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
            extra_paths: Vec::new(),
        }
    }

    pub fn set_extra_paths(&mut self, paths: Vec<PathBuf>) {
        self.extra_paths = paths;
    }

    fn call(&mut self, tx: &mut dyn RpcTransport, kind: PendingKind, body: Value) -> Result<i64> {
        let id = self.next_id;
        self.next_id += 1;
        tx.send(&body)?;
        self.pending.insert(id, kind);
        Ok(id)
    }

    pub fn send_initialize(
        &mut self,
        tx: &mut dyn RpcTransport,
        root_uri: Option<&str>,
    ) -> Result<i64> {
        let id = self.next_id;
        let body = initialize_request(id, root_uri);
        self.call(tx, PendingKind::Initialize, body)
    }

    pub fn send_initialized(&mut self, tx: &mut dyn RpcTransport) -> Result<()> {
        tx.send(&initialized_notification())
    }

    pub fn send_configuration(
        &mut self,
        tx: &mut dyn RpcTransport,
        stubs: Option<&Path>,
    ) -> Result<()> {
        tx.send(&did_change_configuration_with_paths(
            stubs,
            &self.extra_paths,
        ))
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
                        self.completions
                            .push_back((id, parse_completion_items(&result)));
                    }
                    Some(PendingKind::Hover) => {
                        self.hovers.push_back((id, parse_hover(&result)));
                    }
                    Some(PendingKind::Signature) => {
                        self.signatures
                            .push_back((id, parse_signature_help(&result)));
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
            "workspace/configuration" => {
                let section = msg
                    .pointer("/params/items/0/section")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                configuration_response_with_paths(id, section, stubs, &self.extra_paths)
            }
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
