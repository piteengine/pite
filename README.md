# Pite

A game engine inspired by Godot, written in Rust — scene tree, first-class GUI editor, Python scripting.

## Quickstart

```sh
cargo run -p pite-cli -- new hello --template minimal-2d
cargo run -p pite-cli -- run --scene examples/minimal-2d/scenes/main.pitescene
cargo run -p pite-cli -- check
cargo run -p pite-cli -- edit --scene examples/minimal-2d/scenes/main.pitescene
```

`new` scaffolds a project, `run` plays a scene, `check` validates it, `edit` opens the editor.

## Workspace

| Crate | Role |
|---|---|
| `pite-cli` | `pite` binary: new / run / check / edit |
| `pite-runtime` | Window, main loop, script driving, hot reload |
| `pite-core` | Node tree, handles, props, lifecycle dispatch |
| `pite-render` | `Renderer2D` trait (wgpu seam) |
| `pite-script` | Embedded Python (`pite` module) |
| `pite-scene` | TOML scenes, instantiation, validation |
| `pite-editor` | eframe/egui panels on the same tree |
| `pite-assets` | `Importer` trait, uid registry |
| `pite-project` | `pite.toml`, `res://` paths, templates |

Docs: [`docs/scene-format.md`](docs/scene-format.md), [`docs/scripting-api.md`](docs/scripting-api.md).

## Status

`v0.1-m1`: 2D scaffold — empty scenes load, one Python script drives one node, scripts and scenes hot-reload, minimal editor edits the live tree. See [`CHANGELOG.md`](CHANGELOG.md).

## License

Dual-licensed MIT + Apache-2.0 (Rust standard): [`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE).
New source files carry an SPDX header (`SPDX-License-Identifier: MIT OR Apache-2.0`); see [`CONTRIBUTING.md`](CONTRIBUTING.md).
