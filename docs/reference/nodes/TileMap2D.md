# TileMap2D

Renders 2D multi-layer grid tilemaps with automatic physics colliders.

**Inherits:** `Node2D` → `Node`

## Properties

### TileMap2D

| Property | Type | Default | Description |
|---|---|---|---|
| `tilemap` | str | `""` | Path to a tilemap sidecar (`.ptmap`). |

### Node2D

| Property | Type | Default | Description |
|---|---|---|---|
| `position` | vec2 | `(0, 0)` | Local position in world units, relative to the parent. Global position is this plus every ancestor's. |
| `rotation` | number | `0` | Rotation around the node's origin. Stored in radians; the inspector edits degrees. |

