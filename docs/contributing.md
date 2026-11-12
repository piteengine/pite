---
icon: lucide/signpost
---

# Contributing Guide

We welcome contributions across all areas of the project—code, documentation, examples, bug reports, and architectural discussions. Whether you are fixing a typo, improving docs, or proposing a new subsystem, we are excited to help you get your contribution merged.

## How to Help

- **Reporting Issues:** If you find a bug or have an idea for improvement, open an issue. Please include details on your environment, reproducible steps, expected results, and actual behavior. Console logs and minimal test cases are especially helpful.
- **Submitting Pull Requests:** When you are ready to implement a change, open a pull request. Keep each PR focused on a single change and follow the Conventional Commits specification (`feat:`, `fix:`, `docs:`, `chore:`). Smaller, focused PRs are reviewed and merged much faster.
- **Joining Discussions:** If you are unsure where to begin or wish to discuss design ideas before writing code, open a GitHub Discussion. All questions and ideas are welcome.
- **Getting Unblocked:** If you encounter roadblocks during development, reach out on your PR, issue, or discussion. The maintainers are happy to provide guidance.

## Development Workflow

- Maintain focused changes: keep unrelated modifications in separate commits or pull requests.
- Adhere to Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`).
- Never commit credentials, local filesystem paths, or build artifacts (`target/`, `_cache/`).

## Continuous Integration

Every pull request must pass the full workspace verification suite:

```sh
cargo fmt --all -- --check
cargo build --workspace --locked
cargo test --workspace --locked
PITE_LSP_SMOKE=1 cargo test -p pite-editor --lib live_pyright_smoke
bash scripts/check-examples.sh
```

## Release Checklist

The release process requires coordinated version bumps across internal crates and documentation before tagging:

- `Cargo.toml`: Bump `version` across all 13 internal `pite-*` path-dependency pins (workspace crates publish in lockstep).
- `docs/changelog.md`: Move unreleased entries under the new version header with a release date.
- MSRV proof: Ensure `msrv-proof.yml` passes on the pinned workspace `rust-version`.
- Python bundle pins: When updating the embedded CPython distribution, update `python_bundle.rs` (`PINNED_PYTHON`, `PINNED_BUILD`, filenames, and SHA-256 checksums).
- Pyright language server: When upgrading pyright, synchronize the version pin in `pite-editor/src/lsp.rs`, `ci.yml`, and `AGENTS.md`.
- Scene format changes: If the `.ptscn` schema evolves, update `FORMAT_VERSION`, `CACHE_VERSION`, the template literals, and `docs/reference/scene-format.md`.
- Release tagging: Strict `v*.*.*` tags must match the workspace version; milestone tags (`v0.1-mN`) are non-publishing tags.

## Local Environment Setup

- **Python & Audio Headers:** Building embedded scripting requires CPython 3.12 with development headers, and Linux audio requires ALSA development headers (`libasound2-dev` and `pkg-config` on Debian/Ubuntu). Set environment variables to point PyO3 at your Python installation:

```sh
export PYO3_PYTHON=/path/to/python3.12
export LD_LIBRARY_PATH=/path/to/python3.12/lib:$LD_LIBRARY_PATH
```

- **WSL (Windows Subsystem for Linux):** On WSL environments without Wayland support, run the editor using the X11 backend:

```sh
WINIT_UNIX_BACKEND=x11 cargo run -p pite -- edit
```

## Community Governance and Policies

Contributors and community participants must adhere to our project policies:

- **Code of Conduct:** We are committed to a welcoming and harassment-free community. See [Code of Conduct](https://github.com/piteengine/pite/blob/main/CODE_OF_CONDUCT.md).
- **Security Policy:** To report vulnerabilities or review our security practices, see [Security Policy](https://github.com/piteengine/pite/blob/main/SECURITY.md).
- **Accessibility:** For our accessibility baseline, supported assistive environments, and barrier reporting, see [Accessibility Statement](https://github.com/piteengine/pite/blob/main/ACCESSIBILITY.md).

## Licensing

All contributions are dual-licensed under MIT and Apache-2.0. New Rust source files must begin with the standard SPDX header:

```rs
// SPDX-License-Identifier: MIT OR Apache-2.0
```

New Python files shipped as part of the engine must begin with:

```py
# SPDX-License-Identifier: MIT OR Apache-2.0
```

Example game scripts located under `examples/` omit license headers to keep starter projects clean and minimal.
