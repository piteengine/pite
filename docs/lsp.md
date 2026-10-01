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
- Completion via `Ctrl+Space` (click a candidate to insert it).
- Hover via `Ctrl+H` (logged to the console).
- Signature help automatically after typing `(` (shown above the code pane).
- `publishDiagnostics` merged into the existing error gutter as `[lsp] …`
  lines, next to the runtime traceback marks.

The shipped `python/pite/pite.pyi` stubs are put on the server's
`extraPaths` (via `didChangeConfiguration` plus `workspace/configuration`
answers), so `import pite` resolves with the full engine API. No DAP, no
rename support — out of scope for this slice.
