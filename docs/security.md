---
icon: lucide/shield-check
---

# Security Policy

Security and memory safety are foundational requirements of the Pite engine architecture and its embedded Python runtime.

## Supported Versions

Pite provides active security patches for the latest development branch and the latest published minor release series. Older releases are not actively maintained.

| Branch or Release Series | Security Support Status |
|---|---|
| Latest `main` branch | Supported |
| Latest minor release series | Supported |
| Earlier minor and patch releases | Not supported |

## Reporting a Vulnerability

If you discover a security vulnerability, memory corruption defect, or potential privilege escalation vector within Pite:

- Do not open a public GitHub issue or disclose the vulnerability in public forums.
- Submit reports through GitHub Private Vulnerability Reporting at [GitHub Security Advisories](https://github.com/piteengine/pite/security/advisories/new).
- Alternatively, email the maintainer directly at [khalili1388mahan@gmail.com](mailto:khalili1388mahan@gmail.com) with the subject line `[SECURITY] Pite Engine Vulnerability Report`.

Please include in your report:

- A description of the vulnerability and its potential impact.
- Affected engine crates, CLI tools, or editor versions.
- A minimal reproducible example, such as a proof of concept `.ptscn` scene or Python script.
- Any suggested remediation or patch if available.

## Disclosure Timeline and SLA

- Maintainers will acknowledge receipt of your vulnerability report within 48 hours.
- We will coordinate patch verification, impact assessment, and CVE assignment through GitHub Private Vulnerability Reporting.
- Coordinated public disclosure will take place following release of the patch across crates.io and engine release binaries.

## Security Architecture and Invariants

All engine contributions and AI agents working in this repository must uphold strict security invariants:

### Memory Safety and Unsafe Code

- All `unsafe` blocks in Rust must include an explicit `// SAFETY:` rationale documenting caller preconditions, pointer alignment, and memory bounds.
- Use checked arithmetic or saturation operations when performing spatial transforms and buffer indexing on external or untrusted scene data.
- Enforce strict validation on scene geometry and binary cache deserialization before dispatching buffers to `wgpu`.

### Embedded Python Scripting Boundaries

- Python scripts executed within the engine runtime are scoped to game logic and standard library primitives.
- Toolchain packaging and export bundling verify CPython distribution integrity using SHA-256 checksums pinned in `python_bundle.rs`.
- Third-party packages required by games must be vendored and pinned in `pite.toml` via `pite vendor add` to avoid untrusted arbitrary package resolution.

### Dependency Supply Chain Auditing

- Workspace lockfile integrity is verified with deterministic `Cargo.lock` builds.
- Dependencies are regularly audited against Rust security advisories using `cargo audit`.
- Vendored Python dependencies are scanned for known CVEs.

### Zero Secret Exposure

- Tokens, private keys, API secrets, and credentials must never be committed to source control or logged in engine trace output.
- Automated static checks reject secrets before commits are integrated.
