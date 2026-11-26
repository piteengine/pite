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

mod bridge;
mod protocol;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use anyhow::Result;

pub use bridge::{Bridge, ChildGuard, RpcTransport, StdioTransport};
pub use protocol::{
    byte_offset_of_char, completion_request, configuration_response, did_change_configuration,
    did_change_notification, did_open_notification, encode_message, hover_request,
    initialize_request, initialized_notification, line_start_byte, offset_to_position,
    parse_completion_items, parse_hover, parse_publish_diagnostics, parse_signature_help,
    path_to_uri, signature_request, uri_from_path_text, word_end_after, word_start_before,
    CompletionItem, Diagnostic, FramingReader,
};

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
            Some(format!(
                "LSP: `{cmd} --version` failed; proceeding without a version check."
            )),
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
            Some(format!(
                "LSP: could not parse server version from {text:?}; proceeding."
            )),
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

/// Stubs baked into the editor binary (`python/pite/` at build time), so
/// `import pite` resolves even where no checkout exists — installed bundles
/// never shipped these files, and the walk-up search above finds nothing
/// there. `pite.py` is the source sibling the checker needs beside `pite.pyi`
/// (the real module is native via PyO3); types always come from the stubs.
pub(crate) const EMBEDDED_STUBS: &str = include_str!(concat!(env!("OUT_DIR"), "/pite.pyi"));
pub(crate) const EMBEDDED_SOURCE: &str = include_str!(concat!(env!("OUT_DIR"), "/pite.py"));

/// Write the embedded stubs into `dir`, skipping files that already match.
/// Returns the dir for `extraPaths`.
pub fn write_stubs_to(dir: &Path) -> Result<PathBuf> {
    for (name, contents) in [(STUBS_FILE, EMBEDDED_STUBS), ("pite.py", EMBEDDED_SOURCE)] {
        let path = dir.join(name);
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current != contents {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, contents)?;
        }
    }
    Ok(dir.to_path_buf())
}

/// Versioned scratch dir for the embedded stubs; the last resort behind
/// `find_stubs_dir`, so a checkout's own stubs always win.
pub fn materialized_stubs_dir() -> Option<PathBuf> {
    let dir = std::env::temp_dir().join(format!("pite-{}", env!("CARGO_PKG_VERSION")));
    write_stubs_to(&dir).ok()
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
        self.bridge
            .request_completion(&mut *self.transport, uri, line, character)
    }

    pub fn request_hover(&mut self, uri: &str, line: u32, character: u32) -> Result<i64> {
        self.bridge
            .request_hover(&mut *self.transport, uri, line, character)
    }

    pub fn request_signature(&mut self, uri: &str, line: u32, character: u32) -> Result<i64> {
        self.bridge
            .request_signature(&mut *self.transport, uri, line, character)
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
        self.bridge
            .diagnostics
            .get(uri)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn diagnostic_count(&self) -> usize {
        self.bridge.diagnostics.values().map(Vec::len).sum()
    }
}

/// Starter outcome delivered to the UI thread.
pub enum LspOutcome {
    Ready {
        client: Box<LspClient>,
        warning: Option<String>,
    },
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
        .or_else(|| find_stubs_dir(Path::new(".")))
        .or_else(materialized_stubs_dir);
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
    if let Some(root) = root_dir {
        bridge.set_extra_paths(vec![root.join("plugins")]);
    }
    let init_id = match bridge.send_initialize(&mut transport, root_uri.as_deref()) {
        Ok(id) => id,
        Err(e) => return LspOutcome::Failed(format!("LSP: cannot talk to server: {e:#}")),
    };
    let stubs = stubs_dir.clone();
    match bridge.await_response(&mut transport, init_id, stubs.as_deref(), INIT_TIMEOUT) {
        Ok(_) => {}
        Err(e) => {
            return LspOutcome::Failed(format!(
                "LSP: initialize failed ({e:#}); editing continues without it."
            ))
        }
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
    LspOutcome::Ready {
        client: Box::new(client),
        warning,
    }
}
