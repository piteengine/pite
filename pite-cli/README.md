# pite

The primary command-line tool and visual editor launcher for the [Pite](https://piteengine.github.io/pite) 2D scene-tree game engine.

## Installation

```sh
cargo install pite
```

### System Prerequisites

- **Python 3.12** with development headers (e.g. `python3.12-dev` on Debian/Ubuntu, or Python 3.12 on macOS/Windows).
- **ALSA headers** on Linux: `sudo apt-get install libasound2-dev pkg-config`.

## Commands

- `pite` — open the visual editor for the project in the current directory
- `pite new <project>` — scaffold a new game project
- `pite run` — play the project's main scene directly
- `pite edit` — open the visual scene & script editor
- `pite check` — validate scene references, assets, and Python imports
- `pite export <platform>` — package a standalone distribution for Linux, Windows, or macOS
- `pite vendor` — vendor third-party Python wheels into the project

## Documentation

Full documentation, quickstart tutorials, and API references:
<https://piteengine.github.io/pite>

- Quickstart: <https://piteengine.github.io/pite/getting-started/quickstart/>
- CLI and Project Workflow: <https://piteengine.github.io/pite/guides/editor/>

## License

Dual-licensed MIT + Apache-2.0 ([LICENSE-MIT](https://github.com/piteengine/pite/blob/main/LICENSE-MIT), [LICENSE-APACHE](https://github.com/piteengine/pite/blob/main/LICENSE-APACHE)).

