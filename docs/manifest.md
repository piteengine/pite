# Manifest (`pite.toml`)

Locked field set. Every field has a default, so old projects parse untouched; unknown keys are preserved and reported, never silently dropped.

```toml
[project]
name = "my-game"
pite_version = "0.2.0"
main_scene = "res://scenes/main.ptscn"
python = "3.12"
author = ""
icon = ""
window_width = 1280
window_height = 720

[export]
# binary_name = "my-game"  # optional, defaults to the project name
# include = ["README.md"]  # optional extra files

[vendor]
# requests = { version = "2.32.3", sha256 = "…" }  # managed by `pite vendor`
```

## Rules

- Wrong types fail loudly at parse (`cannot parse pite.toml`) — never silent defaults.
- `window_width` / `window_height` must be positive; `python` must be non-empty; `icon` is empty (no icon) or `res://…` (absolute paths are forbidden, a missing file warns).
- The window size is the design resolution: the game opens maximized and scales content uniformly to fit, keeping aspect (letterboxed); clicks map through the same transform. The editor preview frames this same resolution.
- Unknown fields (`custom`, `project.nickname`, `export.bundle`) survive a parse→write round-trip and warn in `pite check`.
- `[vendor]` rows are `{ version, sha256 }` plus `direct` for asked-for packages (transitives record `false`); rows without `form` are zipped wheels. Managed by `pite vendor add/remove/list/verify`, never hand-edited.
- `export.platforms` was removed in 0.1.1 (export never read it). Old manifests carrying it still parse and get a deprecation warning from `pite check`.
- Export copies the manifest verbatim and reads only `name`, `main_scene`, `binary_name`, `include`.
- `pite check --strict` promotes warnings to failures; `pite check --json` emits a machine-readable report (`schema_version: 2`).
