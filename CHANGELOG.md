# Changelog

## Unreleased

- Input edges (`pite-runtime`): `GameSession::frame()` retires edge input after scripts consume it — `pite.pressed()`/`released()` and button clicks now work in the real loop, not just in tests. Mouse coordinates are physical pixels (no scale-factor division), fixing clicks on scaled displays.
- `Button.pressed` is declared by the engine for every `Button` node — scripts just connect, no `pressed = pite.signal()` needed.
- Audio device sink (`pite-audio/device.rs`, cpal 0.17): real output through the default device with silent-simulation fallback where none exists. Proven on Windows: compiles, 9/9 tests green incl. real device open, hit sound audible in `pite run`.
- `pite check` warns about orphan assets: scanned png/wav files no reachable scene references (`asset "res://…" is never referenced by a reachable scene`). Reachable-only by design — uninstantiated scenes and loose scripts stay silent. Atlas sheets count as referenced through their sidecar.
- Editor: play/pause/stop icon buttons beside the menus (mirroring the `Run` menu), filename-only scene label; Play mode forwards viewport keyboard/mouse into the running scene's scripts.
- Editor Assets is a real file tree: collapsible folders with a static folder glyph, per-type file icons (script, scene, gear, image, speaker, `?` for unknown) in the hand-drawn vector style, plain-text rows, full-panel scroll. `dist/`, `target/`, `.git/`, dotfiles, and the scene binary cache stay hidden.
- CI is split into named jobs (format, build, unit tests, LSP smoke, bundling smoke, `pite check`) with a shared cargo cache and branch concurrency; the export tests share one serialization lock so the offline-probe test can no longer redirect the bundling download mid-flight, and the downloader retries transient network errors.

## v0.1-m3

Correctness pass over rendering and export, plus scene-tree script markers. 135 green.

- Windows bundle fix (`a6fa4e8`): the bundled interpreter never produced a working Windows export. The pinned spec named the stdlib at `lib/python3.12`, but the Windows `install_only` archive keeps it in `Lib/`, so `stage()` bailed *after* copying the library — leaving 136 MB of unpacked CPython in the output directory, which the exists-guard then refused to overwrite. The staged library was also `python3.dll`, a 56 KB forwarder onto `python312.dll` rather than the interpreter. Windows does not bootstrap a stdlib zip from `PYTHONPATH` the way Linux does: it reads `python312._pth` from beside the DLL and ignores `PYTHONPATH` once that file exists, so the export now writes one. A failed export no longer leaves its directory behind. Verified by running `run.bat` on Windows — it previously died with "Could not find platform independent libraries" and now boots with no system Python installed. The layout tests read the real cached archives instead of a fixture that assumed the Linux layout, which is why this shipped; the opt-in bundle smoke test also asserted a path `stage()` never wrote to, so it had never passed.
- Staged-stdlib slimming (`9154343`): drop `venv` (8 MB of the Windows stdlib), `lib2to3`, `pydoc_data`, `turtledemo`, `msilib`. Windows stdlib zip 19 MB → 9.0 MB (export 47 MB → 37 MB), Linux → 9.3 MB. Nothing Pite ships imports any of them; both zips keep every startup-critical module.
- Export atlas dependency (`6948895`): export shipped `sheet.atlas.json` without the `sheet.png` it names, so the runtime fell back to a 1x1 magenta placeholder and both atlased sprites drew as magenta blocks. A referenced sidecar now also pulls in its sheet, resolved through the same helper the runtime uses; unreferenced files stay out.
- Colour-space parity (`7f003d9`): the editor viewport and the game window cleared to the same value but presented it differently — the sRGB surface encoded it, the non-sRGB offscreen target stored it raw and the UI read it as already-encoded. Sprites took the same double conversion. The offscreen target is now sRGB, matching the surface and the texture format.
- Sprite quad geometry (`8b7dd3f`): the pipeline is `TriangleList`, which needs six vertices per quad, but four were emitted — so half of each sprite drew and a spurious triangle bridged one quad to the next, visible as a diagonal smear between sprites. Extents were also halved, rendering a 32x32 frame as 16x16. Three tests encoded the wrong values and now check pixel spans.
- Editor viewport size (`ecc02ac`): the viewport rendered at a hardcoded 640x400 while the game window used the project's configured size, so authoring framing never matched the exported game. It now reads `window_width`/`window_height` from the manifest. The per-node caption overlay is gone.
- Scene-tree script markers (`50bba0b`): nodes with an attached script are marked with a page glyph; hovering names the class and clicking opens the file in the code pane and notifies the language server. The add icon is redrawn as two filled bars.
- Documentation (`e7f90ad`, `6820e36`): removed references to external planning documents and internal milestone labels from sources, docs and this file; corrected two stale claims about the signal API.

## v0.1-m2

Playable 2D run: real rendering, signals, text, audio, export, assets, editor, plus scene cache, manifest schema, and an LSP bridge. Counts are workspace `cargo test` greens at each commit.

- Playable 2D (`a027137`): textured sprites through Camera2D (pixel-proven), key-driven input, parent-carried transforms, 26 green.
- Typed signals (`068aa52`): typed `pite.signal(T)` descriptors + Rust `SignalRegistry`, drop-disconnect, Space→recoil dogfood, 28 green.
- Editor theme (`739c828`): single `theme.rs`, dark + green accent, OFL fonts; brand assets + `DESIGN.md` alongside. Behavior untouched, 29 green.
- Text nodes (`e608871`): `Label` via `ab_glyph` on the sprite pipeline (no wgpu upgrade), two-way script text sync, HP label on the hit signal, 38 green.
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
- Atlas slice: sprite-sheet atlases (one PNG + `<sheet>.atlas.json`) with named frames and UV rects; `Sprite2D` gains `atlas` + `frame` (mutually exclusive with `texture`, loud error otherwise), all frames of a sheet draw in one batched call, `pite check` validates frame names and flags unreferenced frames, `pite.frames()` lists them. Dogfood packs player + enemy into `assets/sheet.png`. 116 green.
- Python bundling: export fetches a pinned, checksum-verified CPython 3.12 into a toolchain cache and ships the interpreter library + stdlib zip (no `site-packages`) so exports run with no system Python; `--no-bundle-python` keeps the old gate. Loader story proved on Linux (`LD_LIBRARY_PATH` from the launcher, and `RUNPATH $ORIGIN` when built with it); offline and checksum-mismatch paths fail loudly. 127 green.
- Upgrade note (`92f5fc9`): prop keys are written in sorted order, so the first save after upgrading reorders the existing `[node.props]` keys of a scene once. No values change, and every save after that is a no-op.
- Parked (not hidden): DAP, camera follow/smoothing/deadzone + bounds, multi-class Python files, 3D, real physics and networking.

## v0.1-m1

First bootable release: 2D scaffold with scripting and a minimal editor.

- Workspace skeleton (`5a6cd67`): nine-crate workspace, `pite new/run/check` skeleton, winit window, empty scene, `examples/minimal-2d` dogfood, Python stubs, CI.
- Tree and instantiation (`319fbc5`): tree remove/reparent with root protection, real scene instantiation with overrides, strict `check`.
- Embedded scripting (`d1f6489`): embedded Python via PyO3 — `class X(pite.Node2D)`, `_ready`/`_process`, Vec2 crossing, error isolation, real `pite.pyi`.
- Hot reload and editor shell (`71fc871`): notify hot reload for scripts and scenes, eframe editor (viewport, tree, inspector, assets, console, code pane), working `get_node`, `pite edit`.
- Docs (`19ff1fc`): `docs/scene-format.md`, `docs/scripting-api.md` one-pagers.
