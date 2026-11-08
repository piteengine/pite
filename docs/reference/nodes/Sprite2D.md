# Sprite2D

Displays a 2D texture or one frame from a spritesheet atlas.

**Inherits:** `Node2D` → `Node`

## Properties

### Sprite2D

| Property | Type | Default | Description |
|---|---|---|---|
| `texture` | str | `""` | Path to a png/jpg/webp image to draw. Mutually exclusive with `atlas`. |
| `atlas` | str | `""` | Path to a spritesheet sidecar (`.atlas.json`). Requires `frame`. |
| `frame` | str | `""` | Named frame inside the atlas. Ignored without `atlas`. |

### Node2D

| Property | Type | Default | Description |
|---|---|---|---|
| `position` | vec2 | `(0, 0)` | Local position in world units, relative to the parent. Global position is this plus every ancestor's. |
| `rotation` | number | `0` | Rotation around the node's origin. Stored in radians; the inspector edits degrees. |

