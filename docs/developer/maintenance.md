---
icon: lucide/user-star
---

# Maintenance Notes

This document keeps important maintenance tasks and workflows documented. Its usual usage is by maintainers, but others can use it too to know how Pite maintenance team works.

## Maintainer Responsibilities

Maintainers share four duties: triage incoming reports, review and merge pull requests, cut releases, and handle security and conduct reports privately. Nobody is on call; every duty below is best effort and can be picked up by any maintainer.

## Triage

- Security reports are never triaged in public. Move them to a private advisory before discussing details.
- Work in this order: private security reports, regressions in a released version, bugs on `main`, feature requests, then questions. Redirect usage questions to [GitHub Discussions](https://github.com/piteengine/pite/discussions) and close the issue with `invalid` or a link to the discussion.
- Apply exactly one `type:` label and, when the affected area is clear, one or more `subsystem:` labels.
- Use `needs more info` when a report lacks reproduction steps, versions, or logs, and state exactly what is missing. Close as `invalid` when the requested information never arrives.
- Mark duplicates with `duplicate` and link the original before closing.
- Explain `wontfix` with a reason: out of scope, conflicts with the architecture, or maintenance cost outweighs the benefit.
- Reserve `good first issue` and `help wanted` for work that is already scoped and does not depend on an undecided design.
- Group planned work into milestones. Issues without a milestone are revisited when the next milestone is cut.

## Labels

### Type Labels

Every issue and pull request gets one type label. Pull request authors tick the matching box in the template; maintainers confirm or correct it.

| Label            | Meaning                                                        |
| ---------------- | -------------------------------------------------------------- |
| `type: a11y`     | Finds or removes a barrier affecting people with disabilities. |
| `type: chore`    | Maintenance, CI, or internal tooling.                          |
| `type: dep`      | Related to or caused by dependencies.                          |
| `type: docs`     | Improves or fixes the documentation.                           |
| `type: feature`  | Adds a feature or improves its behavior.                       |
| `type: fix`      | A bug fix or a patch.                                          |
| `type: perf`     | Finds or fixes a performance problem.                          |
| `type: refactor` | Internal refactoring with no behavior change.                  |
| `type: style`    | Non-functional styling.                                        |
| `type: test`     | New or improved tests.                                         |

The feature label is `type: feature` while the commit prefix is `feat`; the [pull request guide](pr-guide.md) carries the full prefix-to-label mapping. Dependabot applies `type: dep` automatically to the pull requests it opens.

### Subsystem Labels

Subsystem labels describe where a change lives. A change that crosses crates may carry more than one. Documentation and governance issues carry no subsystem label.

| Label                          | Focus                                              |
| ------------------------------ | -------------------------------------------------- |
| `subsystem: audio`             | Audio decoding and playback.                       |
| `subsystem: cli`               | The `pite` command-line interface.                 |
| `subsystem: editor`            | The `egui` editor.                                 |
| `subsystem: export`            | Export packaging and bundled Python.               |
| `subsystem: manifest & assets` | Project structure, manifests, and asset importing. |
| `subsystem: physics`           | The 2D physics subsystem.                          |
| `subsystem: render`            | The `wgpu` 2D renderer.                            |
| `subsystem: runtime`           | Windowing, main loop, and hot reload.              |
| `subsystem: scene`             | Scene tree, scene format, and the node tree.       |
| `subsystem: scripting`         | Embedded Python 3.12 scripting.                    |

## Review and Merge

- Outside contributors need one maintainer approval and all required checks. Maintainers may self-merge their own pull requests once the checks are green.
- **Rebase and merge** is the only merge strategy. Merge commits are rejected so history stays linear.
- Because rebase merge preserves the author's commits, ask for fixups to be squashed and subjects reworded before merging.
- Ask for unrelated changes to be split into separate pull requests rather than reviewing a mixed branch.
- If a pull request goes quiet, ping once. Close it after a reasonable wait and note that the author can reopen it.
- Before merging, confirm the type and subsystem labels, the milestone when one applies, and that the changelog has an `Unreleased` entry for user-visible changes.

## Releases

Pite releases are cut ad hoc, when `main` is release-worthy. There is no fixed cadence, and only a maintainer tags a release.

- **Milestone tags** (`v0.1-mN`) close a milestone. They are non-publishing tags and never ship binaries, crates, or stubs.
- **Release tags** (`v*.*.*`) must match the workspace version exactly. A tag push starts the release builds for Linux, Windows, and macOS, publishes every crate to crates.io, and publishes the `pite-stubs` wheel to PyPI.
- `publish-crates.yml` refuses a tag that does not match `Cargo.toml`, and `publish-pypi.yml` refuses a stub version that drifts from the workspace.
- MSRV proof runs every Monday at 06:00 UTC and on demand; a failure means the claimed `rust-version` in `Cargo.toml` is wrong.

### Release Checklist

The release process requires coordinated version bumps across internal crates and documentation before tagging:

- `Cargo.toml`: Bump `version` across all 13 internal `pite-*` path-dependency pins (workspace crates publish in lockstep).
- `docs/changelog.md`: Move unreleased entries under the new version header with a release date.
- MSRV proof: Ensure `msrv-proof.yml` passes on the pinned workspace `rust-version`.
- Python bundle pins: When updating the embedded CPython distribution, update `python_bundle.rs` (`PINNED_PYTHON`, `PINNED_BUILD`, filenames, and SHA-256 checksums).
- Pyright language server: When upgrading pyright, synchronize the version pin in `crates/pite-editor/src/lsp.rs`, `ci.yml`, and `AGENTS.md`.
- PyPI stubs: `python/pyproject.toml` carries the same `version` as the workspace (CI and the tag publish check enforce it).
- Scene format changes: If the `.ptscn` schema evolves, update `FORMAT_VERSION`, `CACHE_VERSION`, the template literals, and `docs/reference/scene-format.md`.
- Release tagging: Strict `v*.*.*` tags must match the workspace version; milestone tags (`v0.1-mN`) are non-publishing tags.

After the workflows finish, confirm the crates and the stub wheel are visible on their registries, check that the documentation deployment succeeded, and announce the release with its changelog heading.

### PyPI Stub Package

`python/` builds the `pite-stubs` distribution: a stub-only wheel for editors outside the built-in one. It installs a `pite` package (`__init__.pyi`, `py.typed`) so the import name matches the engine module; only the PyPI distribution name carries the `-stubs` suffix (PyPI rejects `pite` as too similar to existing projects). The `__init__.pyi` is generated, never committed:

```sh
PITE_STUB_OUT="$PWD/python/pite/__init__.pyi" cargo build -p pite-editor
python -m build --outdir dist python
```

Publishing runs in `publish-pypi.yml` through Trusted Publishing (no token): tag pushes publish to PyPI, manual dispatches default to TestPyPI. Before the first real release, register the workflow and the `pypi` environment as a trusted publisher on PyPI (and `testpypi` on TestPyPI).

## Security and Conduct

- Vulnerability reports follow the [Security Policy](../security.md): private advisories only, acknowledgement within 48 hours, and coordinated disclosure after the patch ships.
- Conduct reports follow the [Code of Conduct](../code-of-conduct.md). A maintainer named in a report recuses from handling it.
- Accessibility barriers follow the [Accessibility Statement](../accessibility.md) and are labeled `type: a11y`.
