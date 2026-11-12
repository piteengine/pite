---
icon: lucide/file-box
title: Assets
---

# Assets Guide

Manage textures, sprite sheet atlases, and audio clips using project-relative resource paths and UID-backed tracking.

Pite provides an asset pipeline designed to make asset referencing clear and resilient to file renames and folder reorganizations.

---

## Resource Paths

Assets in Pite are always referenced using the `res://` path scheme, which represents paths relative to the project root directory:

- In scenes: `texture = "res://assets/player.png"`
- In scripts: `pite.play("res://sfx/coin.wav")`

Using project-relative paths ensures that your scenes and code remain portable across machines and operating systems without hardcoding absolute paths.

---

## The UID Registry

Renaming or moving an asset file outside the editor often breaks path references in traditional engines. Pite avoids this through an automatic asset registry.

When you run `pite check --reimport-assets`, Pite inspects your files, calculates content hashes, and records them in `assets.pite.toml`:

```toml title="assets.pite.toml"
[[asset]]
path = "assets/player.png"
uid = "a8f3b201"
hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
size = 4096
```

- **Opaque UIDs**: Each unique file is assigned a stable UID.
- **Safe Renames**: If you move `assets/player.png` to `assets/characters/player.png`, the engine recognizes the file by its hash/UID.
- **Local Import Cache**: `assets.pite.toml` is a local cache file that is safely excluded from version control and game exports.

---

## Reimporting Assets

When you add, move, or rename assets outside the editor, update the registry with:

```sh title="Terminal"
pite check --reimport-assets
```

This command:

1. Re-scans project files for supported formats by inspecting magic header bytes.
2. Updates `assets.pite.toml` with current paths and hashes.
3. Automatically rewrites all quoted `res://` paths in `.ptscn` scenes and `.py` scripts whenever a known UID has moved to a new path.

`pite check` without the `--reimport-assets` flag never alters files on disk.

---

## Supported Formats

Pite supports the following file formats natively:

| Asset Type         | Supported Extensions            | Verification                                                      |
| :----------------- | :------------------------------ | :---------------------------------------------------------------- |
| **Graphics**       | `.png`                          | PNG magic byte validation                                         |
| **Audio**          | `.wav`, `.ogg`, `.mp3`, `.flac` | Header validation (`RIFF...WAVE`, `OggS`, ID3/frame sync, `fLaC`) |
| **Sprite Atlases** | `.atlas.json`                   | JSON schema validation paired with a sibling PNG                  |

---

## Sprite Sheets and Atlases

For 2D games, packing multiple sprites into a single image (a sprite sheet or texture atlas) improves rendering efficiency by batching draw calls into a single GPU texture operation.

A Pite sprite sheet consists of a PNG image paired with a sidecar JSON file named `<sheet>.atlas.json`:

```json title="assets/sheet.atlas.json"
{
    "texture": "sheet.png",
    "size": [128, 64],
    "frames": {
        "player": { "x": 0, "y": 0, "w": 64, "h": 64 },
        "enemy": { "x": 64, "y": 0, "w": 64, "h": 64 }
    },
    "animations": {
        "walk": {
            "frames": ["player", "enemy"],
            "fps": 8.0,
            "loop": true
        }
    }
}
```

### Using an Atlas in Scenes

Attach an atlas frame to a `Sprite2D` node by specifying `atlas` and `frame`:

```toml title="scenes/main.ptscn"
[[node]]
id = "player"
type = "Sprite2D"
name = "Player"
parent = "root"

[node.props]
atlas = "res://assets/sheet.atlas.json"
frame = "player"
position = [200.0, 300.0]
```

Or play 2D flipbook frame sequences using an `AnimatedSprite2D` node:

```toml title="scenes/main.ptscn"
[[node]]
id = "player"
type = "AnimatedSprite2D"
name = "Player"
parent = "root"

[node.props]
atlas = "res://assets/sheet.atlas.json"
autoplay = "walk"
position = [200.0, 300.0]
```

!!! Note
    A `Sprite2D` node can draw either a standalone `texture` or an `atlas` frame, but cannot use both at the same time.

### Controlling Frames in Python

You can read and modify atlas properties dynamically in gameplay scripts:

```python title="scripts/player.py"
import pite


class Player(pite.Sprite2D):
    def _ready(self):
        # Query all available frame names in the atlas
        available_frames = pite.frames("res://assets/sheet.atlas.json")

    def _process(self, delta: float):
        # Switch frames based on state
        if pite.held.right:
            self.frame = "run"
        else:
            self.frame = "idle"
```

---

## Static Validation with `pite check`

Running `pite check` performs static asset diagnostics across your project:

- **Missing Assets**: Reports an error if a scene property or `pite.play()` call references a file that does not exist on disk.
- **Moved Assets**: Detects if an asset was moved and prompts you to run `pite check --reimport-assets` to update references.
- **Unreachable Assets**: Emits a warning if an asset exists in your project but is never referenced by any scene reachable from `main_scene`.
- **Invalid Atlas Frames**: Reports an error if a scene references a frame name that is not defined in the corresponding `.atlas.json`.
