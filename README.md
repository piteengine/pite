<p align="center">
  <img src="assets/logo.svg" width="128" alt="Pite logo" />
</p>

# Pite

Pite - accessible, handy game engine: Rust core + visual editor + Python scripting, all together.

Build games from scenes and nodes (not ECS) - edit visually, script in Python.

## Quickstart

```sh
cargo install --path pite-cli

pite new hello --template minimal-2d
pite run --scene examples/minimal-2d/scenes/main.pitescene
pite check
pite edit --scene examples/minimal-2d/scenes/main.pitescene
```

Install once, then use `pite`. `new` scaffolds a project, `run` plays a scene, `check` validates it, `edit` opens the editor.

## Export

```sh
pite export linux
```

`export linux|windows` writes a runnable directory (`dist/<name>-<platform>/`) with the engine binary, a `run` launcher, and only the referenced game content (`game/`).

Extra files ride along via `[export] include` in `pite.toml`. All export settings have defaults, so old projects export untouched.

Python is bundled: export fetches a pinned CPython 3.12 (checksum-verified, cached under `~/.cache/pite/toolchains`, never vendored in the repo) and ships the interpreter plus a stdlib zip under `python/`. The exported game runs with no system Python installed.

Offline? Export fails loudly and tells you to pass `--no-bundle-python`, which falls back to system Python 3.12 (`PITE_PYTHON` overrides detection, `--skip-python-check` skips that gate).

Exported runs never watch files (`--no-reload` is baked into the launcher).

## Status

`v0.1-m2` (tagged): a playable 2D loop, not a scaffold. Textured sprites
render through `Camera2D` with key-driven movement and parent-carried
transforms; typed signals (`pite.signal(int)`) connect nodes; `Label` and
`Button` draw and click; `pite.play("…")` runs WAVs; scenes hot-reload while you
type. Around that: a binary scene cache (`_cache/`, content-hash keyed, version
stamped), an asset pipeline with uid tracking (`pite reimport`), a locked
`pite.toml` schema, and an external Python language server in the code pane.
See [`CHANGELOG.md`](CHANGELOG.md) for the release-by-release record.

## Workspace

| Crate | Role |
|---|---|
| `pite-cli` | `pite` binary: new / run / check / reimport / edit / export |
| `pite-runtime` | Window, main loop, script driving, hot reload |
| `pite-core` | Node tree, handles, props, lifecycle dispatch |
| `pite-render` | `Renderer2D` trait (wgpu seam) |
| `pite-script` | Embedded Python (`pite` module) |
| `pite-scene` | TOML scenes, instantiation, validation, binary cache |
| `pite-editor` | eframe/egui panels on the same tree, LSP client |
| `pite-assets` | `Importer` trait, uid registry |
| `pite-audio` | WAV decode + playback state machine |
| `pite-project` | `pite.toml`, `res://` paths, templates |
| `pite-export` | Desktop export (binary + resolved content) |

## Docs

| Doc | Covers |
|---|---|
| [`docs/scene-format.md`](docs/scene-format.md) | `.pitescene` TOML, instantiation, the scene cache |
| [`docs/scripting-api.md`](docs/scripting-api.md) | Python API: nodes, signals, input, audio, text |
| [`docs/assets.md`](docs/assets.md) | `res://` refs, uid manifest, `pite reimport` |
| [`docs/editor.md`](docs/editor.md) | Panels, viewport, save, tree ops |
| [`docs/manifest.md`](docs/manifest.md) | `pite.toml` fields, defaults, `check` rules |
| [`docs/lsp.md`](docs/lsp.md) | Pinned pyright server, completion/hover/diagnostics |

## Contributing

We accept contributions, and we help newcomers. Start with [`CONTRIBUTING.md`](CONTRIBUTING.md).

- Found a bug or have an idea? Open an issue.
- Want to fix or build something? Open a PR.
- Not sure where to start, or just want to talk? Join the discussions.

## License

Dual-licensed MIT + Apache-2.0 (Rust standard): [`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE).
New source files carry an SPDX header (`SPDX-License-Identifier: MIT OR Apache-2.0`); see [`CONTRIBUTING.md`](CONTRIBUTING.md).
