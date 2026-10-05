# Manifest (`pite.toml`)

Locked field set. Every field has a default, so old projects parse untouched; unknown keys are preserved and reported, never silently dropped.

```toml
[project]
name = "my-game"
pite_version = "0.1"
main_scene = "res://scenes/main.pitescene"
python = "3.12"
author = ""
icon = ""
window_width = 800
window_height = 600

[export]
platforms = ["linux"]
# binary_name = "my-game"  # optional, defaults to the project name
# include = ["README.md"]  # optional extra files
```

## Rules

- Wrong types fail loudly at parse (`cannot parse pite.toml`) — never silent defaults.
- `window_width` / `window_height` must be positive; `python` must be non-empty; `icon` is empty (no icon) or `res://…` (absolute paths are forbidden, a missing file warns).
- Unknown fields (`custom`, `project.nickname`, `export.bundle`) survive a parse→write round-trip and warn in `pite check`; deprecated fields would warn with their replacement (none today).
- Export copies the manifest verbatim and reads only `name`, `main_scene`, `binary_name`, `platforms`, `include`.
- `pite check --strict` promotes warnings to failures; `pite check --json` emits a machine-readable report (`schema_version: 2`).
