---
icon: lucide/accessibility
---

# Accessibility Statement

Pite is dedicated to providing an accessible, usable, and welcoming experience for all game developers, contributors, and players. We believe creative game development tools should empower people with diverse physical, sensory, cognitive, and communicative abilities.

This statement outlines our current accessibility baseline, supported interfaces, known limitations, and our feedback and contribution process, adhering to the guidance in [Open Source Guides: Accessibility Best Practices for Your Project](https://opensource.guide/accessibility-best-practices-for-your-project/#start-with-an-accessibility-statement).

## Commitment and Vision

Digital accessibility is an ongoing commitment rather than a static checkbox. We recognize that game engines present unique accessibility challenges—blending visual spatial viewports, code editing, immediate-mode GUI controls, audio management, and real-time simulations.

We strive to meet and exceed Web Content Accessibility Guidelines version 2.1 Level AA criteria for our digital documentation and web properties, while actively improving assistive technology integration across the native desktop editor and command-line interfaces.

## Supported Surfaces and Standards

Our accessibility efforts span four primary interaction surfaces:

### Visual Editor

The Pite editor is built with Rust using `eframe` and `egui`. Accessibility capabilities include:

- Keyboard Navigation: All core panels, inspector fields, file pickers, and scene hierarchy trees support keyboard focus traversal and execution.
- High Contrast Theme: The editor interface utilizes a high-contrast dark palette designed around Pite Teal (`#0b9387`) and slate backgrounds, ensuring text and interactive elements satisfy contrast ratio thresholds of 4.5 to 1 for standard text.
- Scalable Display: The interface respects system DPI scaling and custom zoom controls, ensuring UI elements and text remain sharp and legible across high-density displays.
- Visual Motion Consideration: Animations and transitions in the editor can be minimized to prevent discomfort for users with vestibular sensitivities.

### Command Line Interface

The `pite` CLI tool is designed for clarity and predictable text stream output:

- Color Accessibility: CLI output uses standard ANSI colors that adhere to user terminal color palettes.
- Scripting and Non-Interactive Modes: The CLI detects non-TTY environments and respects flags such as `--json` and standard environment variables like `NO_COLOR`, stripping ANSI escape sequences for text-to-speech tools and automated pipelines.
- Structured Error Messages: Diagnostics adhere to an actionable format stating what failed, why it failed, and how to resolve it.

### Documentation Site

The official Pite documentation hosted at [piteengine.github.io/pite](https://piteengine.github.io/pite) aims for full conformance with WCAG 2.1 Level AA:

- Semantic Document Structure: Pages use logical heading hierarchies without skipping levels.
- Alternative Text: Architectural diagrams and interface screenshots provide informative descriptive alt text.
- Keyboard Accessible Navigation: Menus, tabbed code snippets, search inputs, and page tables of contents are operable via standard keyboard controls.
- Color Contrast and Theme Toggles: The documentation site supports both dark and light modes with contrast-validated tokens.

### Player Runtime

Games produced with Pite are powered by a lean 2D runtime:

- Rebindable Input: The runtime supports abstract action mappings, enabling game developers to build rebindable key, mouse, and gamepad controls for players with limited motor control.
- Audio Architecture: The audio subsystem supports spatial and non-spatial sound playback with independent bus volume controls, aiding hearing-impaired players and subtitle synchronizations.

## Known Barriers and Workarounds

Transparency regarding existing technical gaps is essential. We openly document current accessibility barriers across the engine alongside recommended workarounds:

### Immediate Mode GUI Screen Reader Integration

- Barrier: The visual editor runs on immediate-mode rendering via `egui`. While initial screen-reader support via AccessKit is actively maturing across the Rust ecosystem, rich semantic screen-reader accessibility for complex custom widgets (such as the 2D scene viewport canvas and node visual wiring) is still developing.
- Workaround: Pite scene files (`.ptscn`) are human-readable, byte-stable TOML documents. Developers who rely on screen readers can inspect, author, and modify node trees, properties, and script connections directly using accessible text editors such as VS Code with NVDA, JAWS, or Orca, then validate the scene with `pite check`.

### Spatial Viewport 2D Manipulation

- Barrier: Placing and transforming visual sprites within the 2D canvas currently relies primarily on mouse drag and transform gizmos.
- Workaround: Exact numeric positioning, scaling, and rotation can be keyed in directly via the Inspector panel using keyboard inputs, avoiding reliance on fine-motor mouse manipulation.

## Reporting Accessibility Barriers

We welcome feedback, bug reports, and suggestions regarding accessibility barriers from users and contributors of all abilities.

If you encounter an accessibility hurdle:

- File an issue on GitHub using our [Accessibility Barrier Report form](https://github.com/piteengine/pite/issues/new?template=accessibility.yml). Issues will be automatically tagged with the `area/accessibility` label for maintainer triage.
- Email the maintainer directly at [khalili1388mahan@gmail.com](mailto:khalili1388mahan@gmail.com) with the subject `[ACCESSIBILITY] Pite Accessibility Feedback`.

Please include the assistive technology used (e.g. NVDA version, VoiceOver, high-contrast mode, keyboard only) and the specific interface surface where the obstruction occurred. Maintainers will review and respond within 48 hours.

## Inclusive Contribution and Development Practices

Pite adheres to the philosophy of centering people with lived experiences:

- We encourage contributors with disabilities to review interface designs, documentation pull requests, and new editor features.
- Every pull request touching user-facing interfaces or documentation includes an accessibility impact check in our pull request template.
- For behavioral and community conduct expectations, see our [Code of Conduct](https://github.com/piteengine/pite/blob/main/CODE_OF_CONDUCT.md).
