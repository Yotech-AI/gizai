// GA-83: the chat composer is one taller box, like Claude's. The text is on top, two lines high when empty and growing to
// 200px; under it, in the same box, one row: + on the left, the Enter, Shift Enter and @ tips, then Runs on and Send (Stop
// before it while the Team Lead answers) on the right. No hint row under the box any more. Runs on is as wide as the
// picked CLI's name (a hidden copy of it sizes the select), and a click in the box around the text puts the cursor in it.
// Rendered to HTML on the server; Composer and RunsOn are taken from the elements ChatThread returns when called as a
// function with plain stand-ins for React's hooks, then rendered with the props each case needs.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { DependencyList, EffectCallback, ReactElement } from "react";
import type { ChatCli, Member } from "../../types";

vi.mock("../../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) out[k] = typeof v !== "function" ? v : () => new Promise(() => {});
  return out;
});
// Calling a component as a function: useState gives its initial value, effects don't run, refs are plain objects.
const fake = vi.hoisted(() => ({ on: false }));
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => (fake.on ? [typeof init === "function" ? (init as () => unknown)() : init, () => {}] : R.useState(init))) as typeof R.useState;
  const useEffect = ((effect: EffectCallback, deps?: DependencyList) => (fake.on ? undefined : R.useEffect(effect, deps))) as typeof R.useEffect;
  const useLayoutEffect = ((effect: EffectCallback, deps?: DependencyList) => (fake.on ? undefined : R.useLayoutEffect(effect, deps))) as typeof R.useLayoutEffect;
  const useRef = ((init: unknown) => (fake.on ? { current: init } : R.useRef(init))) as typeof R.useRef;
  const hooks = { useState, useEffect, useLayoutEffect, useRef };
  return { ...R, ...hooks, default: { ...R, ...hooks } };
});
vi.mock("./useChat", () => ({
  useChat: () => ({ messages: [], draft: "", tool: null, working: false, queue: [], error: null, setWorking: () => {} }),
}));

const { ChatThread } = await import("./ChatThread");

const lead = (status = "active") => ({ actorId: "lead", name: "Team Lead", kind: "agent", roleKey: "lead", handle: "lead", status, isLead: false,
  allowedTools: [], chatEnabled: true }) as Member;
const CLIS: ChatCli[] = [
  { id: "claude_code", name: "Claude Code", kind: "claude_code" },
  { id: "cc2", name: "Claude Code 2", kind: "claude_code" },
  { id: "codex", name: "Codex", kind: "codex", problem: "the chat runs on Claude Code only" },
];

// A tree of elements: what a component returned, with the elements in its props (children, picker) too.
type El = { type: unknown; props: Record<string, unknown> };
const isEl = (x: unknown): x is El => !!x && typeof x === "object" && "type" in x && "props" in x;
function elements(node: unknown, out: El[] = []): El[] {
  if (Array.isArray(node)) node.forEach((n) => elements(n, out));
  else if (isEl(node)) { out.push(node); Object.values(node.props).forEach((v) => elements(v, out)); }
  return out;
}
function call<P>(component: (p: P) => unknown, props: P): El[] {
  fake.on = true;
  try { return elements(component(props)); } finally { fake.on = false; }
}

type Props = Record<string, unknown>;
const composerEl = call(ChatThread, { threadId: "c1", agent: lead() }).find((e) => typeof e.type === "function" && "picker" in e.props)!;
const Composer = composerEl.type as (p: Props) => ReactElement;
const RunsOn = (composerEl.props.picker as El).type as (p: Props) => ReactElement;
const composer = (over: Props = {}) => renderToStaticMarkup(<Composer {...composerEl.props} picker={<span className="picker-here" />} {...over} />);
const runsOn = (over: Props) => renderToStaticMarkup(<RunsOn value="claude_code" clis={CLIS} disabled={false} onChange={() => {}} {...over} />);

/** Where `needle` first is in `html`, after `from`; -1 when it isn't there. */
const at = (html: string, needle: string, from = 0) => html.indexOf(needle, from);
/** Where the tag holding `needle` (an attribute) opens. */
const tagOf = (html: string, needle: string, from = 0) => html.lastIndexOf("<", at(html, needle, from));
/** True when the position `pos` lies inside the element that opens at `start` (a div): more divs opened than closed. */
function inside(html: string, start: number, pos: number): boolean {
  const between = html.slice(start, pos);
  return (between.match(/<div[\s>]/g) ?? []).length > (between.match(/<\/div>/g) ?? []).length;
}
const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();

describe("the composer is one box (GA-83)", () => {
  const page = renderToStaticMarkup(<ChatThread threadId="c1" agent={lead()} />);
  const box = at(page, '<div class="composer-box"');

  it("has the text on top and, under it in the same box, one row: +, the three tips, Runs on, then Send", () => {
    expect(box).toBeGreaterThan(-1);
    const order = ['class="md-editor composer-editor"', 'class="composer-foot"', 'aria-label="Add files or link an item"', 'class="composer-hint"',
      'class="runs-on"', 'aria-label="Send"'].map((n) => at(page, n, box));
    expect(order.every((p) => p > box)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
    for (const p of order) expect(inside(page, box, p)).toBe(true);
    // The row is inside the box, not after it.
    expect(inside(page, tagOf(page, 'class="composer-foot"', box), order[5]!)).toBe(true);
  });

  it("has no hint row under the box any more: the tips and Runs on show once, in the box", () => {
    expect(page.match(/class="composer-hint"/g)).toHaveLength(1);
    expect(page.match(/class="composer-foot"/g)).toHaveLength(1);
    expect(page.match(/aria-label="Runs on"/g)).toHaveLength(1);
    expect(page).not.toContain("composer-row");
    const hint = page.slice(tagOf(page, 'class="composer-hint"'));
    expect(text(hint.slice(0, at(hint, 'class="runs-on"')))).toMatch(/^Enter sends Shift Enter new line @ links a task, project, client or agent/);
  });

  it("shows files you added as removable chips above the text, inside the box", () => {
    const html = composer({ files: ["/home/you/invoice 2026.pdf"] });
    const start = at(html, '<div class="composer-box"');
    const chips = at(html, '<ul class="files compact"');
    expect(chips).toBeGreaterThan(start);
    expect(chips).toBeLessThan(at(html, "composer-editor"));
    expect(inside(html, start, chips)).toBe(true);
    expect(html).toContain('aria-label="Remove invoice 2026.pdf"');
  });

  it("puts Runs on (the picker it is given) between the tips and Send", () => {
    const html = composer();
    expect(at(html, 'class="picker-here"')).toBeGreaterThan(at(html, 'class="composer-hint"'));
    expect(at(html, 'class="picker-here"')).toBeLessThan(at(html, 'aria-label="Send"'));
  });

  it("while the Team Lead answers: Enter queues, Stop shows before Queue once there is text, and only Stop without it", () => {
    const typed = composer({ working: true, value: "and one more thing" });
    expect(text(typed.slice(tagOf(typed, 'class="composer-hint"')))).toMatch(/^Enter queues Shift Enter new line/);
    const stop = at(typed, 'class="btn sm stop-btn"');
    expect(stop).toBeGreaterThan(at(typed, 'class="picker-here"'));
    expect(at(typed, 'aria-label="Queue"')).toBeGreaterThan(stop);
    expect(typed).not.toContain('aria-label="Send"');
    const empty = composer({ working: true, value: "" });
    expect(empty).toContain('class="btn sm stop-btn"');
    expect(empty).not.toContain('aria-label="Queue"');
    expect(empty).not.toContain('aria-label="Send"');
  });

  it("is off while the Team Lead is paused, the row's + and Send too", () => {
    const html = renderToStaticMarkup(<ChatThread threadId="c1" agent={lead("paused")} />);
    expect(html).toContain('class="composer-box disabled"');
    expect(html).toMatch(/class="btn ghost sm icon-only composer-plus" disabled=""/);
    expect(html).toMatch(/<button class="btn primary sm icon-only" disabled="" aria-label="Send"/);
  });
});

describe("Runs on in the composer's row (GA-83)", () => {
  it("is as wide as the picked CLI: a hidden copy of its name sizes the select", () => {
    const html = runsOn({ value: "cc2" });
    expect(html).toContain('<span class="runs-on-pick"><span class="runs-on-size" aria-hidden="true">Claude Code 2</span><select class="select" aria-label="Runs on"');
    expect(runsOn({ value: "claude_code" })).toContain('<span class="runs-on-size" aria-hidden="true">Claude Code</span>');
  });

  it("still lists every CLI from Settings, the ones that can't run the chat disabled with why, and no extra entry", () => {
    const html = runsOn({ value: "cc2" });
    const options = [...html.matchAll(/<option ([^>]*)>([^<]*)<\/option>/g)].map((m) => ({ attrs: m[1] ?? "", name: m[2] ?? "" }));
    expect(options.map((o) => o.name)).toEqual(["Claude Code", "Claude Code 2", "Codex (the chat runs on Claude Code only)"]);
    expect(options.map((o) => o.attrs.includes('disabled=""'))).toEqual([false, false, true]);
    expect(options[1]?.attrs).toContain('selected=""');
  });

  it("says Loading… before the CLIs are known, and shows an unknown CLI's id, both in the picker and its hidden copy", () => {
    const loading = runsOn({ clis: null });
    expect(loading).toContain('<span class="runs-on-size" aria-hidden="true">Loading…</span>');
    expect(loading).toMatch(/<select class="select" aria-label="Runs on" disabled="">/);
    expect(loading.match(/<option[^>]*>Loading…<\/option>/g)).toHaveLength(1);
    const unknown = runsOn({ value: "gemini", clis: [] });
    expect(unknown).toContain('<span class="runs-on-size" aria-hidden="true">gemini</span>');
    expect(unknown.match(/<option[^>]*>gemini<\/option>/g)).toHaveLength(1);
  });

  it("is greyed out while an answer runs, saying when it can change", () => {
    const html = runsOn({ disabled: true });
    expect(html).toContain('title="Runs on can change when this answer is done; a change applies from the next message"');
    expect(html).toMatch(/<select class="select" aria-label="Runs on" disabled="">/);
    expect(runsOn({})).toContain('title="The coding CLI this chat&#x27;s answers run on"');
  });
});

describe("a click in the box around the text (GA-83)", () => {
  // The box's onMouseDown, with a stand-in for the editor and for the element clicked (its closest() answers for the
  // selectors of the elements it is in).
  const press = (within: string[], over: Props = {}) => {
    const focus = vi.fn();
    const el = call(Composer, { ...composerEl.props, picker: null, editor: { current: { focus } }, ...over })[0]!;
    expect(el.props.className).toMatch(/^composer-box/);
    const preventDefault = vi.fn();
    const target = { closest: (sel: string) => (sel.split(",").some((s) => within.includes(s.trim())) ? {} : null) };
    (el.props.onMouseDown as (e: unknown) => void)({ target, preventDefault });
    return { focused: focus.mock.calls.length > 0, prevented: preventDefault.mock.calls.length > 0 };
  };

  it("on the tips or the box's edges puts the cursor in the text", () => {
    expect(press([])).toEqual({ focused: true, prevented: true });
  });
  it("on a control (+, Send, Stop, Runs on, a file's Remove, the + menu) or in the text itself does nothing more", () => {
    for (const within of [["button"], ["select"], ["label"], ["a"], [".md-editor"], [".pop"]]) expect(press(within)).toEqual({ focused: false, prevented: false });
  });
  it("does nothing while the Team Lead is paused", () => {
    expect(press([], { disabled: true })).toEqual({ focused: false, prevented: false });
  });
});

// Vitest hands a .css file back empty, even with ?raw, so the stylesheet is read from disk. node:fs comes in through a
// computed specifier: the build type-checks tests without Node's types.
const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const componentsCss: string = fs.readFileSync(new URL("../../styles/components.css", import.meta.url), "utf8");
/** The declarations of the first block whose selector is exactly `selector`. */
function rule(selector: string): Map<string, string> {
  const start = componentsCss.indexOf(`\n${selector} {`);
  if (start < 0) throw new Error(`no ${selector} block`);
  const body = componentsCss.slice(componentsCss.indexOf("{", start) + 1, componentsCss.indexOf("}", start)).replace(/\/\*[\s\S]*?\*\//g, "");
  const out = new Map<string, string>();
  for (const m of body.matchAll(/([\w-]+)\s*:\s*([^;]+);/g)) out.set(m[1] ?? "", (m[2] ?? "").trim());
  return out;
}
const px = (v: string) => v.split(/\s+/).map((p) => parseFloat(p));

describe("the composer's styles (GA-83)", () => {
  it("make the empty text box two lines high; it grows to 200px, then scrolls", () => {
    const content = rule(".composer-editor .cm-content");
    expect(parseFloat(content.get("min-height") ?? "")).toBeCloseTo(2 * parseFloat(content.get("line-height") ?? ""), 5);
    expect(content.get("min-height")).toMatch(/em$/);
    expect(rule(".composer-editor .cm-editor").get("max-height")).toBe("200px");
    expect(rule(".composer-editor .cm-scroller").get("overflow-y")).toBe("auto");
  });

  it("keep the tips on one line: a tip that doesn't fit drops out whole instead of wrapping the row", () => {
    const hint = rule(".composer-hint");
    expect(hint.get("flex-wrap")).toBe("wrap");
    expect(hint.get("overflow")).toBe("hidden");
    expect(hint.get("height")).toBe(rule(".composer-hint > span").get("height"));
    expect(rule(".composer-hint > span").get("white-space")).toBe("nowrap");
    expect(hint.get("min-width")).toBe("0");
  });

  it("give the hidden copy the select's font and its padding plus the 1px border, so the select is as wide as the name", () => {
    const size = rule(".runs-on-size");
    const select = rule(".runs-on .select");
    expect(size.get("visibility")).toBe("hidden");
    expect(size.get("font-size")).toBe(select.get("font-size"));
    expect(size.get("font-weight")).toBe(select.get("font-weight"));
    expect(rule(".input, .select, .textarea").get("border")).toMatch(/^1px solid/);
    expect(px(size.get("padding") ?? "")).toEqual(px(select.get("padding") ?? "").map((p, i) => (i % 2 ? p + 1 : p)));
    expect(select.get("position")).toBe("absolute");
    expect(select.get("width")).toBe("100%");
  });

  it("make the row's controls 28px high and keep Runs on from squeezing the tips' row", () => {
    expect(rule(".composer-foot .btn.sm").get("height")).toBe("28px");
    expect(rule(".composer-foot .btn.sm.icon-only").get("width")).toBe("28px");
    expect(rule(".runs-on-size").get("height")).toBe("28px");
    expect(rule(".runs-on").get("flex")).toBe("none");
    expect(rule(".composer-hint").get("flex")).toBe("1");
  });
});
