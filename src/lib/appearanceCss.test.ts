// GA-42: the stylesheet side of Settings → Appearance. appearance.css repeats the design tokens' sizes (tokens.css, made from
// design/tokens.json) with the growth added, so with no size variable on <html> (the defaults) every size must still be the
// token's: these tests fail when one of them drifts. Every growth variable has a fallback that changes nothing, so a size
// can't break while it's unset. And each font choice has its rule, its bundled woff2 files (no font from the internet) and
// its licence in the repo.
import { describe, expect, it } from "vitest";
import runPanel from "../components/RunPanel.tsx?raw";
import editor from "../components/MarkdownEditor.tsx?raw";
import { FONTS } from "./appearance";

// Vitest hands a .css file back empty, even with ?raw, so the stylesheets are read from disk. node:fs comes in through a
// computed specifier: the build type-checks tests without Node's types.
const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const css = (name: string): string => fs.readFileSync(new URL(`../styles/${name}`, import.meta.url), "utf8");
const tokensCss = css("tokens.css");
const appearanceCss = css("appearance.css");
const fontsCss = css("fonts.css");
const componentsCss = css("components.css");
const appCss = css("app.css");

const fontFiles = Object.keys(import.meta.glob("../assets/fonts/*.woff2")).map((p) => p.replace("../assets/fonts/", ""));
const licences = import.meta.glob("../assets/fonts/LICENSE-*", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

/** `--name: value;` declarations of the first block whose selector is exactly `selector`. */
function block(css: string, selector: string): Map<string, string> {
  const at = css.indexOf(`\n${selector} {`) >= 0 ? css.indexOf(`\n${selector} {`) + 1 : css.startsWith(`${selector} {`) ? 0 : -1;
  if (at < 0) throw new Error(`no ${selector} block`);
  const body = css.slice(css.indexOf("{", at) + 1, css.indexOf("}", at));
  const out = new Map<string, string>();
  for (const m of body.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) out.set(m[1] ?? "", (m[2] ?? "").trim());
  return out;
}
/** A size with every growth variable unset: `calc(13.5px + var(--ui-grow, 0px))` is 13.5px. */
function atDefault(value: string): string {
  const m = /^calc\((\d+(?:\.\d+)?px) \+ (?:\d+ \* )?var\(--[\w-]+, 0px\)\)$/.exec(value);
  if (!m) throw new Error(`not a size with a growth: ${value}`);
  return m[1] ?? "";
}
// tokens.css has the sizes on a :root block of their own (after the theme colours)
const tokenSizes = new Map<string, string>();
for (const m of tokensCss.matchAll(/(--(?:fs[\w-]*|size-[\w-]+))\s*:\s*([^;]+);/g)) tokenSizes.set(m[1] ?? "", (m[2] ?? "").trim());
const root = block(appearanceCss, ":root");

describe("the sizes at the defaults", () => {
  it("are the design tokens' sizes, for every token appearance.css grows", () => {
    const grown = [...root.keys()].filter((k) => tokenSizes.has(k));
    expect(grown.sort()).toEqual(["--fs", "--fs-lg", "--fs-md", "--fs-sm", "--fs-xl", "--fs-xs", "--size-board-col", "--size-row", "--size-side", "--size-topbar"]);
    for (const k of grown) expect([k, atDefault(root.get(k) ?? "")]).toEqual([k, tokenSizes.get(k)]);
  });
  it("keep the chat at the interface text (your messages, the composer) and Markdown at --fs-md, as before", () => {
    expect(atDefault(root.get("--fs-chat") ?? "")).toBe(tokenSizes.get("--fs"));
    expect(atDefault(root.get("--fs-prose") ?? "")).toBe(tokenSizes.get("--fs-md"));
    const chat = block(appearanceCss, ".chat-main:not(.chat-archive), .chat-sample");
    expect(atDefault(chat.get("--fs-prose") ?? "")).toBe(tokenSizes.get("--fs-md"));
  });
  it("grow the interface by --ui-grow, chat text by --chat-grow and tasks and docs by --docs-grow", () => {
    expect(root.get("--fs")).toContain("var(--ui-grow, 0px)");
    expect(root.get("--fs-chat")).toContain("var(--chat-grow, 0px)");
    expect(root.get("--fs-prose")).toContain("var(--docs-grow, 0px)");
    // headings half, small text at most the meta step
    for (const k of ["--fs-md", "--fs-lg", "--fs-xl"]) expect(root.get(k)).toContain("var(--ui-grow-head, 0px)");
    for (const k of ["--fs-xs", "--fs-sm"]) expect(root.get(k)).toContain("var(--ui-grow-meta, 0px)");
  });
});

describe("every growth variable", () => {
  const sources: [string, string][] = [["components.css", componentsCss], ["app.css", appCss], ["appearance.css", appearanceCss], ["RunPanel.tsx", runPanel], ["MarkdownEditor.tsx", editor]];
  const GROWTH = /var\(\s*(--(?:ui-grow(?:-head|-meta)?|ui-box|chat-grow(?:-head)?|docs-grow(?:-head)?|prose-grow-head|chat-scale))\s*(,\s*([^)]*))?\)/g;
  it("has a fallback that changes nothing (0px, or 1 for the chat column's scale)", () => {
    let seen = 0;
    for (const [file, text] of sources) {
      for (const m of text.matchAll(GROWTH)) {
        seen++;
        const want = m[1] === "--chat-scale" ? "1" : "0px";
        expect([file, m[0], (m[3] ?? "").trim()]).toEqual([file, m[0], want]);
      }
    }
    expect(seen).toBeGreaterThan(30);
  });
});

describe("the font choices", () => {
  const faces = [...fontsCss.matchAll(/@font-face\s*\{([^}]*)\}/g)].map((m) => ({
    family: /font-family:\s*'([^']+)'/.exec(m[1] ?? "")?.[1] ?? "",
    urls: [...(m[1] ?? "").matchAll(/url\('([^']+)'\)/g)].map((u) => u[1] ?? ""),
  }));
  const families = (key: string) => {
    const rule = block(appearanceCss, `[data-font="${key}"]`);
    const named = (v: string) => [...v.matchAll(/"([^"]+)"/g)].map((x) => x[1] ?? "");
    return { sans: named(rule.get("--font-sans") ?? ""), mono: named(rule.get("--font-mono") ?? "") };
  };
  it("set the text font and the code font the card gives each", () => {
    expect(Object.fromEntries(FONTS.map((f) => [f.key, families(f.key)]))).toEqual({
      atkinson: { sans: ["Atkinson Hyperlegible Next"], mono: ["Atkinson Hyperlegible Mono"] },
      "jetbrains-mono": { sans: ["JetBrains Mono"], mono: ["JetBrains Mono"] },
      inter: { sans: ["Inter"], mono: ["JetBrains Mono"] },
      geist: { sans: ["Geist"], mono: ["Geist Mono"] },
      hack: { sans: ["Hack"], mono: ["Hack"] },
    });
  });
  it("the default's stacks are the design tokens' families", () => {
    const at = block(appearanceCss, '[data-font="atkinson"]');
    expect(at.get("--font-sans")).toBe(tokenSizesOrFonts("--font-sans"));
    expect(at.get("--font-mono")).toBe(tokenSizesOrFonts("--font-mono"));
  });
  it("ship with the app: every family has an @font-face on a woff2 file in src/assets/fonts, none from the internet", () => {
    const all = new Set(FONTS.flatMap((f) => { const x = families(f.key); return [...x.sans, ...x.mono]; }));
    for (const family of all) {
      const mine = faces.filter((f) => f.family === family);
      expect([family, mine.length > 0]).toEqual([family, true]);
      for (const url of mine.flatMap((f) => f.urls)) {
        expect(url).toMatch(/^\.\.\/assets\/fonts\/[\w-]+\.woff2$/);
        expect(fontFiles).toContain(url.replace("../assets/fonts/", ""));
      }
    }
    expect(fontsCss + appearanceCss).not.toMatch(/https?:\/\/|@import/);
  });
  it("each have their licence in the repo: SIL Open Font License 1.1, or MIT for Hack", () => {
    const licenceOf = (name: string) => licences[`../assets/fonts/${name}`] ?? "";
    for (const [file, family] of [["LICENSE-Inter.txt", "Inter"], ["LICENSE-JetBrainsMono.txt", "JetBrains Mono"], ["LICENSE-Geist.txt", "Geist"],
      ["LICENSE-GeistMono.txt", "Geist"], ["LICENSE-AtkinsonHyperlegibleNext.txt", "Atkinson Hyperlegible Next"], ["LICENSE-AtkinsonHyperlegibleMono.txt", "Atkinson Hyperlegible Mono"]]) {
      expect([file, licenceOf(file ?? "").includes(family ?? "")]).toEqual([file, true]);
      expect(licenceOf(file ?? "")).toContain("SIL Open Font License, Version 1.1");
    }
    expect(licenceOf("LICENSE-Hack.md")).toMatch(/Hack[\s\S]*MIT License/);
  });
});

function tokenSizesOrFonts(name: string): string {
  return new RegExp(`${name}\\s*:\\s*([^;]+);`).exec(tokensCss)?.[1]?.trim() ?? "";
}
