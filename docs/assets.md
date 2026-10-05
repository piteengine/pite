# Assets (`res://`, uids, reimport)

Asset references are plain `res://` strings: `texture = "res://assets/foe.png"` in
node props, `pite.play("res://sfx/hit.wav")` in scripts. Paths stay human-readable;
stability across renames comes from a uid sidecar, not from the reference syntax.

## Manifest (`assets.pite.toml`)

`pite reimport` scans the project root (png + wav only, hidden dirs / `target/` /
`dist/` skipped) and writes `assets.pite.toml`: `[[asset]]` entries of
`{ path, uid, hash, size }`, sorted by path. The uid is opaque and assigned once;
the content hash lets a rename be recognized when the path changes. The manifest
is a local import cache — gitignored, never exported.

## `pite check`

Every `res://` string in node props (main scene + instanced scenes) and every
`play("…")` literal must resolve to a file, else `check` errors:

- `references missing asset "res://…"` — nothing on disk, no uid match.
- `references moved asset "res://…" (now at "res://…"); run 'pite reimport'` —
  the path is gone but the uid (content hash) was found elsewhere.
- `asset "res://…" is never referenced by a reachable scene` — the file is
  scanned but nothing reachable uses it (warning only, never an error).
  Reachable means: node props and instance overrides of scenes transitively
  instantiated from the main scene, `play("…")` literals in their attached
  scripts, atlas sheets via their sidecar, and the manifest icon. Assets used
  only by uninstantiated scenes or loose scripts stay silent by design.

## `pite reimport`

Re-runs the matching importer over added/changed files (png: magic bytes, wav:
`RIFF…WAVE`), updates the manifest, and rewrites quote-wrapped `res://` refs in
`.pitescene` / `.py` files when a uid moved. Single files that fail import are
reported, not fatal to the rest. `check` itself never writes.

## Importers (`pite-assets`)

`Importer` trait (`match_ext`, `import`) + `UidRegistry`.
`png` and `wav` are registered; new formats are new impls, no registry change.

## Atlas (sprite sheets)

One PNG plus a JSON sidecar describes a sprite sheet. The sidecar sits beside
the sheet as `<sheet>.atlas.json`:

```json
{
  "texture": "sheet.png",
  "size": [64, 32],
  "frames": {
    "player": { "x": 0, "y": 0, "w": 32, "h": 32 },
    "enemy": { "x": 32, "y": 0, "w": 32, "h": 32 }
  }
}
```

`size` is the sheet in pixels; a frame is `{x, y, w, h}`.
Frames must fit the sheet, and a sheet with no frames is an error.

A `Sprite2D` draws either a plain texture or one atlas frame:

```toml
[node.props]
atlas = "res://assets/sheet.atlas.json"
frame = "player"
```

- `texture` and `atlas` are mutually exclusive; setting both fails loudly
  (`check` error, runtime refuses the frame).
- `atlas` requires `frame`, and `frame` without `atlas` is an error.
- Every frame of one sheet shares a single texture, so all of them are drawn
  in one batched draw call.
- `pite check` errors on a frame the atlas does not define (listing the names
  it does have) and warns about frames nothing references.
- From Python: `pite.frames("res://assets/sheet.atlas.json")` lists the names,
  and `Sprite2D.atlas` / `Sprite2D.frame` are readable and writable.
