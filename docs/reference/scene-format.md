---
icon: lucide/tv-minimal-play
---

# Scene format

Canonical, human-editable TOML. Text is the source of truth; no binary format is authored (the runtime keeps its own cache, see below). Node ids are human strings (`D7`), never UUIDs.

```toml
format_version = 1
root = "root"

[[node]]
id = "root"
type = "Node2D"
name = "Main"

[[node]]
id = "player"
type = "Sprite2D"
name = "Player"
parent = "root"

[node.props]
texture = "res://assets/player.png"
position = [100.0, 200.0]

[node.script]
path = "res://scripts/player.py"
class = "Player"
```

## Rules

- `format_version` starts at 1. An unsupported version fails loudly at parse — never silently reinterpreted.
- `root` must name a node in the list. A node whose `parent` names nothing is tolerated at load and reported by `pite check` as an error.
- `type` must be registered (`Node`, `Node2D`, `Sprite2D`, `Camera2D`, `Timer`, `Label`, `Button`, `StaticBody2D`, `CharacterBody2D`, `RigidBody2D`, `Area2D`, `CollisionShape2D`, `TileMap2D`); anything else is a `check` error.
- `props` is open: scalars map to typed values, a 2-number array maps to `Vec2`, anything else is kept as text. **Unknown props warn and survive a save** (forward compat).
- `script` is optional. A missing script file is a `check` warning and loads as a placeholder node — never a load failure.
- All paths are project-relative (`res://…`); absolute paths are forbidden.
- Every `res://` string in `props` is an asset ref: `pite check` errors on missing
  files and tells you where a renamed asset went (uid-tracked, see `assets.md`).
  Run `pite check --reimport-assets` after renaming or editing assets outside the editor.

## Cache

TOML is the source of truth. The first load writes a binary snapshot next to
the scene at `<scene_dir>/_cache/<stem>.<content-hash>.bin` (gitignored, never
exported); later loads prefer it when valid. The filename hash invalidates on
any source change (stale files are pruned), and a version stamp inside the file
rebuilds on format bumps instead of misreading. A corrupt or mismatched cache
falls back to TOML with a loud warning — never silently. `pite check` reports
per-scene `hit` / `miss` / `rebuilt (reason)` status.

## Instantiation

```toml
[[instance]]
scene = "res://scenes/enemy.ptscn"
parent = "root"
prefix = "e1_"

[instance.overrides]
"sprite.position" = [480.0, 144.0]
```

Loading clones the referenced subtree under `parent` (`parent` defaults to the host root), remaps every id as `prefix + id` (collisions fail loudly), and applies overrides keyed `"node_id.prop"` (unknown targets fail loudly).

Saving preserves `[[instance]]` entries rather than inlining the subtree, and records prop edits inside an instance as overrides — see [`editor.md`](../guides/editor#save-scene). Prop keys are written in sorted order, so saves are byte-stable.

## Rendering

- The manifest size is the design resolution: the game opens maximized and scales content uniformly to fit, keeping aspect (letterboxed). Clicks map through the same transform.
- `Sprite2D` draws its `texture` (a `res://` png path) centered on the node's global position at texture size times the camera zoom (which folds in the window fit scale). A missing texture renders magenta and warns once.
- The first `Camera2D` node frames the view: the screen centers on its global position. Its optional `zoom` prop (number, default `1.0`) scales the world.
- Global position is local `position` plus every ancestor's — moving a parent carries its children.

## Text

- `Label` draws its `text` (string, default empty) centered on the node's global position, through the same camera as sprites. `font_size` (number, default `16.0`) is in world units and scales with zoom; `color` is `#rgb`, `#rrggbb`, or `#rrggbbaa` (default white, garbage falls back to white).
- Text rasterizes from the bundled Inter font; scripts and other nodes read/write it as a plain `text` prop.

## Buttons

- `Button` draws a `color` box (`size = [w, h]` in world units, default `[120, 40]`) centered on the node's global position, with its `text` captioned on top like a `Label`.
- Clicks are press-inside plus release-inside (mouse position maps through the camera). The engine declares and fires the node's `pressed` signal itself — just connect it like any signal, no declaration needed (re-declaring is harmless).

## Tilemaps

- `TileMap2D` renders 2D tilemaps defined in an external `.ptmap` TOML sidecar (`tilemap = "res://maps/level.ptmap"`).
- The `.ptmap` format specifies the atlas (`atlas = "res://...atlas.json"`), tile size (`tile_size = [w, h]`), and layers (`[[layer]]`).
- Each layer defines `name`, `visible` (bool), `collision_enabled` (bool), `collision_layer`, `collision_mask`, optional `solid_frames`, and sparse tiles `tiles = [[col, row, "frame"], ...]`.
- When collisions are enabled on a layer, solid tiles are greedily merged into minimal rectangular static box colliders in Rapier2D, preventing internal edge snagging.
