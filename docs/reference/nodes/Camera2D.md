# Camera2D

Frames the view: the screen centers on this node's global position.

**Inherits:** `Node2D` → `Node`

## Properties

### Camera2D

| Property | Type | Default | Description |
|---|---|---|---|
| `zoom` | number | `1` | View scale; larger values show less of the world. Multiplied by the window fit scale. |

### Node2D

| Property | Type | Default | Description |
|---|---|---|---|
| `position` | vec2 | `(0, 0)` | Local position in world units, relative to the parent. Global position is this plus every ancestor's. |
| `rotation` | number | `0` | Rotation around the node's origin. Stored in radians; the inspector edits degrees. |

