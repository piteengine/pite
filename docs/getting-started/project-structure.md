---
icon: lucide/folder-tree
---

# Project Structure

Understanding the layout and conventions of a Pite game project.

## Directory Layout

A standard Pite project organizes scenes, scripts, assets, and project settings into dedicated folders:

```text
my-game/
├── pite.toml               # Project manifest & configuration
├── assets.pite.toml        # Local asset registry & UID mapping (gitignored)
├── scenes/                 # Scene tree files (.ptscn)
│   ├── main.ptscn          # Initial entry scene
│   └── _cache/             # Binary scene cache (derived, gitignored)
├── scripts/                # Python gameplay scripts (.py)
│   ├── player.py
│   └── hud.py
├── assets/                 # Textures and sprite sheets (.png, .atlas.json)
├── sfx/                    # Sound effects and music (.wav, .ogg, .mp3, .flac)
├── vendor/                 # Vendored third-party Python wheels
└── dist/                   # Exported playable builds (gitignored)
```

---

## Project Manifest

The root `pite.toml` file declares project identity, display resolution, and build configuration:

```toml title="pite.toml"
[project]
name = "my-game"
pite_version = "0.3.0"
main_scene = "res://scenes/main.ptscn"
python = "3.12"
window_width = 1280
window_height = 720

[export]
# binary_name = "my-game"
# include = ["README.md"]

[vendor]
# pinned third-party dependencies managed by `pite vendor`
```

### Key Fields

- `name`: Human-readable name of your game.
- `main_scene`: The default scene loaded when the game launches (formatted as a `res://` path).
- `window_width` and `window_height`: The target design resolution. The game window scales to fit the player's monitor while preserving this aspect ratio with letterboxing.
- `python`: Target Python version (embedded CPython 3.12).
- `icon`: Optional project icon path (`res://icon.png`).

For exhaustive configuration options, see the [Manifest Reference](../reference/manifest).

---

## Scenes

Scenes are saved as `.ptscn` files. Pite scenes are human-readable TOML files representing a tree of nodes:

```toml title="scenes/main.ptscn"
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

### Key Principles

- **Git-Friendly Format**: Plain text scenes produce clear diffs in version control, making teamwork and branch merging straightforward.
- **Node IDs**: Descriptive string identifiers (such as `player` or `cam`) identify nodes across scenes and scripts.
- **Scene Instancing**: Scenes can instance other scenes using `[[instance]]` entries, keeping scenes modular without flattening or duplicating subtrees.
- **Binary Cache**: On first load, Pite generates a binary snapshot in `scenes/_cache/` keyed by content hash to accelerate load times. The original `.ptscn` file remains the source of truth, and this cache directory is safely ignored by version control.

For complete scene syntax and node property options, see the [Scene Format Reference](../reference/scene-format).

---

## Scripts

Gameplay logic resides in Python scripts under `scripts/`. Pite organizes scripts around predictable conventions:

- **One Class per File**: Each script defines a primary class in a `snake_case` file (for example, `scripts/player.py` defines `class Player(pite.Node2D)`).
- **Engine Lifecycle**:
    - `_ready(self)`: Called once when the node enters the active scene tree.
    - `_process(self, delta: float)`: Called every frame with the elapsed frame time in seconds. If a method is not needed, omitting it is completely valid.
- **Tree Navigation**: Access other nodes with `self.get_node("../Path")` or declarative descriptors such as `target = pite.node("../Target")`.

For full lifecycle details, input queries, and signal APIs, see the [Scripting API Reference](../reference/scripting-api).

---

## Assets and Paths

All project assets (sprites, textures, sprite sheet atlases, and audio clips) are addressed with the `res://` prefix, denoting paths relative to the project root:

- `texture = "res://assets/player.png"`
- `pite.play("res://sfx/coin.wav")`

### The UID Registry

Running `pite check --reimport-assets` scans all assets, computes content hashes, and maintains an `assets.pite.toml` registry file:

- **Rename Tracking**: When files are moved or renamed, the registry reconciles paths using asset UIDs, enabling automatic reference updates across scenes and scripts.
- **Local Import Cache**: This file serves as a local development cache and is excluded from source control and exports.

Learn more in the [Assets Guide](../guides/assets).

---

## Third-Party Packages

Pite exports bundle a self-contained CPython standard library while omitting the host machine's external `site-packages`. This prevents ambient system packages from causing discrepancies on player machines.

When your game needs an external Python package:

1. Add it through the CLI:

   ```sh title="Terminal"
   pite vendor add requests
   ```

2. The package wheels are downloaded into `vendor/` and recorded under `[vendor]` in `pite.toml`.
3. Pite bundles the `vendor/` directory into game exports automatically.

---

## Recommended Gitignore

When tracking your game with Git, commit scenes, scripts, assets, and `pite.toml`, while ignoring runtime caches and build outputs.

A standard `.gitignore` for Pite projects:

```gitignore title=".gitignore"
# Pite runtime and scene cache
_cache/
assets.pite.toml
.pitetoolchain/

# Build and export distributions
/dist
/target

# Python bytecode and caches
__pycache__/
*.py[cod]
*$py.class
.ruff_cache/
```

---

## Next Steps

Now that you understand how a Pite project is organized:

- Build a game from the ground up in [Your First Game](../tutorials/first-game).
- Explore the interactive tools in the [Editor Guide](../guides/editor).
