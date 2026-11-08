# Button

Clickable rectangle with a caption. Fires the engine-owned `pressed` signal.

**Inherits:** `Control` → `Node2D` → `Node`

## Properties

### Button

| Property | Type | Default | Description |
|---|---|---|---|
| `text` | str | `""` | Caption drawn centered on the button. |
| `color` | color | `#334155ff` | Fill color as sRGB hex with alpha, e.g. `#334155ff`. |
| `size` | vec2 | `(120, 40)` | Hit rectangle size in world units; both rendering and click hit-testing use it. |
| `disabled` | bool | `false` | When true the button draws dimmed and ignores clicks. |

### Control

| Property | Type | Default | Description |
|---|---|---|---|
| `font_size` | number | `16` | Text size in world units; scales with camera zoom. |

### Node2D

| Property | Type | Default | Description |
|---|---|---|---|
| `position` | vec2 | `(0, 0)` | Local position in world units, relative to the parent. Global position is this plus every ancestor's. |
| `rotation` | number | `0` | Rotation around the node's origin. Stored in radians; the inspector edits degrees. |

