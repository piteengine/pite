# pite-script

Embedded Python 3.12 scripting bridge and game API for the [Pite](https://github.com/piteengine/pite) game engine.

Powered by PyO3: embeds CPython 3.12, drives `_ready`, `_process(delta)`, and `_physics_process(delta)`, provides the `pite.*` module with native `Vec2` math, direct property mutation handles, typed signals, attribute-based input (`pite.held`, `pite.pressed`), declarative descriptors (`child`, `node`), and proxy caching.

## Versioning

Lockstep with the engine: `pite-script 0.x.y` is engine `0.x.y`.

## Docs

- API docs: <https://docs.rs/pite-script>
- Engine repo: <https://github.com/piteengine/pite>

## License

Dual-licensed MIT + Apache-2.0 ([LICENSE-MIT](https://github.com/piteengine/pite/blob/main/LICENSE-MIT), [LICENSE-APACHE](https://github.com/piteengine/pite/blob/main/LICENSE-APACHE)).
