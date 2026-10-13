# GizAi brand (GA-52)

GizAi's mark, logo, favicon and app icon as SVG files, made by `gen-brand.mjs`. The brandbook itself is in Figma: the
file "Giz-AI", page **Brandbook**
([open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=5-2)). Its Logo, App icon and Favicon
components were imported from these files, so the two are the same drawings. The homepage (GA-144) is designed from it;
the app takes the icon, name and colours over in a card of its own.

Run `node design/brand/gen-brand.mjs` to write the files again. If the drawings change, import them into the Figma file's
Components frame again.

## The files

| File | What it is |
|---|---|
| `logo-horizontal-*.svg` | The mark and the wordmark side by side, 358.58 × 90. |
| `logo-stacked-*.svg` | The mark over the wordmark, 241.58 × 248.2. |
| `mark-*.svg` | The mark alone, 54 × 54. |
| `favicon.svg` | The SVG favicon: the 32 px drawing, which browsers scale. |
| `favicon-16.svg`, `-32.svg`, `-48.svg` | The favicon drawn on the pixel grid, for the `.ico` and the PNGs. |
| `app-icon-1024.svg` | The app icon's master for Linux and Windows: an 896 px tile in the 1024 px canvas. Scale it down for 512, 256 and 128 px. |
| `app-icon-macos-1024.svg` | macOS: Apple's 824 px rounded square in the 1024 px canvas, with its shadow. |
| `app-icon-16.svg`, `-32.svg` | The app icon at 16 and 32 px, drawn on the pixel grid (the favicon's drawings). |

Each logo comes in four colours: `-on-dark` (#f472b6), `-on-light` (#c0287a), `-white` and `-black`. The backgrounds are
transparent; "on dark" and "on light" say where each pink goes.

## What the design decides

- **The mark** is the board's Review glyph, a ring with a dot, drawn as a G: the ring opens at the top right and a bar
  carries it into the dot. Ring and bar are a third of the radius thick, the dot is 0.34 of the radius (the glyph's own
  proportion), the top terminal is cut along the radius at 48°. One closed outline, no strokes.
- **The wordmark** reads GizAi: Geist Bold, letter spacing −2%, outlined, with round dots on the i's like the mark's dot.
  Never "Gizai", "GIZAI" or "Giz AI".
- **Colour**: the logo is one colour, the Review and Deploy pink (`--needs`): #f472b6 on dark (7.2:1 on `bg`), #c0287a on
  light (5.5:1 on white); white and black for one colour.
- **The pink and "needs you"** share the value and keep apart by place and shape. The brand pink goes on the logo, the
  app icon and the favicon, and on the website at most one more pink thing per screen; never on buttons, links, focus or
  behind text (those are the accent blue). Inside the app's window pink means only "needs you" (Review, Deploy, holds, the
  Inbox count), so the app shows its logo in one colour (`text-2`). The status glyph is a thin, closed 16 px ring beside
  its word; the mark is a heavy, open G and never stands in a list or on a card.
- **Clear space**: x on every side, x being half the mark's height. **Smallest**: horizontal 20 px high (80 px wide),
  stacked 64 px wide, the mark 24 px; under 24 px use the favicon's drawing on its tile.
- **The icon** is the pink mark at 60% of a dark tile (#1d1f25 to #0e0f12, top to bottom) with a faint inner edge; the
  corners are Figma's 60% corner smoothing. At 16 px the ring is 2 px thick, the dot 5 px and the opening 2 px.
- **Type**: Geist (SemiBold and Bold) for headings and the wordmark, Atkinson Hyperlegible Next for body text, Atkinson
  Hyperlegible Mono for IDs, branches and code. All three ship with the app already. The website's scale (Figma text
  styles `Web/*`, in px): Display 64/68 Bold −3%, H1 48/54 Bold −2.5%, H2 36/42 Bold −2%, H3 24/30 SemiBold −1%, H4 20/26
  SemiBold −0.5%, Eyebrow 13/16 Mono +6% capitals, Lead 19/30, Body 16/26, Body strong 16/26 SemiBold, Small 14/22,
  Caption 12.5/18 Medium, Button 15/20 SemiBold, Code 14/22 Mono.

## In the Figma file

- Variables: **GizAi colour** (modes Dark and Light): `brand/pink`, the app's tokens from `design/tokens.json` (surfaces,
  lines, text, accent, status, status glyphs, label colours) and the band's blue, line and glow; **GizAi size**: radii
  and spacing. Code syntax is the CSS name, such as `var(--needs)`.
- Components (in the Components frame): Logo (Layout × Colour), App icon, Favicon, Button (Primary and Secondary × five
  states), Link, Input, Badge (Working, Needs you, Review, Testing, Done, On hold), Card label (frontend, backend, bug, qa)
  and Mountains (the app's band and the website's hero, line for line from GA-139's `mountains.svg` and
  `mountains-hero.svg`, brought in a part at a time).
- `band-hero-line` and `band-hero-glow` are a proposal: the hero at GA-139's Medium strength; the app keeps Subtle.
