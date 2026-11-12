## Summary

Provide a concise summary of the purpose, changes, and architectural rationale of this pull request.

## Related Issues

Closes #
Fixes #

## Type of Change

- [ ] `feat`: A new feature or engine capability
- [ ] `fix`: A bug fix or patch
- [ ] `docs`: Documentation updates or additions
- [ ] `style`: Formatting, whitespace, or non-functional styling
- [ ] `refactor`: Internal code refactoring with no behavior change
- [ ] `perf`: Performance improvement
- [ ] `test`: New tests or test fixture improvements
- [ ] `chore`: Maintenance, CI, dependencies, or internal tooling

## Subsystems Affected

- [ ] Editor (`pite-editor`)
- [ ] Core & Scene Tree (`pite-core`, `pite-scene`)
- [ ] Scripting & Python Bridge (`pite-script`)
- [ ] Physics (`pite-physics`)
- [ ] Render (`pite-render`)
- [ ] Audio (`pite-audio`)
- [ ] Assets & Manifest (`pite-assets`, `pite-project`)
- [ ] Export & Player (`pite-export`, `pite-player`, `pite-cli`)
- [ ] Documentation & Governance

## Verification and Testing

Commands executed locally:

```sh
cargo fmt --all -- --check
cargo build --workspace --locked
cargo test --workspace --locked
bash scripts/check-examples.sh
```

- [ ] All tests pass locally
- [ ] Formatting checks pass with zero warnings
- [ ] Clean build with no compiler warnings

## Quality and Safety Checklist

- [ ] Commits adhere to Conventional Commits specification
- [ ] Zero credentials, secrets, or platform tokens committed
- [ ] No hardcoded local paths or generated cache files staged
- [ ] Any `unsafe` block includes an explicit `// SAFETY:` rationale
- [ ] Documentation updated if public API, CLI flags, or scene format changed
- [ ] Accessibility impact considered for UI, CLI, or documentation changes
