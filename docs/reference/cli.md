---
icon: lucide/terminal
---

# Command Line Interface

The `pite` binary is the central command-line entry point for project creation, running, integrity checking, visual editing, asset management, package vendoring, and game export.

```sh title="Terminal"
pite [COMMAND] [OPTIONS]
```

Running plain `pite` inside a project folder automatically opens the visual editor (`pite edit`). Outside a project, running `pite` displays CLI help.

---

## Commands

### `pite new`

Scaffold a new Pite game project directory with a valid `pite.toml` manifest, starter scene, and asset directory structure.

```sh title="Terminal"
pite new <NAME> [--template <TEMPLATE>]
```

| Argument / Flag         | Description                                                                              |
| :---------------------- | :--------------------------------------------------------------------------------------- |
| `<NAME>` *(Required)*   | Name of the project directory to create.                                                 |
| `--template <TEMPLATE>` | Project template to scaffold. Available: `minimal-2d`. Omit to generate a blank project. |

---

### `pite run`

Run a scene directly using the engine runtime window.

```sh title="Terminal"
pite run [SCENE] [--no-reload]
```

| Argument / Flag | Description                                                                                          |
| :-------------- | :--------------------------------------------------------------------------------------------------- |
| `[SCENE]`       | Scene to run as a `res://` path or filesystem path. Defaults to `main_scene` configured in `pite.toml`. |
| `--no-reload`   | Disable live watching and hot reloading of scripts and assets during execution.                      |

---

### `pite edit`

Launch the visual desktop editor (`egui`/`eframe`) with real-time viewport, scene tree, property inspector, asset manager, and code editor.

```sh title="Terminal"
pite edit [SCENE]
```

| Argument  | Description                                                                                   |
| :-------- | :-------------------------------------------------------------------------------------------- |
| `[SCENE]` | Scene file to open initially. Defaults to `main_scene` declared in the project's `pite.toml`. |

---

### `pite check`

Validate project integrity: parse manifests, verify scene syntax, check asset references, validate audio headers, scan script imports, and enforce type invariants.

```sh title="Terminal"
pite check [PATH] [--reimport-assets] [--strict] [--json]
```

| Flag                | Description                                                                                                    |
| :------------------ | :------------------------------------------------------------------------------------------------------------- |
| `[PATH]`            | Path to project root. Defaults to current directory.                                                           |
| `--reimport-assets` | Re-scan project assets, reconcile the UID registry (`assets.pite.toml`), and rewrite moved asset references. |
| `--strict`          | Promote all warnings to errors, exiting with a non-zero exit status if any warning is reported.                |
| `--json`            | Format diagnostic findings as machine-readable JSON (`schema_version: 2`).                                     |

---

### `pite export`

Package a project into a self-contained, standalone distribution folder for Windows, Linux, or macOS.

```sh title="Terminal"
pite export <PLATFORM> [OPTIONS]
```

| Argument / Flag       | Description                                                                                          |
| :-------------------- | :--------------------------------------------------------------------------------------------------- |
| `<PLATFORM>`          | Target operating system: `linux`, `windows`, or `macos`. *(Required)*                                |
| `--path <PATH>`       | Project root directory. Defaults to current directory.                                               |
| `--out <OUT>`         | Destination folder for the exported bundle. Defaults to `dist/<project>-<platform>/`.                 |
| `--binary <BINARY>`   | Ship a specific pre-built player binary instead of downloading/resolving the default runner.          |
| `--skip-python-check` | Bypass host Python version verification when preparing exports.                                      |
| `--no-bundle-python`  | Require the target machine's system Python 3.12 instead of embedding a pinned CPython runtime bundle. |

---

### `pite toolchain`

Manage offline toolchain archives (runner binaries and embedded CPython runtimes) cached under `~/.cache/pite/toolchains`.

```sh title="Terminal"
pite toolchain <COMMAND>
```

| Subcommand | Description                                                                                |
| :--------- | :----------------------------------------------------------------------------------------- |
| `status`   | Print the status and presence of cached Python runtimes and player binaries.               |
| `download` | Pre-download all engine player binaries and Python runtimes for offline export capability. |
| `verify`   | Verify checksums and archive integrity of cached toolchains against engine hashes.         |
| `clean`    | Delete all cached toolchain downloads from the local machine.                              |

---

### `pite vendor`

Manage third-party Python packages stored directly in the project's `vendor/` folder and pinned in `pite.toml`.

```sh title="Terminal"
pite vendor <COMMAND>
```

| Subcommand | Usage                     | Description                                                                |
| :--------- | :------------------------ | :------------------------------------------------------------------------- |
| `add`      | `pite vendor add pkg==ver`| Download wheels and dependencies into `vendor/`, pinning SHA-256 in manifest. |
| `remove`   | `pite vendor remove pkg`  | Remove a vendored package and clean up its files from `vendor/`.           |
| `list`     | `pite vendor list`        | List all currently vendored packages, forms, and pinned versions.          |
| `verify`   | `pite vendor verify`      | Verify that all vendored packages on disk match their pinned hashes.       |

---

## Global Options

| Option        | Description                              |
| :------------ | :--------------------------------------- |
| `-h`, `--help`| Print help information for any command.  |
| `-V`, `--version` | Print engine version (`pite x.y.z`). |

---

## Environment Variables

| Variable             | Description                                                                                               |
| :------------------- | :-------------------------------------------------------------------------------------------------------- |
| `PITE_PYTHON`        | Explicit path to Python 3.12 binary to override auto-detection for development execution.                 |
| `PITE_LSP_CMD`       | Custom language server command for the code editor (defaults to `pyright-langserver --stdio`).            |
| `PITE_PLAYER_DIR`    | Custom directory location for caching downloaded export runner binaries.                                 |
| `WINIT_UNIX_BACKEND` | Graphics backend for Linux systems under WSL (e.g. `WINIT_UNIX_BACKEND=x11`).                            |
| `PREFIX`             | Installation destination directory used by `install.sh` (defaults to `$HOME/.local/bin`).                 |
