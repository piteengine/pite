# Changelog

All notable changes to the Pite game engine are documented here.

## Unreleased

- **2D Tilemaps (`TileMap2D`)**:
    - Added multi-layer grid tilemaps via `.ptmap` TOML sidecars referencing atlas frames.
    - Greedy collider merging: adjacent solid tiles coalesce into maximal static box colliders in Rapier2D, preventing internal edge snagging.
    - Frustum-culled batch rendering: offscreen tiles are culled, batching visible tiles into GPU draw calls.
    - Scripting API: `pite.TileMap2D` exposes `get_cell`, `set_cell`, `erase_cell`, `clear`, coordinate helpers `world_to_map` / `map_to_world`, layer introspection, and `tile_size`.
    - Editor tools: inspector tile palette with layer switcher and brush/eraser toggles; viewport interactive painting with grid snapping.
    - Project verification: `pite check` validates `.ptmap` files, atlas references, and frame usage.
- **Simultaneous Scene & Code Save**: `Ctrl+S` / `Cmd+S` saves both the active `.ptscn` scene and open Python script simultaneously, triggering an immediate hot reload.
- **Assets Panel File Management**:
    - Folder-first sorting, empty folder preservation, and double-click file opening.
    - Creation dialogs for scripts, scenes, and folders via `+ File` and `+ Folder`.
    - Context menu and keyboard shortcuts: Cut, Copy, Paste, Duplicate, Delete, and Rename (`F2`) with automatic reference rewriting across scenes and scripts.
    - Drag-and-drop support: import files from the OS, move items between directories, drop scripts onto Scene Tree nodes, and drag textures into Inspector fields.
- **UI Hierarchy Refactoring**:
    - `Control` now inherits from `Node2D` (`Node → Node2D → Control → Label & Button`).
    - `Label` and `Button` inherit `position` and `rotation` transforms directly from `Node2D` alongside UI styling (`color`, `font_size`).
- **Custom Project Properties (`[props]`)**:
    - Declared under `[props]` in `pite.toml` (`string`, `int`, `number`, `bool`, `vec2`, `color`).
    - Accessible in scripts via `get_custom_property`, `set_custom_property`, and `remove_custom_property`.
- **Schema-Driven Documentation and Stubs**:
    - `NodeTypeSchema` and `PropSchema` drive editor tree descriptions, inspector hover tooltips, and node reference docs.
    - `pite.pyi` and `pite.py` type stubs are generated at build time directly from `NODE_SCHEMAS`, guaranteeing synchronization with the engine.
- **Python Accessor Updates**:
    - `CharacterBody2D.is_on_floor`, `is_on_wall`, and `is_on_ceiling` are now attribute accessors (e.g. `self.is_on_floor`), not methods.
    - `Node2D.rotation` is exposed to Python and reflected by the renderer.
    - Direct tree-backed prop accessors ensure property modifications (`position`, `velocity`, `color`, etc.) persist cleanly across hot reloads.
- **Scene Tree Panel Overhaul**:
    - Modal node creation dialog (`Ctrl+A` / `+ Node`) with search filtering, category descriptions, and inheritance hierarchy.
    - Sibling and child drag-and-drop reordering with 3-2 drop zones (child vs before/after sibling), horizontal insertion lines, and floating drag preview cards.
    - Subtree duplication (`Ctrl+D`), inline renaming (`F2`), and child scene instantiation (`Ctrl+Shift+A`).
    - Phosphor icons and color-coded node categories (2D, physics, UI, timer).
- **Inspector Panel Overhaul**:
    - Properties organized into collapsible inheritance-grouped sections (`Node` → `Node2D` → derived class).
    - Revert-to-default buttons for properties deviating from schema defaults.
    - Interactive Color editor with hex inputs and live preview swatches.
- **Crates.io & `cargo install pite` Packaging**:
    - Configured all 13 workspace crates for crates.io publication; engine CLI and editor installable via `cargo install pite`.
- **Scripting & Physics Synchronization**:
    - Pre-step synchronization of node tree positions into Rapier2D for kinematic and static bodies and areas manipulated in Python.
    - Signal error reporting prints full Python tracebacks to stderr and logs when handlers raise exceptions.
    - Color property setters accept hex color strings (e.g. `"#38bdf8"`) in addition to numeric tuples.
    - `Sprite2D` treats empty `atlas` and `frame` strings as unset rather than throwing invalid reference errors.
- **Dependencies & MSRV**:
    - Upgraded Rapier2D to 0.36.1 and Zip archive handling to 9.4.1.
    - Bumped workspace MSRV to Rust 1.95.
- **Color Property Type**: Added native `Color` property variant with `#rgb`, `#rrggbb`, and `#rrggbbaa` hex parsing and alpha support in the editor.

## v0.3.0

- **2D Physics Subsystem (Rapier2D)**:
    - Integrated 2D physics simulation running on a fixed 60 Hz tick (`_physics_process(delta)`).
    - Node types: `StaticBody2D` (static geometry), `CharacterBody2D` (kinematic controller with `move_and_slide()`), `RigidBody2D` (forces and impulses), `Area2D` (trigger zones), and `CollisionShape2D` (`box`, `circle`, `capsule`).
    - Kinematic character controller: `move_and_slide()` resolves sliding collisions against solid geometry while ignoring sensors, updating `velocity` and contact flags (`is_on_floor`, `is_on_wall`, `is_on_ceiling`).
    - Spatial queries: `pite.raycast(from, to, mask)` queries solid obstacles and returns intersection points, normals, distances, and hit bodies.
    - Editor visualization: viewport wireframe overlays for collision shapes with Inspector configuration controls.
- **Editor Menu Bar & Recent Projects**:
    - Native menu bar (`Scene`, `Project`, `Help`) with file dialogs, project settings, documentation links, and recent project history.
    - Open and edit text configuration files (`pite.toml`, `.gitignore`) directly in the embedded code editor.
- **CLI Toolchain Management**:
    - Added toolchain manager (`pite toolchain`) and streamlined cross-platform installer scripts (`install.sh`, `install.ps1`).
- **Python API Redesign**:
    - Native `Vec2` math class with arithmetic operator overloading, in-place mutation, and tuple sequence unpacking.
    - Direct engine transform mutations without end-of-frame reflection overhead.
    - Attribute-based input API: `pite.held.<key>`, `pite.pressed.<key>`, and `pite.released.<key>`.
    - Declarative node descriptors: `pite.child("Name")` and `pite.node("../Path")` with first-access caching.
    - Flexible signal callbacks: `signal.connect()` accepts arbitrary Python callables (methods, lambdas, free functions).
- **Hot Reload Reliability**:
    - Continuous editor idle watching reloads external script and scene edits without starting gameplay.
    - Hot reload selectively updates scenes matching the active hierarchy, preserving Python instance state and physics velocities.
    - Texture and atlas modifications automatically invalidate cached renderer textures.
- **Expanded Audio Support**: Added pure-Rust decoding via Symphonia for Ogg Vorbis (`.ogg`), MP3 (`.mp3`), FLAC (`.flac`), and extended WAV formats.
- **Scene File Extension Standardized to `.ptscn`**: Migrated scene file naming to `.ptscn` across editor, tooling, templates, and documentation.
- **Standalone Player Releases & Cross-Platform Export**:
    - `pite export` automatically downloads platform-specific `pite-player` release archives when missing locally.
    - Release pipeline publishes standalone and multi-platform player archives.
    - Added macOS export and cross-compilation support.

## v0.2.0

- **Python Dependency Vendoring (`pite vendor`)**:
    - Added `pite vendor add/remove/list/verify` to vendor third-party wheels into `vendor/` with cryptographic SHA-256 locking in `[vendor]`.
    - The runtime configures `vendor/` on `sys.path` across development and exported builds.
    - `pite check` audits game scripts and warns on non-vendored third-party imports.
- **Integrated Code Completion**:
    - Embedded Pyright language server provides autocomplete popups, hover information, and diagnostics directly in the code editor.
    - Baked-in `pite.pyi` stubs ensure full engine API autocompletion out of the box.
- **Design Resolution & Window Scaling**:
    - Default project resolution updated to 1280×720 with uniform aspect-ratio scaling, letterboxing, and mouse coordinate mapping.
- **Project Settings Window**:
    - Added interactive settings dialog (`Project > Project Settings…`) for editing manifest fields with validation and byte-stable saving.
- **Code Editor Enhancements**:
    - Added line-number gutter with error row indicators and click-to-jump error navigation.
    - Syntax highlighting for Python and configuration file formats (`toml`, `json`, `yaml`, `ini`).

## v0.1.1

- **Timer Subsystem (`Timer.timeout`)**: Engine-owned `timeout` signal automatically fires every `wait_time` seconds and loops, with bidirectional property synchronization between Rust and Python.
- **Button Sizing**: `Button.size` property exposed as a script-accessible, type-hinted `Vec2` property for hit-testing and rendering.
- **Manifest Cleanup**: Deprecated unused `export.platforms` manifest key; `pite export` validates execution from within a project root.
- **Signal Type Safety**: Enhanced signal emission validation via Pyright type stubs in `pite.pyi`.

## v0.1.0

Initial public release series establishing Pite's foundational architecture:

- **Scene Tree Architecture**:
    - Hierarchical node tree (`Node`, `Node2D`, `Sprite2D`, `Camera2D`, `Timer`, `Label`, `Button`) with parent-child coordinate inheritance and lifecycle dispatch (`_ready`, `_process`).
    - Modular scene instantiation: nest sub-scenes within scenes with namespaced IDs and property overrides.
- **Embedded Python Scripting**:
    - High-performance embedded CPython 3.12 runtime powered by PyO3.
    - Public `pite` module with transform bindings, vector math, input polling, typed signals (`pite.signal`), and `pite.pyi` type stubs.
    - Live script hot reload on save without restarting the engine.
- **Visual Desktop Editor**:
    - Desktop editor built with `egui`/`eframe`: real-time 2D viewport, visual scene tree, property inspector, asset file tree, and code editor.
    - Design system with canonical Pite Teal palette, dark UI theming, and embedded typography.
- **TOML Scene Format & Binary Caching**:
    - Human-readable TOML scenes (`.ptscn`) as the canonical source of truth for version control.
    - Automatic content-hashed binary scene caches (`_cache/*.bin`) for fast loading with transparent fallback.
- **Asset Pipeline & Verification Tooling**:
    - Project-relative resource paths (`res://…`) and content-addressed UID manifest (`assets.pite.toml`).
    - `pite check` CLI tool for validating asset references, script imports, and scene schemas, with automatic path rewriting on asset moves (`--reimport-assets`).
    - Texture atlas batching (`.atlas.json`) for efficient sprite sheet rendering.
- **Audio Output**:
    - Multi-voice audio playback via CPAL with graceful silent simulation when no hardware output device is detected.
- **Standalone Desktop Export**:
    - Self-contained export pipeline producing lean, runnable bundles for Linux and Windows with bundled CPython and minimal `pite-player` runner.
- **Crates.io Release**:
    - Modular workspace crates (`pite-core`, `pite-scene`, `pite-project`, `pite-runtime`, `pite-render`, `pite-script`, `pite-audio`, `pite-assets`, `pite-export`, `pite-player`, `pite-editor`, `pite-cli`) published to crates.io.
