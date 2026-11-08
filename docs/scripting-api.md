# Scripting API (Python)

Gameplay scripts are Python classes inheriting the `pite` module, executed embedded in the engine process (CPython 3.12).

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

## Rules

- One file exports one class; attach with `path` + `class` in the scene file. `snake_case` everywhere.
- `_ready()` runs once when the node attaches; `_process(delta)` runs every frame (`delta` in seconds, capped at 0.1). **Missing methods are no-ops, never errors.**
- `position` is a mutable `Vec2` proxy backed directly by the engine's node transform. You can mutate `.x` and `.y` in-place (`self.position.x += 10`), reassign with a tuple or vector (`self.position = (100, 200)`), or unpack as a tuple (`x, y = self.position`). `Sprite2D` adds `texture: str`, `Timer` adds `wait_time: float` and an engine-owned `timeout` signal: every elapsed `wait_time` seconds the engine fires `timeout` and restarts the clock (looping). Non-positive `wait_time` never fires. Connect during `_ready` like `Button.pressed` — no declaration needed, `Label` adds `text: str` (same direct update — set it and the screen updates). `Button` extends `Node2D` with `text` plus a `size` prop (`[w, h]`, default `[120, 40]`).
- Declarative node descriptors: declare child or node references as class attributes:
  ```python
  sprite = pite.child("Sprite2D")
  target = pite.node("../Target")
  ```
  Descriptors resolve once on first access and cache the proxy. A missing node raises `KeyError`.
- The `get_node` proxy also exposes `text`, so any node can drive a label: `self.get_node("../HpLabel").text = f"HP: {self.hp}"`. Repeated `get_node` calls are automatically cached.
- Talk across nodes with direct calls through `get_node`, which returns a proxy to the live node:

```python
self.get_node("../Player").take_damage(1)
```

Paths resolve against node names: `..` (parent), `.` (self), `Child` (named child), `/root/…` (absolute). A miss raises `KeyError`.

For the type checker `get_node` returns a plain `Node`: anything beyond the declared surface (custom signals, script methods, per-type members) is dynamically typed instead of warning, and a miss still raises at runtime.

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
        self.get_node(".").pressed.connect(self.on_pressed)

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

## Errors and reload

- A script exception is reported with node path, file, and line; the node keeps its last good state and the game keeps running. Repeat offenders are parked after the first error (see the console).
- Saving a `.py` file hot-reloads it: the module re-executes, the class rebinds to existing nodes, `position` is preserved, and `_ready` is *not* called again. A failed reload keeps the old code running.

## Imports

- `import pite` plus the stdlib always work. Each script's own directory is prepended to `sys.path`, so sibling modules (`import utils`) import in dev.
- Third-party packages live in project-root `vendor/` (`pite vendor add pkg==ver` pins them into `[vendor]` with hashes; `list`/`verify`/`remove` manage the lock). Trees and wheels both import: the runtime puts `vendor/` on `sys.path` in dev and export alike, so resolution is identical everywhere. Zipped is the default; wheels with native modules unpack (they can't load from a zip).
- Exports drop `site-packages` entirely: anything outside stdlib + `pite` + siblings + `vendor/` dies in the export. `pite check` warns per offending import.
- Sibling modules outside `vendor/` still need `[export] include` to be staged. Safe rule: `pite` + stdlib + siblings + vendor.
