# Label

Draws text centered on its global position.

**Inherits:** `Control` → `Node2D` → `Node`

## Properties

### Label

| Property | Type | Default | Description |
|---|---|---|---|
| `text` | str | `""` | The string to draw. Empty text draws nothing. |

### Control

| Property | Type | Default | Description |
|---|---|---|---|
| `size` | vec2 | `(100, 40)` | Bounding box size in world units. |
| `color` | color | `#ffffffff` | Fill color as sRGB hex with alpha, e.g. `#ffffffff`. |
| `font_size` | number | `16` | Text size in world units; scales with camera zoom. |

### Node2D

| Property | Type | Default | Description |
|---|---|---|---|
| `position` | vec2 | `(0, 0)` | Local position in world units, relative to the parent. Global position is this plus every ancestor's. |
| `rotation` | number | `0` | Rotation around the node's origin. Stored in radians; the inspector edits degrees. |

