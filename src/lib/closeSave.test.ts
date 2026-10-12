// GA-122: closing Gizai with unsaved text in an editor. The list of open editors, who hears whether any has unsaved text,
// and saveBeforeClose: it saves each unsaved editor once, waits at most CLOSE_SAVE_MS (2 s) for all of them, and names the
// first one that didn't save (failed or too slow) so CloseGuard can ask. Fake editors stand in for useDocEditor, fake
// timers for the clock.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CLOSE_SAVE_MS, anyUnsaved, closeQuestion, editsChanged, onUnsavedChange, openEdits, registerEdit, saveBeforeClose, type OpenEdit } from "./closeSave";

type Fake = OpenEdit & { dirty: boolean; flushes: number; answer: (ok: boolean) => void; reject: (e: unknown) => void };
/** An editor whose flush waits until the test answers it (or answers at once with `auto`). */
function fake(title: string, dirty = true, auto?: boolean): Fake {
  let settle: { ok: (v: boolean) => void; fail: (e: unknown) => void } | null = null;
  const f: Fake = {
    dirty, flushes: 0,
    title: () => title,
    unsaved: () => f.dirty,
    flush: () => {
      f.flushes++;
      if (auto !== undefined) { if (auto) f.dirty = false; return Promise.resolve(auto); }
      return new Promise<boolean>((ok, fail) => { settle = { ok, fail }; });
    },
    answer: (ok) => { if (ok) f.dirty = false; settle!.ok(ok); },
    reject: (e) => settle!.fail(e),
  };
  return f;
}
/** Whether the promise has settled yet, after the promises that can run have run. */
async function settled<T>(p: Promise<T>) {
  let done = false;
  p.then(() => { done = true; }, () => { done = true; });
  await vi.advanceTimersByTimeAsync(0);
  return done;
}

const offs: (() => void)[] = [];
beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { for (const off of offs.splice(0)) off(); vi.useRealTimers(); });
const listed = (e: OpenEdit) => { offs.push(registerEdit(e)); return e; };

describe("the question and the wait", () => {
  it("waits 2 seconds, in one exported constant", () => {
    expect(CLOSE_SAVE_MS).toBe(2000);
  });
  it("asks with the doc's or the note's title", () => {
    expect(closeQuestion("Requirements")).toBe("Close Gizai without saving your changes to Requirements?");
    expect(closeQuestion("Use SQLite")).toBe("Close Gizai without saving your changes to Use SQLite?");
  });
});

describe("saveBeforeClose", () => {
  it("goes on at once and saves nothing when no editor has unsaved text", async () => {
    const a = fake("A", false), b = fake("B", false);
    await expect(saveBeforeClose([a, b])).resolves.toEqual({ saved: true });
    expect(a.flushes + b.flushes).toBe(0);
    await expect(saveBeforeClose([])).resolves.toEqual({ saved: true });
  });

  it("saves only the editors with unsaved text, once each, and goes on when they saved", async () => {
    const a = fake("A"), b = fake("B", false), c = fake("C");
    const p = saveBeforeClose([a, b, c]);
    expect([a.flushes, b.flushes, c.flushes]).toEqual([1, 0, 1]);
    a.answer(true);
    expect(await settled(p)).toBe(false); // C still saves
    c.answer(true);
    await expect(p).resolves.toEqual({ saved: true });
    expect([a.flushes, b.flushes, c.flushes]).toEqual([1, 0, 1]);
  });

  it("names the editor whose save failed", async () => {
    const a = fake("A"), b = fake("Requirements");
    const p = saveBeforeClose([a, b]);
    a.answer(true);
    b.answer(false);
    await expect(p).resolves.toEqual({ saved: false, title: "Requirements" });
  });

  it("names the first one in the list when more failed, and a save that throws counts as failed", async () => {
    const a = fake("A"), b = fake("B"), c = fake("C");
    const p = saveBeforeClose([a, b, c]);
    c.answer(false);
    b.reject(new Error("disk full"));
    a.answer(true);
    await expect(p).resolves.toEqual({ saved: false, title: "B" });
  });

  it("waits at most 2 s: a save still under way is named after CLOSE_SAVE_MS, not before", async () => {
    const a = fake("Slow doc");
    const p = saveBeforeClose([a]);
    await vi.advanceTimersByTimeAsync(CLOSE_SAVE_MS - 1);
    expect(await settled(p)).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await expect(p).resolves.toEqual({ saved: false, title: "Slow doc" });
    a.answer(true); // the save ends later: nothing more happens
    await vi.advanceTimersByTimeAsync(0);
  });

  it("goes on when the save ends just inside the 2 s", async () => {
    const a = fake("A");
    const p = saveBeforeClose([a]);
    await vi.advanceTimersByTimeAsync(CLOSE_SAVE_MS - 1);
    a.answer(true);
    await expect(p).resolves.toEqual({ saved: true });
  });

  it("names the slow one when another saved in time", async () => {
    const a = fake("Fast"), b = fake("Slow");
    const p = saveBeforeClose([a, b]);
    a.answer(true);
    await vi.advanceTimersByTimeAsync(CLOSE_SAVE_MS);
    await expect(p).resolves.toEqual({ saved: false, title: "Slow" });
  });

  it("takes another wait when given one", async () => {
    const a = fake("A");
    const p = saveBeforeClose([a], 500);
    await vi.advanceTimersByTimeAsync(500);
    await expect(p).resolves.toEqual({ saved: false, title: "A" });
  });

  it("leaves no timer behind", async () => {
    await saveBeforeClose([fake("A", true, true)]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("saves the editors that are open now when given none", async () => {
    const a = listed(fake("A", true, true)), b = listed(fake("B", false));
    expect(openEdits()).toEqual(expect.arrayContaining([a, b]));
    await expect(saveBeforeClose()).resolves.toEqual({ saved: true });
    expect((a as Fake).flushes).toBe(1);
    expect((b as Fake).flushes).toBe(0);
  });
});

describe("the list of open editors and who hears about unsaved text", () => {
  it("lists an editor while it is open", () => {
    const a = fake("A");
    const off = registerEdit(a);
    expect(openEdits()).toContain(a);
    off();
    expect(openEdits()).not.toContain(a);
  });

  it("anyUnsaved: whether one of them has unsaved text", () => {
    expect(anyUnsaved([fake("A", false), fake("B", false)])).toBe(false);
    expect(anyUnsaved([fake("A", false), fake("B", true)])).toBe(true);
    expect(anyUnsaved([])).toBe(false);
  });

  it("tells at once, then only when the answer changes", () => {
    const heard: boolean[] = [];
    const stop = onUnsavedChange((u) => heard.push(u));
    expect(heard).toEqual([false]);
    const a = listed(fake("A", false));
    expect(heard).toEqual([false]);
    (a as Fake).dirty = true;
    editsChanged();
    editsChanged();
    expect(heard).toEqual([false, true]);
    const b = fake("B", true);
    const offB = registerEdit(b); // another editor with unsaved text: still true, nothing new to tell
    expect(heard).toEqual([false, true]);
    (a as Fake).dirty = false;
    editsChanged();
    expect(heard).toEqual([false, true]);
    offB(); // the last one with unsaved text goes: false
    expect(heard).toEqual([false, true, false]);
    stop();
    (a as Fake).dirty = true;
    editsChanged();
    expect(heard).toEqual([false, true, false]);
  });

  it("tells a new listener the current answer, and an old listener's stop doesn't stop the new one", () => {
    const a = listed(fake("A", true));
    const first: boolean[] = [], second: boolean[] = [];
    const stopFirst = onUnsavedChange((u) => first.push(u));
    const stopSecond = onUnsavedChange((u) => second.push(u));
    expect(first).toEqual([true]);
    expect(second).toEqual([true]);
    stopFirst();
    (a as Fake).dirty = false;
    editsChanged();
    expect(second).toEqual([true, false]);
    expect(first).toEqual([true]);
    stopSecond();
  });
});
