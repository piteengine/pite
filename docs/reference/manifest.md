---
icon: lucide/file-cog
---

# Project Manifest (`pite.toml`)

Every Pite project is defined by a `pite.toml` manifest located in its root directory. The manifest schema is designed for stability and compatibility: all standard fields provide sensible defaults, enabling older configurations to load seamlessly, while unknown custom keys are preserved across round-trip edits rather than silently discarded.

```toml
[project]
name = "my-game"
pite_version = "0.3.0"
main_scene = "res://scenes/main.ptscn"
python = "3.12"
author = ""
icon = ""
window_width = 1280
window_height = 720

[export]
# binary_name = "my-game"  # Optional: defaults to the project name
# include = ["README.md"]  # Optional: additional files to bundle in exports

[vendor]
# requests = { version = "2.32.3", sha256 = "…" }  # Managed by `pite vendor`

[props]
# spawn_group = "string"  # Project-level custom property declarations
# boss_hp = "int"
# tint = "color"
```

---

## Configuration Fields

### `[project]`

- **`name`** (`str`, required): Project name used for window titles and default export binary naming.
- **`pite_version`** (`str`, required): The engine version targeted by the project.
- **`main_scene`** (`str`, required): Project-relative path (`res://…`) to the primary starter scene.
- **`python`** (`str`, default `"3.12"`): Target Python version for runtime and tooling.
- **`author`** (`str`, optional): Author or studio credit string.
- **`icon`** (`str`, optional): Project-relative path (`res://…`) to an application icon (`.png`).
- **`window_width`** / **`window_height`** (`int`, default `1280` / `720`): Base design resolution. The game opens maximized and uniformly scales content to fit the screen, preserving aspect ratio with clean letterboxing. Input coordinates map proportionally through the same projection.

---

## Subsystems

### `[export]`

Configures distribution packaging behavior:

- **`binary_name`** (`str`, optional): Custom executable filename for standalone export builds. Defaults to `project.name`.
- **`include`** (`list[str]`, optional): Extra non-asset files or folders to include in exported game packages (such as `README.md` or license files).
- Export copies the manifest into the distribution package, reading `name`, `main_scene`, `binary_name`, and `include`.

### `[vendor]`

Tracks third-party Python wheel dependencies:

- Each entry records package metadata: `{ version = "…", sha256 = "…" }`, with an optional `direct = true/false` flag indicating whether the package was requested directly or pulled as a transitive dependency.
- Entries are managed exclusively through the `pite vendor` CLI commands (`add`, `remove`, `list`, `verify`). Manual edits are discouraged.

### `[props]`

Declares custom, project-wide properties for game nodes:

- Entries follow the format `property_name = "type"`, where `type` is one of `string`, `int`, `number`, `bool`, `vec2`, or `color`.
- Declared properties appear in the editor inspector and can be read and written from Python scripts (`get_custom_property` / `set_custom_property`).
- Registering custom properties here prevents `pite check` from raising unknown-property warnings and ensures values are safely preserved across scene saves. Property names must not collide with core engine schema keys.

---

## Validation Rules

- **Type Safety**: Fields are strictly validated on load. A type mismatch produces a descriptive error (`cannot parse pite.toml`) indicating the offending line.
- **Path Resolution**: Paths must be project-relative resource strings (`res://…`). Absolute paths are prohibited and flagged during verification.
- **Forward Compatibility**: Unrecognized top-level or table keys survive read-write cycles in the editor and produce informational warnings during `pite check`.
- **Verification Modes**: Run `pite check` to inspect manifest integrity. Pass `--strict` to promote warnings to errors, or `--json` to output a machine-readable validation report.
