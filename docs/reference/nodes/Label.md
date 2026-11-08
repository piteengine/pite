# Label

Draws text centered on its global position.

**Inherits:** `Control` → `Node`

## Properties

### Label

| Property | Type | Default | Description |
|---|---|---|---|
| `text` | str | `""` | The string to draw. Empty text draws nothing. |

### Control

| Property | Type | Default | Description |
|---|---|---|---|
| `position` | vec2 | `(0, 0)` | Position relative to the parent, in world units. |
| `size` | vec2 | `(100, 40)` | Bounding box size in world units. |
| `color` | color | `#ffffffff` | Fill color as sRGB hex with alpha, e.g. `#ffffffff`. |
| `font_size` | number | `16` | Text size in world units; scales with camera zoom. |

