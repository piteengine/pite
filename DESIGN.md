# Pite Design System

Brand identity, color tokens, logo usage, and UI theming for the Pite game engine
and its editor. This is the single source of truth for visual decisions.

## 1. Logo

Canonical mark: [`assets/logo.svg`](assets/logo.svg) — a hexagonal "P" monogram on a
512×512 grid. Single path, `fill-rule="evenodd"`, no strokes, no gradients. It
recolors cleanly via the `fill` attribute, which is how all variants are derived.

| File | Fill | Use |
|------|------|-----|
| `assets/logo.svg` | `#111111` | Neutral / print / single-color contexts |
| `assets/logo-light.svg` | `#0D9488` (Primary) | Light backgrounds: README, docs, website |
| `assets/logo-dark.svg` | `#5EEAD4` (Light) | Dark backgrounds: editor header, splash, dark docs |
| `assets/logo-white.svg` | `#FFFFFF` | Single-color light mark for dark/photographic backgrounds |
| `assets/logo-profile.svg` | `#5EEAD4` on `#042F2E` | Backgrounded variant (rounded square): profiles, avatars, social |
| `assets/tray-icon.svg` | `#5EEAD4` on `#042F2E` | Duplicate of the profile variant, sized for system-tray use |
| `assets/tray-icon-16/22/24/32.png` | — | Pre-rendered tray sizes (RGBA PNG) |
| `assets/icon.png` | — | 256 px render of the profile variant; editor window icon (see §6.1) |
| `assets/logo-showcase.svg` | various | Color exploration sheet — not for production use |

### Usage rules

- **Do not** stretch, outline, rotate, add shadows, or place on low-contrast
  backgrounds. Minimum legible size: 16 px.
- **Light background →** `logo-light.svg` (`#0D9488`).
  **Dark background →** `logo-dark.svg` (`#5EEAD4`). Never use `#0D9488` on
  `#042F2E` — contrast is too low; that pairing is reserved for fills/borders.
- The mark is monochromatic by design. For tinted contexts (e.g. error states),
  any showcase color may be used at full opacity only.
- egui cannot render SVG natively. In-app surfaces (window icon, editor header)
  need PNG renditions — export from the SVG at 16 / 32 / 64 / 128 / 256 px.
  Window-icon wiring is still open (see §6).

## 2. Core palette — Pite Teal

| Name | Hex | RGB | Role |
|------|-----|-----|------|
| Primary | `#0D9488` | 13, 148, 136 | Brand identity. Links, selection, active states, logo on light |
| Light | `#5EEAD4` | 94, 234, 212 | Inverted variant. Logo on dark, highlights on dark |
| Dark | `#134E4A` | 19, 78, 74 | Deep tone. Panels on dark backgrounds |
| Background | `#042F2E` | 4, 47, 46 | Dark theme background |
| Pale | `#CCFBF1` | 204, 251, 241 | Soft / subtle tone. Secondary text on dark |

## 3. Theme tokens

### Light theme

| Token | Hex | Used for |
|-------|-----|----------|
| `background` | `#FFFFFF` | App / page background |
| `logo` | `#0D9488` | Logo mark (`assets/logo-light.svg`) |
| `accent` | `#5EEAD4` | Accent fills, hovers, highlights |
| `text` | `#111827` | Primary text |

### Dark theme

| Token | Hex | Used for |
|-------|-----|----------|
| `background` | `#042F2E` | App / page background |
| `logo` | `#5EEAD4` | Logo mark (`assets/logo-dark.svg`) |
| `accent` | `#0D9488` | Accent fills, selection, borders |
| `text` | `#F0FDFA` | Primary text |

## 4. Editor mapping (`pite-editor/src/theme.rs`)

The editor is dark-theme-only today. Surfaces are neutral near-black; the teal
`ACCENT` is a signal color only (selection stroke, hyperlinks, active widget
stroke — never full fills):

| Constant | Value | Used for |
|----------|-------|----------|
| `ACCENT` | `#0D9488` (13, 148, 136) | Selection stroke, hyperlinks, active widget stroke |
| `BG` | `#0B0E11` (11, 14, 17) | Window, code-box (`extreme_bg_color` feeds `TextEdit`), viewport fallback backdrop |
| `PANEL` | `#14181D` (20, 24, 29) | Panels — a subtle lift above `BG` so structure reads without color |
| `PANEL_STROKE` | `#2A3138` (42, 49, 56) | Neutral gray widget/panel strokes |
| `TEXT` | `#E6E9EC` (230, 233, 236) | Primary text |
| `FAINT` | `#9AA3AD` (154, 163, 173) | Weak/secondary text |

Selection fill is `ACCENT` at alpha 50 (`from_rgba_premultiplied(13, 148, 136, 50)`).
Widget state fills: noninteractive `PANEL`, inactive/open `#171C22` (23, 28, 34),
hovered `#1F262E` (31, 38, 46), active `#1E3A35` (30, 58, 53, a dark teal tint).
Corner radii stay 8 (window) / 6 (widgets, menu). The viewport fallback painter
(`app.rs`, GPU-unavailable path only) fills with `theme::BG` and draws muted
overlays: `#6FB3A7` (111, 179, 167) sprites, `#7C9BB8` (124, 155, 184) other
nodes, `#C9D1D8` (201, 209, 216) captions.

Rationale: accent-as-signal, surfaces carry structure. The previous theme washed
every panel and widget stroke in full-strength teal, so nothing could stand out
and long sessions felt loud. Neutrals now do the quiet work — `BG`/`PANEL`
separation plus a gray stroke delineate panels, and brightness steps in the
widget fills communicate hover/press. Teal appears only where the user must look:
what is selected, what is a link, what is actively pressed. The viewport
follows the same rule: the textured game frame is the content, and its overlay
glyphs drop from neon to muted slate so captions annotate instead of shouting.

## 5. Typography

- **UI / proportional:** Inter (`fonts/Inter-Regular.ttf`), 15 px body/buttons,
  20 px headings, 12 px small — embedded via `include_bytes!("../../fonts/…")`.
- **Code / monospace:** JetBrains Mono (`fonts/JetBrainsMono-Regular.ttf`), 14 px.
- Spacing: item 8×6, button padding 10×5, indent 20, scrollbar width 10,
  corner radii 8 (window) / 6 (widgets, menu).

## 6. Open branding work

1. **Window icons.** Editor: wired — `pite-editor/src/app.rs` sets
   `ViewportBuilder::with_icon` from `assets/icon.png` (256 px render of the
   profile variant) via `eframe::icon_data::from_png_bytes`. This replaces the
   default egui "e" fallback icon in taskbar / alt-tab / tray.
   Game window: wired — `pite-runtime/src/lib.rs` sets
   `.with_window_icon(...)` from the same `assets/icon.png`, decoded at startup
   via the workspace `image` crate (`window_icon()` helper; icon is optional —
   a corrupt PNG logs nothing and the window still opens).
   System tray: no tray integration exists yet — `assets/tray-icon.svg` +
   `tray-icon-{16,22,24,32}.png` are ready for whenever a tray crate is wired.
2. **Editor header wordmark.** `TopBottomPanel::top("transport")` in `app.rs` has no
   logo — render `logo-dark.svg` (as PNG texture via `egui::Image`) next to the
   transport controls.
3. **Renderer clear color.** `pite-render` clears to `(0.07, 0.07, 0.10)`; align the
   launch/splash backdrop with `#042F2E` → `(0.016, 0.184, 0.180)`.
4. **Light-theme editor.** Tokens are specified (§3) but only the dark theme is
   implemented.
