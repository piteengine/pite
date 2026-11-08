---
icon: lucide/share
---

# Export Guide

Package your Pite project into self-contained, standalone distribution builds for Windows, Linux, and macOS.

Pite exports produce lean, standalone game folders. Each exported bundle includes the lightweight engine runner binary (`pite-player`), a bundled CPython 3.12 runtime, and all required project assets. Your players can unzip and play immediately without installing Python, Rust, or any external runtimes.

---

## Basic Export Command

To export your project, run `pite export` from your project directory and specify the target platform:

=== "Windows"
    ```sh title="Terminal"
    pite export windows
    ```

=== "Linux"
    ```sh title="Terminal"
    pite export linux
    ```

=== "macOS"
    ```sh title="Terminal"
    pite export macos
    ```

The command outputs a ready-to-distribute folder in `dist/<project-name>-<platform>/`.

---

## Command Options

```sh title="Terminal"
pite export <platform> [options]
```

| Flag / Option         | Description                                                      | Default                     |
| :-------------------- | :--------------------------------------------------------------- | :-------------------------- |
| `platform`            | Target platform (`windows`, `linux`, `macos`)                    | *(Required)*                |
| `--path <dir>`        | Path to the project root directory                               | Current directory           |
| `--out <dir>`         | Override destination directory                                   | `dist/<project>-<platform>` |
| `--binary <file>`     | Ship a custom player binary instead of the bundled default       | Auto-resolved               |
| `--no-bundle-python`  | Require system Python 3.12 instead of bundling embedded CPython  | `false`                     |
| `--skip-python-check` | Skip verifying host Python when preparing cross-platform exports | `false`                     |

---

## Cross-Platform Exporting

Pite supports exporting to any target platform from any development operating system. For example, you can build Windows `.exe` packages and macOS builds directly from Linux, or produce Linux builds on Windows.

### How It Works

1. **Standalone Player Binary**: Pite ships games using `pite-player`, a minimal runner that includes only the runtime engine (excluding the editor). If the target platform's player is not already present beside your `pite` executable, Pite automatically downloads and caches it in `~/.cache/pite/toolchains/player/`.
2. **Bundled Python**: Pite fetches a slim CPython 3.12 archive for the target platform and bundles it alongside the executable.
3. **No Cross-Compilers Needed**: Because precompiled runner binaries and runtime distributions are resolved automatically, you do not need cross-compilers, toolchains, or virtual machines installed.

??? Tip "Pre-downloading for Offline Export"
    If you plan to export while offline or in isolated CI environments, pre-fetch all runners and Python packages beforehand:

    ```sh title="Terminal"
    pite toolchain download
    ```

---

## Bundled Python Environment

By default, exports include a pinned CPython 3.12 environment with a compressed Python standard library zip.

### System Python Alternative

If you prefer to keep exports smaller and require users to have Python 3.12 installed on their system:

```sh title="Terminal"
pite export windows --no-bundle-python
```

When `--no-bundle-python` is passed:

- The bundle omits the Python DLL/shared library and standard library archive.
- The launcher scripts (`run.sh` or `run.bat`) resolve the interpreter using the `PITE_PYTHON` environment variable, falling back to `python3.12` on the system `PATH`.

---

## Content Staging and Packaging

Pite analyzes project references so that only assets actually used by your game are packaged into the export:

- **Reachable Scenes**: Scenes referenced directly or transitively from `main_scene` via `[[instance]]` declarations.
- **Scripts**: Python scripts attached to nodes in reachable scenes.
- **Assets**: Textures, audio files, and atlases referenced in scene properties (`res://...`) or script literals (`pite.play("res://...")`).
- **Vendored Packages**: The entire `vendor/` directory containing third-party packages installed via `pite vendor add`.

### Including Extra Files

If your game relies on external text files, data files, or documentation, declare them in `pite.toml` under `[export]`:

```toml title="pite.toml" hl_lines="2"
[export]
include = ["README.txt", "data/levels.json"]
```

Files listed in `include` are copied directly into the exported game folder during packaging.

---

## Exported Directory Structure

An exported Windows distribution folder resembles the following structure:

```text
dist/gem-collector-windows/
├── run.bat                   # Convenient launcher script
├── README.txt                # Included metadata
├── bin/
│   ├── gem-collector.exe     # Lean engine runner binary
│   └── python312.dll         # Embedded Python shared library
├── python/
│   └── python312.zip         # Compressed Python standard library
└── game/
    ├── pite.toml             # Project settings
    ├── scenes/               # Packaged scenes
    ├── scripts/              # Packaged scripts
    └── assets/               # Packaged textures and sounds
```

To test the exported build, open the folder and execute the launcher script (`run.bat` on Windows or `./run.sh` on Linux and macOS).
