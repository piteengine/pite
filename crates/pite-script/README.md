# pite-script

Embedded Python 3.12 scripting bridge and gameplay API for the [Pite](https://github.com/piteengine/pite) game engine.

Powered by PyO3: embeds CPython 3.12, orchestrates `_ready`, `_process(delta)`, and `_physics_process(delta)` callbacks, and provides the public `pite` module with native `Vec2` math, direct property mutation handles, typed signals, attribute-based input (`pite.held`, `pite.pressed`), declarative node descriptors (`child`, `node`), and proxy caching.

## Versioning

Published in lockstep with the engine: `pite-script 0.x.y` corresponds to engine version `0.x.y`.

## Documentation

- Documentation & Guides: <https://piteengine.github.io/pite>
- Scripting API Reference: <https://piteengine.github.io/pite/reference/scripting-api>
- API Docs: <https://docs.rs/pite-script>
- Engine Repository: <https://github.com/piteengine/pite>

## License

Dual-licensed MIT + Apache-2.0 ([LICENSE-MIT](https://github.com/piteengine/pite/blob/main/LICENSE-MIT), [LICENSE-APACHE](https://github.com/piteengine/pite/blob/main/LICENSE-APACHE)).
