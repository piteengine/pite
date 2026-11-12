---
icon: lucide/tv-minimal-play
---

# Scene Format

Pite scenes are defined as human-readable, version-controllable TOML documents (`.ptscn`). The plain-text TOML format is the single source of truth for all scene descriptions. Node identifiers are human-friendly strings (such as `root`, `player`, `ui`), rather than arbitrary UUIDs.

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

---

## Schema Rules

- **`format_version`**: Explicit schema version integer (currently `1`). An unsupported version causes an immediate parse failure to prevent data corruption.
- **`root`**: Identifies the scene's top-level root node in the `[[node]]` list. A node referencing an invalid `parent` is tolerated during editing but flagged as an error by `pite check`.
- **`type`**: Must correspond to a registered engine node class (`Node`, `Node2D`, `Sprite2D`, `Camera2D`, `Timer`, `Label`, `Button`, `StaticBody2D`, `CharacterBody2D`, `RigidBody2D`, `Area2D`, `CollisionShape2D`, `TileMap2D`). Unrecognized types produce validation errors.
- **`props`**: Extensible key-value table. Scalar values map to typed primitives; 2-element numeric arrays parse into `Vec2`; other types are preserved as text. **Unknown properties are preserved across saves** to ensure forward compatibility, and surfaced as warnings.
- **`script`**: Optional Python binding (`path` and `class`). If the referenced script file is missing, the engine creates a placeholder node and logs a warning rather than failing to load the scene.
- **`res://` Paths**: All asset and script references must use project-relative resource paths (`res://…`). Absolute paths are prohibited. Asset references are verified by `pite check`, which uses the content-addressed UID manifest to track moved or renamed files.

---

## Binary Scene Cache

While TOML provides readability and clean git diffs, runtime startup benefits from pre-parsed binary caching:

1. **Generation**: Upon first load, the engine serializes an optimized binary snapshot to `<scene_dir>/_cache/<stem>.<content-hash>.bin` (automatically gitignored and excluded from exports).
2. **Validation**: Subsequent loads verify the content hash in the filename. If the TOML source has changed, the stale cache file is pruned and rebuilt.
3. **Fallback**: If a binary cache file is corrupted or stamped with an incompatible version, the engine logs a warning and transparently falls back to parsing the TOML source.
4. **Verification**: `pite check` reports per-scene cache status (`hit`, `miss`, or `rebuilt`).

---

## Scene Instantiation

Scenes can be nested within other scenes as reusable components:

```toml
[[instance]]
scene = "res://scenes/enemy.ptscn"
parent = "root"
prefix = "e1_"

[instance.overrides]
"sprite.position" = [480.0, 144.0]
```

- **Subtree Cloning**: Loading clones the target scene's node hierarchy under `parent` (defaulting to the host scene root).
- **ID Namespacing**: Every node identifier in the instantiated subtree is prefixed with `prefix` to avoid collisions.
- **Property Overrides**: The `[instance.overrides]` table applies modifications keyed as `"node_id.prop"`.
- **Byte-Stable Persistence**: Saving preserves `[[instance]]` entries rather than inlining subtrees into the host scene. Property keys are written in sorted order, ensuring byte-stable output (see the [Editor Guide](../guides/editor#saving-scenes)).

---

## Rendering and Coordinate Spaces

- **Design Resolution**: The window size declared in `pite.toml` establishes the design resolution. The viewport scales content uniformly to fit the window while maintaining aspect ratio via letterboxing. Input coordinates map through the same projection.
- **Sprite2D**: Renders its `texture` centered at the node's global position, scaled by camera zoom and window scale. If a texture is missing, a magenta placeholder quad renders with a one-time console warning.
- **Camera2D**: The first active `Camera2D` in the scene centers the viewport on its global position. The optional `zoom` property (`float`, default `1.0`) scales the viewport.
- **Transform Propagation**: Global positions are computed by accumulating local `position` vectors along ancestor chains, so moving or rotating a parent node carries all descendants naturally.

---

## User Interface Elements

### Labels

- `Label` renders text centered on its global position using the bundled Inter font.
- `font_size` (`float`, default `16.0`) is expressed in world units and scales with camera zoom.
- `color` accepts `#rgb`, `#rrggbb`, or `#rrggbbaa` hex strings (defaulting to white).

### Buttons

- `Button` renders a stylized background box (`size = [width, height]` in world units, default `[120, 40]`) with centered caption text.
- Clicks require both mouse-press and mouse-release inside the button boundary.
- The engine provides a built-in `pressed` signal that can be connected directly without manual declaration.

---

## Tilemaps

- `TileMap2D` displays multi-layer grid tilemaps configured in an external `.ptmap` sidecar (`tilemap = "res://maps/level.ptmap"`).
- The `.ptmap` file defines the sprite atlas (`atlas = "res://…atlas.json"`), tile dimensions (`tile_size = [width, height]`), and layer definitions (`[[layer]]`).
- Layers declare `name`, visibility, collision settings (`collision_layer`, `collision_mask`), and sparse cell coordinates: `tiles = [[col, row, "frame"], …]`.
- When collisions are enabled, contiguous solid tiles are automatically merged into optimized static box colliders in Rapier2D to eliminate internal edge collisions.
