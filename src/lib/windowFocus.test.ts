// GA-177 QA: onWindowFocus (src/lib/windowFocus.ts) with fake timers. On Omarchy (Hyprland) the focus follows the mouse,
// so the window's focus fires again and again: a focus runs `fn` only when it last ran more than 5 seconds ago, whether
// that run came from a focus or from its interval; the interval keeps its timing and skips while the window is hidden.
// There is no DOM here: the window is a plain EventTarget and the document only says whether it is hidden.
// GA-190 QA: quiet while nobody looks. away() (hidden, or another window has the focus), onShown, whenShown, the
// interval's lighter `tick`, useClock and watchAway. The document is an EventTarget too (visibilitychange), says whether
// it has the focus, and its root keeps attributes; useClock runs in a small hook runtime standing in for React.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FOCUS_GAP, away, hidden, onShown, onWindowFocus, useClock, watchAway, whenShown } from "./windowFocus";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
const hk = vi.hoisted(() => ({ cur: null as { slots: unknown[]; i: number } | null, dirty: false, effects: [] as (() => void)[] }));
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const slot = <T,>(make: () => T): T => {
    const c = hk.cur!;
    if (c.i === c.slots.length) c.slots.push(make());
    return c.slots[c.i++] as T;
  };
  const changed = (deps?: unknown[], prev?: unknown[]) => !deps || !prev || deps.length !== prev.length || deps.some((d, i) => !Object.is(d, prev[i]));
  const useState = ((init: unknown) => {
    const box = slot(() => {
      const b = { v: typeof init === "function" ? (init as () => unknown)() : init, set: (x: unknown) => {
        const v = typeof x === "function" ? (x as (p: unknown) => unknown)(b.v) : x;
        if (!Object.is(v, b.v)) { b.v = v; hk.dirty = true; }
      } };
      return b;
    });
    return [box.v, box.set];
  }) as typeof R.useState;
  const useEffect = ((f: () => void | (() => void), deps?: unknown[]) => {
    const s = slot<EffectSlot>(() => ({ effect: true, first: true }));
    if (s.first || changed(deps, s.deps)) {
      s.first = false;
      s.deps = deps;
      hk.effects.push(() => { s.cleanup?.(); const c = f(); s.cleanup = typeof c === "function" ? c : undefined; });
    }
  }) as typeof R.useEffect;
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});

const win = new EventTarget();
vi.stubGlobal("window", win);
/** The root element: only its attributes (watchAway's data-away). */
const attrs = new Map<string, string>();
const root = { setAttribute: (k: string, v: string) => { attrs.set(k, v); }, removeAttribute: (k: string) => { attrs.delete(k); } };
const docEvents = new EventTarget();
let focused = true;
const doc = {
  hidden: false, documentElement: root, hasFocus: () => focused,
  addEventListener: docEvents.addEventListener.bind(docEvents), removeEventListener: docEvents.removeEventListener.bind(docEvents),
};
vi.stubGlobal("document", doc);
/** The window is hidden (closed to the tray) or shows again: WebKitGTK's visibilitychange. */
const setHidden = (on: boolean) => { doc.hidden = on; docEvents.dispatchEvent(new Event("visibilitychange")); };
/** Another window takes the focus (blur), or Gizai gets it back (focus). */
const blur = () => { focused = false; win.dispatchEvent(new Event("blur")); };

const NOW = Date.UTC(2026, 9, 14, 12, 0, 0);
const focus = () => win.dispatchEvent(new Event("focus"));
const stops: (() => void)[] = [];
/** A counted fn, with onWindowFocus started on it. */
function start(minGapMs?: number, everyMs?: number) {
  const fn = vi.fn();
  const stop = onWindowFocus(fn, minGapMs, everyMs);
  stops.push(stop);
  return Object.assign(fn, { stop });
}

beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(NOW); doc.hidden = false; focused = true; attrs.clear(); });
afterEach(() => { for (const s of stops.splice(0)) s(); vi.useRealTimers(); });

describe("a focus runs fn at most once every 5 seconds", () => {
  it("the gap is 5 seconds by default", () => {
    expect(FOCUS_GAP).toBe(5000);
    const fn = start();
    focus();
    vi.advanceTimersByTime(5000);
    focus();
    expect(fn).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    focus();
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("starting it runs nothing, and without everyMs there is no interval", () => {
    const fn = start();
    vi.advanceTimersByTime(10 * 60_000);
    expect(fn).not.toHaveBeenCalled();
  });

  it("the first focus runs fn at once (opening a page doesn't start the clock)", () => {
    const fn = start();
    focus();
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("20 focus events within 3 seconds run fn once, not 20 times", () => {
    const fn = start();
    for (let i = 0; i < 20; i++) { focus(); vi.advanceTimersByTime(150); }
    expect(Date.now() - NOW).toBe(3000);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("a focus 5 seconds or less after the last run does nothing; one more than 5 seconds after it runs at once", () => {
    const fn = start();
    focus();
    vi.advanceTimersByTime(2000);
    focus();
    vi.advanceTimersByTime(3000);
    focus();
    expect(fn).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    focus();
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("the gap counts from the last run, not from the last focus: a mouse that keeps moving still refreshes every 5 seconds", () => {
    const fn = start();
    // a focus every 500 ms for 16.5 seconds: runs at 0, 5.5, 11 and 16.5 s
    for (let i = 0; i <= 33; i++) { focus(); if (i < 33) vi.advanceTimersByTime(500); }
    expect(fn).toHaveBeenCalledTimes(4);
  });

  it("after a quiet while, the first focus runs at once again", () => {
    const fn = start();
    focus();
    vi.advanceTimersByTime(30 * 60_000);
    focus();
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("the gap can be set", () => {
    const fn = start(1000);
    focus();
    vi.advanceTimersByTime(1000);
    focus();
    expect(fn).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    focus();
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("a clock set back doesn't hold the focus off until it catches up", () => {
    const fn = start();
    focus();
    vi.setSystemTime(NOW - 60 * 60_000);
    focus();
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("each call keeps its own clock", () => {
    const a = start();
    focus();
    const b = start();
    vi.advanceTimersByTime(1000);
    focus();
    expect(a).toHaveBeenCalledTimes(1);
    expect(b).toHaveBeenCalledTimes(1);
  });
});

describe("with everyMs, the interval", () => {
  it("runs fn every everyMs whatever the focus did: a focus doesn't move it", () => {
    const fn = start(FOCUS_GAP, 10_000);
    vi.advanceTimersByTime(8000);
    focus(); // 8 s: runs
    expect(fn).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(2000); // 10 s: the interval runs, though the focus ran 2 s ago
    expect(fn).toHaveBeenCalledTimes(2);
    vi.advanceTimersByTime(10_000);
    expect(fn).toHaveBeenCalledTimes(3);
    vi.advanceTimersByTime(9999);
    expect(fn).toHaveBeenCalledTimes(3);
    vi.advanceTimersByTime(1);
    expect(fn).toHaveBeenCalledTimes(4);
  });

  it("an interval run counts for the gap: a focus 5 seconds or less after it does nothing, later it runs", () => {
    const fn = start(FOCUS_GAP, 10_000);
    vi.advanceTimersByTime(10_000);
    expect(fn).toHaveBeenCalledTimes(1);
    focus();
    vi.advanceTimersByTime(5000);
    focus();
    expect(fn).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    focus();
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("keeps a 30-second timing too", () => {
    const fn = start(FOCUS_GAP, 30_000);
    vi.advanceTimersByTime(29_999);
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fn).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(60_000);
    expect(fn).toHaveBeenCalledTimes(3);
  });

  it("skips while the window is hidden, and a skipped run doesn't count for the gap", () => {
    const fn = start(FOCUS_GAP, 10_000);
    doc.hidden = true;
    vi.advanceTimersByTime(20 * 60_000);
    expect(fn).not.toHaveBeenCalled();
    doc.hidden = false;
    focus(); // the first focus after the window shows again runs at once
    expect(fn).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(10_000); // and the interval runs again
    expect(fn).toHaveBeenCalledTimes(2);
  });
});

describe("the cleanup", () => {
  it("removes the focus listener and the interval", () => {
    const fn = start(FOCUS_GAP, 10_000);
    fn.stop();
    focus();
    vi.advanceTimersByTime(10 * 60_000);
    focus();
    expect(fn).not.toHaveBeenCalled();
  });

  it("leaves another call's listener and interval alone", () => {
    const a = start(FOCUS_GAP, 10_000);
    const b = start(FOCUS_GAP, 10_000);
    a.stop();
    focus();
    vi.advanceTimersByTime(10_000);
    expect(a).not.toHaveBeenCalled();
    expect(b).toHaveBeenCalledTimes(2);
  });
});

// ---------------- GA-190: quiet while nobody looks ----------------

describe("GA-190: away() and hidden()", () => {
  it("away: hidden, or shown without the focus; hidden: only hidden", () => {
    expect([away(), hidden()]).toEqual([false, false]);
    doc.hidden = true;
    expect([away(), hidden()]).toEqual([true, true]);
    doc.hidden = false;
    focused = false; // on another Hyprland workspace: shown for GTK, but another window has the focus
    expect([away(), hidden()]).toEqual([true, false]);
    doc.hidden = true;
    expect([away(), hidden()]).toEqual([true, true]);
  });

  it("a document that can't say whether it has the focus is in use while shown; without a document nothing is away", () => {
    vi.stubGlobal("document", { hidden: false });
    try {
      expect(away()).toBe(false);
      vi.stubGlobal("document", { hidden: true });
      expect(away()).toBe(true);
      vi.stubGlobal("document", undefined);
      expect([away(), hidden()]).toEqual([false, false]);
      expect(() => onShown(() => {})()).not.toThrow();
      expect(() => watchAway()()).not.toThrow();
    } finally { vi.stubGlobal("document", doc); }
  });
});

describe("GA-190: onShown", () => {
  it("runs fn each time the window shows again, not when it hides; stop removes it", () => {
    const fn = vi.fn();
    const stop = onShown(fn);
    setHidden(true);
    expect(fn).not.toHaveBeenCalled();
    setHidden(false);
    expect(fn).toHaveBeenCalledTimes(1);
    setHidden(true);
    setHidden(false);
    expect(fn).toHaveBeenCalledTimes(2);
    stop();
    setHidden(true);
    setHidden(false);
    expect(fn).toHaveBeenCalledTimes(2);
  });
});

describe("GA-190: whenShown (an event's reload)", () => {
  it("shown: each event reloads at once", () => {
    const fn = vi.fn();
    const w = whenShown(fn);
    w.run();
    w.run();
    expect(fn).toHaveBeenCalledTimes(2);
    w.stop();
  });

  it("hidden: many events, one reload when the window shows again; none while hidden", () => {
    const fn = vi.fn();
    const w = whenShown(fn);
    setHidden(true);
    for (let i = 0; i < 25; i++) w.run();
    expect(fn).not.toHaveBeenCalled();
    setHidden(false);
    expect(fn).toHaveBeenCalledTimes(1);
    // shown again without an event in between: nothing to reload
    setHidden(true);
    setHidden(false);
    expect(fn).toHaveBeenCalledTimes(1);
    w.stop();
  });

  it("only the window's showing reloads: an event while shown after hidden ones still reloads at once", () => {
    const fn = vi.fn();
    const w = whenShown(fn);
    setHidden(true);
    w.run();
    setHidden(false);
    w.run();
    expect(fn).toHaveBeenCalledTimes(2);
    w.stop();
  });

  it("stopped, a hidden event never reloads", () => {
    const fn = vi.fn();
    const w = whenShown(fn);
    setHidden(true);
    w.run();
    w.stop();
    setHidden(false);
    expect(fn).not.toHaveBeenCalled();
  });
});

describe("GA-190: onWindowFocus while nobody uses the window, and its lighter tick", () => {
  function startTick(everyMs = 30_000) {
    const fn = vi.fn(), tick = vi.fn();
    stops.push(onWindowFocus(fn, FOCUS_GAP, everyMs, tick));
    return { fn, tick };
  }

  it("with tick, the interval runs tick, not fn", () => {
    const { fn, tick } = startTick();
    vi.advanceTimersByTime(30_000);
    expect([fn.mock.calls.length, tick.mock.calls.length]).toEqual([0, 1]);
    vi.advanceTimersByTime(4 * 30_000);
    expect([fn.mock.calls.length, tick.mock.calls.length]).toEqual([0, 5]);
  });

  it("tick runs don't count for the gap: a focus just after one runs fn; the focus's own gap still holds", () => {
    const { fn, tick } = startTick();
    vi.advanceTimersByTime(28_000);
    focus(); // 28 s: fn
    vi.advanceTimersByTime(2000); // 30 s: tick
    expect([fn.mock.calls.length, tick.mock.calls.length]).toEqual([1, 1]);
    vi.advanceTimersByTime(2000);
    focus(); // 32 s: 4 s after fn ran: nothing
    expect(fn).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(2000);
    focus(); // 34 s: 6 s after fn, 4 s after the tick: fn
    expect(fn).toHaveBeenCalledTimes(2);
    // and a tick right after a focus still runs on its timing
    vi.advanceTimersByTime(26_000); // 60 s
    expect(tick).toHaveBeenCalledTimes(2);
  });

  it("the interval (tick or fn) skips while the window is hidden", () => {
    const { fn, tick } = startTick();
    const plain = start(FOCUS_GAP, 10_000);
    doc.hidden = true;
    vi.advanceTimersByTime(10 * 60_000);
    expect([fn.mock.calls.length, tick.mock.calls.length, plain.mock.calls.length]).toEqual([0, 0, 0]);
  });

  it("the interval (tick or fn) skips while the window is shown but another window has the focus", () => {
    const { fn, tick } = startTick();
    const plain = start(FOCUS_GAP, 10_000);
    blur();
    vi.advanceTimersByTime(10 * 60_000);
    expect([fn.mock.calls.length, tick.mock.calls.length, plain.mock.calls.length]).toEqual([0, 0, 0]);
    // the focus comes back: fn at once, then the intervals again
    focused = true;
    focus();
    expect([fn.mock.calls.length, plain.mock.calls.length]).toEqual([1, 1]);
    vi.advanceTimersByTime(30_000);
    expect([tick.mock.calls.length, plain.mock.calls.length]).toEqual([1, 4]);
  });

  it("the window showing again runs fn at once (as a focus does), within the same gap", () => {
    const { fn } = startTick();
    const plain = start(FOCUS_GAP, 10_000);
    setHidden(true);
    vi.advanceTimersByTime(5 * 60_000);
    expect([fn.mock.calls.length, plain.mock.calls.length]).toEqual([0, 0]);
    setHidden(false);
    expect([fn.mock.calls.length, plain.mock.calls.length]).toEqual([1, 1]);
    // the focus that comes with it, 1 s later, runs nothing more
    vi.advanceTimersByTime(1000);
    focus();
    expect([fn.mock.calls.length, plain.mock.calls.length]).toEqual([1, 1]);
    // hidden again and shown 3 s after: within the gap, nothing
    setHidden(true);
    vi.advanceTimersByTime(2000);
    setHidden(false);
    expect([fn.mock.calls.length, plain.mock.calls.length]).toEqual([1, 1]);
  });

  it("the cleanup also stops the run on the window's showing", () => {
    const fn = vi.fn();
    const stop = onWindowFocus(fn, FOCUS_GAP, 10_000, () => {});
    stop();
    setHidden(true);
    setHidden(false);
    focus();
    vi.advanceTimersByTime(60_000);
    expect(fn).not.toHaveBeenCalled();
  });
});

// ---- useClock in a small hook runtime: one component that only reads the clock ----
function mountClock(everyMs: number) {
  const inst: Inst = { slots: [], i: 0 };
  let now = 0;
  const render = () => {
    for (let n = 0; n < 10; n++) {
      hk.dirty = false;
      inst.i = 0;
      hk.cur = inst;
      try { now = useClock(everyMs); } finally { hk.cur = null; }
      for (const f of hk.effects.splice(0)) f();
      if (!hk.dirty) return;
    }
    throw new Error("the render never settled");
  };
  render();
  return {
    /** What the component shows now (a render after whatever set the state). */
    read: () => { render(); return now; },
    unmount: () => { for (const s of inst.slots) if ((s as EffectSlot)?.effect) (s as EffectSlot).cleanup?.(); },
  };
}

describe("GA-190: useClock (the run panel's and the Usage page's clocks)", () => {
  it("shows the time, again every everyMs while the window shows", () => {
    const c = mountClock(1000);
    expect(c.read()).toBe(NOW);
    vi.advanceTimersByTime(1000);
    expect(c.read()).toBe(NOW + 1000);
    vi.advanceTimersByTime(3000);
    expect(c.read()).toBe(NOW + 4000);
    c.unmount();
  });

  it("rests while the window is hidden: no interval at all, the time doesn't move; shown, the new time at once", () => {
    const c = mountClock(1000);
    expect(vi.getTimerCount()).toBe(1);
    setHidden(true);
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(90_000);
    expect(c.read()).toBe(NOW);
    setHidden(false);
    expect(c.read()).toBe(NOW + 90_000); // at once, before any interval
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(1000);
    expect(c.read()).toBe(NOW + 91_000);
    c.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("mounted while hidden: no interval until the window shows", () => {
    doc.hidden = true;
    const c = mountClock(60_000);
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(5 * 60_000);
    expect(c.read()).toBe(NOW);
    setHidden(false);
    expect(c.read()).toBe(NOW + 5 * 60_000);
    vi.advanceTimersByTime(60_000);
    expect(c.read()).toBe(NOW + 6 * 60_000);
    c.unmount();
  });

  it("keeps ticking while shown without the focus (a window you can see on another screen)", () => {
    const c = mountClock(1000);
    blur();
    vi.advanceTimersByTime(5000);
    expect(c.read()).toBe(NOW + 5000);
    c.unmount();
  });

  it("unmounted, it no longer listens for the window showing", () => {
    const c = mountClock(1000);
    c.unmount();
    setHidden(true);
    setHidden(false);
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("GA-190: watchAway marks the page while nobody uses the window (data-away)", () => {
  it("sets data-away while hidden or without the focus, removes it when back; the cleanup removes it and stops", () => {
    const stop = watchAway();
    expect(attrs.has("data-away")).toBe(false);
    setHidden(true);
    expect(attrs.get("data-away")).toBe("");
    setHidden(false);
    expect(attrs.has("data-away")).toBe(false);
    blur();
    expect(attrs.has("data-away")).toBe(true);
    focused = true;
    focus();
    expect(attrs.has("data-away")).toBe(false);
    blur();
    expect(attrs.has("data-away")).toBe(true);
    stop();
    expect(attrs.has("data-away")).toBe(false);
    setHidden(true);
    expect(attrs.has("data-away")).toBe(false);
  });

  it("marks at once when it starts while away", () => {
    doc.hidden = true;
    const stop = watchAway();
    expect(attrs.has("data-away")).toBe(true);
    stop();
  });

  it("the stylesheet pauses the pulse, the answer's caret and a spinner under data-away", async () => {
    const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
    const css: string = fs.readFileSync(new URL("../styles/components.css", import.meta.url), "utf8");
    expect(css).toContain(":root[data-away] .pulse::after, :root[data-away] .caret, :root[data-away] .icon.spin { animation-play-state: paused; }");
  });
});
