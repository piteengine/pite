# Changelog

## Unreleased

- Assets panel file management:
    - Full-featured file browser: folders sorted first, empty folders preserved, single-click row selection, and double-click file opening or folder expanding.
    - Creation modals: header `+ File` and `+ Folder` buttons plus context menu `New…` open modal dialogs with starter templates for scripts (`.py`), scenes (`.ptscn`), and empty files/folders.
    - Context menu and shortcuts: Cut (`Ctrl+X`), Copy (`Ctrl+C`), Paste (`Ctrl+V`), Duplicate (`Ctrl+D`), Rename (`F2`) with automatic reference rewriting across scenes and scripts via `pite_assets::apply_renames`, and Delete (`Del`) with confirmation.
    - Operating system file drop: dragging files or folders from the OS into the Assets panel copies them into the target directory with automatic collision suffixing (`file (1).ext`) and asset manifest reimport.
    - Internal drag-and-drop moving: moving files or folders into other directories on disk with cycle protection and project reference rewriting.
    - Cross-panel drag-and-drop: dragging a `.py` script onto a Scene Tree node attaches it, and dragging an image or atlas onto Inspector fields assigns the property.
    - Path helpers: context menu options for `Copy Relative Path (res://…)`, `Copy Absolute Path`, and `Show in File Manager`.
- Scene panel modal node creation and drag reordering:
    - Modal node creation dialog featuring a Godot-style node inheritance tree, search filtering, and descriptions.
    - New nodes insert directly under the selected node (or root), expanding the parent.
    - 3-2 drag reordering (center = child, bottom = after sibling, top of first child = before sibling) with visual drop indicators and floating preview.
    - Node duplication (`Ctrl+D`), renaming (`F2`), and instantiate child scene modal (`Ctrl+Shift+A`).
    - Category color coding and Phosphor icons for distinct node types.
- Inspector overhaul:
    - Inheritance-grouped property sections (e.g., `Node`, `Node2D`, `CharacterBody2D`) reflecting the class hierarchy.
    - Revert-to-default buttons next to overridden properties.
    - Responsive layout with drag-target property assignment.

## v0.3.0

- Added continuous editor hot reload: `poll_watch` runs while editing so external script and scene changes reload without starting gameplay.
- Changed hot reload to rebuild only the active scene when the touched `.ptscn` matches the main scene or its transitive sub-scenes.
- Fixed script reload to preserve custom instance attributes and body velocity while clearing stale `sys.modules` entries.
- Fixed asset cache invalidation so editing `.png` or `.atlas.json` files invalidates cached textures and redraws viewports.
- Added `Vec2`: native 2D math with operators, in-place mutation, indexing, tuple unpacking, `.length()`, and `.normalized()`.
- Changed transform mutation to update `ScriptHost` directly, removing the end-of-frame sync pass.
- Added the attribute input API `pite.held.<key>`, `pite.pressed.<key>`, and `pite.released.<key>`, with `TypeError` on string calls.
- Added node descriptors `pite.child("Name")` and `pite.node("../Path")` with automatic first-access proxy caching.
- Added node proxy caching so repeated `get_node()` lookups skip tree walks.
- Changed `signal.connect()` to accept arbitrary Python callables alongside engine-dispatched signals.
- Added editor pause-on-error: gameplay script exceptions pause the editor, log the traceback, and open the failing script.
- Added an export fallback that downloads a missing `pite-player` archive into the toolchain cache.
- Added standalone per-platform player bundles and all-platform player archives to the release workflow.
- Added Ogg Vorbis, MP3, FLAC, and extended WAV decoding via `symphonia`, with importers and `pite check` validation.
- Changed the scene file extension from `.pitescene` to `.ptscn`; existing projects only need to rename their scene files.
- Added plain-text editing for `toml`/`cfg`/`ini`/`json`/`yaml`/`yml` in the code pane, while LSP features remain Python-only.
- Added palette-based syntax highlighting for Python and config files in the code pane.
- Changed editor menus to `Scene`, `Project`, and `Help`, removing the `Run` menu while keeping transport buttons.
- Added macOS support for the editor and games, including cross-compilation.
- Added a 2D physics subsystem powered by Rapier2D, simulated at a fixed 60 Hz and driving `_physics_process(delta)`.
- Added physics nodes `StaticBody2D`, `CharacterBody2D`, `RigidBody2D`, `Area2D`, and `CollisionShape2D` with box, circle, and capsule shapes.
- Added `move_and_slide()` character sliding with velocity and contact flags (`is_on_floor()`, `is_on_wall()`, `is_on_ceiling()`).
- Added `apply_impulse()` and `apply_force()` for rigid bodies with `mass`, `gravity_scale`, and `lock_rotation`.
- Added `Area2D` trigger signals `body_entered`, `body_exited`, `area_entered`, and `area_exited`.
- Added `pite.raycast(from, to, mask=1)` returning intersection point, normal, distance, collider, and body.
- Added physics visualization in the editor: collision wireframes in the viewport and physics controls in the Inspector.

## v0.2.0

- Added `pite vendor add pkg==ver` to download wheels and dependencies into `vendor/`, with `list`, `verify`, and `remove` managing the lock file.
- Changed the runtime to put `vendor/` on `sys.path` in dev and export, with `pite check` staying silent for vendored imports.
- Added `pite check` warnings for third-party imports in game scripts.
- Changed `pite run` without `--scene` to start the project's `main_scene`, failing loudly outside a project.
- Changed the default window to a 1280×720 design resolution that opens maximized, scales with aspect, and maps clicks through the same transform.
- Added editor completion: cursor-anchored popup, filtering as you type, arrow-key selection, `Enter` to accept, and `Esc` or click to dismiss.
- Fixed the editor gutter so line numbers tile laid-out rows exactly and long files no longer drift out of alignment.
- Added clickable gutter line numbers with yellow error rows and a shared scroll area with the code pane.
- Fixed `import pite` to resolve everywhere: stubs are baked into the editor binary and materialized to a versioned scratch dir when no checkout copy exists.
- Changed `get_node` results to be typed as plain `Node`, with beyond-surface members dynamically typed instead of warning.
- Added the Project Settings window to edit every known manifest field with shared validation and byte-stable saves.
- Added the first `pite.toml` writer (`pite_project::save_manifest`) that preserves unknown keys.

## v0.1.1

- Added `Timer`, with an engine-owned `timeout` signal ticking every `wait_time` and two-way sync like `position` and `text`.
- Added script read/write access to `Button.size` through the same proxy/instance/sync channel, typed in `pite.pyi`.
- Removed the unused `export.platforms` field; old manifests still parse with a deprecation warning.
- Fixed `pite export` to fail loudly outside a project instead of exporting the working directory.
- Removed the dead string-signal detector from `pite check`; signal safety now comes from pyright stub visibility.
- Changed template scenes to derive `format_version` from the `pite-scene` const and manifest `pite_version` to track the engine version.

## v0.1.0

- Added the first bootable release: a nine-crate workspace, `pite new`/`run`/`check`, a window, an empty scene, and a minimal-2d example.
- Added the scene tree with remove/reparent, root protection, instantiation with overrides, and strict `check` validation.
- Added embedded Python scripting via PyO3, with `pite.Node2D` subclasses, `_ready`/`_process`, `Vec2`, error isolation, and `pite.pyi` stubs.
- Added hot reload for scripts and scenes plus the editor shell: viewport, tree, inspector, assets, console, and code pane.
- Added the playable 2D loop: textured sprites through `Camera2D`, key-driven input, and parent-carried transforms.
- Added typed signals with `pite.signal(T)` descriptors and a Rust `SignalRegistry`, including drop-disconnect.
- Added text nodes via `Label` with two-way script text sync, and `Button` with press-inside/release-inside clicks and a `pressed` signal.
- Added audio playback through `pite-audio`, `pite.play`/`pite.stop`, and real default-device output with a silent fallback.
- Added export: a runnable directory with referenced files only, a Python gate, and manifest round-trip.
- Added the asset pipeline: scanning, a uid manifest, dangling/moved asset errors, `pite reimport`, and png/wav importers.
- Added sprite-sheet atlases with named frames and UV rects, `Sprite2D.atlas`/`frame`, and batched frame drawing.
- Added the binary scene cache: TOML stays the source of truth, with content-hash binaries in `_cache/` and loud fallback.
- Added the locked `pite.toml` manifest schema with defaulted fields and preserved unknown keys.
- Added the LSP bridge: external pyright over stdio with completion, hover, signature help, and diagnostics in the code pane.
- Fixed editor saves to preserve `[[instance]]` entries, store prop edits as overrides, and write deterministic, byte-stable output.
    - Note: the first save after upgrading writes `[node.props]` keys in sorted order once; values are unchanged and later saves are no-ops.
- Added Python bundling: export fetches a pinned, checksum-verified CPython 3.12 and ships the interpreter plus stdlib zip, so exports need no system Python.
- Fixed the Windows bundle to use the `Lib/` stdlib layout, write `python312._pth`, and clean up failed export directories.
- Reduced export size by stripping debug symbols and engine symbols, shipping the interpreter once, deflating the stdlib zip, and slimming the staged stdlib.
- Fixed export to ship a referenced atlas sidecar's sheet, and fixed sprite quad geometry to emit six vertices with correct extents.
- Fixed colour-space parity so the editor viewport and game window present the same values, and made the editor viewport read the manifest's window size.
- Added scene-tree script markers that open the script in the code pane and notify the language server.
- Changed the editor menus and transport, added the Assets file tree, and repaired the layout so viewports are contained and panels scroll.
- Changed the brand to the teal palette, unified across logo, tray icons, and editor accent.
- Added the registry debut for `pite-core`, `pite-project`, and `pite-scene`, published automatically from `v*` tags.
- Added the lean `pite-player` export target: a bare `--scene` CLI with no reload or editor, resolved via `--binary` or a sibling binary, with release bundles and `SHA256SUMS` published per platform.
- Added scene-format, scripting-api, manifest, editor, and LSP documentation.
