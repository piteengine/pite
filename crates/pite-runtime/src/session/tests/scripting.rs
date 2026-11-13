// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

fn hud_text(session: &GameSession) -> String {
    session
        .host
        .with_tree(|t| match t.get(&NodeId::from("hud".to_string())) {
            Some(n) => match n.props.get("text") {
                Some(PropValue::Str(s)) => s.clone(),
                _ => String::new(),
            },
            None => String::new(),
        })
}

fn make_timer_project(tag: &str) -> PathBuf {
    const TICKER: &str = "import pite\n\nclass Ticker(pite.Timer):\n    def _ready(self):\n        self.get_node(\".\").timeout.connect(self.on_timeout)\n    def on_timeout(self):\n        hud = self.get_node(\"../Hud\")\n        hud.text = str(int(hud.text) + 1)\n";

    let dir = std::env::temp_dir().join(format!("pite-sess-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"tick\"\ntype = \"Timer\"\nname = \"Tick\"\nparent = \"root\"\n\n[node.props]\nwait_time = 0.05\n\n[node.script]\npath = \"res://scripts/ticker.py\"\nclass = \"Ticker\"\n\n[[node]]\nid = \"hud\"\ntype = \"Label\"\nname = \"Hud\"\nparent = \"root\"\n\n[node.props]\ntext = \"0\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("scripts").join("ticker.py"), TICKER).unwrap();
    dir
}

#[test]
fn script_text_reaches_tree_and_renderer() {
    const HUD: &str = "import pite\n\nclass Hud(pite.Label):\n    def _process(self, delta):\n        self.text = \"HP: 2\"\n";

    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-sess-hud-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"hud\"\ntype = \"Label\"\nname = \"Hud\"\nparent = \"root\"\n\n[node.props]\ntext = \"HP: 3\"\nfont_size = 20.0\n\n[node.script]\npath = \"res://scripts/hud.py\"\nclass = \"Hud\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("scripts").join("hud.py"), HUD).unwrap();
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();
    assert_eq!(hud_text(&session), "HP: 3");
    session.update(0.016);
    assert_eq!(hud_text(&session), "HP: 2");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn signal_reaches_label_through_proxy() {
    const PINGER: &str = "import pite\n\nclass Pinger(pite.Node2D):\n    ping = pite.signal(int)\n    def _process(self, delta):\n        if not getattr(self, \"_sent\", False):\n            self._sent = True\n            self.ping.emit(1)\n";
    const PONGER: &str = "import pite\n\nclass Ponger(pite.Node2D):\n    def _ready(self):\n        self.get_node(\"../Pinger\").ping.connect(self.on_ping)\n    def on_ping(self, n):\n        self.get_node(\"../Hud\").text = \"hit!\"\n";

    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-sess-siglbl-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"pinger\"\ntype = \"Node2D\"\nname = \"Pinger\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/pinger.py\"\nclass = \"Pinger\"\n\n[[node]]\nid = \"ponger\"\ntype = \"Node2D\"\nname = \"Ponger\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/ponger.py\"\nclass = \"Ponger\"\n\n[[node]]\nid = \"hud\"\ntype = \"Label\"\nname = \"Hud\"\nparent = \"root\"\n\n[node.props]\ntext = \"waiting\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("scripts").join("pinger.py"), PINGER).unwrap();
    std::fs::write(dir.join("scripts").join("ponger.py"), PONGER).unwrap();
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();
    assert_eq!(hud_text(&session), "waiting");
    session.update(0.016);
    assert_eq!(hud_text(&session), "hit!");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn timer_fires_and_loops() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = make_timer_project("timer-loop");
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();
    assert_eq!(hud_text(&session), "0");
    for _ in 0..3 {
        session.update(0.016);
    }
    assert_eq!(hud_text(&session), "0");
    session.update(0.016);
    assert_eq!(hud_text(&session), "1");
    for _ in 0..4 {
        session.update(0.016);
    }
    assert_eq!(hud_text(&session), "2");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn vendored_import_resolves_in_session() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-sess-vendor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::create_dir_all(dir.join("vendor").join("vpkg")).unwrap();
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("vendor").join("vpkg").join("__init__.py"),
        "VALUE = 7\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"m\"\ntype = \"Label\"\nname = \"M\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/m.py\"\nclass = \"M\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scripts").join("m.py"),
        "import pite\nimport vpkg\n\nclass M(pite.Label):\n    def _ready(self):\n        self.text = str(vpkg.VALUE)\n",
    )
    .unwrap();
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();
    session.update(0.016);
    assert!(session.errors().is_empty(), "got: {:?}", session.errors());
    let text = session
        .host
        .with_tree(|t| match t.get(&NodeId::from("m".to_string())) {
            Some(n) => match n.props.get("text") {
                Some(PropValue::Str(s)) => s.clone(),
                _ => String::new(),
            },
            None => String::from("<gone>"),
        });
    assert_eq!(text, "7");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn zipped_wheel_import_resolves_in_session() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("pite-sess-zip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::create_dir_all(dir.join("vendor")).unwrap();
    let crafted = std::process::Command::new("python3")
        .arg("-c")
        .arg(
            "import sys, zipfile; z = zipfile.ZipFile(sys.argv[1], 'w'); z.writestr('zpkg/__init__.py', 'VALUE = 9\\n'); z.close()",
        )
        .arg(dir.join("vendor").join("zpkg-1.0-py3-none-any.whl"))
        .output();
    if !crafted.as_ref().is_ok_and(|o| o.status.success()) {
        eprintln!("skipped: python3 with zipfile unavailable");
        return;
    }
    std::fs::write(dir.join("pite.toml"), "[project]\nname = \"t\"\n").unwrap();
    std::fs::write(
        dir.join("scenes").join("main.ptscn"),
        "format_version = 1\nroot = \"root\"\n\n[[node]]\nid = \"root\"\ntype = \"Node2D\"\nname = \"Main\"\n\n[[node]]\nid = \"m\"\ntype = \"Label\"\nname = \"M\"\nparent = \"root\"\n\n[node.script]\npath = \"res://scripts/m.py\"\nclass = \"M\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("scripts").join("m.py"),
        "import pite\nimport zpkg\n\nclass M(pite.Label):\n    def _ready(self):\n        self.text = str(zpkg.VALUE)\n",
    )
    .unwrap();
    let scene = dir.join("scenes").join("main.ptscn");
    let mut session = GameSession::open(&scene, true).unwrap();
    session.update(0.016);
    assert!(session.errors().is_empty(), "got: {:?}", session.errors());
    let text = session
        .host
        .with_tree(|t| match t.get(&NodeId::from("m".to_string())) {
            Some(n) => match n.props.get("text") {
                Some(PropValue::Str(s)) => s.clone(),
                _ => String::new(),
            },
            None => String::from("<gone>"),
        });
    assert_eq!(text, "9");
    std::fs::remove_dir_all(&dir).ok();
}
