# LSP (code intelligence)

The editor spawns an external Python language server over stdio — never
in-process, never bundled. A missing server logs a loud console message and
editing continues without it.

## Prerequisite

```sh
pip install "pyright==1.1.411"
```

This ships `pyright-langserver --stdio`, the default command. Point
`PITE_LSP_CMD` at another pyright-family binary to override it. Servers older
than the pin warn in the console but still serve.

## What's provided

- `didOpen` when a script opens, `didChange` on every edit.
- The project root (the `pite.toml` directory) is sent as the workspace
  folder, so the server analyzes real files instead of reporting
  `No source files found`.
- Completion pops up as you type and refreshes on every keystroke;
  `Ctrl+Space` forces it anywhere. The popup anchors at the cursor, narrows
  on what you typed, follows the selection as you arrow, and accepts with
  `Enter` (`Esc` dismisses, click works too).
- Hover via `Ctrl+H` (logged to the console).
- Signature help automatically after typing `(` (shown above the code pane).
- `publishDiagnostics` merged into the existing error gutter as `[lsp] …`
  lines, next to the runtime traceback marks.
- `workspace/configuration` pulls are answered with the requested section
  value (`result[0]` pairs with `items[0]`): for section `"python"` that is
  the `{"analysis": …}` object itself. A wrapped `{"python": …}` answer reads
  as a section with no `analysis` and is silently dropped — which once
  starved `import pite` of its stubs with no error anywhere.

The `pite.pyi` stubs are **generated at build time** by `pite-editor/build.rs`
from `pite_core::NODE_SCHEMAS`, so adding a node type automatically adds its
stub class. The generated stub plus its `pite.py` source sibling are baked
into the editor binary via `include_str!` and materialized to a versioned
scratch dir on first use; that dir goes on the server's `extraPaths` (via
`didChangeConfiguration` plus `workspace/configuration` answers), so
`import pite` resolves with the full engine API everywhere — installed
bundles and source checkouts alike. There is no committed `.pyi`; the build
output is the single source of truth, and `pite-editor`'s
`generated_stub_covers_pyclass_surface` test fails if a schema type is
missing a stub class. `pite.py` ships beside `pite.pyi` as the source
sibling the checker needs (the real module is native via PyO3) — no
diagnostic rules are disabled. No DAP, no rename support — out of scope for
this slice.
