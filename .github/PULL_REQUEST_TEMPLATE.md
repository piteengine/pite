## Summary

Provide a concise summary of the purpose, changes, and architectural rationale of this pull request. Please read comments (texts inside `<!-- -->`) and follow instructions; This will help us merge your request as soon as possible. Replace this paragraph with requested information.

## Related Issues

<!--
    Mention related issues in following style (replace X with issue number) to link this PR to them.
    Also linking related PRs is possible. If your change hasn't any related issue, create an issue first or describe problem clearly in the "Summary" section.
-->

- Closes #X
- Fixes #X

## Type and Scope

<!--
    Contributors: Select item that applies in the Type and Scope sections below. A maintainer will confirm or correct them and remove this section after applying appropriate labels.
-->

### Type of Change

<!--

-->

- [ ] `feat`: A new feature or engine capability
- [ ] `fix`: A bug fix or patch
- [ ] `docs`: Documentation fix or improvement
- [ ] `style`: Formatting, whitespace, or non-functional styling
- [ ] `refactor`: Internal code refactoring with no behavior change
- [ ] `perf`: Performance improvement
- [ ] `test`: New tests or test fixture improvements
- [ ] `chore`: Maintenance, CI, or internal tooling
- [ ] `dep`: Dependency updates / issues (usually by Dependabot)
- [ ] `a11y`: Accessibility improvement

### Subsystems Affected

- [ ] Editor
- [ ] Command-line interface (CLI)
- [ ] Scene Tree
- [ ] Scripting & Python Bridge
- [ ] Physics
- [ ] Render
- [ ] Audio
- [ ] Assets & Manifest
- [ ] Export & Player Bundle
- [ ] Engine Runtime
- [ ] Documentation & Governance

## Verification and Testing

Execute these command locally, then fill checklist below:

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

Fill checklist before mark PR as ready:

- [ ] Commits adhere to Conventional Commits specification
- [ ] Zero credentials, secrets, or platform tokens committed
- [ ] No hardcoded local paths or generated cache files staged
- [ ] Any `unsafe` block includes an explicit `// SAFETY:` rationale
- [ ] Documentation updated if public API, CLI flags, or scene format changed
- [ ] Accessibility impact considered for UI, CLI, or documentation changes
