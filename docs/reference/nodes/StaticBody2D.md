# StaticBody2D

Immovable physics body: floors, walls, platforms. Never responds to impulses.

**Inherits:** `PhysicsBody2D` → `CollisionObject2D` → `Node2D` → `Node`

## Properties

### PhysicsBody2D

| Property | Type | Default | Description |
|---|---|---|---|
| `friction` | number | `0.5` | Coulomb friction coefficient applied to contacts. 0 = ice, 0.5 = wood, 1+ = sticky. |
| `restitution` | number | `0` | Bounciness of collisions: 0 absorbs the impact, 1 bounces back with equal speed. |

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

