// GA-122: closing Gizai with unsaved text in a doc or a memory note (useDocEditor, shared by the doc page and the Memory
// page). The editor is on closeSave's list while it is open; closing saves its unsaved text as one version (not forced, so
// an agent's newer version still counts as a conflict), waits for a save already under way, and names the doc or the note
// when the save fails or takes longer than CLOSE_SAVE_MS, leaving the text and "Unsaved changes" as they were. Nothing
// unsaved: nothing is saved. The same small hook runtime as useDocEditor.test.ts stands in for React, fake timers for the
// clock, and a fake api records the saves.
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

const { AUTOSAVE_MS, useDocEditor } = await import("./useDocEditor");
const { CLOSE_SAVE_MS, anyUnsaved, onUnsavedChange, openEdits, saveBeforeClose } = await import("./closeSave");

// ---- the hook runtime: one useDocEditor ----
let inst: Inst = { slots: [], i: 0 };
let ed!: ReturnType<typeof useDocEditor>;
let mounted = false;
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
const unmount = () => { mounted = false; for (const s of inst.slots) if ((s as EffectSlot)?.effect) (s as EffectSlot).cleanup?.(); };
/** Lets promises run, then renders what changed (while the editor is open). */
async function settle() { await vi.advanceTimersByTimeAsync(0); if (mounted) render(); }
const type = (md: string) => { ed.edit(md); render(); };
/** The last save asked for is answered with the next version (or `version`). */
async function answer(version?: number) { const s = h.saves.at(-1)!; s.settle.ok(version ?? s.base + 1); await settle(); }
async function refuse(error: string) { h.saves.at(-1)!.settle.fail(new Error(error)); await settle(); }
/** Whether the promise has settled yet, after the promises that can run have run. */
async function settled<T>(p: Promise<T>) {
  let done = false;
  p.then(() => { done = true; }, () => { done = true; });
  await vi.advanceTimersByTimeAsync(0);
  return done;
}
const MIN = 60 * 1000;

const doc = (more: Partial<Doc> = {}): Doc => ({ id: "d1", title: "Notes", bodyMd: "# Notes\n", currentVersion: 4, updatedAt: 0, ...more });

beforeEach(() => {
  vi.useFakeTimers();
  inst = { slots: [], i: 0 };
  h.effects.length = 0;
  h.saves.length = 0;
  h.gets = 0;
  h.doc = doc();
  expect(openEdits()).toHaveLength(0); // the last test's editor is off the list
  mounted = true;
  render();
});
afterEach(async () => {
  if (mounted) unmount();
  // Saves still under way end, so the editor leaves the list.
  for (const s of h.saves) s.settle.ok(s.base + 1);
  await vi.advanceTimersByTimeAsync(0);
  vi.useRealTimers();
});

describe("useDocEditor on closeSave's list", () => {
  it("is listed while open, with nothing unsaved", () => {
    expect(openEdits()).toHaveLength(1);
    expect(anyUnsaved()).toBe(false);
  });

  it("has unsaved text after typing, and none once it is back to the saved text", () => {
    type("# Notes\nMore");
    expect(anyUnsaved()).toBe(true);
    type("# Notes\n");
    expect(anyUnsaved()).toBe(false);
  });

  it("tells the listener (CloseGuard) when there is unsaved text and when it has saved", async () => {
    const heard: boolean[] = [];
    const stop = onUnsavedChange((u) => heard.push(u));
    expect(heard).toEqual([false]);
    type("# Notes\nMore");
    expect(heard).toEqual([false, true]);
    ed.save(ed.text!);
    render();
    expect(heard).toEqual([false, true]); // a save under way still counts as unsaved
    await answer();
    expect(ed.status).toBe("saved");
    expect(heard).toEqual([false, true, false]);
    stop();
  });

  it("is off the list after leaving the page with nothing unsaved", () => {
    unmount();
    expect(openEdits()).toHaveLength(0);
  });

  it("stays on the list after leaving the page until the save on leaving has ended", async () => {
    type("# Notes\nLeaving");
    unmount();
    expect(h.saves.map((s) => s.md)).toEqual(["# Notes\nLeaving"]);
    expect(openEdits()).toHaveLength(1);
    expect(anyUnsaved()).toBe(true);
    // Gizai closes meanwhile: it waits for that save and makes no second version.
    const p = saveBeforeClose();
    expect(await settled(p)).toBe(false);
    await answer();
    await expect(p).resolves.toEqual({ saved: true });
    expect(h.saves).toHaveLength(1);
    expect(openEdits()).toHaveLength(0);
  });
});

describe("closing Gizai", () => {
  it("saves nothing and goes on when nothing is unsaved", async () => {
    await expect(saveBeforeClose()).resolves.toEqual({ saved: true });
    expect(h.saves).toHaveLength(0);
    expect(ed.status).toBe("saved");
  });

  it("saves the unsaved text as one version on the version it is based on, well within the 5 minutes", async () => {
    type("# Notes\nTyped then closed");
    await vi.advanceTimersByTimeAsync(MIN);
    render();
    const p = saveBeforeClose();
    expect(h.saves.map(({ id, md, base }) => ({ id, md, base }))).toEqual([{ id: "d1", md: "# Notes\nTyped then closed", base: 4 }]);
    expect(h.gets).toBe(0); // not forced
    render();
    expect(ed.status).toBe("saving");
    await answer();
    await expect(p).resolves.toEqual({ saved: true });
    expect(ed.status).toBe("saved");
    expect(ed.base).toBe(5);
    expect(anyUnsaved()).toBe(false);
    // The automatic save has nothing left to do.
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS * 2);
    render();
    expect(h.saves).toHaveLength(1);
  });

  it("waits for a save already under way, and makes no second version of the same text", async () => {
    type("# Notes\nSaving");
    ed.save(ed.text!);
    render();
    const p = saveBeforeClose();
    expect(await settled(p)).toBe(false);
    await answer();
    await expect(p).resolves.toEqual({ saved: true });
    expect(h.saves.map((s) => [s.md, s.base])).toEqual([["# Notes\nSaving", 4]]);
  });

  it("after a save under way, saves what was typed meanwhile on the new version", async () => {
    type("# Notes\nOne");
    ed.save("# Notes\nOne");
    render();
    type("# Notes\nOne two");
    const p = saveBeforeClose();
    await answer(); // version 5
    expect(h.saves.map((s) => [s.md, s.base])).toEqual([["# Notes\nOne", 4], ["# Notes\nOne two", 5]]);
    await answer();
    await expect(p).resolves.toEqual({ saved: true });
    expect(ed.status).toBe("saved");
    expect(ed.base).toBe(6);
  });

  it("names the doc when the save fails, and keeps the text and Unsaved changes (Stay)", async () => {
    type("# Notes\nMine");
    const p = saveBeforeClose();
    await refuse("disk full");
    await expect(p).resolves.toEqual({ saved: false, title: "Notes" });
    expect(ed.text).toBe("# Notes\nMine");
    expect(ed.status).toBe("unsaved");
    expect(ed.err).toContain("disk full");
    expect(anyUnsaved()).toBe(true);
    // Closing again tries again.
    const again = saveBeforeClose();
    expect(h.saves).toHaveLength(2);
    await answer();
    await expect(again).resolves.toEqual({ saved: true });
    expect(ed.status).toBe("saved");
  });

  it("names the doc when the save takes longer than 2 s, and the text stays", async () => {
    type("# Notes\nSlow");
    const p = saveBeforeClose();
    await vi.advanceTimersByTimeAsync(CLOSE_SAVE_MS - 1);
    expect(await settled(p)).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await expect(p).resolves.toEqual({ saved: false, title: "Notes" });
    render();
    expect(ed.text).toBe("# Notes\nSlow");
    expect(ed.status).toBe("saving");
    expect(anyUnsaved()).toBe(true);
    await answer(); // it ends later, as one version
    expect(ed.status).toBe("saved");
    expect(h.saves).toHaveLength(1);
  });

  it("doesn't save over an agent's newer version: the conflict counts as not saved", async () => {
    type("# Notes\nMine");
    h.doc = doc({ currentVersion: 5, bodyMd: "Agent text" });
    render();
    const p = saveBeforeClose();
    expect(h.saves[0].base).toBe(4);
    expect(h.gets).toBe(0);
    await refuse("the doc changed since version 4");
    await expect(p).resolves.toEqual({ saved: false, title: "Notes" });
    expect(ed.status).toBe("conflict");
    expect(ed.text).toBe("# Notes\nMine");
    expect(anyUnsaved()).toBe(true);
  });

  it("names a memory note by its title from its path", async () => {
    unmount();
    h.doc = doc({ kind: "memory", path: "Decisions/Use SQLite", title: "Use SQLite.md" });
    inst = { slots: [], i: 0 };
    mounted = true;
    render();
    type("# Notes\nA key");
    const p = saveBeforeClose();
    await refuse("Memory doesn't keep secrets");
    await expect(p).resolves.toEqual({ saved: false, title: "Use SQLite" });
  });

  it("makes no version on its own in the 5 minutes when Gizai doesn't close (minimising, switching windows)", async () => {
    type("# Notes\nStill here");
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS - 1);
    render();
    expect(h.saves).toHaveLength(0);
    expect(ed.status).toBe("unsaved");
  });
});
