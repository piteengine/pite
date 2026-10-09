# Contributing

Yes, we want your contributions - code, docs, examples, bug reports, ideas. Newcomers are welcome, and we help you get your first change merged.

## How to help

- **Issues:** found a bug or have an idea? Open an issue. Include what you did, what you expected, and what happened. Logs and repro steps help a lot.
- **Pull requests:** want to fix or build something? Open a PR. One focused change per PR, conventional commits (`feat:`, `fix:`, `docs:`, `chore:`). Small PRs merge faster than big ones.
- **Discussions:** not sure where to start, or just want to talk design? Open a discussion. No question is too small.
- **Stuck?** Ask in your issue, PR, or discussion. We answer and unblock.

## Rhythm

- One focused change per commit; keep unrelated changes in separate commits.
- Conventional commits (`feat:`, `fix:`, `docs:`, `chore:`).
- Never commit secrets, `target/`, or local-only paths.

## CI (must be green)

```sh
cargo fmt --all -- --check
cargo build --workspace --locked
cargo test --workspace
PITE_LSP_SMOKE=1 cargo test -p pite-editor --lib live_pyright_smoke
cargo run -p pite-cli -- check
```

## Release bump checklist

The `v*` tag flow automates almost nothing — bump these by hand, every release:

- `Cargo.toml`: `version` + all 12 internal `pite-*` path-dep pins (nothing enforces lockstep; `cargo publish` breaks otherwise).
- `CHANGELOG.md`: move notes from `## Unreleased` to the release heading.
- MSRV proof re-runs automatically (weekly + manual); if it goes red, fix code or raise `rust-version`.
- Only when the toolchain changes: `python_bundle.rs` (`PINNED_PYTHON`, `PINNED_BUILD`, 4 filenames, 2 checksums).
- Only when pyright changes: `pite-editor/src/lsp.rs`, `docs/lsp.md`, `ci.yml`, `AGENTS.md` (all pin the same version).
- Only when the scene format changes: `FORMAT_VERSION`, `CACHE_VERSION`, the `format_version` template literal, `docs/scene-format.md`.
- Tag rule: strict `v*.*.*` tags must equal the workspace version (the publish workflow fails otherwise); milestone tags (`v0.1-mN`) never touch the registry.

## Local environment notes

- Script builds need CPython 3.12 with dev headers, plus ALSA headers for the audio dependency (`libasound2-dev`, `pkg-config` on Debian/Ubuntu — without them the build fails). Point PyO3 at it and make the shared lib discoverable:

```sh
export PYO3_PYTHON=/path/to/python3.12
export LD_LIBRARY_PATH=/path/to/python3.12/lib:$LD_LIBRARY_PATH
```

- On WSL without Wayland, the editor needs the X11 backend:

```sh
WINIT_UNIX_BACKEND=x11 cargo run -p pite-cli -- edit
```

## License

All contributions are dual-licensed MIT + Apache-2.0. New Rust source files start with:

```
// SPDX-License-Identifier: MIT OR Apache-2.0
```

New Python files shipped with the engine start with:

```
# SPDX-License-Identifier: MIT OR Apache-2.0
```

Example game scripts (`examples/`) stay header-free so new projects scaffold clean.
