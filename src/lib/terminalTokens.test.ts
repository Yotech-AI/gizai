// GA-135 QA: what the terminal is made of besides its store. TD's 22 term-* colours and 5 sizes are in design/tokens.json
// where design/terminal/new-tokens.json says, tokens.css is made from it, both themes give every colour a value, the
// store uses every term-* colour, the two sizes grow as TD's terminal-tokens.css shows; xterm.js and its addons are pinned
// to research §4's versions; terminal.css brings xterm.js's own stylesheet and the app loads it; and the address
// #/terminal and #/terminal/<id> round-trips.
import { describe, expect, it } from "vitest";
import { tokensCss, type Tokens } from "./tokensCss";
import { THEME_TOKENS } from "./terminals";
import { href, PAGES, parseHash, type Route } from "../router";

// Files are read from disk (Vitest hands .css back empty). node:fs comes in through a computed specifier: the build
// type-checks tests without Node's types.
const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const read = (path: string): string => fs.readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");
type Token = { name: string; value: unknown; usage: string };
const tokensJson = JSON.parse(read("design/tokens.json")) as Tokens & { color: { tokens: Token[] }; size: { tokens: Token[] } };
const td = JSON.parse(read("design/terminal/new-tokens.json")) as { color: { tokens: Token[] }; size: { tokens: Token[] } };
const tokensFile = read("src/styles/tokens.css");

/** `--name: value;` declarations of the first block whose selector is exactly `selector`. */
function block(css: string, selector: string): Map<string, string> {
  const at = css.indexOf(`${selector} {`);
  if (at < 0) throw new Error(`no ${selector} block`);
  const body = css.slice(css.indexOf("{", at) + 1, css.indexOf("}", at)).replace(/\/\*[\s\S]*?\*\//g, "");
  return new Map([...body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1] ?? "", (m[2] ?? "").trim()]));
}
const dark = block(tokensFile, ':root, [data-theme="dark"]');
const light = block(tokensFile, '[data-theme="light"]');
function colour(theme: "dark" | "light", name: string): string {
  const value = (theme === "light" ? light.get(name) : undefined) ?? dark.get(name);
  if (value === undefined) throw new Error(`${name} isn't in tokens.css`);
  const alias = /^var\((--[\w-]+)\)$/.exec(value);
  return alias ? colour(theme, alias[1] ?? "") : value;
}

describe("the term-* tokens (design/terminal/new-tokens.json)", () => {
  it("are TD's 22 colours right after git-deleted and 5 sizes right after size-statusbar, and tokens.css is made from them", () => {
    expect(td.color.tokens.length).toBe(22);
    expect(td.size.tokens.length).toBe(5);
    const colours = tokensJson.color.tokens;
    const at = colours.findIndex((t) => t.name === "git-deleted") + 1;
    expect(colours.slice(at, at + 22)).toEqual(td.color.tokens);
    const sizes = tokensJson.size.tokens;
    const sat = sizes.findIndex((t) => t.name === "size-statusbar") + 1;
    expect(sizes.slice(sat, sat + 5)).toEqual(td.size.tokens);
    expect(tokensFile).toBe(tokensCss(tokensJson));
  });

  it("give every colour a value in dark and in light", () => {
    for (const t of td.color.tokens) {
      for (const theme of ["dark", "light"] as const) expect([t.name, theme, colour(theme, `--${t.name}`)]).toEqual([t.name, theme, expect.stringMatching(/^#[0-9a-f]{6}$/)]);
    }
    expect(colour("dark", "--term-bg")).toBe(colour("dark", "--bg"));
    expect(colour("light", "--term-red")).toBe(colour("light", "--diff-del"));
  });

  it("are all used for xterm.js's theme", () => {
    const used = THEME_TOKENS.map(([, t]) => t).filter((t) => t.startsWith("term-")).sort();
    expect(used).toEqual(td.color.tokens.map((t) => t.name).sort());
  });

  it("grow the terminal's tabs and the panel's least height as TD's terminal-tokens.css does", () => {
    const grown = block(read("src/styles/appearance.css"), ":root");
    const tdCss = block(read("design/terminal/terminal-tokens.css"), ":root");
    for (const k of ["--size-term-tab", "--size-term-panel-min"]) expect([k, grown.get(k)]).toEqual([k, tdCss.get(k)]);
  });
});

describe("xterm.js (research §4)", () => {
  it("is 6.0.0 with its addons, each pinned to the version research §4 names", () => {
    const pkg = JSON.parse(read("package.json")) as { dependencies: Record<string, string> };
    const want = { "@xterm/xterm": "6.0.0", "@xterm/addon-fit": "0.11.0", "@xterm/addon-webgl": "0.19.0", "@xterm/addon-search": "0.16.0",
      "@xterm/addon-web-links": "0.12.0", "@xterm/addon-unicode11": "0.9.0" };
    for (const [name, version] of Object.entries(want)) expect([name, pkg.dependencies[name]]).toEqual([name, version]);
    const lock = JSON.parse(read("package-lock.json")) as { packages: Record<string, { version: string }> };
    for (const [name, version] of Object.entries(want)) expect([name, lock.packages[`node_modules/${name}`]?.version]).toEqual([name, version]);
  });

  it("brings its own stylesheet through terminal.css, which the app loads", () => {
    const css = read("src/styles/terminal.css").replace(/\/\*[\s\S]*?\*\//g, "").trim();
    expect(css.startsWith('@import "@xterm/xterm/css/xterm.css";')).toBe(true);
    expect(read("src/main.tsx")).toContain('import "./styles/terminal.css";');
  });
});

describe("the Terminal page's address (GA-135)", () => {
  it("#/terminal is New terminal, #/terminal/<id> one terminal", () => {
    expect(PAGES).toContain("terminal");
    expect(parseHash("#/terminal")).toEqual({ page: "terminal" });
    expect(parseHash("#/terminal/")).toEqual({ page: "terminal" });
    expect(parseHash("#/terminal/0192ab-cd")).toEqual({ page: "terminal", id: "0192ab-cd" });
    expect(href({ page: "terminal" })).toBe("#/terminal");
    const routes: Route[] = [{ page: "terminal" }, { page: "terminal", id: "0192ab-cd" }, { page: "terminal", id: "a/b #c" }];
    for (const r of routes) expect(parseHash(href(r)), JSON.stringify(r)).toEqual(r);
  });
});
