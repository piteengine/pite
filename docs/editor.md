# Editor

Godot-style layout: menu bar on top (`File`, `Run`) with play / pause / stop /
save icon buttons at top-right; `Scene` tree above `Assets` in the left panel;
`Viewport` / `Code` tabs in the center; `Inspector` on the right; `Console` at
the bottom. All toolbar glyphs are hand-drawn vector icons (`icons.rs`, painted
with the egui painter — never font glyphs, which the bundled Inter does not
cover). Theme tokens live in `pite-editor/src/theme.rs` (see `DESIGN.md` §4):
neutral near-black surfaces with a restrained teal accent used only for
selection, links, and the active widget stroke.

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

`File` > `Save scene`, the save icon button, or `Ctrl+S` / `Cmd+S`
(`command` modifier). Serializes the live tree (`ops::build_doc` →
`pite_scene::save_scene`) and writes `scene_path`. Success and failure both
land in the console; saves never panic and never fail silently. `File` > `Quit`
closes the editor window; `Run` > `Play`/`Pause`/`Stop` mirrors the icon
buttons. Below the `Viewport` / `Code` tabs a path bar shows the open scene or
the open script with its own Save button; the Code tab itself is just the
gutter plus a borderless editor filling the panel like the viewport.

## Tree ops

Header `Add` opens an inline add-child form: a ComboBox over the registered
types (`Node`, `Node2D`, `Sprite2D`, `Camera2D`, `Timer`, `Label`, `Button`),
a name field, `Add`/`Cancel`. The new node goes under the selected node, or
the root when nothing is selected. Header `Del` deletes the selected node; the
root is protected and the attempt is logged. Rows are drag-and-droppable via a
dedicated grip handle left of the label: only the 12 px handle arms a drag, so
click-to-select, double-click expand, and right-click never fight it. The grip
dots paint on row hover only (allocation stays hover-sensitive); they are
centered on the label's vertical middle after layout, so they track the text
exactly. Right-click on a row opens a context menu: Delete, Move up / Move down
(sibling reorder), Expand / Collapse (branches).

Drop targeting is positional with a live preview: the top and bottom edges of
a row both mean *insert before* (same parent, top underline), the middle band
means *move under* (row outline). There is deliberately no insert-after drop —
use the context menu's Move down. Dropping on empty tree space appends under
the root. Root takes only "under". Root has no handle and refuses moves;
cycles fail in the console. Header keeps only the Add icon — Delete lives in
the context menu.

Branch rows are a bare `selectable_label(name (type))`: single click selects,
double-click expands/collapses children (indented beneath while the parent's
per-node open flag holds). No toggle buttons, no background boxes — rows carry
no frame; while a drag is over a row only a thin underline marks the drop
target. Leaf rows are a plain `selectable_label`.

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
| future variants | compile error here by design (no silent fallback) |

Title line and script section (label + `Open script`) are unchanged. The
inspector panel keeps its 280 px default width but is resizable.

## Console

Bottom panel, resizable with `min_height` 100, `ScrollArea` with `max_height`
200, `auto_shrink([false, false])` so the log list always spans the full width
and the scrollbar sits at the far right end, plus `stick_to_bottom(true)` so
the latest log line stays visible. Side panels carry `min_width` (inspector
200) so nothing collapses to a sliver.

## Error gutter

Above the code editor, one `line N: msg` monospace warning-colored button per
mark. Clicking does nothing (jump is out of scope). Marks come from
`ops::gutter_marks` over `session.errors()` entries that mention the open
file's name (`File "...", line N` → 1-based line + the exception line).
No gutter when no file is open or no entries carry a line number.

Headless logic lives in `pite-editor/src/ops.rs` (no egui) with unit tests:
add/remove round-trip, root removal refused, reparent cycle refused, prop
set/get round-trip, `build_doc` + `save_scene` + `parse_scene_str` round-trip,
and `gutter_marks` traceback extraction.
