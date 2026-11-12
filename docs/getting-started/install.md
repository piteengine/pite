---
icon: lucide/download
---

# Installation

This guide explains how to install Pite on Windows, macOS, and Linux. Installation typically takes about a minute, depending on your internet connection.

![GitHub Release](https://img.shields.io/github/v/release/piteengine/pite?style=flat-square&label=Current%20Latest%20Version:%20Pite&labelColor=0b9387&color=0b9387)

!!! Warning "If you are upgrading"
    **Pite is still v0.x,** so its new versions may break compatibility with older versions and projects created in those versions. Before updating Pite, please review [release notes](https://github.com/piteengine/pite/releases) of the version you are updating to it to see possible breakings, and **consider backing up your projects.** Using a [VCS](https://en.wikipedia.org/wiki/Version_control) like [Git](https://git-scm.com/) can be useful too.

## Prerequisites

- **Operating System Setup:**
    - Windows 10 or 11 (64-bit)
    - macOS 11+
    - Linux with Vulkan support (Ubuntu 22.04+)[^1] + X11 or XWayland + `libasound2` installed
- **CPU:** No limit[^2]
- **RAM:** 500 MB free[^3]
- **GPU API:** DirectX 12+ or Vulkan 1.2+ or Metal 2+
- **Display:** 1280×720 recommended at least[^4]
- **Python:** 3.12+[^5] to run games in dev mode[^6]

## Installation Methods

### Installer Script

Run below command in your terminal:

=== "Linux / macOS"
    ```sh title="Terminal"
    curl -fsSL https://raw.githubusercontent.com/piteengine/pite/main/install.sh | sh
    ```

    !!! Note "Requires `curl` and `tar` to be installed."

    !!! Info "Only works for x86_64. Try other methods for other architectures."

=== "Windows"
    ```pwsh title="PowerShell"
    irm https://raw.githubusercontent.com/piteengine/pite/main/install.ps1 | iex
    ```

The script identifies latest version, detects OS and architecture, downloads, verifies, and then installs Pite plus each platform [Pite Player](../guides/export#cross-platform-exporting) for the current user. You should see a success message with the version and installation location. The `pite` command will be available after installation completes.

!!! Info "To Update: Same command works for updating to newer versions."

??? Tip "Installing a specific version"
    === "Linux / macOS"
        You can install another version than the latest one with `PITE_VERSION` environment variable or positional `<tag>` argument:

        ```sh title="Terminal"
        PITE_VERSION="v0.3.0" sh -c "$(curl -fsSL https://raw.githubusercontent.com/piteengine/pite/main/install.sh)"

        curl -fsSL https://raw.githubusercontent.com/piteengine/pite/main/install.sh | sh -s -- v0.3.0
        ```

    === "Windows"
        You can install another version than the latest one with `-Tag` argument:

        ```pwsh title="PowerShell"
        iex "& { $(irm https://raw.githubusercontent.com/piteengine/pite/main/install.ps1) } -Tag 'v0.3.0'"
        ```

??? Tip "Installing to a specific path"
    === "Linux / macOS"
        Installer puts executables to `#!sh $HOME/.local/bin` by default. You can change this to a custom path with `PREFIX` environment variable:

        ```sh title="Terminal"
        PREFIX="$HOME/bin" sh -c "$(curl -fsSL https://raw.githubusercontent.com/piteengine/pite/main/install.sh)"
        ```

    === "Windows"
        Installer puts executables to `%LOCALAPPDATA%\Pite` by default. You can change this to a custom path with `-Prefix` argument:

        ```pwsh title="PowerShell"
        iex "& { $(irm https://raw.githubusercontent.com/piteengine/pite/main/install.ps1) } -Prefix 'C:\Tools\Pite'"
        ```

### Portable Install via GitHub

Engine executables are available in [GitHub releases](https://github.com/piteengine/pite/releases) too. Each release contains engine binary for each platform with export utilities for all platforms alongside it:

- `pite-<version>-linux.tar.gz`
- `pite-<version>-windows.zip`
- `pite-<version>-macos.tar.gz`

You can download your platform build and then run `pite` (or `pite.exe`) executable inside it without installation (portable method).

!!! Info "**To Update:** You can use any number of version with this method. If you don't want to keep old versions extract new version in same place to overwrite its files."

!!! Note
    The remaining files are standalone `pite-player` binaries — one per platform plus all-platform archives — used by `pite export` when no player sibling exists, and by `cargo install` users (cargo only builds `pite`, not the players).

    For more information, see the [Export Guide](../guides/export#cross-platform-exporting).

### Build From Source via Cargo

Pite can be built from source via [crates.io](https://crates.io/crates/pite):

```sh title="Terminal"
cargo install pite
```

!!! Note
    Cargo installation method builds from source and has system requirements:

    - **Rust 1.95+ toolchain** (`cargo`)
    - **Python 3.12** with development headers (e.g. `python3.12-dev` on Debian/Ubuntu, or Python 3.12 on macOS/Windows).
    - **ALSA headers** on Linux: `sudo apt-get install libasound2-dev pkg-config`.

!!! Note "This method takes longer to finish, usually 5-15 minutes."

!!! Info "To Update: Use `--force` flag: `cargo install pite --force`"

!!! Info
    This method won't install [Pite Players](../guides/export#cross-platform-exporting), they will be downloaded at export time or when you run `pite toolchain download`.

## Verify the Installation

To confirm that Pite installed correctly run following command inside terminal, you may need to restart your terminal:

```sh title="Terminal"
pite --version
```

It prints `pite x.y.z` on success.

## Troubleshooting

??? Warning "The installer does not start"
    Verify you have internet connection, you can also download installer script from GitHub and then run it locally:

    - [Windows: `install.ps1`](https://github.com/piteengine/pite/blob/main/install.ps1)
    - [Linux / macOS: `install.sh`](https://github.com/piteengine/pite/blob/main/install.sh)

??? Warning "The editor fails to launch"
    Check the console log for more information. Try updating your GPU drivers to match requirements.

If the issue persists, **[Create an Issue](https://github.com/piteengine/pite/issues/new/choose)** and describe the situation alongside information about your OS, Pite version, and relevant logs.

## Uninstall

Remove executables from where you installed them (`~/.local/bin` on Linux and macOS and `%LOCALAPPDATA%\Pite` on Windows).

[^1]: Only depends on supporting Vulkan 1.2+
[^2]: Requires SSE4.2 support, which is supported in most CPUs since 2008.
[^3]: Engine baseline: 200 MB; RAM usage of running game highly depends on Python scripts usage.
[^4]: Default windowed size is configured to this size, but is configurable in `pite.toml` file.
[^5]: Engine defaults Python 3.12 to be used exported games, so it's recommended to use Python 3.12 to have exactly same behavior as exported games in your dev runs.
[^6]: Running game in dev mode means using `pite run` command or `▶` play button inside editor to run the game from source. The engine fetches a pinned slim Python 3.12 version and embeds it inside exported game, so if you just want to export a game (and not to run it in dev mode), having Python installed is not necessary.
