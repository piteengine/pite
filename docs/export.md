# Export (`pite export`)

Ships a playable build of the project: a lean `pite-player` binary, the
resolved content, and a launcher (`run.sh` / `run.bat`).

```sh
pite export linux|windows [--path <dir>] [--out <dir>] [--binary <file>]
  [--skip-python-check] [--no-bundle-python]
```

Must run inside a project (a directory with `pite.toml` at or above it);
outside one the command fails instead of exporting the working directory.
`--path` points it at another project, `--out` overrides the output
directory. `export.platforms` was removed in 0.1.1 (export never read it);
old manifests carrying it still parse with a deprecation warning.

## Player binary

Which binary an export ships: an explicit `--binary`, else a `pite-player`
sibling (`pite-player.exe` on Windows) next to the engine binary. Every
engine bundle ships both players, so cross-platform export needs no download
and no compiler. Without either, export fails and tells you to pass
`--binary`.

## Python

By default export bundles a pinned CPython (one-time download per platform
at export time) plus a deflated stdlib zip. Two flags change that:

- `--no-bundle-python` keeps the system-Python requirement: the target
  machine needs CPython 3.12 on `PATH`. The launchers resolve it via
  `PITE_PYTHON` first, then fall back to `python3.12`.
- `--skip-python-check` skips the host-side interpreter check, for
  cross-machine builds where the host lacks the target's Python.

## Content

Only referenced files ship: reachable scenes (from `main_scene`), attached
scripts, `res://` asset strings, `pite.play("…")` literals, atlas sheets,
plus `[export] include` extras. The stdlib zip carries no `site-packages`,
so third-party imports die in the export (`pite check` warns), and sibling
helper modules must be listed under `include` to be staged.

## Output

Export prints the output directory, the shipped binary, the staged file
count, and the Python choice (`bundled` vs `system CPython 3.12 required`).
Next step is printed per platform (`run.sh` / `run.bat`). Sizes are slim by
design (see CHANGELOG `v0.1-m5`: stdlib zip deflated, staged `libpython`
stripped, interpreter shipped once).
