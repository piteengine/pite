# AGENTS.md — Pite

Rust workspace (resolver 2, edition 2021, rust-version 1.85). Scene-tree game engine (not ECS): Rust core + embedded Python 3.12 + eframe/egui editor. `target/`, scene `_cache/`, `assets.pite.toml` are gitignored — never commit them.

## Commands (CI order)

```sh
cargo fmt --all -- --check
cargo build --workspace --locked
cargo test --workspace
PITE_LSP_SMOKE=1 cargo test -p pite-editor --lib live_pyright_smoke   # needs pyright==1.1.411, gated off by default
cargo run -p pite-cli -- check
PITE_BUNDLE_SMOKE=1 cargo test -p pite-export --lib                   # real CPython download; CI-only, slow
```

- Single crate: `cargo test -p <crate>` (e.g. `-p pite-scene`). Focused check: `cargo run -p pite-cli -- check --scene <path>`; `--strict` promotes warnings, `--json` gives machine report.
- Build needs CPython 3.12 dev headers + ALSA headers or it fails to link: `libasound2-dev pkg-config` on Debian/Ubuntu, plus `export PYO3_PYTHON=$(which python3.12)` and `LD_LIBRARY_PATH` pointing at its `LIBDIR`.
- WSL without Wayland: `WINIT_UNIX_BACKEND=x11 cargo run -p pite-cli -- edit`.
- `pite` dogfood flow: `new --template minimal-2d` → `run --scene …` → `check` → `edit --scene …`. Export via `pite export linux|windows`; offline export needs `--no-bundle-python` (+ `PITE_PYTHON` / `--skip-python-check`).

## Architecture (crate → role)

`pite-cli` (the `pite` binary) → `pite-runtime` (window, main loop, hot reload) → `pite-core` (node tree, handles, lifecycle) + `pite-scene` (TOML load/validate/cache) + `pite-script` (embedded `pite` module) + `pite-render` (`Renderer2D` wgpu seam) + `pite-assets` (`Importer` trait, uid registry) + `pite-audio` (WAV, silent-simulation fallback) + `pite-project` (`pite.toml`, `res://`, templates) + `pite-export` (binary + resolved content) + `pite-editor` (egui panels, LSP client). Docs in `docs/` are authoritative per topic: `scene-format`, `scripting-api`, `assets`, `editor`, `manifest`, `lsp`.

## Gotchas agents miss

- **Scenes**: `.pitescene` TOML is source of truth; binary `_cache/<stem>.<hash>.bin` beside it is derived. Node ids are human strings, never UUIDs. Unknown `props` warn and survive saves (forward compat) — don't "clean" them. Saves are byte-stable (sorted prop keys); `[[instance]]` entries stay as entries with edits recorded as `"node.prop"` overrides, never inlined.
- **Assets**: all refs are `res://` project-relative strings; absolute paths are forbidden. After renaming/moving an asset outside the editor, run `pite reimport` (rewrites refs by uid); `check` never writes. Orphan-asset warnings only consider scenes transitively reachable from `main_scene`.
- **Scripts**: one file = one class inheriting `pite.*`, `snake_case`. Missing `_ready`/`_process` is a no-op, never an error. `Timer.wait_time` is stored only (never fires). Cross-node calls go through `get_node("../Name")`; a miss raises `KeyError`. Signals are typed (`pite.signal(int)`); `Button.pressed` is engine-owned — just `connect`, don't declare.
- **Logging**: `tracing` everywhere — never `println!`/`log::` in engine code.
- **Visuals**: `DESIGN.md` + `palette.json` are the single source of truth (teal `#0b9387`, dark-only editor in `pite-editor/src/theme.rs`). Don't invent hexes.
- **LSP**: pinned pyright server; live test is env-gated (`PITE_LSP_SMOKE=1`), don't run it unconditionally.

## Conventions

- Conventional commits (`feat:`/`fix:`/`docs:`/`chore:`), one focused change per commit/PR; never commit secrets, `target/`, or local paths.
- New Rust files start with `// SPDX-License-Identifier: MIT OR Apache-2.0` (Python: `# …`); `examples/` game scripts stay header-free.
