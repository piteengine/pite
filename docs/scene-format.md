# Scene format (`.pitescene`)

Canonical, human-editable TOML. Text is the source of truth; there is no binary format. Node ids are human strings (`D7`), never UUIDs.

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
- `type` must be registered (`Node`, `Node2D`, `Sprite2D`, `Camera2D`, `Timer`); anything else is a `check` error.
- `props` is open: scalars map to typed values, a 2-number array maps to `Vec2`, anything else is kept as text. **Unknown props warn and survive a save** (forward compat).
- `script` is optional. A missing script file is a `check` warning and loads as a placeholder node — never a load failure.
- All paths are project-relative (`res://…`); absolute paths are forbidden.

## Instantiation

```toml
[[instance]]
scene = "res://scenes/enemy.pitescene"
parent = "root"
prefix = "e1_"

[instance.overrides]
"sprite.position" = [300.0, 120.0]
```

Loading clones the referenced subtree under `parent` (`parent` defaults to the host root), remaps every id as `prefix + id` (collisions fail loudly), and applies overrides keyed `"node_id.prop"` (unknown targets fail loudly).
