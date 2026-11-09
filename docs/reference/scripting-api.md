---
icon: lucide/file-code-corner
---

# Scripting API (Python)

Gameplay logic in Pite is authored in Python scripts that inherit from engine node classes in the embedded `pite` module. Scripts execute directly within the engine process via embedded CPython 3.12, providing high performance with full access to standard Python features.

```python
import pite

class Player(pite.Node2D):
    speed: float = 200.0

    def _ready(self):
        self.position = (100.0, 200.0)

    def _process(self, delta: float):
        if pite.held.right:
            self.position.x += self.speed * delta
```

---

## Script Architecture and Lifecycle

- **Class Mapping**: Each Python script file defines a single node class. Scripts are attached to scene nodes via the scene's `[node.script]` definition (`path` and `class`). Class and module names follow standard Python conventions (`snake_case` filenames and `PascalCase` classes).
- **`_ready(self)`**: Invoked once when the node enters the active scene tree. Use this for one-time initialization, finding child nodes, and connecting signals.
- **`_process(self, delta: float)`**: Invoked every frame during variable-rate rendering updates. The `delta` parameter provides the elapsed frame duration in seconds (clamped to a maximum of `0.1` to avoid physics tunneling during stalls).
- **`_physics_process(self, delta: float)`**: Invoked at a fixed 60 Hz rate (`delta = 1/60`) for deterministic physics simulation and character movement.
- **Graceful Lifecycle**: Missing lifecycle methods (`_ready`, `_process`, `_physics_process`) are safe no-ops and never produce errors.

---

## Core Node Properties

### Node2D

All 2D spatial objects inherit from `pite.Node2D`:

- **`position: Vec2`**: Mutable 2D position vector relative to the parent. Supports in-place component mutation (`self.position.x += 10`), assignment via tuples or vectors (`self.position = (100, 200)`), and tuple unpacking (`x, y = self.position`).
- **`rotation: float`**: Orientation in radians. Rotates descendants around the node's local origin.

### Visual Elements

- **`Sprite2D`**: Inherits `Node2D`. Adds `texture: str` (`res://` path to image), `atlas: str` (`res://` path to `.atlas.json`), and `frame: str` (active frame name within the atlas).
- **`Camera2D`**: Inherits `Node2D`. Adds `zoom: float` (default `1.0`). Centers the game viewport around the camera's global position.

### User Interface Elements

- **`Control`**: Base UI class inheriting `Node2D`. Provides `font_size: float` (text size in world units, default `16.0`) and `color: str` (hex string e.g. `"#ffffff"`).
- **`Label`**: Inherits `Control`. Adds `text: str` for rendering text centered on the node position.
- **`Button`**: Inherits `Control`. Adds `text: str`, `size: Vec2` (interactive bounding box), `disabled: bool`, and a built-in `pressed` signal.

!!! Important "Calling Superclass `_process`"
    If your script overrides `_process` on a `Timer` or `Button` subclass, you **must** invoke `super()._process(delta)` to allow the engine to process internal timer accumulation and click polling.

### Timers

- **`Timer`**: Standalone timing node providing `wait_time: float` and `current_elapsed: float`. Fires the engine-owned `timeout` signal each time `wait_time` seconds elapse (looping automatically). Non-positive `wait_time` values pause the timer.

---

## Node References and Navigation

### Declarative Descriptors

Declare node references cleanly as class attributes using descriptors:

```python
class GameController(pite.Node2D):
    player = pite.node("../Player")
    hud = pite.child("HUD")
```

Descriptors resolve on first access and cache the resulting node proxy. If the target node is missing, access raises a `KeyError`.

### Dynamic Traversal via `get_node`

Traverse the scene hierarchy dynamically using relative or absolute path strings:

```python
self.get_node("../Player").take_damage(1)
```

- Paths are relative to the calling node: `.` (self), `..` (parent), `ChildName` (direct child), or `/root/…` (absolute path from root).
- If a target node path does not exist, `get_node` raises a `KeyError`.
- Repeated lookups through `get_node` are automatically cached by the engine runtime.
- For dynamic label updates, any node proxy exposes a convenient `text` property:
    ```python
    self.get_node("../ScoreLabel").text = f"Score: {self.score}"
    ```

## Custom Properties

Nodes can access project-level custom properties declared under `[props]` in `pite.toml`:

```python
# Read custom prop (returns None if unset):
spawn = self.get_custom_property("spawn_group")

# Write custom prop (type-checked against manifest, raises ValueError on mismatch):
self.set_custom_property("spawn_group", "elite_guards")

# Remove custom prop (returns True if it was present):
was_present = self.remove_custom_property("spawn_group")
```

Schema properties (`position`, `text`, etc.) are rejected by `get_custom_property`/`set_custom_property`/`remove_custom_property` with `ValueError` — use direct typed attributes instead.

## Signals

Nodes react to each other with typed signals. Declare them as class variables; access them as members:

```python
class Player(pite.Node2D):
    hit = pite.signal(int)

    def _process(self, delta):
        if pite.pressed.space:
            self.hit.emit(1)
```

- `pite.signal(...)` takes payload types, bare or as strings (`pite.signal(int, "str")`). Both `hit = pite.signal(int)` and the annotated `hit: pite.signal(int)` form declare the same signal.
- Emitting the wrong arity or type fails loudly; emitting an undeclared signal fails loudly. There are no untyped string signals.
- `connect(handler)` connects any Python callable (bound methods, lambdas, free functions).
- `Button` clicks arrive as a zero-payload `pressed` signal fired by the engine (press-inside plus release-inside). `pressed` belongs to the `Button` type itself — just connect it, no declaration needed (re-declaring it is harmless):

```python
class HitButton(pite.Button):
    def _ready(self):
        self.pressed.connect(self.on_pressed)

    def on_pressed(self):
        self.get_node("../Player").hit.emit(1)
```

## Input

```python
if pite.held.right:
    self.position.x += self.speed * delta
if pite.pressed.space:
    self.position.x += 10.0
```

- `pite.held.<key>` is true while down (continuous movement); `pite.pressed.<key>` fires on the single frame it goes down (single steps); `pite.released.<key>` on the frame it comes up. `mouse()` returns a `Vec2` with `.x` and `.y` (and tuple unpacking `x, y = pite.mouse()`).
- Key attributes: `space`, `enter`, `escape`, `tab`, `backspace`, `shift`, `ctrl`, `alt`, `left`, `right`, `up`, `down`, `mouse_left`, `mouse_right`, `mouse_middle`, `a`–`z`, `num0`–`num9`.
- `pite.screen_to_world((screen_x, screen_y)) -> (world_x, world_y)` transforms screen pixel coordinates to world coordinates using the current camera view (used e.g. for button hit-testing).
- String-based input calls (e.g., `pite.held("Space")`) are deprecated and raise `TypeError`. Input is pumped on the main thread and visible inside `_process` and `_physics_process`.

## Audio

```python
voice = pite.play("res://sfx/hit.wav", volume=0.8)
pite.set_volume(voice, 0.5)
if not pite.is_playing(voice):
    pass
pite.stop(voice)
```

- `play(path, volume=1.0)` decodes the file and returns a voice id; `.wav` (8/16/24/32-bit int and float), `.ogg` (Vorbis), `.mp3`, and `.flac` are supported. Output goes to the OS default audio device; on machines with no audio device playback is simulated (a warning is logged; same API, no sound). A missing or undecodable file raises loudly — the game keeps running. `stop(id)` on an unknown id is a silent no-op, as is `set_volume` (clamped to `0.0`–`1.0`); `is_playing` on unknown ids is `False`.
- `pite check` validates every `play("…")` literal in project scripts: the file must exist and decode.

## Tilemaps

```python
class Level(pite.TileMap2D):
    def _ready(self):
        # Cell queries and mutations
        tile = self.get_cell(0, 0, layer=0)
        self.set_cell(0, 0, "grass", layer=0)
        self.erase_cell(0, 0, layer=0)
        self.clear(layer=None)

        # Coordinate math
        col, row = self.world_to_map(pite.mouse())
        pos = self.map_to_world(col, row)

        # Tilemap properties & introspection
        tile_w, tile_h = self.tile_size
        layers = self.layers
        cells = self.get_used_cells(layer=0)
```

- `tile_size -> tuple[int, int]`: Tile dimensions `(width, height)` in pixels.
- `layers -> list[str]`: List of layer names in the tilemap.
- `get_cell(col, row, layer=0) -> str | None`: Returns the atlas frame name at `(col, row)`.
- `set_cell(col, row, frame, layer=0)`: Places an atlas frame at `(col, row)`.
- `erase_cell(col, row, layer=0) -> bool`: Erases cell at `(col, row)`.
- `clear(layer=None)`: Clears all cells in the specified layer (or all layers if omitted).
- `world_to_map(pos_or_x, y=None) -> tuple[int, int]`: Converts world position to grid cell `(col, row)`.
- `map_to_world(col, row) -> Vec2`: Converts grid cell `(col, row)` to world center position.
- `get_used_cells(layer=0) -> list[tuple[int, int]]`: Returns list of populated `(col, row)` coordinates.

## Physics

Pite includes a pure-Rust 2D physics subsystem built on `rapier2d`. Simulation runs in a fixed 60 Hz loop (`dt = 1/60`), executing `_physics_process(delta)` across active scripts.

### Nodes

- `StaticBody2D`: Non-moving collision body (floors, walls).
- `CharacterBody2D`: Kinematic character controller with `move_and_slide()`.
- `RigidBody2D`: Dynamic simulation body affected by gravity and forces.
- `Area2D`: Trigger zones with entry/exit overlap signals.
- `CollisionShape2D`: Child of any physics body or area defining geometric collider (`shape`: `"box"`, `"circle"`, `"capsule"`).

### Character Movement

```python
class Player(pite.CharacterBody2D):
    speed: float = 200.0

    def _physics_process(self, delta: float):
        if pite.held.right:
            self.velocity.x = self.speed
        elif pite.held.left:
            self.velocity.x = -self.speed
        else:
            self.velocity.x = 0.0

        # Gravity
        self.velocity.y += 980.0 * delta

        if pite.pressed.space and self.is_on_floor:
            self.velocity.y = -400.0

        self.move_and_slide()
```

- `move_and_slide() -> bool`: Advances shape along `velocity * dt` using obstacle sliding collision resolution (ignoring sensor areas). Updates `velocity`, `position`, and collision contact flags.
- `is_on_floor: bool`, `is_on_wall: bool`, `is_on_ceiling: bool`: Collision contact flags relative to `up_direction` (default `(0.0, -1.0)`). These are **attributes**, not methods — `if self.is_on_floor:` (no parens). Calling them as methods raises `TypeError`.

### RigidBody2D Dynamics

```python
class Ball(pite.RigidBody2D):
    def _ready(self):
        self.apply_impulse((0.0, -200.0))
```

- `apply_impulse((x, y))`: Instantaneous momentum change scaled by mass.
- `apply_force((x, y))`: Continuous force application over current step.
- Configurable properties: `mass: float`, `gravity_scale: float`, `lock_rotation: bool`.

### Area2D Triggers

```python
class Coin(pite.Area2D):
    def _ready(self):
        self.body_entered.connect(self.on_body)

    def on_body(self, body_id: str):
        print(f"Collected by {body_id}")
```

Built-in signals:

- `body_entered(body_id: str)`
- `body_exited(body_id: str)`
- `area_entered(area_id: str)`
- `area_exited(area_id: str)`

### Spatial Queries (Raycasts)

```python
hit = pite.raycast((x, y), (target_x, target_y), mask=1)
if hit is not None:
    print(f"Hit {hit['body']} at {hit['point']} with normal {hit['normal']}")
```

Returns `None` if no solid obstacle intersects, or a dict containing:

- `point`: `(float, float)` world coordinate of intersection.
- `normal`: `(float, float)` surface normal at intersection point.
- `distance`: `float` distance from ray origin.
- `collider`: `str` node ID of `CollisionShape2D`.
- `body`: `str` node ID of parent physics body.

## Error Handling and Live Reload

- **Fault Tolerance**: When an unhandled exception occurs in a script callback, Pite logs the complete traceback including node path, source file, and line number. The node preserves its last valid state and the game session continues running without crashing. Repeated failures on the same callback are throttled to keep the console clean and readable.
- **Hot Reloading**: Modifying and saving a `.py` file triggers an immediate hot reload: the module re-executes, class definitions rebind to existing node instances in memory, transforms and physics velocities are preserved, and `_ready()` is not re-run. If a reloaded script contains syntax or import errors, the previous working version continues executing safely.

---

## Module Imports and Dependency Vendoring

- **Standard Library & Engine API**: `import pite` and all CPython 3.12 standard library modules are always available. Each script's parent directory is prepended to `sys.path`, allowing sibling modules (e.g. `import utils`) to resolve directly.
- **Third-Party Dependencies**: Third-party packages must be vendored into the project's `vendor/` directory using `pite vendor add package==version`. The CLI pins the package and records its SHA-256 hash in `[vendor]`. Both pure-Python wheels and source trees are supported, and `vendor/` is automatically added to `sys.path` in both editor and exported builds.
- **Export Isolation**: Standalone game exports deliberately exclude host `site-packages` to guarantee portable, self-contained builds. Any dependency outside the standard library, `pite`, project scripts, and `vendor/` will not be bundled. Use `pite check` to audit imports and verify that all external dependencies are properly vendored.
- **Auxiliary Code Files**: Sibling Python modules located outside standard script directories should be declared under `[export] include` in `pite.toml` to ensure they are packaged during export.
