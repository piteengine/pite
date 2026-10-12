# Editor

Godot-style layout: menu bar on top (`File`, `Run`, `Project`) with play / pause / stop
icon buttons pinned to the bar's right end (mirroring the `Run` menu). `Scene` tree above `Assets` in the left panel;
`Viewport` / `Code` tabs in the center; `Inspector` on the right; `Console` at
the bottom. All toolbar glyphs are hand-drawn vector icons (`icons.rs`, painted
with the egui painter — never font glyphs, which the bundled Inter does not
cover). Theme tokens live in `pite-editor/src/theme.rs` (see `DESIGN.md` §4):
neutral near-black surfaces with a restrained teal accent used only for
selection, links, and the active widget stroke.

## Viewport

The viewport shows the real game frame: `EditorApp` owns a lazily created
`pite_render::OffscreenRenderer` (sized from the manifest's `window_width` /
`window_height`, the panel contain-fits and centers the image) and renders via
`session.draw_into(&mut renderer)` — the same camera/viewport traversal the
game window uses — then `render_to_rgba()` into an `egui::TextureHandle`
(`ColorImage::from_rgba_unmultiplied`, `NEAREST`). The texture re-renders only
while `playing` or when a `viewport_dirty` flag is set (add/delete/move/prop/save
ops); otherwise the last handle is reused. When no GPU is available the
renderer stays `None` forever and a dots painter remains as fallback (muted
overlay colors, node captions only on this path).

While playing, keyboard and mouse over the viewport reach the running scene:
`pump_game_input` forwards egui key/mouse state into the script input state
before each `session.frame()`, with the cursor rescaled into offscreen
pixels. Input stays out of the way — it only forwards while the pointer is
over the viewport, so the code pane and shortcuts keep priority.

## Save scene

`File` > `Save scene`, the save icon button, or `Ctrl+S` / `Cmd+S`
(`command` modifier). Serializes the live tree (`ops::build_doc` →
`pite_scene::save_scene`) and writes `scene_path`. Success and failure both
land in the console; saves never panic and never fail silently. `File` > `Quit`
closes the editor window; `Run` > `Play`/`Pause`/`Stop` mirrors the icon
buttons. Below the `Viewport` / `Code` tabs a path bar shows the open scene or
the open script with its own save icon button; the Code tab itself is just the
gutter plus a borderless editor filling the panel like the viewport.

The scene file on disk stays authoritative: save reads it first and re-attaches
its `[[instance]]` entries instead of flattening them into plain nodes. Prop
edits inside an instantiated subtree are written back as `overrides`, and edits
the format cannot express (rename, retype, reparent, add or delete inside a
subtree, removing an inherited prop) are refused with a console error rather
than silently dropped. Props serialize in sorted order, so a save → load → save
cycle is byte-identical and diffs stay readable.

## Project Settings

`Project` > `Project Settings…` opens a window over every known manifest
field: the `[project]` entries (name, author, main scene, Python, icon,
window size; `pite_version` shown read-only) plus `[export]` (`binary_name`,
extra staged files one per line). Unknown keys are never shown but survive
saves untouched. `Save` validates through the shared `validate_manifest`
(same errors `pite check` reports) and writes `pite.toml` via
`pite_project::save_manifest`, which serializes through sorted tables so a
save → load → save cycle is byte-identical. The viewport picks up a changed
window size immediately. Failures land in the window and the console; saves
never panic and never fail silently.

## Tree ops

Header `Add` opens an inline add-child form: a ComboBox over the registered
types (`Node`, `Node2D`, `Sprite2D`, `Camera2D`, `Timer`, `Label`, `Button`),
a name field, `Add`/`Cancel`. The new node goes under the selected node, or
the root when nothing is selected. Rows are drag-and-droppable via a dedicated
12 px grip handle left of the label, so click-to-select, double-click expand,
and right-click never fight it. Right-click on a row opens a context menu:
Delete (the root is protected and the attempt is logged), Move up / Move down
(sibling reorder), Expand / Collapse (branches).

Drop targeting is positional with a live preview: row edges mean *insert
before*, the middle band means *move under* — there is deliberately no
insert-after drop, use Move down. Dropping on empty tree space appends under
the root. Root takes only "under", has no handle, and refuses moves; cycles
fail in the console. The header keeps only the Add icon.

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
inspector panel is 240 px by default, bounded 240–360.

## Console

Bottom panel, 120 px by default and bounded 120–240, with a vertical
`ScrollArea` (`auto_shrink([false, false])`, `stick_to_bottom(true)`) so the
latest log line stays visible. Side panels bottom out at 240 px so nothing
collapses to a sliver.

## Error gutter

A real line-number gutter runs down the left of the code editor: one row per
line, error lines in warning yellow, the rest faint gray, details on hover.
Clicking a row moves the editor cursor there, focuses the pane, and scrolls
it into view. Marks come from `ops::gutter_marks` over the open file's
`session.errors()` entries plus `[lsp] …` server diagnostics. No gutter when
no file is open. The editor itself never wraps (one text row per line) and is
sized to its full content inside a shared scroll area, with numbers tiling
the laid-out galley rows, so gutter and code cannot drift apart; the editor never scrolls internally.

## Completion

Completions pop up as you type (every word character requests; `Ctrl+Space`
forces it anywhere) in a frameless list anchored at the cursor, constrained to
the screen. The list narrows on what you typed (optimistic local filtering
while the server answers), follows the arrow-key selection with the scroll,
and accepts with `Enter` — replacing the whole identifier under the cursor.
`Esc` dismisses (focus is handed back so typing resumes), as does typing a
non-word character, moving the cursor out of the word, or an empty server
answer. Stale responses (cursor moved past their trigger) are dropped by
request id, so fast typing never resurrects old lists. Mouse click accepts
too. See [`lsp.md`](lsp.md) for the protocol side.

## Assets

The `Assets` panel is a real file tree: collapsible folders, per-type file
icons, click a file to open it in the `Code` tab (scenes open in the tree
instead). `dist/`, `target/`, `.git/`, dotfiles, and the scene binary cache
stay hidden.

Headless logic lives in `pite-editor/src/ops.rs` (no egui) with unit tests:
add/remove round-trip, root removal refused, reparent cycle refused, prop
set/get round-trip, `build_doc` + `save_scene` + `parse_scene_str` round-trip,
and `gutter_marks` traceback extraction.
