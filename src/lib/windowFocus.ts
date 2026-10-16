// A window focus refreshes, at most once every 5 seconds (GA-177; research docs/research/performance.md F7, P9). On
// Omarchy (Hyprland) the focus follows the mouse, so moving the pointer over Gizai and away fires the window's focus
// again and again, and each refresh on the Editor page, the sidebar's Editor folders and the terminal page starts git.
//
// Quiet while nobody looks (GA-190). Closed to the tray, Gizai's window is hidden (GTK hides it), and WebKitGTK then
// says the page is hidden: `document.hidden`, with a `visibilitychange`. A window on another Hyprland workspace isn't
// hidden for GTK 3 on Wayland, so for WebKitGTK it is still in sight; but it has lost the focus. So the checks that start
// git rest while the window is hidden or another window has the focus (`away`), and come back on the focus; the clocks,
// the reloads after a write and a hidden terminal's drawing rest only while it is hidden, as a window you can see without
// using it (on a second screen) keeps showing what happens. Not seen on Hyprland itself in GA-190's run: no window may
// open on Jeffrey's screen.

import { useEffect, useState } from "react";

/** How long after a refresh a window focus leaves it alone. */
export const FOCUS_GAP = 5000;

const doc = () => (typeof document === "undefined" ? null : document);

/** The window is out of sight: closed to the tray, or minimised. */
export const hidden = () => !!doc()?.hidden;

/** Nobody uses the window now: it is hidden, or another window has the focus. */
export function away(): boolean {
  const d = doc();
  if (!d) return false;
  return d.hidden || (typeof d.hasFocus === "function" && !d.hasFocus());
}

/** Calls `fn` each time the window shows again after it was hidden; returns how to stop. */
export function onShown(fn: () => void): () => void {
  const d = doc();
  if (!d || typeof d.addEventListener !== "function") return () => {};
  const f = () => { if (!d.hidden) fn(); };
  d.addEventListener("visibilitychange", f);
  return () => d.removeEventListener("visibilitychange", f);
}

/** `fn` for an event's reload: at once while the window shows, else once when it shows again, however many events came
 *  meanwhile. Call `stop` when done. */
export function whenShown(fn: () => void): { run: () => void; stop: () => void } {
  let waiting = false;
  const stop = onShown(() => { if (waiting) { waiting = false; fn(); } });
  return { run: () => { if (hidden()) waiting = true; else fn(); }, stop };
}

/** Runs `fn` when the window gets the focus, or shows again after it was hidden, unless it last ran `minGapMs` or less
 *  ago. With `everyMs`, also every `everyMs` while the window isn't `away` (hidden, or without the focus), whatever the
 *  focus did; those runs count for the gap too. With `tick`, the interval runs `tick` instead of `fn` (a lighter check),
 *  and those runs don't count for the gap. The first focus runs `fn` at once. Returns the cleanup. */
export function onWindowFocus(fn: () => void, minGapMs = FOCUS_GAP, everyMs?: number, tick?: () => void): () => void {
  let last = -Infinity;
  const run = () => { last = Date.now(); fn(); };
  // a clock set back doesn't hold the focus off until it catches up
  const focus = () => { const now = Date.now(); if (now - last > minGapMs || now < last) run(); };
  const t = everyMs ? setInterval(() => { if (away()) return; if (tick) tick(); else run(); }, everyMs) : undefined;
  window.addEventListener("focus", focus);
  const unshown = onShown(focus);
  return () => { clearInterval(t); window.removeEventListener("focus", focus); unshown(); };
}

/** Marks the page while nobody uses the window (`data-away` on the root element), so the animations that run at rest
 *  (the live pulse, the answer's caret, a spinner) pause: they kept WebKit drawing every frame (F7). Returns how to stop. */
export function watchAway(): () => void {
  const d = doc();
  if (!d || typeof d.addEventListener !== "function" || typeof window === "undefined") return () => {};
  const mark = () => {
    const root = d.documentElement;
    if (!root) return;
    if (away()) root.setAttribute("data-away", "");
    else root.removeAttribute("data-away");
  };
  mark();
  window.addEventListener("focus", mark);
  window.addEventListener("blur", mark);
  d.addEventListener("visibilitychange", mark);
  return () => {
    window.removeEventListener("focus", mark);
    window.removeEventListener("blur", mark);
    d.removeEventListener("visibilitychange", mark);
    d.documentElement?.removeAttribute("data-away");
  };
}

/** The time now, again every `everyMs` while the window isn't hidden, and at once when it shows again: a clock that rests
 *  while nobody can see it (the run panel's, the Usage page's). */
export function useClock(everyMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    let t: ReturnType<typeof setInterval> | undefined;
    const run = () => {
      clearInterval(t);
      t = hidden() ? undefined : setInterval(() => setNow(Date.now()), everyMs);
    };
    run();
    const stop = onShown(() => setNow(Date.now()));
    const d = doc();
    const again = () => run();
    if (d && typeof d.addEventListener === "function") d.addEventListener("visibilitychange", again);
    return () => { clearInterval(t); stop(); if (d && typeof d.removeEventListener === "function") d.removeEventListener("visibilitychange", again); };
  }, [everyMs]);
  return now;
}
