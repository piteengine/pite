// SPDX-License-Identifier: MIT OR Apache-2.0
use std::collections::{HashMap, VecDeque};
use std::io::{BufReader, Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::Value;

use super::*;

/// In-test mock server: canned answers + diagnostics on open/change.
struct MockServer {
    opened: HashMap<String, String>,
}

impl MockServer {
    fn new() -> Self {
        Self {
            opened: HashMap::new(),
        }
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
                let uri = doc
                    .and_then(|d| d.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let text = if method.ends_with("didOpen") {
                    msg.pointer("/params/textDocument/text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                } else {
                    msg.pointer("/params/contentChanges/0/text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
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
        Self {
            outbox: Vec::new(),
            inbox: VecDeque::new(),
            server: MockServer::new(),
        }
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
    assert_eq!(
        open.pointer("/params/textDocument/uri").unwrap(),
        "file:///a.py"
    );
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
    assert_eq!(
        init.pointer("/params/capabilities/workspace/configuration"),
        Some(&Value::Bool(true))
    );
    let bare = initialize_request(1, None);
    assert_eq!(
        bare.pointer("/params/workspaceFolders").unwrap(),
        &serde_json::json!([])
    );
    let cfg = did_change_configuration(Some(Path::new("/stubs")));
    assert_eq!(
        cfg.pointer("/params/settings/python/analysis/extraPaths/0")
            .unwrap(),
        "/stubs"
    );
}

#[test]
fn mock_round_trip_open_change_completion_diagnostics() {
    let mut bridge = Bridge::new();
    let mut tx = MockTransport::new();
    let uri = "file:///proj/scripts/player.py";

    bridge
        .did_open(&mut tx, uri, "import pite\noops\n")
        .unwrap();
    assert!(bridge.poll(&mut tx, None));
    let diags = bridge.diagnostics.get(uri).cloned().unwrap_or_default();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].line, 0);
    assert!(diags[0].message.contains("undefined name"));

    bridge.did_change(&mut tx, uri, "import pite\n").unwrap();
    assert!(bridge.poll(&mut tx, None));
    assert!(bridge
        .diagnostics
        .get(uri)
        .cloned()
        .unwrap_or_default()
        .is_empty());
    let versions: Vec<i64> = tx
        .outbox
        .iter()
        .filter(|m| m.get("method").and_then(Value::as_str) == Some("textDocument/didChange"))
        .filter_map(|m| {
            m.pointer("/params/textDocument/version")
                .and_then(Value::as_i64)
        })
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
    assert!(tx
        .outbox
        .iter()
        .any(|m| m.get("method").unwrap() == "textDocument/didOpen"));
}

#[test]
fn server_requests_get_answers() {
    let mut bridge = Bridge::new();
    let mut tx = MockTransport::new();
    bridge.ingest(
        serde_json::json!({"jsonrpc": "2.0", "id": 9,
            "method": "workspace/configuration",
            "params": {"items": [{"scopeUri": "file:///proj", "section": "python"}]}}),
        &mut tx,
        Some(Path::new("/stubs")),
    );
    let reply = tx.outbox.last().expect("server request answered");
    assert_eq!(reply.get("id").unwrap(), 9);
    assert_eq!(
        reply.pointer("/result/0/analysis/extraPaths/0").unwrap(),
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
fn configuration_answers_section_values() {
    // `workspace/configuration` pulls `items[0].section`; the answer is
    // the section VALUE — a wrapped `{"python": ...}` reads as a section
    // with no `analysis` and is silently dropped by the server.
    let py = configuration_response(7, "python", Some(Path::new("/stubs")));
    assert_eq!(
        py.pointer("/result/0/analysis/extraPaths/0").unwrap(),
        "/stubs"
    );
    assert!(py.pointer("/result/0/python").is_none());
    let other = configuration_response(8, "pyright", Some(Path::new("/stubs")));
    assert_eq!(other.pointer("/result/0").unwrap(), &serde_json::json!({}));
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
}

#[cfg(unix)]
#[test]
fn path_to_uri_absolute_unix() {
    assert_eq!(path_to_uri(Path::new("/tmp/a.py")), "file:///tmp/a.py");
    assert_eq!(
        path_to_uri(Path::new("/home/dev/proj/scripts/player.py")),
        "file:///home/dev/proj/scripts/player.py"
    );
}

#[cfg(windows)]
#[test]
fn path_to_uri_absolute_windows() {
    assert_eq!(
        path_to_uri(Path::new(r"C:\tmp\a.py")),
        "file:///C:/tmp/a.py"
    );
    assert_eq!(
        path_to_uri(Path::new(r"D:\Projects\Pite\pite\scripts\player.py")),
        "file:///D:/Projects/Pite/pite/scripts/player.py"
    );
}

#[test]
fn positions_and_word_starts() {
    let text = "ab\ncλd\n";
    assert_eq!(offset_to_position(text, 0), (0, 0));
    assert_eq!(offset_to_position(text, 3), (1, 0));
    assert_eq!(offset_to_position(text, 4), (1, 1));
    assert_eq!(byte_offset_of_char(text, 0), 0);
    assert_eq!(byte_offset_of_char(text, 4), 4);
    assert_eq!(word_start_before("self.position", 13), 5);
    assert_eq!(word_start_before("self.position", 4), 0);
    assert_eq!(word_start_before("x = 1", 4), 4);
    assert_eq!(word_start_before("x = 1", 5), 4);
    assert_eq!(word_end_after("self.position", 5), 13);
    assert_eq!(word_end_after("self.position", 0), 4);
    assert_eq!(word_end_after("x = 1", 4), 5);
    assert_eq!(word_end_after("x = 1", 5), 5);
}

#[test]
fn line_starts_land() {
    let text = "a\nbb\n\nc";
    assert_eq!(line_start_byte(text, 0), 0);
    assert_eq!(line_start_byte(text, 1), 0);
    assert_eq!(line_start_byte(text, 2), 2);
    assert_eq!(line_start_byte(text, 3), 5);
    assert_eq!(line_start_byte(text, 4), 6);
    assert_eq!(line_start_byte(text, 99), text.len());
}

#[test]
fn embedded_stubs_materialize_and_repair() {
    let dir = std::env::temp_dir().join(format!("pite-lsp-stubtest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let back = write_stubs_to(&dir).unwrap();
    assert_eq!(back, dir);
    assert_eq!(
        std::fs::read_to_string(dir.join(STUBS_FILE)).unwrap(),
        EMBEDDED_STUBS
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("pite.py")).unwrap(),
        EMBEDDED_SOURCE
    );
    std::fs::write(dir.join(STUBS_FILE), "stale").unwrap();
    write_stubs_to(&dir).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join(STUBS_FILE)).unwrap(),
        EMBEDDED_STUBS
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn generated_stub_covers_pyclass_surface() {
    for schema in pite_core::NODE_SCHEMAS {
        let header_with_parent = format!("class {}(", schema.type_name);
        let header_bare = format!("class {}:", schema.type_name);
        let present =
            EMBEDDED_STUBS.contains(&header_with_parent) || EMBEDDED_STUBS.contains(&header_bare);
        if schema.abstract_class {
            assert!(
                !present,
                "abstract type {} must not appear in the stub",
                schema.type_name
            );
        } else {
            assert!(
                present,
                "concrete type {} is missing from the stub — update class_members in build.rs",
                schema.type_name
            );
        }
    }
    for line in [
        "class Signal(",
        "class Vec2:",
        "class InputQuery:",
        "class ChildDescriptor:",
        "class NodeDescriptor:",
        "held: InputQuery",
        "def raycast(",
    ] {
        assert!(EMBEDDED_STUBS.contains(line), "stub missing {line:?}");
    }
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

#[test]
fn live_pyright_smoke_open_completion_change() {
    if std::env::var("PITE_LSP_SMOKE").is_err() {
        eprintln!("skipped: set PITE_LSP_SMOKE=1 for the live pyright smoke");
        return;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples")
        .join("minimal-2d");
    let script = root.join("scripts").join("player.py");
    let text = std::fs::read_to_string(&script).expect("dogfood script exists");
    let rx = spawn_lsp(Some(root), &script);
    let mut client = match rx
        .recv_timeout(Duration::from_secs(180))
        .expect("starter answers")
    {
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

    while let Some(notice) = client.take_notice() {
        assert!(
            !notice.contains("<default workspace root>"),
            "smoke: server ignored the project root: {notice}"
        );
    }

    let (dot_line, dot_col) = text
        .lines()
        .enumerate()
        .find_map(|(l, line)| {
            let col = line.find("self.")? + 5;
            Some((l as u32, col as u32))
        })
        .expect("smoke script contains `self.`");
    let found = smoke_await_completion(&mut client, &uri, dot_line, dot_col);
    assert!(!found.is_empty(), "smoke: completion came back empty");
    assert!(
        found.iter().any(|i| i.label == "position"),
        "smoke: expected `position`, got {:?}",
        found.iter().map(|i| &i.label).collect::<Vec<_>>()
    );

    let broken = format!("{text}\nundefined_smoke_name_xyz\n");
    client.did_change(&uri, &broken).expect("didChange sends");
    let after = smoke_await_completion(&mut client, &uri, dot_line, dot_col);
    assert!(
        after.iter().any(|i| i.label == "position"),
        "smoke: completion broke after didChange, got {:?}",
        after.iter().map(|i| &i.label).collect::<Vec<_>>()
    );
}

fn smoke_await_completion(
    client: &mut LspClient,
    uri: &str,
    line: u32,
    character: u32,
) -> Vec<CompletionItem> {
    client
        .request_completion(uri, line, character)
        .expect("completion sends");
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
