<p align="center">
  <img src="assets/logo.svg" width="128" alt="Pite logo" />
</p>

# Pite

Pite - A game engine that's handy to use, to change, to own. Rust core, Python scripting, visual editor.

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

### Export

```sh
pite export linux
```

`export linux|windows` writes a runnable directory (`dist/<name>-<platform>/`) with the engine binary, a launcher script, a manual for how to run, and only the referenced game content. Provides `[export] include` in `pite.toml` for extra files.

Python is bundled: export fetches a pinned CPython 3.12 (checksum-verified, cached under `~/.cache/pite/toolchains`) and ships in the exported game. The exported game runs with no system Python installed. Passing `--no-bundle-python` falls back to end-user's system Python 3.12 (`PITE_PYTHON` overrides detection).

## Why it's built this way  

Breakdown of each architectural decision behind Pite and its reasons:

### Python for scripting 

Scripting game logic is decision-making, not computation. So you must be able to use a high-level (scripting) language and Python is the very dynamic and ecosystem-rich choice. 

### Rust at the core

Games need to be fast and efficient, so a low-level language is needed to use native performance. Pite uses Rust to run anything fast, in parallel, and safe, Python scripts are the glue code orchestrating the game flow.

### Scene Tree for game model + Visual Editor

That Python smooth flow doesn't complete without a real way to edit anything and seeing results real time. Pite editor brings code, scene, asset, and result preview all together. 

Half of Pite is about being handy, Scene Tree (Scenes & Nodes) is a proven way to model game systems in an understandable way and also is as flexible as needed to implement any type of game systems efficiently in target scale of Pite. Consequently you need a visual representation of these scenes.

### `pite check` tooling

A lot of games grow to tons of images and scenes, `pite check` keeps it clean and robust: detects orphans, resolves file moves, validates decoding, scans code references. Thanks to Rust, it reports what is outside its place instantly. 

### TOML scenes

TOML for scenes provides inspectable changes, real Git history, editable meaningful files, and sure a way to cache scene binary by its text. 

### Why Pite instead of other game engines? 

- **Godot:** Pite isn't trying to replace Godot. If Godot fits your game, use Godot. Pite exists for the cases where it doesn't — and for the people who want a smaller, Python-native alternative. If you make indie games for windows, you can use Pite to reach 17MB (*) exports (usually under 30MB with a real game) without any additional config.

(*) Measured on the `minimal-2d` template on Windows 11, x86_64, release build. Same hello-world in Godot exports to ~70MB.

- **Bevy:** Its ECS is main thing it gives to you and this is full abstraction, there is no scripting, no editor, etc. Bevy is for people who want to write their game in Rust, ECS-first, without an editor. Pite is for people who want to write their game in Python, scene-first, with an editor.

- **Love2D/PyGame/etc:** Love2D and Pygame are excellent, mature frameworks with loyal communities. They give you a window, an input loop, and a draw call. What they don't give you is an editor, a scene model, an asset system, or a one-command export. Pite does.

Pite stands on being handy to use, to change, to own. It provides Python scripting and Rust performance together and focuses on your development experience. Pite is a growing, thought-out toolbox for game developers. 

### What you get for free

No audio device? Silent simulation. Change a script? Hot reload. Type a method name? Pyright autocompletes. Move a sprite? Nothing breaks — assets are uid-addressed. Ship to a friend? No Python install needed. Each of these is a small decision. Together they're why Pite feels different. 

## What Pite is not

- Pite is **not** for 3D, mobile, or web games. Not now, maybe never.
- Pite is **not** finished. It's 0.x, expect rough edges.

## Who Pite is for

- Python developers who don't want to learn GDScript.
- Solo devs who want a real editor, not a framework.
- Teachers who want to teach game dev with a language students already know. 
- Anyone who's ever wanted to read their game's source and understand all of it. 

## Status

4th milestone of v0.1 is done:
- Renderer stability
- Exporter test on Linux & Windows
- Input handling API for Python scripts
- Working audio player
- Orphan-asset warnings in `pite check`
- Playable running preview inside editor
- File-tree for Assets panel

See [`CHANGELOG.md`](CHANGELOG.md) for full history and status.

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
| `pite-audio` | WAV decode, default-device output, silent-simulation fallback |
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

Pite is built by Mahan Khalili (@mkh-user) as a long-term bet that Python-native game development deserves a real engine. Contributions and feedback are welcome.