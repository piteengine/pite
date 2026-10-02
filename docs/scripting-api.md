# Scripting API (Python)

Gameplay scripts are Python classes inheriting the `pite` module, executed embedded in the engine process (CPython 3.12).

```python
import pite

class Player(pite.Node2D):
    speed: float = 200.0

    def _ready(self):
        self.position = (100.0, 200.0)

    def _process(self, delta: float):
        x, y = self.position
        self.position = (x + self.speed * delta, y)
```

## Rules

- One file exports one class; attach with `path` + `class` in the scene file. `snake_case` everywhere.
- `_ready()` runs once when the node attaches; `_process(delta)` runs every frame (`delta` in seconds, capped at 0.1). **Missing methods are no-ops, never errors.**
- `position` is a plain `(x, y)` float tuple, synced both ways with the scene tree each frame. `Sprite2D` adds `texture: str`, `Timer` adds `wait_time: float`, `Label` adds `text: str` (same two-way sync — set it and the screen updates). `Button` extends `Node2D` with `text` plus a `size` prop (`[w, h]`, default `[120, 40]`).
- The `get_node` proxy also exposes `text`, so any node can drive a label: `self.get_node("../HpLabel").text = f"HP: {self.hp}"`.
- Talk across nodes with direct calls through `get_node`, which returns a proxy to the live node:

```python
self.get_node("../Player").take_damage(1)
```

Paths resolve against node names: `..` (parent), `.` (self), `Child` (named child), `/root/…` (absolute). A miss raises `KeyError`.

## Signals

Nodes react to each other with typed signals. Declare them as class variables; access them as members:

```python
class Player(pite.Node2D):
    hit = pite.signal(int)

    def _process(self, delta):
        if pite.pressed("Space"):
            self.hit.emit(1)

class Enemy(pite.Node2D):
    def _ready(self):
        self.get_node("../Player").hit.connect(self.on_player_hit)

    def on_player_hit(self, damage):
        self.hp = self.hp - damage
```

- `pite.signal(...)` takes payload types, bare or as strings (`pite.signal(int, "str")`). Both `hit = pite.signal(int)` and the annotated `hit: pite.signal(int)` form declare the same signal.
- Emitting the wrong arity or type fails loudly; emitting an undeclared signal fails loudly. There are no untyped string signals.
- `connect(handler)` takes a bound method of a live node; when that node drops, its connections go with it — emitting afterwards simply skips it, no crash.
- `pite check` warns about `.emit(`/`.connect(` calls with no matching `pite.signal(` declaration in the same file. Do not define your own methods named `emit` or `connect`.
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
if pite.held("ArrowRight"):
    x += self.speed * delta
if pite.pressed("Space"):
    x += 10.0
```

- `held(key)` is true while down (continuous movement); `pressed(key)` fires on the single frame it goes down (single steps); `released(key)` on the frame it comes up. `mouse()` returns the cursor `(x, y)` in pixels.
- Key names: `A`–`Z`, `0`–`9`, `ArrowLeft/Right/Up/Down`, `Space`, `Enter`, `Escape`, `Tab`, `Backspace`, `Shift`, `Control`, `Alt`, plus `MouseLeft/MouseRight/MouseMiddle`. Input is pumped on the main thread and visible inside `_process`.

## Audio

```python
voice = pite.play("res://sfx/hit.wav", volume=0.8)
pite.set_volume(voice, 0.5)
if not pite.is_playing(voice):
    pass
pite.stop(voice)
```

- `play(path, volume=1.0)` decodes the file and returns a voice id; only 16-bit `.wav` is supported. Output goes to the OS default audio device; on machines with no audio device playback is silently simulated (same API, no sound). A missing or undecodable file raises loudly — the game keeps running. `stop(id)` on an unknown id is a silent no-op, as is `set_volume` (clamped to `0.0`–`1.0`); `is_playing` on unknown ids is `False`.
- `pite check` validates every `play("…")` literal in project scripts: the file must exist and decode.

## Errors and reload

- A script exception is reported with node path, file, and line; the node keeps its last good state and the game keeps running. Repeat offenders are parked after the first error (see the console).
- Saving a `.py` file hot-reloads it: the module re-executes, the class rebinds to existing nodes, `position` is preserved, and `_ready` is *not* called again. A failed reload keeps the old code running.
- Editor note: `import pite` may show as unresolvable in external editors — the module only exists inside the running engine. The shipped `python/pite/pite.pyi` stubs provide completion; no action needed.
