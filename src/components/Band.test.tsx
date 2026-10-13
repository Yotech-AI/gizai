// GA-143 QA: the band's element (Band.tsx). While Settings → Appearance → "Show mountains along the bottom" is on it is one
// div.band with GA-139's mountains inline (so the lines take currentColor), hidden from screen readers and out of the tab
// order; while it is off there is nothing at all. Rendered to HTML on the server; band.css (src/lib/band.test.ts) places it.
import { afterEach, describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { Band } from "./Band";
import { resetAppearance, setAppearance } from "../lib/appearance";

const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const svg = fs.readFileSync(new URL("../assets/mountains.svg", import.meta.url), "utf8") as string;

afterEach(() => resetAppearance());

describe("the band", () => {
  it("is on by default: one div.band, aria-hidden, with the artwork inline, not as an image", () => {
    const html = renderToStaticMarkup(<Band />);
    expect(html.startsWith('<div class="band" aria-hidden="true"><svg ')).toBe(true);
    expect(html.endsWith("</svg>\n</div>") || html.endsWith("</svg></div>")).toBe(true);
    expect(html).toContain(svg.trim());
    expect(html).toContain('stroke="currentColor"');
    expect(html).not.toMatch(/<img|role=|tabindex=|<title|<a /);
    expect(html.match(/<div/g)?.length).toBe(1);
  });

  it("is gone at once while the switch is off, and back when it is on again", () => {
    setAppearance({ band: false }, false);
    expect(renderToStaticMarkup(<Band />)).toBe("");
    setAppearance({ band: true }, false);
    expect(renderToStaticMarkup(<Band />)).toContain('class="band"');
  });

  it("comes back with Reset to defaults", () => {
    setAppearance({ band: false }, false);
    resetAppearance();
    expect(renderToStaticMarkup(<Band />)).toContain('class="band"');
  });
});
