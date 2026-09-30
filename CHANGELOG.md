# Changelog

## v0.1-m1

First bootable milestone: 2D scaffold with scripting and a minimal editor.

- M0 (`5a6cd67`): nine-crate workspace, `pite new/run/check` skeleton, winit window, empty scene, `examples/minimal-2d` dogfood, Python stubs, CI.
- M1a (`319fbc5`): tree remove/reparent with root protection, real scene instantiation with overrides, strict `check`.
- M1b (`d1f6489`): embedded Python via PyO3 — `class X(pite.Node2D)`, `_ready`/`_process`, Vec2 crossing, error isolation, real `pite.pyi`.
- M1c (`71fc871`): notify hot reload for scripts and scenes, eframe editor (viewport, tree, inspector, assets, console, code pane), working `get_node`, `pite edit`.
- Docs (`19ff1fc`): `docs/scene-format.md`, `docs/scripting-api.md` one-pagers.
