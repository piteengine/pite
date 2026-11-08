# CollisionShape2D

Geometric collider attached to a parent physics body or Area2D.

**Inherits:** `Node2D` → `Node`

## Properties

### CollisionShape2D

| Property | Type | Default | Description |
|---|---|---|---|
| `shape` | str | `"box"` | Geometry kind: `box`, `circle`, or `capsule`. |
| `size` | vec2 | `(32, 32)` | Bounding box size in world units. Only used when `shape = "box"`. |
| `radius` | number | `16` | Circle or capsule radius in world units. |
| `height` | number | `32` | Capsule height in world units (excludes the two hemispherical caps). |
| `disabled` | bool | `false` | When true the collider is not registered with the physics world. |

### Node2D

| Property | Type | Default | Description |
|---|---|---|---|
| `position` | vec2 | `(0, 0)` | Local position in world units, relative to the parent. Global position is this plus every ancestor's. |
| `rotation` | number | `0` | Rotation around the node's origin. Stored in radians; the inspector edits degrees. |

