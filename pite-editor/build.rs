// SPDX-License-Identifier: MIT OR Apache-2.0
//! Generates `pite.pyi` (type stubs) and `pite.py` (source sibling for the
//! type checker) into `$OUT_DIR`. `lsp.rs` embeds them via
//! `include_str!(concat!(env!("OUT_DIR"), "/pite.pyi"))`, so the shipped
//! stubs can never drift from the schema — adding a type to
//! `pite_core::NODE_SCHEMAS` auto-adds its stub class.
//!
//! The pyclass hierarchy is flatter than the schema chain because abstract
//! bases (`Control`, `CollisionObject2D`, `PhysicsBody2D`) have no Python
//! class. The generator walks schema inheritance and skips abstract nodes.

use std::fmt::Write;

use pite_core::{NodeTypeSchema, NODE_SCHEMAS};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../pite-core/src/schema.rs");
    println!("cargo:rerun-if-changed=../pite-core/src/nodes");

    // The list of types is schema-derived; the class *bodies* are hand-
    // mapped below (see `class_members`) to match each pyclass's actual
    // `pite_props!` declarations exactly. A test in `src/lsp.rs` fails if
    // a concrete schema type has no emitted class, so schema additions
    // can't silently land without a stub.
    let stubs = generate_stub();
    let source = generate_source();
    let out = std::env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    std::fs::write(std::path::Path::new(&out).join("pite.pyi"), stubs).expect("write pite.pyi");
    std::fs::write(std::path::Path::new(&out).join("pite.py"), source).expect("write pite.py");

    generate_docs();
}

/// Concrete types, parent-first. `Node` is the root; `pite_core::NODE_SCHEMAS`
/// already lists types in a topological order that respects the parent
/// relationship, so a straight filter preserves it.
fn concrete_types() -> Vec<&'static NodeTypeSchema> {
    NODE_SCHEMAS
        .iter()
        .copied()
        .filter(|s| !s.abstract_class)
        .collect()
}

fn python_parent(type_name: &str) -> Option<&'static str> {
    let mut current = pite_core::node_type_schema(type_name)?.inherits;
    while let Some(name) = current {
        let schema = pite_core::node_type_schema(name)?;
        if !schema.abstract_class {
            return Some(schema.type_name);
        }
        current = schema.inherits;
    }
    None
}

/// The exact member list a pyclass exposes. Kept in hand-parity with
/// `pite-script/src/python/nodes/*.rs` — every entry here corresponds to a
/// `pite_props!` accessor, an explicit `#[getter]`, or a `#[pymethods]`
/// callable. The `generated_stub_covers_pyclass_surface` test in `src/lsp.rs`
/// catches missing *classes*; missing *members* fall through to
/// `__getattr__`/`__setattr__` on `Node` and stay silent at runtime, so
/// they're caught by inspection rather than a test. When adding a pyclass
/// accessor, add it here too.
fn class_members(type_name: &str) -> &'static [&'static str] {
    match type_name {
        "Node" => &[
            "node_path: str",
            "def get_node(self, path: str) -> Node: ...",
            "def get_custom_property(self, key: str) -> Any: ...",
            "def set_custom_property(self, key: str, value: Any) -> None: ...",
            "def remove_custom_property(self, key: str) -> bool: ...",
            "def _ready(self) -> None: ...",
            "def _process(self, delta: float) -> None: ...",
            "def _physics_process(self, delta: float) -> None: ...",
            "def __getattr__(self, name: str) -> Any: ...",
            "def __setattr__(self, name: str, value: Any) -> None: ...",
        ],
        "Node2D" => &["position: Vec2", "rotation: float"],
        "Sprite2D" => &["texture: str", "atlas: str", "frame: str"],
        "Camera2D" => &["zoom: float"],
        "CollisionShape2D" => &[
            "shape: str",
            "size: Vec2",
            "radius: float",
            "height: float",
            "disabled: bool",
        ],
        "Area2D" => &[
            "collision_layer: int",
            "collision_mask: int",
            "body_entered: Signal",
            "body_exited: Signal",
            "area_entered: Signal",
            "area_exited: Signal",
        ],
        "StaticBody2D" => &[
            "collision_layer: int",
            "collision_mask: int",
            "friction: float",
            "restitution: float",
        ],
        "CharacterBody2D" => &[
            "velocity: Vec2",
            "up_direction: Vec2",
            "friction: float",
            "restitution: float",
            "collision_layer: int",
            "collision_mask: int",
            "is_on_floor: bool",
            "is_on_wall: bool",
            "is_on_ceiling: bool",
            "def move_and_slide(self) -> bool: ...",
        ],
        "RigidBody2D" => &[
            "mass: float",
            "gravity_scale: float",
            "restitution: float",
            "friction: float",
            "lock_rotation: bool",
            "collision_layer: int",
            "collision_mask: int",
            "def apply_impulse(self, impulse: tuple[float, float] | Vec2) -> None: ...",
            "def apply_force(self, force: tuple[float, float] | Vec2) -> None: ...",
        ],
        "Timer" => &[
            "wait_time: float",
            "current_elapsed: float",
            "timeout: Signal",
        ],
        "Label" => &[
            "text: str",
            "font_size: float",
            "color: tuple[int, int, int, int]",
        ],
        "Button" => &[
            "text: str",
            "size: Vec2",
            "font_size: float",
            "color: tuple[int, int, int, int]",
            "disabled: bool",
            "pressed: Signal",
        ],
        _ => &[],
    }
}

fn emit_class(out: &mut String, schema: &'static NodeTypeSchema) {
    match python_parent(schema.type_name) {
        Some(parent) => {
            let _ = writeln!(out, "class {}({}):", schema.type_name, parent);
        }
        None => {
            let _ = writeln!(out, "class {}:", schema.type_name);
        }
    }
    let members = class_members(schema.type_name);
    if members.is_empty() {
        out.push_str("    ...\n\n");
        return;
    }
    for m in members {
        out.push_str("    ");
        out.push_str(m);
        out.push('\n');
    }
    out.push('\n');
}

/// Module-level surface — `Signal`, `Vec2`, `InputQuery`, free functions.
/// Hand-written because it's not schema-derived (no node type owns these).
/// Kept in sync with `pite-script/src/python/mod.rs` and `vec2.rs`.
const HEADER: &str = "\
# SPDX-License-Identifier: MIT OR Apache-2.0
# AUTO-GENERATED from pite_core::NODE_SCHEMAS — do not edit by hand.
# Regenerate by building pite-editor; source in pite-editor/build.rs.
from __future__ import annotations

from typing import Any, Iterator

class Signal:
    def emit(self, *values: object) -> None: ...
    def connect(self, handler: object) -> None: ...

def signal(*types: object) -> Signal: ...

class Vec2:
    x: float
    y: float
    def __init__(self, x: float = 0.0, y: float = 0.0) -> None: ...
    def __add__(self, other: Vec2 | tuple[float, float]) -> Vec2: ...
    def __sub__(self, other: Vec2 | tuple[float, float]) -> Vec2: ...
    def __mul__(self, scalar: float) -> Vec2: ...
    def __rmul__(self, scalar: float) -> Vec2: ...
    def __truediv__(self, scalar: float) -> Vec2: ...
    def __iadd__(self, other: Vec2 | tuple[float, float]) -> Vec2: ...
    def __isub__(self, other: Vec2 | tuple[float, float]) -> Vec2: ...
    def __imul__(self, scalar: float) -> Vec2: ...
    def __itruediv__(self, scalar: float) -> Vec2: ...
    def __len__(self) -> int: ...
    def __getitem__(self, idx: int) -> float: ...
    def __setitem__(self, idx: int, val: float) -> None: ...
    def __iter__(self) -> Iterator[float]: ...
    def length(self) -> float: ...
    def normalized(self) -> Vec2: ...
    def to_tuple(self) -> tuple[float, float]: ...

class InputQuery:
    def __getattr__(self, name: str) -> bool: ...

held: InputQuery
pressed: InputQuery
released: InputQuery

def mouse() -> Vec2: ...
def frames(atlas: str) -> list[str]: ...
def play(path: str, volume: float = ...) -> int: ...
def stop(id: int) -> None: ...
def set_volume(id: int, volume: float) -> None: ...
def is_playing(id: int) -> bool: ...
def child(name: str) -> Any: ...
def node(path: str) -> Any: ...
def raycast(from_pos: tuple[float, float] | Vec2, to_pos: tuple[float, float] | Vec2, mask: int = ...) -> dict[str, Any] | None: ...

";

fn generate_stub() -> String {
    let mut out = String::from(HEADER);
    for schema in concrete_types() {
        emit_class(&mut out, schema);
    }
    out
}

fn generate_source() -> String {
    "\
# SPDX-License-Identifier: MIT OR Apache-2.0
\"\"\"Source sibling for the shipped `pite.pyi` stubs.

The real `pite` module is native, registered from Rust via PyO3
(`append_to_inittab!`); there is no Python implementation. This file exists
only so type checkers resolve `import pite` to a source file — types always
come from `pite.pyi`. Never imported at runtime; never put on `sys.path`.
\"\"\"
"
    .to_string()
}

// ---------------------------------------------------------------------------
// docs/reference/nodes/*.md — one markdown file per concrete node type,
// plus an index. Written into the workspace `docs/` tree (skipped when
// unchanged so `git status` stays quiet unless the schema actually moved).
// ---------------------------------------------------------------------------

fn generate_docs() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let root = std::path::Path::new(&manifest)
        .parent()
        .expect("workspace root");
    let dir = root.join("docs").join("reference").join("nodes");
    std::fs::create_dir_all(&dir).expect("create docs dir");

    let mut names: Vec<&str> = Vec::new();
    for schema in concrete_types() {
        write_if_changed(
            &dir.join(format!("{}.md", schema.type_name)),
            &render_node_doc(schema),
        );
        names.push(schema.type_name);
    }
    names.sort_unstable();

    let mut index = String::from(
        "# Node Reference\n\n\
         Every node type in the engine, derived from `pite_core::NODE_SCHEMAS` \
         at build time. Do not edit these files by hand — `pite-editor/build.rs` \
         rewrites them whenever the schema changes.\n\n",
    );
    for name in &names {
        let desc = pite_core::node_type_schema(name)
            .map(|s| s.description)
            .unwrap_or("");
        index.push_str(&format!("- [`{name}`]({name}.md) — {desc}\n"));
    }
    write_if_changed(&dir.join("index.md"), &index);
}

fn write_if_changed(path: &std::path::Path, content: &str) {
    if let Ok(existing) = std::fs::read_to_string(path) {
        if existing == content {
            return;
        }
    }
    std::fs::write(path, content).expect("write docs file");
}

fn render_node_doc(schema: &pite_core::NodeTypeSchema) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", schema.type_name));
    out.push_str(&format!("{}\n\n", schema.description));

    let chain = pite_core::inheritance_chain(schema.type_name);
    let ancestors: Vec<&str> = chain
        .iter()
        .copied()
        .filter(|n| *n != schema.type_name)
        .collect();
    if !ancestors.is_empty() {
        let links: Vec<String> = ancestors.iter().map(|n| format!("`{n}`")).collect();
        out.push_str(&format!("**Inherits:** {}\n\n", links.join(" → ")));
    }

    let mut seen_keys = std::collections::HashSet::new();
    let mut sections: Vec<(&str, Vec<&pite_core::PropSchema>)> = Vec::new();
    for owner in &chain {
        let props = pite_core::type_props(owner);
        let unshadowed: Vec<&pite_core::PropSchema> =
            props.iter().filter(|p| seen_keys.insert(p.key)).collect();
        if !unshadowed.is_empty() {
            sections.push((*owner, unshadowed));
        }
    }
    if sections.is_empty() {
        out.push_str("_No properties._\n");
        return out;
    }
    out.push_str("## Properties\n\n");
    for (owner, props) in sections {
        out.push_str(&format!("### {}\n\n", owner));
        out.push_str("| Property | Type | Default | Description |\n");
        out.push_str("|---|---|---|---|\n");
        for p in props {
            let ty = default_type_name(&p.default_value);
            let default = format_default(&p.default_value);
            let doc = if p.doc.is_empty() { "—" } else { p.doc };
            out.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                p.key, ty, default, doc
            ));
        }
        out.push('\n');
    }
    out
}

fn default_type_name(v: &pite_core::DefaultPropVal) -> &'static str {
    use pite_core::DefaultPropVal::*;
    match v {
        Str(_) => "str",
        Num(_) => "number",
        Int(_) => "int",
        Bool(_) => "bool",
        Vec2(_, _) => "vec2",
        Color(_) => "color",
    }
}

fn format_default(v: &pite_core::DefaultPropVal) -> String {
    use pite_core::DefaultPropVal::*;
    match v {
        Str(s) => format!("`{:?}`", s),
        Num(n) => format!("`{}`", n),
        Int(i) => format!("`{}`", i),
        Bool(b) => format!("`{}`", b),
        Vec2(x, y) => format!("`({}, {})`", x, y),
        Color([r, g, b, a]) => format!("`#{:02x}{:02x}{:02x}{:02x}`", r, g, b, a),
    }
}
