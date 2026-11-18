# pite-stubs — type stubs for Pite

Type stubs (PEP 561) for the `pite` Python module, which the
[Pite](https://github.com/piteengine/pite) game engine embeds in its own
CPython interpreter.

The distribution is named `pite-stubs` because PyPI rejects `pite` as too
similar to existing projects. The import name is still `pite`: it is the
engine's native module, so `pip install pite-stubs` gives your editor types
for `import pite`.

The module is native (registered from Rust via PyO3), so it is **not
importable** from a normal Python environment. This package exists purely so
editors and type checkers — Pylance/Pyright — resolve `import pite` and offer
autocompletion and diagnostics when you work on a Pite project outside the
built-in editor.

## Install

```sh
pip install pite-stubs
```

Add it to your project's development dependencies, not to the game's runtime
dependencies: scripts always execute inside the engine (`pite edit`,
`pite run`), never in the venv where the stubs are installed.

## What you get

```python
import pite


class Player(pite.Node2D):
    hud = pite.child("HUD")  # resolved lazily to a Node at access time
    hit = pite.signal(int)

    def _ready(self) -> None:
        self.hit.connect(self._on_hit)

    def _process(self, delta: float) -> None:
        self.position += pite.Vec2(1.0, 0.0)

    def _on_hit(self, amount: int) -> None:
        self.hud.get_node("../ScoreLabel").text = f"Score: {amount}"
```

Every node class, property, method, and free function from the engine's schema
is covered. Signal payloads are typed through `pite.signal(...)`, and
`pite.child(...)` / `pite.node(...)` resolve to `Node` through descriptor
typing.

## Runtime behaviour

`import pite` raises `ImportError` in a normal interpreter, with a message
pointing back at the engine. That is intentional: the name belongs to the
engine's native module, and failing loudly beats an empty module that silently
does nothing.

## License

MIT OR Apache-2.0, matching the engine.
