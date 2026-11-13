<p align="center">
  <img src="assets/logo.svg" width="128" alt="Pite logo" />
</p>

# Pite

Pite - A game engine that's handy to use, to change, to own. Rust core, Python scripting, visual editor.

Build games from scenes and nodes (not ECS) - edit visually, script in Python.

<p align="center">
  <img src="docs/images/editor.png" width="800" alt="Pite editor" />
  <br/>
  <em>The Pite editor running the "Gem Collector" example</em>
</p>

## Quickstart

Install the engine bundle (engine + player binaries for all platforms, so any machine
exports for any platform) — latest stable, verified on download:

Linux / macOS Terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/piteengine/pite/main/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/piteengine/pite/main/install.ps1 | iex
```

Or install with `cargo`:

```sh
cargo install pite
```

Then:

```sh
# Create a new project (blank by default, or pass --template minimal-2d)
pite new hello

# Enter the project and launch the visual editor (or run `pite edit`)
cd hello
pite

# Run the game (defaults to main_scene, or pass a scene path)
pite run

# Validate assets, scripts, and scenes (add --reimport-assets to sync moved files)
pite check
```

Install once, then use `pite`. Running plain `pite` inside a project opens the visual editor. `new` scaffolds a project (blank or templated), `run` plays a scene (defaults to the project's main scene), `edit` opens the visual editor, `check` validates project integrity (`--reimport-assets` syncs moved assets and hashes), `toolchain` manages standalone runtimes for offline export, and `vendor` pins third-party Python packages. Building from source instead: `cargo build --release -p pite -p pite-player` (needs CPython 3.12 dev headers, see `CONTRIBUTING.md`).

### Export & Toolchains

```sh
# Pre-cache toolchains (CPython + player binaries) for offline export
pite toolchain download

# Export a standalone game for any platform
pite export linux
```

`export linux|windows|macos` writes a runnable directory (`dist/<name>-<platform>/`) with the lean player binary (no editor), a launcher script, a manual for how to run, and only the referenced game content. Provides `[export] include` in `pite.toml` for extra files, plus `vendor/` for third-party Python packages. The player is resolved as `--binary`, else a `pite-player` sibling of `pite` — engine bundles ship player binaries for all platforms, so any machine exports for any platform with no download and no compiler. When a player sibling is absent, export falls back to downloading and caching the platform's player template into `~/.cache/pite/toolchains`. Use `pite toolchain status|download|verify|clean` to manage these toolchain artifacts.

Python is bundled: export fetches a pinned CPython 3.12 (checksum-verified, cached under `~/.cache/pite/toolchains`) and ships in the exported game. The exported game runs with no system Python installed. Passing `--no-bundle-python` falls back to end-user's system Python 3.12 (`PITE_PYTHON` overrides detection).

## Why it's built this way

Pite is designed around deliberate architectural choices that prioritize developer ergonomics, reliability, and fast iteration:

### Python for scripting

Gameplay scripting is primarily about decision-making, game flow, and rapid iteration rather than raw numeric compute. Python provides an expressive, approachable syntax with an immense ecosystem, making gameplay logic intuitive to write and delightful to read.

### Rust at the core

Games demand predictable frame times, memory safety, and native execution speed. Pite builds its engine core in Rust: 2D rendering, physics simulation, scene graph traversal, and asset decoding all execute with native performance and concurrency safety, leaving Python free to orchestrate gameplay without performance bottlenecks.

### Scene Tree model and visual editor

A smooth development workflow requires immediate visual feedback. The Pite editor integrates scene hierarchy, property inspection, asset management, and live gameplay preview into a single focused workspace. The Scene Tree model (scenes and nodes) represents game worlds naturally: parent transforms cascade cleanly down the tree, and scenes compose modularly as reusable branches.

### `pite check` static verification

As games grow to hundreds of assets and scripts, broken references and missing files can easily slip in. `pite check` verifies project integrity in milliseconds: it detects orphan assets, resolves file moves via content-addressed UIDs, validates texture and audio decoding, checks script node references, and warns on third-party imports that standalone exports would drop.

### Human-readable TOML scenes

Storing scenes in human-readable TOML (`.ptscn`) makes version control natural and git diffs clean and reviewable. Merge conflicts are straightforward to resolve, scenes can be edited in any text editor, and runtime loading remains instantaneous thanks to automatic, content-hashed binary caching.

### Why Pite instead of other game engines?

- **Godot:** Pite is not trying to replace Godot. If Godot fits your workflow, use Godot. Pite exists for developers who prefer standard Python over GDScript, value lean standalone exports (~20–30 MB rather than 70+ MB), and desire a lightweight, focused 2D toolset without unnecessary complexity.
- **Bevy:** Bevy provides an outstanding Entity Component System (ECS) architecture for developers writing games purely in Rust without an editor. Pite is designed for developers who want to write games in Python with a hierarchical Scene Tree and an integrated visual editor.
- **Pygame and LÖVE:** Pygame and LÖVE are excellent, mature frameworks with passionate communities. They provide a window, input loop, and drawing primitives. Pite complements code with an integrated visual editor, a declarative scene tree, built-in 2D physics and audio mixing, resilient asset tracking, and zero-configuration standalone desktop export.

Pite is built to be handy to use, to change, and to own. It unites Python scripting and Rust performance to deliver a focused, thoughtful developer experience.

### Thoughtful defaults

- **Missing audio hardware?** Automatic silent simulation keeps the game running without crashing.
- **Script modified?** Live hot reload preserves node state and physics velocities.
- **Typing code?** Built-in pyright provides instant autocomplete, hover docs, and diagnostics.
- **Asset moved or renamed?** Content-addressed UIDs keep references intact across scenes and scripts.
- **Sharing your game?** One-command export produces a self-contained bundle with no Python installation required on the player's machine.

## Scope and focus

- **Focused on 2D desktop:** Pite is dedicated strictly to 2D games on Linux, Windows, and macOS. 3D, mobile, and web targets are outside its current scope.
- **Active development:** Pite is in the 0.x release series. Core APIs and file formats are stable and tested, with continuous refinements to features and tooling.

## Who Pite is for

- Python developers who want a dedicated 2D game engine using standard Python.
- Solo developers and indie teams looking for an integrated visual editor rather than a code-only framework.
- Educators seeking to teach game development with an accessible, industry-standard language.
- Anyone who wants to understand and inspect their engine's complete architecture from top to bottom.

## Status

**v0.4 is out:**

- 2D flipbook animations (`AnimatedSprite2D`) with `.atlas.json` sprite sheet sequencing, fps/loop timing, playback API, and animation signals
- Multi-layer 2D tilemaps (`TileMap2D`) via `.ptmap` sidecars with Rapier2D greedy collider coalescing and frustum-culled GPU batch rendering
- Enhanced `pite check` CLI diagnostics with structured What/Why/Fix cards, 16-color ANSI output, and schema v3 JSON reporting
- Assets panel file management: creation dialogs, context menus, drag-and-drop file organization, and cross-panel asset assignments
- Simultaneous scene and code saving on `Ctrl+S` triggering immediate hot reload
- Schema-driven documentation, inspector descriptions, and build-time generated `pite.pyi` / `pite.py` stubs
- UI hierarchy refactoring: `Control` extends `Node2D`, inheriting position and rotation transforms directly

**Previous:** v0.3.0 (2D physics with Rapier2D, Vec2 math, hot reload isolation, Symphonia audio), v0.2.0 (vendor dependencies, window scaling, editor gutter and completion), v0.1.1 (`Timer.timeout`), v0.1.0 (crates.io), and earlier milestones — see [`docs/changelog.md`](docs/changelog.md).

## Workspace

Pite is structured as a modular Rust workspace consisting of 13 crates, all located in [`crates/`](crates). For crate roles, crate dependencies, and architecture documentation, see the [Rust Architecture Reference](https://piteengine.github.io/pite/developer/rust-docs/).

All 13 crates are published in lockstep with the engine on [crates.io](https://crates.io) and documented on [docs.rs](https://docs.rs).

## Docs

Documentation is available online at **[piteengine.github.io/pite](https://piteengine.github.io/pite)**.

All documentation source files are located in the [`docs/`](docs) folder. Node class reference documentation is automatically generated from the engine schema defined in [`crates/pite-core/src/schema.rs`](crates/pite-core/src/schema.rs) and [`crates/pite-core/src/nodes/`](crates/pite-core/src/nodes) (with Python bindings in [`crates/pite-script/src/python/nodes/`](crates/pite-script/src/python/nodes)).

## Contributing

We accept contributions, and we help newcomers. Review our core contribution and governance guides:

- [Contributing Guide](CONTRIBUTING.md) — Local development workflow, testing, and PR conventions.
- [Code of Conduct](CODE_OF_CONDUCT.md) — Community standards, pledge, and enforcement responsibilities.
- [Security Policy](SECURITY.md) — Vulnerability reporting, supported versions, and memory safety invariants.
- [Accessibility Statement](ACCESSIBILITY.md) — Accessibility baseline, supported assistive tech, and barrier reporting.

Ways to participate:

- Found a bug or have an idea? Open an issue.
- Want to fix or build something? Open a PR.
- Not sure where to start, or just want to talk? Join the discussions.

## License

Dual-licensed MIT + Apache-2.0 (Rust standard): [`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE).
New source files carry an SPDX header (`SPDX-License-Identifier: MIT OR Apache-2.0`); see [`CONTRIBUTING.md`](CONTRIBUTING.md).

Pite is built by Mahan Khalili (@mkh-user) as a long-term bet that Python-native game development deserves a real engine. Contributions and feedback are welcome.
