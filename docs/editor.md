# Editor

Panels (left to right, unchanged order): Scene tree, Assets | Viewport + Code
(center) | Inspector (right) | Console (bottom). A `File` menu bar sits above
the transport bar. Theme tokens live in `pite-editor/src/theme.rs` (see
`DESIGN.md` §4 for the mapping): neutral near-black surfaces with a restrained
teal accent used only for selection, links, and the active widget stroke.

## Viewport

The viewport shows the real game frame: `EditorApp` owns a lazily created
`pite_render::OffscreenRenderer` (fixed offscreen `640x400`, egui scales the
image) and renders via `session.draw_into(&mut renderer)` — the same
camera/viewport traversal the game window uses — then `render_to_rgba()` into
an `egui::TextureHandle` (`ColorImage::from_rgba_unmultiplied`, `NEAREST`).
Node captions overlay the texture as before. The texture re-renders only while
`playing` or when a `viewport_dirty` flag is set (add/delete/move/prop/save
ops); otherwise the last handle is reused. When no GPU is available the
renderer stays `None` forever and the old dots painter remains as fallback
(muted overlay colors).

## Save scene

Transport bar `Save scene` button, `File` > `Save scene`, or `Ctrl+S` / `Cmd+S`
(`command` modifier). Serializes the live tree (`ops::build_doc` →
`pite_scene::save_scene`) and writes `scene_path`. Success and failure both
land in the console; saves never panic and never fail silently. `File` > `Quit`
closes the editor window.

## Tree ops

Header `Add` opens an inline add-child form: a ComboBox over the registered
types (`Node`, `Node2D`, `Sprite2D`, `Camera2D`, `Timer`, `Label`, `Button`),
a name field, `Add`/`Cancel`. The new node goes under the selected node, or
the root when nothing is selected. Header `Del` deletes the selected node; the
root is protected and the attempt is logged. Header `Out` reparents the
selection to its grandparent (already-at-top is a no-op with a log line); `In`
reparents it under its previous sibling. Cycles and other failures are
reported in the console. (Row drag-and-drop was tried and removed: drag
sources hijacked click-to-select, so reparenting is explicit buttons.) All
header and transport buttons are text (`Add`, `Del`, `Out`, `In`, `Play` /
`Pause`, `Stop`, `Save scene`) with hover tooltips — no symbolic or emoji
glyphs, which the bundled Inter font does not cover.

Branch rows are a horizontal pair: a `[+]` / `[-]` toggle button plus a
`selectable_label(name (type))` that selects on click. Children render indented
beneath while the parent's per-node open flag holds (a `HashSet<String>`
defaulting to open). Leaf rows are a plain `selectable_label`.

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

Title line and script section (label + `Open script`) are unchanged. The
inspector panel keeps its 280 px default width but is resizable.

## Console

Bottom panel, resizable, `ScrollArea` with `max_height` 200 and
`stick_to_bottom(true)` so the latest log line stays visible. All side panels
(tree, assets, inspector) and the console are resizable; the inspector keeps
its 280 default.

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
