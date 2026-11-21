---
icon: lucide/terminal
---

# Command Line Interface

The `pite` binary is the central command-line entry point for project creation, running, integrity checking, visual editing, asset management, package vendoring, and game export.

```sh title="Terminal"
pite [COMMAND] [OPTIONS]
```

Running plain `pite` inside a project folder automatically opens the visual editor (`pite edit`). Outside a project, running `pite` displays CLI help on standard output and exits successfully; with `--json` it emits an error envelope and exits with status `2` instead.

---

## Output Streams

Commands separate human output from machine output:

- **Standard output (stdout)** carries JSON payloads (with `--json`) and explicitly requested data. Nothing decorative is printed there.
- **Standard error (stderr)** carries progress lines, success confirmations, diagnostic cards, tables, and warnings. Human readers see them on the terminal; pipelines can ignore them.

`--json` always emits exactly one JSON envelope on stdout and never emits ANSI escapes, regardless of `--color`.

### Human Output Style

Human output is built from styled spans, never from markup embedded in message strings, so the same text renders to ANSI on a terminal and to plain text in JSON:

- **Bold** marks paths, commands, and file names.
- **Cyan** marks identifiers: flags, codes, property names, template names, and package names. The `Fix:` label is cyan, and so is `Get started with:` after `pite new`.
- **Dim** marks secondary context such as hit counts, timings, and parenthetical notes.

Diagnostic cards state `Error:` or `Warning:` and the problem, then explain how it was found under `Why:` and what to do under `Fix:`. The former per-card `Hint:` field was merged into `Fix:`. When findings share one fix, a cyan `Hint:` line after the cards names the command and how many issues it clears, separated from both the cards and the summary bars by a blank line.

Commands with a verdict or stats end in a full-width bar: label on the left, stats on the right. Bars are green (success), yellow (warnings), red (failure), or dim (no changes, or purely informational output). Text on a colored bar is bold black; width follows the terminal (40–200 columns, 80 when not a TTY) and content grows the line instead of being truncated.

A command that prints several bars keeps them together as one block at the bottom: exactly one blank line separates the block from the work above it, and no blank lines fall between the bars.

Section headings such as `Binary`, `Files`, or `Reimporting assets:` are cyan and unindented; everything inside a section (tables, progress lines, warnings, extra detail) is indented by two spaces. Long-running steps rewrite their own line in place on a terminal (`Downloading …` becomes a green `Downloaded …`); when stderr is piped, both lines are printed in full.

Display text uses human casing (`Windows`, `Linux`, `macOS`; `Hit`, `Miss`, `Cached`, `Missing`, `Mismatch`). JSON status values stay machine-lowercase.

### JSON Envelope

Every command returns the same top-level shape:

```json
{
  "schema_version": 1,
  "command": "check",
  "ok": false,
  "data": { "...": "command payload" }
}
```

| Field            | Description                                                                                              |
| :--------------- | :------------------------------------------------------------------------------------------------------- |
| `schema_version` | Version of the envelope contract. Changes only if the envelope shape itself changes.                     |
| `command`        | The command name (`new`, `run`, `check`, `edit`, `export`, `toolchain`, `vendor`, or `pite`).            |
| `ok`             | `true` when the command succeeded. A failed check/verification sets `false` but still includes `data`.   |
| `data`           | Command payload, present on completion. Payload structs carry their own independent `schema_version: 1`. |
| `error`          | `{ "message", "chain" }`, present only when the command failed before producing a result.                |

### Exit Status

| Status | Meaning                                                                                                                                       |
| :----- | :-------------------------------------------------------------------------------------------------------------------------------------------- |
| `0`    | Command succeeded.                                                                                                                            |
| `1`    | The command ran but reported failure (check errors or strict warnings, `toolchain verify` / `vendor verify` mismatches) or returned an error. |
| `2`    | Usage error that never reached a command, including `--json` with no subcommand.                                                              |

---

## Commands

### `pite new`

Scaffold a new Pite game project directory with a valid `pite.toml` manifest, starter scene, and asset directory structure.

```sh title="Terminal"
pite new <NAME> [--template <TEMPLATE>]
```

| Argument / Flag         | Description                                                                                               |
| :---------------------- | :-------------------------------------------------------------------------------------------------------- |
| `<NAME>` *(Required)*   | Name of the project directory to create.                                                                  |
| `--template <TEMPLATE>` | Project template to scaffold. Available: `minimal-2d`, `gem-collector`. Omit to generate a blank project. |

---

### `pite run`

Run a scene directly using the engine runtime window.

```sh title="Terminal"
pite run [SCENE] [--no-reload]
```

| Argument / Flag | Description                                                                                             |
| :-------------- | :------------------------------------------------------------------------------------------------------ |
| `[SCENE]`       | Scene to run as a `res://` path or filesystem path. Defaults to `main_scene` configured in `pite.toml`. |
| `--no-reload`   | Disable live watching and hot reloading of scripts and assets during execution.                         |

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
pite check [PATH] [--reimport-assets] [--strict]
```

| Flag                | Description                                                                                                  |
| :------------------ | :----------------------------------------------------------------------------------------------------------- |
| `[PATH]`            | Path to project root. Defaults to current directory.                                                         |
| `--reimport-assets` | Re-scan project assets, reconcile asset manifest (`assets.pite.toml`), and rewrite moved asset refs.         |
| `--strict`          | Promote all warnings to errors, exiting with a non-zero exit status if any warning is reported.              |

Diagnostics are rendered as cards with a colored severity label (`Error:` / `Warning:`) followed by aligned `At:`, `Why:`, and `Fix:` fields: `Why:` explains how the problem was detected and `Fix:` what to do. `-v` also prints a `Checking scene caches:` block with one row per scene file, even when a scene is instanced by another. `--reimport-assets` prints a cyan `Reimporting assets:` block with one row per added, updated, renamed, or removed asset (renames include rewritten-reference counts); when nothing changed it says `Nothing to change.` The scene-cache, reimport, and check bars then print as one contiguous group at the bottom. Machine-readable output is available through the global `--json` flag.

---

### `pite export`

Package a project into a self-contained, standalone distribution folder for Windows, Linux, or macOS.

```sh title="Terminal"
pite export <PLATFORM> [OPTIONS]
```

| Argument / Flag       | Description                                                                                           |
| :-------------------- | :---------------------------------------------------------------------------------------------------- |
| `<PLATFORM>`          | Target operating system: `linux`, `windows`, or `macos`. *(Required)*                                 |
| `--path <PATH>`       | Project root directory. Defaults to current directory.                                                |
| `--out <OUT>`         | Destination folder for the exported bundle. Defaults to `dist/<project>-<platform>/`.                 |
| `--binary <BINARY>`   | Ship a specific pre-built player binary instead of downloading/resolving the default runner.          |
| `--skip-python-check` | Bypass host Python version verification when preparing exports.                                       |
| `--no-bundle-python`  | Require the target machine's system Python 3.12 instead of embedding a pinned CPython runtime bundle. |

Progress is grouped into `Binary`, `Python`, and `Files` sections. `-v` lists each copied project file as `Copied <path>` under `Copying N project files`. The final bar and `Next:` line render paths with the target platform's separator: backslashes for a Windows export, forward slashes otherwise.

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
| `clean`    | Remove only the toolchain artifacts you select, or the whole cache with `--all`.           |

`pite toolchain clean` requires explicit selectors: `--python <platforms>` and/or `--player <platforms>`, each taking a comma-separated list (`--player windows,linux --python macos`), or `--all` to remove every cached artifact. A bare `clean` is rejected so a typo cannot wipe the cache. Removed artifacts are listed as a table before the summary bar; when the selection has nothing cached, the bar reads `nothing to remove`.

---

### `pite vendor`

Manage third-party Python packages stored directly in the project's `vendor/` folder and pinned in `pite.toml`.

```sh title="Terminal"
pite vendor <COMMAND>
```

| Subcommand | Usage                      | Description                                                                   |
| :--------- | :------------------------- | :---------------------------------------------------------------------------- |
| `add`      | `pite vendor add pkg==ver` | Download wheels and dependencies into `vendor/`, pinning SHA-256 in manifest. |
| `remove`   | `pite vendor remove pkg`   | Remove a vendored package and clean up its files from `vendor/`.              |
| `list`     | `pite vendor list`         | List all currently vendored packages, forms, and pinned versions.             |
| `verify`   | `pite vendor verify`       | Verify that all vendored packages on disk match their pinned hashes.          |

`pite vendor add -v` labels each downloaded artifact as `Wheel <file>` and prints `Pinned to <hash>` indented under the package row it pins. `pite vendor verify` reports `Matches the pinned hash` or `Got unexpected <hash>` in the DETAIL column.

---

## Global Options

These options are accepted before or after any subcommand.

| Option            | Description                                                                                                                                                            |
| :---------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--color <WHEN>`  | Control colored output: `auto` (default, checks TTY and `NO_COLOR`), `always`, or `never`. `--json` always disables color.                                             |
| `--json`          | Emit a single machine-readable JSON envelope on stdout.                                                                                                                |
| `-v`, `--verbose` | Print extra detail about the command's work (per-file export copies, scene-cache lines, pinned hashes). It never raises the log level; use `RUST_LOG` for engine logs. |
| `-q`, `--quiet`   | Suppress progress, success lines, tables, and informational bars. Warnings, errors, and a failing verdict bar still print.                                             |
| `-h`, `--help`    | Print help information for any command.                                                                                                                                |
| `-V`, `--version` | Print engine version (`pite x.y.z`).                                                                                                                                   |

Color precedence: `--json` wins over everything, then an explicit `--color`, then a non-empty `NO_COLOR`, then terminal detection in `auto` mode.

---

## Environment Variables

| Variable             | Description                                                                                    |
| :------------------- | :--------------------------------------------------------------------------------------------- |
| `PITE_PYTHON`        | Explicit path to Python 3.12 binary to override auto-detection for development execution.      |
| `PITE_LSP_CMD`       | Custom language server command for the code editor (defaults to `pyright-langserver --stdio`). |
| `PITE_PLAYER_DIR`    | Custom directory location for caching downloaded export runner binaries.                       |
| `WINIT_UNIX_BACKEND` | Graphics backend for Linux systems under WSL (e.g. `WINIT_UNIX_BACKEND=x11`).                  |
| `PREFIX`             | Installation destination directory used by `install.sh` (defaults to `$HOME/.local/bin`).      |
