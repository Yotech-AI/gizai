# Gizai design system: where things live

- The design system (brand book, tokens, components, icons) is published at
  https://claude.ai/artifact/8fVNzRghAU24wMaq5ZcdtD (private to Jeffrey until shared).
- `design/tokens.json` is its token file. `node scripts/tokens-css.ts` turns it into `src/styles/tokens.css`.
- `src/styles/components.css` is its component stylesheet (`components/bundle.css` there); keep the two equal.
- `design/README.md` is its brand book; `design/gen-design-system.mjs` generated its previews, icons and cover.
- Icons are Lucide (`lucide-react`); status glyphs are `src/components/StatusIcon.tsx` (same paths as the generator).
