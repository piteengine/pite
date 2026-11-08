# Area2D

Trigger zone for overlap detection. Emits body_entered/body_exited and area_* signals.

**Inherits:** `CollisionObject2D` → `Node2D` → `Node`

## Properties

### CollisionObject2D

| Property | Type | Default | Description |
|---|---|---|---|
| `collision_layer` | int | `1` | Bitmask of physics layers this body belongs to (1 = layer 1, 2 = layer 2, …). |
| `collision_mask` | int | `1` | Bitmask of physics layers this body collides with. |

### Node2D

| Property | Type | Default | Description |
|---|---|---|---|
| `position` | vec2 | `(0, 0)` | Local position in world units, relative to the parent. Global position is this plus every ancestor's. |
| `rotation` | number | `0` | Rotation around the node's origin. Stored in radians; the inspector edits degrees. |

