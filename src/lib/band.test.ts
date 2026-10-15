// GA-143 QA: the mountains along the bottom of the window, GA-139's design at the Subtle strength Jefsev picked. The band's
// tokens are in design/tokens.json where GA-139's new-tokens.json says, tokens.css is made from them and band-line and
// band-glow give the Subtle colours in dark and light; text over the band keeps the contrast in GA-139's table (§3); its
// height is clamp(80px, 14vh, 128px) and it goes under 560px, with more contrast, in forced colours and in print; it is one
// fixed layer at z-index -1 that takes no clicks; only the canvases of §5.2 go see-through, so cards, controls, the composer
// box, code, tabs, the status bar and popovers keep their own background; and nothing between the root and the columns
// makes a stacking context, so menus, drawers, the palette and toasts stay above it. Files are read from disk (Vitest hands
// .css back empty); node:fs comes in through a computed specifier, as in terminalTokens.test.ts.
import { describe, expect, it } from "vitest";
import { tokensCss, type Tokens } from "./tokensCss";

const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const read = (path: string): string => fs.readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");
type Token = { name: string; value: unknown; usage: string };
const tokensJson = JSON.parse(read("design/tokens.json")) as Tokens & { color: { tokens: Token[] }; size: { tokens: Token[] } };
const tokensFile = read("src/styles/tokens.css");
const bandCss = read("src/styles/band.css");
const APP_CSS = ["app.css", "components.css", "terminal.css", "changes.css", "appearance.css", "onboarding.css"].map((f) => [f, read(`src/styles/${f}`)] as const);

/** GA-139's Subtle pair (design/branding/new-tokens.json on its branch, cfb17b9). */
const SUBTLE = {
  line: { dark: "rgba(160, 211, 235, 0.10)", light: "rgba(32, 108, 137, 0.14)" },
  glow: { dark: "rgba(20, 100, 128, 0.08)", light: "rgba(201, 227, 239, 0.35)" },
};

/** `--name: value;` declarations of the first block whose selector is exactly `selector`. */
function block(css: string, selector: string): Map<string, string> {
  const at = css.indexOf(`${selector} {`);
  if (at < 0) throw new Error(`no ${selector} block`);
  const body = css.slice(css.indexOf("{", at) + 1, css.indexOf("}", at)).replace(/\/\*[\s\S]*?\*\//g, "");
  return new Map([...body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1] ?? "", (m[2] ?? "").trim()]));
}
const dark = block(tokensFile, ':root, [data-theme="dark"]');
const light = block(tokensFile, '[data-theme="light"]');
const plain = block(tokensFile, ":root");
/** A token's value in a theme, aliases followed: data-theme is on <html>, where :root's aliases are worked out. */
function colour(theme: "dark" | "light", name: string): string {
  const value = (theme === "light" ? light.get(name) : undefined) ?? dark.get(name);
  if (value === undefined) throw new Error(`${name} isn't in tokens.css`);
  const alias = /^var\((--[\w-]+)\)$/.exec(value);
  return alias ? colour(theme, alias[1] ?? "") : value;
}

/** The style rules of a stylesheet, innermost first: each selector of a list on its own, with its declarations. */
function rules(css: string): { selector: string; decl: Map<string, string> }[] {
  const out: { selector: string; decl: Map<string, string> }[] = [];
  for (const m of css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const decl = new Map((m[2] ?? "").split(";").map((d) => d.split(":")).filter((d) => d.length > 1)
      .map(([k, ...v]) => [(k ?? "").trim(), v.join(":").trim()] as [string, string]));
    for (const s of (m[1] ?? "").split(",")) out.push({ selector: s.trim().replace(/\s+/g, " "), decl });
  }
  return out;
}
const band = rules(bandCss);
const decl = (selector: string) => band.find((r) => r.selector === selector)?.decl ?? new Map<string, string>();

// WCAG 2's contrast, over the band where it is strongest: the glow at full strength over the canvas, and a line over that.
type RGBA = [number, number, number, number];
function rgba(c: string): RGBA {
  if (c.startsWith("#")) return [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16)).concat(1) as RGBA;
  const n = (c.match(/[\d.]+/g) ?? []).map(Number);
  return [n[0] ?? 0, n[1] ?? 0, n[2] ?? 0, n[3] ?? 1];
}
const over = (top: RGBA, under: RGBA): RGBA => [0, 1, 2].map((i) => top[i]! * top[3] + under[i]! * (1 - top[3])).concat(1) as RGBA;
const hex = (c: RGBA) => `#${c.slice(0, 3).map((v) => Math.round(v).toString(16).padStart(2, "0")).join("")}`;
const lum = (c: RGBA) => 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
function lin(v: number) { const s = v / 255; return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4; }
function ratio(a: RGBA, b: RGBA) { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x! + 0.05) / (y! + 0.05); }

describe("the band's tokens (GA-139's new-tokens.json, Subtle)", () => {
  it("are the Subtle pair and band-line and band-glow pointing at it, right after scrim, and the two sizes right after size-statusbar", () => {
    const colours = tokensJson.color.tokens;
    const at = colours.findIndex((t) => t.name === "scrim") + 1;
    expect(colours.slice(at, at + 4).map((t) => [t.name, t.value])).toEqual([
      ["band-subtle-line", SUBTLE.line],
      ["band-subtle-glow", SUBTLE.glow],
      ["band-line", "{band-subtle-line}"],
      ["band-glow", "{band-subtle-glow}"],
    ]);
    const sizes = tokensJson.size.tokens;
    const sat = sizes.findIndex((t) => t.name === "size-statusbar") + 1;
    expect(sizes.slice(sat, sat + 2).map((t) => [t.name, t.value])).toEqual([["size-band", "128px"], ["size-band-min", "80px"]]);
    for (const t of [...colours, ...sizes].filter((x) => x.name.includes("band"))) expect(t.usage.length).toBeGreaterThan(0);
  });

  it("are in tokens.css, made from design/tokens.json", () => {
    expect(tokensFile).toBe(tokensCss(tokensJson));
    expect(plain.get("--size-band")).toBe("128px");
    expect(plain.get("--size-band-min")).toBe("80px");
  });

  it("give the band the Subtle colours in dark and in light", () => {
    for (const theme of ["dark", "light"] as const) {
      expect(colour(theme, "--band-line")).toBe(SUBTLE.line[theme]);
      expect(colour(theme, "--band-glow")).toBe(SUBTLE.glow[theme]);
    }
  });
});

describe("text over the band (GA-139 §3, the Subtle rows)", () => {
  // Behind the text between the lines · on a line, and text, text-2, text-3 and focus there, as the design's table has them.
  const TABLE = [
    { theme: "dark", canvas: "bg", behind: ["#0f171c", "#1e2a30"], text: [15.52, 12.63], text2: [7.63, 6.21], text3: [4.78, 3.89], focus: [8.08, 6.57] },
    { theme: "dark", canvas: "side", behind: ["#0c1318", "#1b262d"], text: [16.02, 13.18], text2: [7.88, 6.48], text3: [4.93, 4.06], focus: [8.33, 6.86] },
    { theme: "light", canvas: "bg", behind: ["#ecf5f9", "#d0e2ea"], text: [16.4, 13.56], text2: [7.43, 6.14], text3: [5.12, 4.24], focus: [5.23, 4.32] },
    { theme: "light", canvas: "side", behind: ["#e4eef4", "#c9dce5"], text: [15.39, 12.78], text2: [6.97, 5.79], text3: [4.81, 3.99], focus: [4.9, 4.07] },
  ] as const;

  for (const row of TABLE) {
    it(`keeps the table's contrast in ${row.theme} over ${row.canvas}`, () => {
      const between = over(rgba(colour(row.theme, "--band-glow")), rgba(colour(row.theme, `--${row.canvas}`)));
      const onLine = over(rgba(colour(row.theme, "--band-line")), between);
      expect([hex(between), hex(onLine)]).toEqual(row.behind);
      const got = (name: string) => [ratio(rgba(colour(row.theme, name)), between), ratio(rgba(colour(row.theme, name)), onLine)];
      for (const [name, want] of [["--text", row.text], ["--text-2", row.text2], ["--text-3", row.text3], ["--focus", row.focus]] as const) {
        const [b, l] = got(name);
        expect([name, b, l]).toEqual([name, expect.closeTo(want[0], 2), expect.closeTo(want[1], 2)]);
      }
      // the design's rule for Subtle: all three text colours at 4.5:1 between the lines, text and text-2 on a line too,
      // text-3 at 3:1 on a line; the focus ring (an outline, not text) at 3:1
      for (const name of ["--text", "--text-2", "--text-3"]) expect(got(name)[0]).toBeGreaterThanOrEqual(4.5);
      for (const name of ["--text", "--text-2"]) expect(got(name)[1]).toBeGreaterThanOrEqual(4.5);
      expect(got("--text-3")[1]).toBeGreaterThanOrEqual(3);
      expect(Math.min(...got("--focus"))).toBeGreaterThanOrEqual(3);
    });
  }
});

describe("the layer (band.css)", () => {
  it("is fixed along the window's bottom edge, the window's width, at z-index -1, and takes no clicks or selections", () => {
    const d = decl(".band");
    for (const [k, v] of [["position", "fixed"], ["left", "0"], ["right", "0"], ["bottom", "0"], ["z-index", "-1"], ["overflow", "hidden"],
      ["pointer-events", "none"], ["user-select", "none"], ["color", "var(--band-line)"], ["background", "linear-gradient(transparent, var(--band-glow))"]]) {
      expect([k, d.get(k ?? "")]).toEqual([k, v]);
    }
    expect(decl(".band > svg")).toEqual(new Map([["display", "block"], ["width", "100%"], ["height", "100%"], ["color", "inherit"]]));
  });

  it("is between 80px and 128px high: 14% of the window, 128px from 915px up, as §5.4 lists", () => {
    const h = /^clamp\(var\((--[\w-]+)\), (\d+)vh, var\((--[\w-]+)\)\)$/.exec(decl(".band").get("height") ?? "");
    expect(h?.slice(1)).toEqual(["--size-band-min", "14", "--size-band"]);
    const min = parseFloat(plain.get(h?.[1] ?? "") ?? "");
    const max = parseFloat(plain.get(h?.[3] ?? "") ?? "");
    const at = (win: number) => Math.min(max, Math.max(min, (win * Number(h?.[2])) / 100));
    expect([1080, 915, 900, 768, 600, 560].map(at)).toEqual([128, 128, 126, 107.52, 84, 80]);
    for (let win = 560; win <= 2400; win += 7) {
      expect(at(win)).toBeGreaterThanOrEqual(80);
      expect(at(win)).toBeLessThanOrEqual(128);
    }
  });

  // GA-164: the first start's onboarding (.onb) has the band too, as its first child, and it goes there likewise
  it("goes in a window under 560px high, with more contrast, in forced colours and in print, in the app and in the onboarding", () => {
    const media = /@media ([^{]+)\{\s*\.app > \.band, \.onb > \.band \{\s*display: none;\s*\}\s*\}/.exec(bandCss.replace(/\/\*[\s\S]*?\*\//g, ""));
    expect(media?.[1]?.split(",").map((q) => q.trim())).toEqual(["(max-height: 559px)", "(prefers-contrast: more)", "(forced-colors: active)", "print"]);
  });

  it("is the only thing under the page: no other negative z-index in the app's styles", () => {
    expect(band.filter((r) => /^-/.test(r.decl.get("z-index") ?? "")).map((r) => [r.selector, r.decl.get("z-index")])).toEqual([
      [".band", "-1"], ["[data-band] .side::before", "-2"], ["[data-band] .chat-threads::before", "-2"], ["[data-band] .ed-side::before", "-2"],
    ]);
    for (const [file, css] of APP_CSS) expect([file, rules(css).filter((r) => /^-/.test(r.decl.get("z-index") ?? "")).map((r) => r.selector)]).toEqual([file, []]);
  });

  it("leaves the look as it was while it is off: every other rule needs data-band on <html>", () => {
    for (const r of band) {
      if (r.selector === ".band" || r.selector === ".band > svg" || r.selector === ".app > .band" || r.selector === ".onb > .band") continue;
      expect(r.selector).toMatch(/^\[data-band\] /);
    }
  });
});

describe("see-through and solid (GA-139 §5.2)", () => {
  it("makes only body, the sidebar, the chat's threads and composer strip, the Editor's tree, the main column and the properties panel see-through", () => {
    const cleared = band.filter((r) => r.decl.get("background") === "none").map((r) => r.selector).sort();
    expect(cleared).toEqual(["[data-band] .chat-composer", "[data-band] .chat-threads", "[data-band] .ed-side", "[data-band] .main", "[data-band] .props",
      "[data-band] .side", "[data-band] body"].sort());
    // html keeps the canvas
    const htmlBg = APP_CSS.flatMap(([, css]) => rules(css)).find((r) => r.selector === "html" && r.decl.has("background"));
    expect(htmlBg?.decl.get("background")).toBe("var(--bg)");
  });

  it("keeps the side colour of the sidebar, the threads and the tree on a ::before under the band", () => {
    for (const col of [".side", ".chat-threads", ".ed-side"]) {
      expect([col, decl(`[data-band] ${col}`).get("position")]).toEqual([col, "relative"]);
      const b = decl(`[data-band] ${col}::before`);
      expect([col, b.get("content"), b.get("position"), b.get("inset"), b.get("z-index"), b.get("background"), b.get("pointer-events")])
        .toEqual([col, '""', "absolute", "0", "-2", "var(--side)", "none"]);
    }
  });

  it("leaves cards, controls, the composer box, code, tabs, the status bar and popovers on a solid background of their own", () => {
    const SOLID: [string, string][] = [
      [".card", "a board card"], [".run-card", "a run card"], [".tool-card", "a tool card"], [".chat-msg.user .bubble", "a user bubble"],
      [".btn", "a button"], [".input", "a text field"], [".md-editor", "an editor box"], [".composer-box", "the chat's composer box"],
      [".chat-banner", "the composer's banner"], [".prose pre", "a code block"], [".ed-main", "the Editor's code column"],
      [".ed-tabs", "the Editor's tabs"], [".ed-status", "the Editor's status bar"], [".term-panel", "the terminal panel"],
      [".term-place", "a terminal"], [".chg-diffs", "the Changes view"], [".grid th", "a sticky table head"],
      [".pop", "a menu or popover"], [".drawer", "a drawer"], [".pal", "the palette"], [".toast", "a toast"],
    ];
    const all = APP_CSS.flatMap(([, css]) => rules(css));
    for (const [selector, what] of SOLID) {
      const bg = all.find((r) => r.selector === selector && (r.decl.has("background") || r.decl.has("background-color")));
      const value = bg?.decl.get("background") ?? bg?.decl.get("background-color") ?? "";
      const token = /^var\((--[\w-]+)\)$/.exec(value)?.[1];
      for (const theme of ["dark", "light"] as const) {
        // an opaque colour: band-* are the only see-through ones
        expect([what, theme, token ? colour(theme, token) : value]).toEqual([what, theme, expect.stringMatching(/^#[0-9a-f]{6}$/)]);
      }
      expect([what, band.some((r) => r.selector.endsWith(selector) && r.decl.has("background"))]).toEqual([what, false]);
    }
  });
});

describe("above the band, as before", () => {
  it("nothing between the root and the columns makes a stacking context, so a column's ::before and the band share the root's", () => {
    const COLUMNS = ["html", "body", "#root", ".app", ".main", ".chat", ".ed", ".side", ".chat-threads", ".ed-side"];
    const MAKES = ["z-index", "opacity", "transform", "filter", "isolation", "will-change", "contain", "mix-blend-mode", "backdrop-filter", "perspective", "clip-path", "mask"];
    for (const [file, css] of [...APP_CSS, ["band.css", bandCss] as const]) {
      for (const r of rules(css)) {
        const last = r.selector.split(/[\s>+~]+/).pop() ?? "";
        if (last.includes("::")) continue;
        const target = COLUMNS.find((c) => last === c || last.startsWith(`${c}[`) || last.startsWith(`${c}:`) || last.startsWith(`${c}.`));
        if (!target) continue;
        const found = MAKES.filter((p) => r.decl.has(p)).concat(/^(fixed|sticky)$/.test(r.decl.get("position") ?? "") ? ["position"] : []);
        expect([file, r.selector, found]).toEqual([file, r.selector, []]);
      }
    }
  });

  it("menus, popovers, drawers, the palette, toasts and the ask dialog keep a z-index above the page", () => {
    const all = APP_CSS.flatMap(([, css]) => rules(css));
    for (const s of [".pop", ".pop.item-picker", ".note-preview", ".scrim", ".drawer", ".pal-backdrop", ".pal", ".toast", ".ask-scrim"]) {
      const z = Number(all.find((r) => r.selector === s && r.decl.has("z-index"))?.decl.get("z-index"));
      expect([s, z > 0]).toEqual([s, true]);
    }
  });
});

describe("where it is built in", () => {
  it("band.css comes after appearance.css, and the band is .app's first child", () => {
    const main = read("src/main.tsx");
    expect(main.indexOf('import "./styles/band.css";')).toBeGreaterThan(main.indexOf('import "./styles/appearance.css";'));
    const app = read("src/App.tsx").replace(/\{\/\*[\s\S]*?\*\/\}/g, "");
    expect(app).toMatch(/<div className="app">\s*<Band \/>/);
  });

  it("mountains.svg is GA-139's artwork: 4800 × 128 in quarter pixels, cut at the sides, its lines in currentColor and 1px at any width", () => {
    const svg = read("src/assets/mountains.svg");
    expect(svg.length).toBeLessThan(60_000);
    expect(svg).toMatch(/^<svg [^>]*viewBox="0 0 19200 512"[^>]*preserveAspectRatio="xMidYMax slice"/);
    expect(svg).toContain('stroke="currentColor"');
    expect(svg.match(/<path /g)?.length).toBe(svg.match(/<path vector-effect="non-scaling-stroke"/g)?.length);
    // nothing in it that the Content Security Policy or a screen reader would care about
    expect(svg).not.toMatch(/<(script|image|foreignObject|title|style)\b|href=|style=/);
  });
});
