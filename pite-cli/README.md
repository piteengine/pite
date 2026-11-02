# pite

The CLI binary and visual editor for the [Pite](https://github.com/piteengine/pite) 2D scene-tree game engine.

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
- `pite run` — play the project's main scene
- `pite edit` — open the visual scene & script editor
- `pite check` — validate scene references, assets, and Python imports
- `pite export <platform>` — package a standalone distribution for Linux, Windows, or macOS
- `pite vendor` — vendor third-party Python wheels into the project

## Documentation

Full documentation, tutorials, and guides: <https://github.com/piteengine/pite>

## License

Dual-licensed MIT + Apache-2.0 ([LICENSE-MIT](https://github.com/piteengine/pite/blob/main/LICENSE-MIT), [LICENSE-APACHE](https://github.com/piteengine/pite/blob/main/LICENSE-APACHE)).
