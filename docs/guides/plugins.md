# Plugins

Pite supports Python-based **plugins** under `res://plugins/<id>/` that can extend both game runtime (custom node types, autoload singletons, `Canvas2D` custom drawing) and the trusted `pite` editor (bottom panel tabs, inspector sections, menu actions, and viewport overlays/pointer tools).

## Directory Layout & Manifest

Each plugin lives in a `snake_case` directory under `res://plugins/<id>/` with a `plugin.toml` manifest:

```text
my_game/
├── pite.toml
└── plugins/
    └── dialogue_box/
        ├── plugin.toml
        ├── __init__.py
        ├── runtime.py
        └── editor.py
```

`plugins/` is automatically placed on Python's `sys.path` and Pyright's `extraPaths`, so both game scripts and plugins can import `import dialogue_box` directly.

### `plugin.toml`

```toml
id = "dialogue_box"
name = "Dialogue Box"
version = "0.1.0"
pite_version = ">=0.4.0"
description = "Typewriter dialogue box node and editor preview."
runtime = "dialogue_box.runtime:DialogueRuntime"
editor = "dialogue_box.editor:DialogueEditor"

[autoloads]
DialogueState = "dialogue_box.runtime:DialogueState"

[[nodes]]
type_name = "DialogueBox"
inherits = "Control"
description = "Displays paginated dialogue."
script = "dialogue_box.runtime:DialogueBox"

[[nodes.props]]
key = "speed"
type = "float"
default = 40.0

[[nodes.signals]]
name = "line_finished"
payload = ["str"]

[props]
dialogue_locale = "str"

[dependencies]
# Third-party Python packages verified against vendor/
```

### Enabling in `pite.toml`

Plugins are enabled per project in `pite.toml`:

```toml
[plugins]
dialogue_box = { enabled = true }
```

## CLI Workflow (`pite plugin`)

```sh
pite plugin new <id>                     # Scaffold res://plugins/<id>/
pite plugin add <path|git|url|zip>       # Install disabled by default (enabled = false)
pite plugin add <git-url> --rev <tag>    # Clone a specific tag/branch/commit
pite plugin enable <id>                  # Set enabled = true in pite.toml
pite plugin disable <id>                 # Set enabled = false in pite.toml
pite plugin list                         # List configured plugins and status
pite plugin remove <id>                  # Remove from pite.toml and delete files
```

Newly added plugins always start with `enabled = false` so you can inspect `plugin.toml` and Python code before enabling them.

## Runtime API

- **Runtime & Autoload Singletons**:
    - `pite.plugin("<id>")` returns the instantiated `runtime` entrypoint object.
    - `pite.autoload("<Name>")` returns the named singleton from `[autoloads]`.
    - Autoloads persist across scene reloads, receive `on_scene_loaded(scene_path)`, and tick `_process(dt)` and `_physics_process(dt)` **after** scene nodes.
- **Custom Nodes**:
    - Nodes declared in `[[nodes]]` inherit properties and signals from their `inherits` base type and appear in the editor's Add Node hierarchy and Inspector.
    - When `script` is set on `[[nodes]]` and a scene node has no `[node.script]`, Pite automatically instantiates that default class and binds live descriptors so `self.<prop>` and `proxy.<prop>` read/write scene properties directly.
- **Per-Node Custom Drawing (`_draw`)**:
    - Any node script may define `def _draw(self, canvas: pite.Canvas2D):`.
    - `Node2D` subclasses draw in node-local coordinates (offset by global position and inheriting `z_index`); plain `Node` subclasses draw in world coordinates.
    - `Canvas2D` provides `draw_rect(pos, size, color=None, z_index=None)`, `draw_line(start, end, color=None, width=1.0, z_index=None)`, `draw_text(text, pos, size=16.0, color=None, z_index=None)`, and `draw_sprite(texture, pos, rotation=0.0, z_index=None)`.

## Editor Extensions & Workspace Trust

`editor` entrypoints run **only** when the workspace is trusted (`workspace_trusted == true`). In Safe Mode, declarative `[[nodes]]` and `[props]` still load for scene inspection, while all Python execution is skipped.

An `editor` class can implement any combination of:

- `on_enable(self, ctx)` / `on_ready(self, ctx)`
- `on_bottom_panel(self, ui, ctx)` — adds a tab alongside `Console` in the bottom panel.
- `on_inspector(self, ui, ctx, node_id, node_type)` — adds a section below node properties in the Inspector.
- `menu_items(self) -> list[str]` + `on_menu_action(self, label, ctx)` — adds actions under the **Plugins** menu.
- `on_viewport_overlay(self, ui, ctx)` and `on_viewport_pointer(self, event, ctx) -> bool` — viewport overlays and pointer hooks (paused during Play; returning `True` consumes the pointer event).

`EditorUI` is strictly scoped to the active UI callback (`label`, `heading`, `button`, `small_button`, `checkbox`, `text_edit`, `slider_float`, `slider_int`, `drag_float`, `separator`, `collapsing`, `horizontal`, `vertical`), and `EditorContext` provides `selected_node()`, `scene_path()`, `nodes()`, `log(msg)`, `set_prop(node_id, key, value)`, `add_node(type_name, name, parent_id=None)`, `open_scene(path)`, and `save_scene()`.

## Checking & Exporting

- `pite check` validates enabled plugins, `pite_version` compatibility, type/prop/signal/autoload collisions, vendored `[dependencies]`, and `res://` asset references inside plugin manifests and scripts.
- `pite export` copies only enabled plugins into the exported bundle and strips `editor` entrypoints so editor-only code never ships in exported games.
