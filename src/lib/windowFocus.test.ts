// GA-177 QA: onWindowFocus (src/lib/windowFocus.ts) with fake timers. On Omarchy (Hyprland) the focus follows the mouse,
// so the window's focus fires again and again: a focus runs `fn` only when it last ran more than 5 seconds ago, whether
// that run came from a focus or from its interval; the interval keeps its timing and skips while the window is hidden.
// There is no DOM here: the window is a plain EventTarget and the document only says whether it is hidden.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FOCUS_GAP, onWindowFocus } from "./windowFocus";

const win = new EventTarget();
vi.stubGlobal("window", win);
const doc = { hidden: false };
vi.stubGlobal("document", doc);

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

beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(NOW); doc.hidden = false; });
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
