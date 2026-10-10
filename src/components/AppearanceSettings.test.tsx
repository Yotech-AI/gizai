// GA-42: Settings → Appearance. Five font choices, each shown in its own font (data-font on the choice, which appearance.css
// turns into that font's stacks); a picker per text size with the card's sizes; Theme and Density; and Reset to defaults.
// A click changes the appearance at once (src/lib/appearance.ts, tested on its own); here it is checked that each control
// changes the right value. Rendered to HTML on the server, and clicked on the element tree with React's hooks replaced.
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";

let direct = false;
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => (direct ? [typeof init === "function" ? (init as () => unknown)() : init, () => {}] : R.useState(init))) as typeof R.useState;
  const useEffect = ((f: () => void, deps?: unknown[]) => (direct ? undefined : R.useEffect(f, deps))) as typeof R.useEffect;
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});

const { AppearanceSettings } = await import("./AppearanceSettings");
const { DEFAULTS, getAppearance, resetAppearance, setAppearance } = await import("../lib/appearance");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/\s+/g, " ").trim();
const render = () => renderToStaticMarkup(<AppearanceSettings />);
function expand(node: ReactNode): ReactNode {
  if (Array.isArray(node)) return node.map(expand);
  if (node && typeof node === "object" && "props" in node) {
    const el = node as ReactElement<Record<string, unknown>>;
    if (typeof el.type === "function") return expand((el.type as (p: unknown) => ReactNode)(el.props));
    return { ...el, props: { ...el.props, children: expand(el.props.children as ReactNode) } } as ReactNode;
  }
  return node;
}
function findAll(node: ReactNode, pred: (p: Record<string, unknown>) => boolean, out: Record<string, unknown>[] = []) {
  if (Array.isArray(node)) node.forEach((n) => findAll(n, pred, out));
  else if (node && typeof node === "object" && "props" in node) {
    const p = (node as ReactElement<Record<string, unknown>>).props;
    if (pred(p)) out.push(p);
    findAll(p.children as ReactNode, pred, out);
  }
  return out;
}
function tree() {
  direct = true;
  try { return expand(AppearanceSettings()); } finally { direct = false; }
}
/** The buttons of a group (by its aria-label), as [label, pressed]. */
function group(html: string, label: string): [string, boolean][] {
  const at = html.indexOf(`aria-label="${label}"`);
  const body = html.slice(at, html.indexOf("</div>", at));
  return [...body.matchAll(/<button aria-pressed="(true|false)"[^>]*>(.*?)<\/button>/g)].map((m) => [text(m[2] ?? ""), m[1] === "true"]);
}
/** Clicks the button labelled `label` in the group `groupLabel`. */
function click(groupLabel: string, label: string) {
  const t = tree();
  const g = findAll(t, (p) => p["aria-label"] === groupLabel)[0];
  const b = findAll(g?.children as ReactNode, (p) => typeof p.onClick === "function" && text(renderToStaticMarkup(<>{p.children as ReactNode}</>)) === label)[0] as { onClick: () => void } | undefined;
  if (!b) throw new Error(`no ${label} in ${groupLabel}`);
  b.onClick();
}

beforeEach(() => resetAppearance());

describe("Settings → Appearance", () => {
  it("says changes show at once, without Save settings", () => {
    expect(text(render())).toContain("Changes show at once, without Save settings, and are kept on this computer.");
  });
  it("offers the five fonts, each a choice shown in its own font, the default picked", () => {
    const html = render();
    expect(html).toContain('role="radiogroup" aria-label="Font"');
    const choices = [...html.matchAll(/<label class="font-choice( on)?" data-font="([^"]+)"><input type="radio" name="gizai-font"( checked="")?\/><span class="fc-name">([^<]+)<\/span><span class="fc-sample">[^<]*<span class="mono">KADE-41<\/span>/g)];
    expect(choices.map((m) => [m[2], m[4], !!m[1], !!m[3]])).toEqual([
      ["atkinson", "Atkinson Hyperlegible", true, true],
      ["jetbrains-mono", "JetBrains Mono", false, false],
      ["inter", "Inter", false, false],
      ["geist", "Geist", false, false],
      ["hack", "Hack", false, false],
    ]);
    // Inter says it is like the apps the card names
    expect(text(html)).toContain("Used by many apps, like Figma and Linear");
  });
  it("has a picker per text size with the card's sizes, today's size pressed", () => {
    const html = render();
    expect(group(html, "Chat size")).toEqual([14, 15, 16, 17, 18, 20].map((n) => [String(n), n === 15]));
    expect(group(html, "Interface size")).toEqual([12.5, 13.5, 14.5, 15.5, 16.5].map((n) => [String(n), n === 13.5]));
    expect(group(html, "Tasks and docs size")).toEqual([14, 15, 16, 17, 18, 20].map((n) => [String(n), n === 15]));
    expect(html).toContain('title="15 px, the default"');
    expect(html).toContain('title="13.5 px, the default"');
  });
  it("has Theme (Dark, Light) and Density (Comfortable, Compact)", () => {
    const html = render();
    expect(group(html, "Theme")).toEqual([["Dark", true], ["Light", false]]);
    expect(group(html, "Density")).toEqual([["Comfortable", true], ["Compact", false]]);
  });
  it("shows what is set now", () => {
    setAppearance({ font: "geist", chat: 20, ui: 16.5, docs: 18, theme: "light", density: "compact" });
    const html = render();
    expect(html).toContain('<label class="font-choice on" data-font="geist"><input type="radio" name="gizai-font" checked=""/>');
    expect(group(html, "Chat size").filter(([, on]) => on)).toEqual([["20", true]]);
    expect(group(html, "Interface size").filter(([, on]) => on)).toEqual([["16.5", true]]);
    expect(group(html, "Tasks and docs size").filter(([, on]) => on)).toEqual([["18", true]]);
    expect(group(html, "Theme")).toEqual([["Dark", false], ["Light", true]]);
    expect(group(html, "Density")).toEqual([["Comfortable", false], ["Compact", true]]);
  });
  it("Reset to defaults is off at the defaults and on as soon as anything differs", () => {
    expect(render()).toMatch(/<button class="link" disabled="" title="Everything is at its default">Reset to defaults<\/button>/);
    for (const change of [{ chat: 16 }, { font: "hack" as const }, { theme: "light" as const }, { density: "compact" as const }, { ui: 12.5 }, { docs: 20 }]) {
      resetAppearance();
      setAppearance(change);
      expect(render()).toContain('<button class="link">Reset to defaults</button>');
    }
  });
});

describe("a click", () => {
  it("on a size sets that part's size, and only that part", () => {
    click("Chat size", "20");
    expect(getAppearance()).toEqual({ ...DEFAULTS, chat: 20 });
    click("Interface size", "16.5");
    click("Tasks and docs size", "14");
    expect(getAppearance()).toEqual({ ...DEFAULTS, chat: 20, ui: 16.5, docs: 14 });
  });
  it("on a font picks it", () => {
    const radios = findAll(tree(), (p) => p.type === "radio") as unknown as { onChange: () => void }[];
    expect(radios.length).toBe(5);
    radios[2]?.onChange();
    expect(getAppearance().font).toBe("inter");
    radios[4]?.onChange();
    expect(getAppearance().font).toBe("hack");
  });
  it("on Light, Compact and back", () => {
    click("Theme", "Light");
    click("Density", "Compact");
    expect(getAppearance()).toEqual({ ...DEFAULTS, theme: "light", density: "compact" });
    click("Theme", "Dark");
    click("Density", "Comfortable");
    expect(getAppearance()).toEqual(DEFAULTS);
  });
  it("on Reset to defaults brings everything back", () => {
    setAppearance({ font: "geist", chat: 20, ui: 16.5, docs: 18, theme: "light", density: "compact" });
    const reset = findAll(tree(), (p) => p.className === "link")[0] as { onClick: () => void; children: unknown };
    expect(reset.children).toBe("Reset to defaults");
    reset.onClick();
    expect(getAppearance()).toEqual(DEFAULTS);
  });
});
