// GA-122: CloseGuard, which answers Gizai's close and quit requests while an editor has unsaved text. It tells Gizai whether
// there is unsaved text; on a close (the window's close button) or a quit it saves first (saveBeforeClose) and answers
// "close", or, when that save failed or took too long, shows "Close Gizai without saving your changes to <title>?" with
// Close and Stay (Stay focused, Escape means Stay) and answers "asking", then the button's answer. A small hook runtime
// stands in for React (refs are attached to fake nodes before effects run); the api and saveBeforeClose are fakes.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { ReactElement, ReactNode } from "react";
import type { CloseSave } from "../lib/closeSave";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
const h = vi.hoisted(() => ({
  cur: null as Inst | null,
  dirty: false,
  effects: [] as (() => void)[],
  told: [] as boolean[],
  answers: [] as string[],
  onClose: [] as (() => void)[],
  onQuit: [] as (() => void)[],
  unlistened: 0,
  saves: [] as { done: (r: CloseSave) => void }[],
}));

vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const slot = <T,>(make: () => T): T => {
    const c = h.cur!;
    if (c.i === c.slots.length) c.slots.push(make());
    return c.slots[c.i++] as T;
  };
  const changed = (deps?: unknown[], prev?: unknown[]) => !deps || !prev || deps.length !== prev.length || deps.some((d, i) => !Object.is(d, prev[i]));
  const useState = ((init: unknown) => {
    const box = slot(() => {
      const b = { v: typeof init === "function" ? (init as () => unknown)() : init, set: (x: unknown) => {
        const v = typeof x === "function" ? (x as (p: unknown) => unknown)(b.v) : x;
        if (!Object.is(v, b.v)) { b.v = v; h.dirty = true; }
      } };
      return b;
    });
    return [box.v, box.set];
  }) as typeof R.useState;
  const useRef = ((init: unknown) => slot(() => ({ current: init }))) as typeof R.useRef;
  const useEffect = ((f: () => void | (() => void), deps?: unknown[]) => {
    const s = slot<EffectSlot>(() => ({ effect: true, first: true }));
    if (s.first || changed(deps, s.deps)) {
      s.first = false;
      s.deps = deps;
      h.effects.push(() => { s.cleanup?.(); const c = f(); s.cleanup = typeof c === "function" ? c : undefined; });
    }
  }) as typeof R.useEffect;
  const hooks = { useState, useRef, useEffect };
  return { ...R, default: { ...R, ...hooks }, ...hooks };
});
vi.mock("../api", () => ({
  setUnsavedEdits: (u: boolean) => { h.told.push(u); return Promise.resolve(); },
  closeAnswer: (a: string) => { h.answers.push(a); return Promise.resolve(); },
  onCloseRequested: (cb: () => void) => { h.onClose.push(cb); return Promise.resolve(() => { h.unlistened++; }); },
  onQuitRequested: (cb: () => void) => { h.onQuit.push(cb); return Promise.resolve(() => { h.unlistened++; }); },
}));
vi.mock("../lib/closeSave", async (orig) => ({
  ...(await orig<typeof import("../lib/closeSave")>()),
  // A save before closing waits until the test says how it went.
  saveBeforeClose: () => new Promise<CloseSave>((done) => { h.saves.push({ done }); }),
}));

const { CloseGuard } = await import("./CloseGuard");
const { registerEdit } = await import("../lib/closeSave");

// ---- the hook runtime ----
type El = ReactElement<{ children?: ReactNode; ref?: { current: unknown }; onClick?: () => void; onKeyDown?: (e: unknown) => void; className?: string; role?: string } & Record<string, unknown>>;
type Node = { focused: number; focus: () => void; label: string };
let inst: Inst = { slots: [], i: 0 };
let out: El | null = null;
let nodes: Node[] = [];
const elements = (n: ReactNode): El[] => {
  if (Array.isArray(n)) return n.flatMap(elements);
  if (!n || typeof n !== "object" || !("props" in n)) return [];
  const el = n as El;
  return [el, ...elements(el.props.children)];
};
const textOf = (n: ReactNode): string => (typeof n === "string" || typeof n === "number" ? String(n) : Array.isArray(n) ? n.map(textOf).join("") : n && typeof n === "object" && "props" in n ? textOf((n as El).props.children) : "");
function render() {
  for (let n = 0; n < 30; n++) {
    h.dirty = false;
    inst.i = 0;
    h.cur = inst;
    try { out = CloseGuard() as El | null; } finally { h.cur = null; }
    // React attaches refs before effects run.
    nodes = [];
    for (const el of elements(out)) {
      if (el.props.ref) { const node: Node = { focused: 0, focus() { node.focused++; }, label: textOf(el.props.children) }; el.props.ref.current = node; nodes.push(node); }
    }
    for (const f of h.effects.splice(0)) f();
    if (!h.dirty) return;
  }
  throw new Error("the render never settled");
}
const unmount = () => { for (const s of inst.slots) if ((s as EffectSlot)?.effect) (s as EffectSlot).cleanup?.(); };
async function settle() { await vi.advanceTimersByTimeAsync(0); render(); }
const button = (name: string) => elements(out).find((e) => e.type === "button" && textOf(e.props.children) === name);
const dialog = () => elements(out).find((e) => e.props.role === "alertdialog");
const html = () => (out ? renderToStaticMarkup(out) : "");
/** The last save before closing ends with `r`. */
async function saved(r: CloseSave) { h.saves.at(-1)!.done(r); await settle(); }

const offs: (() => void)[] = [];
beforeEach(() => {
  vi.useFakeTimers();
  inst = { slots: [], i: 0 };
  h.effects.length = 0; h.told.length = 0; h.answers.length = 0; h.onClose.length = 0; h.onQuit.length = 0; h.saves.length = 0;
  h.unlistened = 0;
  render();
});
afterEach(() => { unmount(); for (const off of offs.splice(0)) off(); vi.useRealTimers(); });

describe("CloseGuard tells Gizai", () => {
  it("at once that nothing is unsaved, and again each time that changes", () => {
    expect(h.told).toEqual([false]);
    let dirty = true;
    const off = registerEdit({ title: () => "Notes", unsaved: () => dirty, flush: async () => true });
    expect(h.told).toEqual([false, true]);
    dirty = false;
    off();
    expect(h.told).toEqual([false, true, false]);
  });

  it("listens for the window's close request and for a quit, and stops when it goes", async () => {
    expect(h.onClose).toHaveLength(1);
    expect(h.onQuit).toHaveLength(1);
    unmount();
    await vi.advanceTimersByTimeAsync(0);
    expect(h.unlistened).toBe(2);
    const off = registerEdit({ title: () => "Notes", unsaved: () => true, flush: async () => true });
    offs.push(off);
    expect(h.told).toEqual([false]); // no longer told
    inst = { slots: [], i: 0 };
    render(); // so afterEach's unmount has something to undo
  });
});

describe("a close or a quit", () => {
  it("saves first and then answers close, showing nothing", async () => {
    h.onClose[0]();
    expect(h.saves).toHaveLength(1);
    expect(h.answers).toEqual([]); // not before the save has ended
    await saved({ saved: true });
    expect(h.answers).toEqual(["close"]);
    expect(out).toBeNull();
  });

  it("a quit does the same", async () => {
    h.onQuit[0]();
    await saved({ saved: true });
    expect(h.answers).toEqual(["close"]);
    expect(out).toBeNull();
  });

  it("asks when the save failed or took too long, says so to Gizai, and focuses Stay", async () => {
    h.onQuit[0]();
    await saved({ saved: false, title: "Requirements" });
    expect(h.answers).toEqual(["asking"]);
    expect(dialog()?.props["aria-modal"]).toBe("true");
    expect(textOf(elements(out).find((e) => e.type === "h2")?.props.children)).toBe("Close Gizai without saving your changes to Requirements?");
    expect(html()).toContain("Saving them failed or took too long. Stay keeps them in the editor, unsaved.");
    expect(button("Close")).toBeTruthy();
    expect(button("Stay")).toBeTruthy();
    expect(nodes.find((n) => n.label === "Stay")?.focused).toBe(1);
  });

  it("Stay: answers stay and the question goes; the next close saves again", async () => {
    h.onClose[0]();
    await saved({ saved: false, title: "Notes" });
    button("Stay")!.props.onClick!();
    render();
    expect(h.answers).toEqual(["asking", "stay"]);
    expect(out).toBeNull();
    h.onClose[0]();
    expect(h.saves).toHaveLength(2);
    await saved({ saved: true });
    expect(h.answers).toEqual(["asking", "stay", "close"]);
  });

  it("Close: answers close and the question goes", async () => {
    h.onClose[0]();
    await saved({ saved: false, title: "Notes" });
    button("Close")!.props.onClick!();
    render();
    expect(h.answers).toEqual(["asking", "close"]);
    expect(out).toBeNull();
  });

  it("Escape means Stay", async () => {
    h.onQuit[0]();
    await saved({ saved: false, title: "Notes" });
    const e = { key: "Escape", preventDefault: vi.fn() };
    dialog()!.props.onKeyDown!(e);
    render();
    expect(e.preventDefault).toHaveBeenCalled();
    expect(h.answers).toEqual(["asking", "stay"]);
    expect(out).toBeNull();
  });

  it("other keys don't answer", async () => {
    h.onQuit[0]();
    await saved({ saved: false, title: "Notes" });
    dialog()!.props.onKeyDown!({ key: "Enter", preventDefault: vi.fn() });
    render();
    expect(h.answers).toEqual(["asking"]);
    expect(dialog()).toBeTruthy();
  });

  it("a second request while it saves or asks doesn't save again", async () => {
    h.onClose[0]();
    h.onQuit[0]();
    h.onClose[0]();
    expect(h.saves).toHaveLength(1);
    await saved({ saved: false, title: "Notes" });
    h.onQuit[0]();
    expect(h.saves).toHaveLength(1);
    expect(h.answers).toEqual(["asking"]);
  });
});
