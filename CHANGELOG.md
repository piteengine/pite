# Changelog

## v0.1-m2 (pending tag)

Playable 2D run: real rendering, signals, text, audio, export, assets, editor, plus scene cache, manifest schema, and an LSP bridge. Counts are workspace `cargo test` greens at each landing commit.

- M2a playable (`a027137`): textured sprites through Camera2D (pixel-proven), key-driven input, parent-carried transforms, 26 green.
- M2b signals (`068aa52`): typed `pite.signal(T)` descriptors + Rust `SignalRegistry`, drop-disconnect, Space→recoil dogfood, 28 green.
- Editor theme (`739c828`): single `theme.rs`, dark + green accent, OFL fonts; brand assets + `DESIGN.md` alongside. Behavior untouched, 29 green.
- M2c text (`e608871`): `Label` via `ab_glyph` on the sprite pipeline (no wgpu upgrade), two-way script text sync, HP label on the hit signal, 38 green.
- Button (`e85743b`): rect on the sprite pipeline, armed press-inside/release-inside clicks, `pressed` signal, HitButton dogfood, 43 green.
- Audio (`7838a4f`): `pite-audio` + `WavBackend` state machine, `pite.play/stop` API, `check` literal scan, hit-sound dogfood, 47 green. Follow-up: real device sink (`cpal`/`rodio`) on hardware.
- Export (`76380b4`): runnable dir with referenced-files-only + Python gate at export and launch, manifest round-trip, 52 green. Follow-up: Python bundling (ship without dev Python).
- Asset pipeline (`e0d00a9`): scan + uid manifest + `check` dangling/moved errors + `pite reimport`, png+wav importers, 62 green. Follow-ups: atlases, runtime hot-reimport, orphan warnings.
- Editor level-up (`9795117` + `9ad233d` + `437f348`): save scene, tree add/remove/drag-reparent, generic inspector, error gutter, `docs/editor.md`; textured viewport through the shared draw path; Godot-like layout, vector icons, context menu, sibling-order ops.
- Binary scene cache (`5e8600b`): TOML stays source of truth, `_cache/` content-hash binaries, version stamps, loud fallback, `check` hit/miss/rebuilt (`--json` schema v2), 77 green.
- Manifest schema (`f5fc2bf`): locked `pite.toml` field set (author/icon placeholders, 800×600 window), all defaulted, unknown keys preserved + reported, `docs/manifest.md`, 81 green.
- LSP bridge (`1bce5b7`): external pyright over stdio (pinned `1.1.411`), background handshake, `pite.pyi` on `extraPaths`, completion/hover/signature/diagnostics in the code pane, `docs/lsp.md`, 92 green. No DAP, no bundled binary. Follow-up: the handshake now sends `workspaceFolders` (plus the `workspace/configuration` capability), so the server adopts the project root instead of analyzing nothing, and document/root URIs are built as proper `file:///D:/…` paths so Windows stops reporting the project directory as nonexistent.
- Release pass: CI installs the pinned pyright and runs a live LSP smoke (spawn → open → completion → change) against the dogfood script, exports the embedded-Python env explicitly, README/docs index describe the shipped engine.
- Instance-save fix (`92f5fc9`): editor save no longer flattens `[[instance]]` entries into plain nodes (data-loss fix) — the on-disk scene stays authoritative, instance entries are re-attached, prop edits inside a subtree are stored as `overrides`, and structurally inexpressible edits inside an instance (rename, retype, reparent, add/delete, removing an inherited prop) fail loudly instead of silently. Saves are deterministic now: prop keys are written sorted and empty `props`/`instance` tables are omitted, so save → load → save is byte-identical. 100 green.
- Upgrade note (`92f5fc9`): prop keys are written in sorted order, so the first save after upgrading reorders the existing `[node.props]` keys of a scene once. No values change, and every save after that is a no-op.
- Parked (not hidden): DAP, camera follow/smoothing/deadzone + bounds, multi-class Python files, 3D/physics/networking (out of scope per spec).

## v0.1-m1

First bootable milestone: 2D scaffold with scripting and a minimal editor.

- M0 (`5a6cd67`): nine-crate workspace, `pite new/run/check` skeleton, winit window, empty scene, `examples/minimal-2d` dogfood, Python stubs, CI.
- M1a (`319fbc5`): tree remove/reparent with root protection, real scene instantiation with overrides, strict `check`.
- M1b (`d1f6489`): embedded Python via PyO3 — `class X(pite.Node2D)`, `_ready`/`_process`, Vec2 crossing, error isolation, real `pite.pyi`.
- M1c (`71fc871`): notify hot reload for scripts and scenes, eframe editor (viewport, tree, inspector, assets, console, code pane), working `get_node`, `pite edit`.
- Docs (`19ff1fc`): `docs/scene-format.md`, `docs/scripting-api.md` one-pagers.
