# Editor

Panels (left to right, unchanged order): Scene tree, Assets | Viewport + Code
(center) | Inspector (right) | Console (bottom). Theme and viewport rendering
are untouched by the level-up slice.

## Save scene

Transport bar `Save scene` button or `Ctrl+S` / `Cmd+S` (`command` modifier).
Serializes the live tree (`ops::build_doc` → `pite_scene::save_scene`) and
writes `scene_path`. Success and failure both land in the console; saves never
panic and never fail silently.

## Tree ops

Header `+` opens an inline add-child form: a ComboBox over the registered
types (`Node`, `Node2D`, `Sprite2D`, `Camera2D`, `Timer`, `Label`, `Button`),
a name field, `Add`/`Cancel`. The new node goes under the selected node, or
the root when nothing is selected. Header `−` deletes the selected node; the
root is protected and the attempt is logged. Drag a row onto another row to
reparent under it (egui 0.32 `dnd_drag_source` / `dnd_drop_zone`); cycles and
other failures are reported in the console.

## Inspector type mapping

Generic over `node.props`, sorted by key — only what exists is rendered,
nothing is auto-inserted:

| `PropValue` | Widget |
|---|---|
| `Num(f)` | `DragValue`, speed 0.1 |
| `Int(i)` | integer `DragValue` |
| `Str(s)` | single-line text edit |
| `Bool(b)` | checkbox |
| `Vec2(x, y)` | two `DragValue`s |
| future variants | read-only `key: debug` label |

Title line and script section (label + `Open script`) are unchanged.

## Error gutter

Above the code editor, one `line N: msg` monospace warning-colored button per
mark. Clicking does nothing (jump is out of scope). Marks come from
`ops::gutter_marks` over `session.errors()` entries that mention the open
file's name (`File "...", line N` → 1-based line + first line of the error).
No gutter when no file is open or no entries carry a line number.

Headless logic lives in `pite-editor/src/ops.rs` (no egui) with unit tests:
add/remove round-trip, root removal refused, reparent cycle refused, prop
set/get round-trip, `build_doc` + `save_scene` + `parse_scene_str` round-trip,
and `gutter_marks` traceback extraction.
