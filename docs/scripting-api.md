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
- `position` is a plain `(x, y)` float tuple, synced both ways with the scene tree each frame. `Sprite2D` adds `texture: str`, `Timer` adds `wait_time: float`.
- Talk across nodes with direct calls through `get_node`, which returns a proxy to the live node:

```python
self.get_node("../Player").take_damage(1)
```

Paths resolve against node names: `..` (parent), `.` (self), `Child` (named child), `/root/…` (absolute). A miss raises `KeyError`. The names `emit`/`connect` are reserved for the future signal system — do not define them.

## Errors and reload

- A script exception is reported with node path, file, and line; the node keeps its last good state and the game keeps running. Repeat offenders are parked after the first error (see the console).
- Saving a `.py` file hot-reloads it: the module re-executes, the class rebinds to existing nodes, `position` is preserved, and `_ready` is *not* called again. A failed reload keeps the old code running.
- Editor note: `import pite` may show as unresolvable in external editors — the module only exists inside the running engine. The shipped `python/pite/pite.pyi` stubs provide completion; no action needed.
