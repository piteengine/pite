# Contributing

## Rhythm

- One focused change per commit; keep unrelated changes in separate commits.
- Conventional commits (`feat:`, `fix:`, `docs:`, `chore:`).
- Never commit secrets, `target/`, or local-only paths.

## CI (must be green)

```sh
cargo build --workspace --locked
cargo test --workspace
cargo run -p pite-cli -- check
```

## Local environment notes

- Script builds need CPython 3.12 with dev headers. Point PyO3 at it and make the shared lib discoverable:

```sh
export PYO3_PYTHON=/path/to/python3.12
export LD_LIBRARY_PATH=/path/to/python3.12/lib:$LD_LIBRARY_PATH
```

- On WSL without Wayland, the editor needs the X11 backend:

```sh
WINIT_UNIX_BACKEND=x11 cargo run -p pite-cli -- edit
```

## License

All contributions are dual-licensed MIT + Apache-2.0. New source files start with:

```
// SPDX-License-Identifier: MIT OR Apache-2.0
```
