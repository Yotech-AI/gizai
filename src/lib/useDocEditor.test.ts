// GA-118: when a doc or a memory note (useDocEditor, shared by the doc page and the Memory page) saves a version. Ctrl+S /
// Ctrl+Enter and the Save button save one; leaving the editor doesn't (there is no blur any more); unsaved changes save on
// their own after AUTOSAVE_MS (5 minutes) without typing, each change restarting the wait; leaving the page still saves.
// The automatic save never runs while a save is under way or while the conflict bar shows, and it isn't forced: an
// agent's newer version still brings up the conflict bar. A small hook runtime stands in for React, and fake timers for
// the clock; the api is a fake that records the saves.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Doc } from "../types";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
const h = vi.hoisted(() => ({
  cur: null as Inst | null,
  dirty: false,
  effects: [] as (() => void)[],
  doc: null as unknown,
  saves: [] as { id: string; md: string; base: number; settle: { ok: (v: number) => void; fail: (e: unknown) => void } }[],
  gets: 0,
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
vi.mock("./useData", () => ({ useData: () => ({ data: h.doc, error: null, reload: () => {}, setData: () => {} }) }));
vi.mock("../api", () => ({
  getDoc: () => { h.gets++; return Promise.resolve(h.doc); },
  // A save waits until the test settles it: ok(version) or fail(error).
  saveDoc: (id: string, md: string, base: number) => new Promise<number>((ok, fail) => { h.saves.push({ id, md, base, settle: { ok, fail } }); }),
}));

const { AUTOSAVE_MS, autosaveStep, autosaveTimer, useDocEditor } = await import("./useDocEditor");

// ---- the hook runtime: one useDocEditor ----
let inst: Inst = { slots: [], i: 0 };
let ed!: ReturnType<typeof useDocEditor>;
function render() {
  for (let n = 0; n < 30; n++) {
    h.dirty = false;
    inst.i = 0;
    h.cur = inst;
    try { ed = useDocEditor("d1"); } finally { h.cur = null; }
    for (const f of h.effects.splice(0)) f();
    if (!h.dirty) return;
  }
  throw new Error("the render never settled");
}
const unmount = () => { for (const s of inst.slots) if ((s as EffectSlot)?.effect) (s as EffectSlot).cleanup?.(); };
/** Lets promises run, then renders what changed. */
async function settle() { await vi.advanceTimersByTimeAsync(0); render(); }
/** The clock moves on `ms`; whatever fired renders. */
async function wait(ms: number) { await vi.advanceTimersByTimeAsync(ms); render(); }
const type = (md: string) => { ed.edit(md); render(); };
/** The last save asked for is answered with the next version (or `version`). */
async function answer(version?: number) { const s = h.saves.at(-1)!; s.settle.ok(version ?? s.base + 1); await settle(); }
const MIN = 60 * 1000;

const doc = (more: Partial<Doc> = {}): Doc => ({ id: "d1", title: "Notes", bodyMd: "# Notes\n", currentVersion: 4, updatedAt: 0, ...more });

beforeEach(() => {
  vi.useFakeTimers();
  inst = { slots: [], i: 0 };
  h.effects.length = 0;
  h.saves.length = 0;
  h.gets = 0;
  h.doc = doc();
  render();
});
afterEach(() => { vi.useRealTimers(); });

describe("autosaveTimer and autosaveStep", () => {
  it("wait 5 minutes, in one exported constant", () => {
    expect(AUTOSAVE_MS).toBe(5 * MIN);
  });

  it("fire once, AUTOSAVE_MS after the last restart, and not after a cancel", () => {
    const fire = vi.fn();
    const t = autosaveTimer(fire);
    expect(t.pending()).toBe(false);
    t.restart();
    expect(t.pending()).toBe(true);
    vi.advanceTimersByTime(4 * MIN);
    t.restart(); // typing again restarts the wait
    vi.advanceTimersByTime(AUTOSAVE_MS - 1);
    expect(fire).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fire).toHaveBeenCalledTimes(1);
    expect(t.pending()).toBe(false);
    vi.advanceTimersByTime(30 * MIN);
    expect(fire).toHaveBeenCalledTimes(1);
    t.restart();
    t.cancel();
    expect(t.pending()).toBe(false);
    vi.advanceTimersByTime(30 * MIN);
    expect(fire).toHaveBeenCalledTimes(1);
  });

  it("take another wait in ms when given one", () => {
    const fire = vi.fn();
    autosaveTimer(fire, 1000).restart();
    vi.advanceTimersByTime(999);
    expect(fire).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fire).toHaveBeenCalledTimes(1);
  });

  it("save unsaved text, wait while a save is under way, and do nothing when saved or in conflict", () => {
    expect(autosaveStep("unsaved")).toBe("save");
    expect(autosaveStep("saving")).toBe("wait");
    expect(autosaveStep("saved")).toBe("none");
    expect(autosaveStep("conflict")).toBe("none");
  });
});

describe("useDocEditor saves", () => {
  it("starts with the doc's text and version, saved", () => {
    expect(ed.text).toBe("# Notes\n");
    expect(ed.base).toBe(4);
    expect(ed.status).toBe("saved");
  });

  it("no longer has a save on leaving the editor", () => {
    expect("blur" in ed).toBe(false);
  });

  it("saves unsaved changes as one version after 5 minutes without typing, not before", async () => {
    type("# Notes\nMore");
    expect(ed.status).toBe("unsaved");
    await wait(AUTOSAVE_MS - 1);
    expect(h.saves).toHaveLength(0);
    expect(ed.status).toBe("unsaved");
    await wait(1);
    expect(h.saves.map(({ id, md, base }) => ({ id, md, base }))).toEqual([{ id: "d1", md: "# Notes\nMore", base: 4 }]);
    expect(ed.status).toBe("saving");
    await answer();
    expect(ed.status).toBe("saved");
    expect(ed.base).toBe(5);
    await wait(60 * MIN);
    expect(h.saves).toHaveLength(1);
  });

  it("restarts the wait with every change, then saves the last text once", async () => {
    type("# Notes\nA");
    await wait(4 * MIN);
    type("# Notes\nAB");
    await wait(4 * MIN);
    type("# Notes\nABC");
    await wait(AUTOSAVE_MS - 1);
    expect(h.saves).toHaveLength(0);
    await wait(1);
    expect(h.saves.map((s) => s.md)).toEqual(["# Notes\nABC"]);
  });

  it("makes no version for any time without the 5 minutes passing: leaving the editor doesn't save", async () => {
    type("# Notes\nTyped");
    // A click on the title, the history or another window: nothing reaches useDocEditor any more.
    await wait(AUTOSAVE_MS - 1000);
    expect(h.saves).toHaveLength(0);
    expect(ed.status).toBe("unsaved");
  });

  it("doesn't save on its own when the text is back to the saved version", async () => {
    type("# Notes\nOops");
    type("# Notes\n");
    expect(ed.status).toBe("saved");
    await wait(10 * MIN);
    expect(h.saves).toHaveLength(0);
  });

  it("saves one version on Save or Ctrl+S, and then not again after 5 minutes", async () => {
    type("# Notes\nNow");
    await wait(2 * MIN);
    ed.save(ed.text!); // the Save button and Ctrl+S both call save
    render();
    expect(h.saves).toHaveLength(1);
    expect(ed.status).toBe("saving");
    await answer();
    expect(ed.status).toBe("saved");
    expect(ed.base).toBe(5);
    await wait(10 * MIN);
    expect(h.saves).toHaveLength(1);
  });

  it("makes no version when Save is pressed with nothing to save", async () => {
    ed.save(ed.text!);
    await settle();
    expect(h.saves).toHaveLength(0);
    expect(ed.status).toBe("saved");
  });

  it("waits while a save is under way, then saves what was typed meanwhile", async () => {
    type("# Notes\nOne");
    ed.save("# Notes\nOne");
    render();
    type("# Notes\nOne two"); // typed while saving
    expect(ed.status).toBe("saving");
    await wait(AUTOSAVE_MS);
    expect(h.saves).toHaveLength(1); // the wait is over while saving: no second save at once
    await answer();
    expect(ed.status).toBe("unsaved");
    await wait(AUTOSAVE_MS);
    expect(h.saves.map((s) => [s.md, s.base])).toEqual([["# Notes\nOne", 4], ["# Notes\nOne two", 5]]);
  });

  it("doesn't force its save: an agent's newer version brings up the conflict bar, and nothing saves over it", async () => {
    type("# Notes\nMine");
    h.doc = doc({ currentVersion: 5, bodyMd: "Agent text" }); // an agent saved meanwhile
    render();
    expect(ed.text).toBe("# Notes\nMine"); // unsaved text isn't replaced
    await wait(AUTOSAVE_MS);
    expect(h.saves).toHaveLength(1);
    expect(h.saves[0].base).toBe(4); // based on our version, not the agent's
    expect(h.gets).toBe(0); // a forced save reads the current version first
    h.saves[0].settle.fail(new Error("the doc changed since version 4"));
    await settle();
    expect(ed.status).toBe("conflict");
    type("# Notes\nMine and more");
    expect(ed.status).toBe("conflict");
    await wait(30 * MIN);
    expect(h.saves).toHaveLength(1);
    expect(ed.status).toBe("conflict");
  });

  it("shows the conflict bar when Save is pressed after an agent's save, and the automatic save waits on it", async () => {
    type("# Notes\nMine");
    ed.save(ed.text!);
    render();
    h.saves[0].settle.fail(new Error("the doc changed since version 4"));
    await settle();
    expect(ed.status).toBe("conflict");
    await wait(AUTOSAVE_MS * 3);
    expect(h.saves).toHaveLength(1);
    // Save mine as a new version: a forced save on the current version.
    h.doc = doc({ currentVersion: 5, bodyMd: "Agent text" });
    ed.replace(ed.text!, true);
    await settle();
    expect(h.saves).toHaveLength(2);
    expect(h.saves[1].base).toBe(5);
  });

  it("adopts an agent's newer version while nothing is unsaved", () => {
    h.doc = doc({ currentVersion: 6, bodyMd: "Agent text" });
    render();
    expect(ed.text).toBe("Agent text");
    expect(ed.base).toBe(6);
    expect(ed.status).toBe("saved");
  });

  it("saves unsaved changes as one version on leaving the page, and not again after 5 minutes", async () => {
    type("# Notes\nLeaving");
    await wait(MIN);
    unmount();
    expect(h.saves.map((s) => s.md)).toEqual(["# Notes\nLeaving"]);
    await vi.advanceTimersByTimeAsync(30 * MIN);
    expect(h.saves).toHaveLength(1);
  });

  it("saves nothing on leaving the page when all is saved", async () => {
    unmount();
    await vi.advanceTimersByTimeAsync(30 * MIN);
    expect(h.saves).toHaveLength(0);
  });

  it("saves nothing more on leaving the page while a save is under way", async () => {
    type("# Notes\nSaving");
    ed.save(ed.text!);
    render();
    unmount();
    await vi.advanceTimersByTimeAsync(30 * MIN);
    expect(h.saves).toHaveLength(1);
  });
});
