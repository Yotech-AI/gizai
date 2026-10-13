# Gizai design system: where things live

- The design system (brand book, tokens, components, icons) is published at
  https://claude.ai/artifact/8fVNzRghAU24wMaq5ZcdtD (private to Jeffrey until shared).
- `design/tokens.json` is its token file. `node scripts/tokens-css.ts` turns it into `src/styles/tokens.css`.
- `src/styles/components.css` is its component stylesheet (`components/bundle.css` there); keep the two equal.
- `design/README.md` is its brand book; `design/gen-design-system.mjs` generated its previews, icons and cover.
- Icons are Lucide (`lucide-react`); status glyphs are `src/components/StatusIcon.tsx` (same paths as the generator).
- `design/code-editor/` holds the Code Editor's designs, as pages that open from disk on the app's own stylesheets.
  `editor.html` is the Editor (GA-105), built by `node design/code-editor/gen-editor.mjs`; its new components are in
  `editor.css` and its new tokens in `editor-tokens.css`, until F2 moves them into `components.css` and `tokens.json`.
- `design/branding/` holds Gizai's own artwork: the wireframe mountains along the bottom of the window (GA-139), drawn
  from a fixed seed by `node design/branding/gen-bottom-band.mjs` as `mountains.svg` (the app) and `mountains-hero.svg`
  (the website), with their page `bottom-band.html`, `band.css` and the `band-*` tokens in `new-tokens.json`.
