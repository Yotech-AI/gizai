// Compiles design/tokens.json (the Gizai design system's tokens) to CSS custom properties, the same way the
// design system page does: the first theme on :root, later themes under [data-theme], aliases as var().
type Val = string | Record<string, string>;
type Tok = { name: string; value: Val; usage?: string };
type Style = { name: string; fontSize: string; lineHeight?: string | number; fontWeight?: number | string; letterSpacing?: string; family?: string };
export type Tokens = {
  color: { themes: { id: string }[]; tokens: Tok[] };
  type: { families: Record<string, string>; groups: { family: string; styles: Style[] }[] };
  [family: string]: unknown;
};

const ref = (v: string) => (/^\{.+\}$/.test(v) ? `var(--${v.slice(1, -1)})` : v);
const isTheme = (v: Val): v is Record<string, string> => typeof v === "object";

export function tokensCss(t: Tokens): string {
  const themes = t.color.themes.map((x) => x.id);
  const [first, ...rest] = themes;
  const themed: Tok[] = [...t.color.tokens];
  const plain: Tok[] = [];
  for (const [key, fam] of Object.entries(t)) {
    if (key === "color" || key === "type" || key === "name" || key === "version" || key === "meta") continue;
    for (const tok of (fam as { tokens?: Tok[] }).tokens ?? []) (isTheme(tok.value) ? themed : plain).push(tok);
  }
  const block = (sel: string, scheme: string, lines: string[]) => `${sel} {\n  color-scheme: ${scheme};\n${lines.map((l) => `  ${l}\n`).join("")}}\n`;
  const firstLines = themed.map((tok) => `--${tok.name}: ${ref(isTheme(tok.value) ? tok.value[first] : tok.value)};`);
  let css = "/* Generated from design/tokens.json by scripts/tokens-css.ts. Do not edit by hand. */\n";
  css += block(`:root, [data-theme="${first}"]`, first, firstLines);
  for (const th of rest) {
    css += block(`[data-theme="${th}"]`, th, themed.filter((tok) => isTheme(tok.value) && tok.value[th]).map((tok) => `--${tok.name}: ${ref((tok.value as Record<string, string>)[th])};`));
  }
  const fams = Object.entries(t.type.families).map(([k, v]) => `--font-${k}: ${v};`);
  css += `:root {\n${[...plain.map((tok) => `--${tok.name}: ${tok.value};`), ...fams].map((l) => `  ${l}\n`).join("")}}\n`;
  for (const g of t.type.groups) {
    for (const s of g.styles) {
      const parts = [`font-family: var(--font-${s.family ?? g.family});`, `font-size: ${s.fontSize};`];
      if (s.lineHeight !== undefined) parts.push(`line-height: ${s.lineHeight};`);
      if (s.fontWeight !== undefined) parts.push(`font-weight: ${s.fontWeight};`);
      if (s.letterSpacing) parts.push(`letter-spacing: ${s.letterSpacing};`);
      css += `.${s.name} { ${parts.join(" ")} }\n`;
    }
  }
  return css;
}
