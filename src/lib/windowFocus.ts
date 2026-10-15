// A window focus refreshes, at most once every 5 seconds (GA-177; research docs/research/performance.md F7, P9). On
// Omarchy (Hyprland) the focus follows the mouse, so moving the pointer over Gizai and away fires the window's focus
// again and again, and each refresh on the Editor page, the sidebar's Editor folders and the terminal page starts git.

/** How long after a refresh a window focus leaves it alone. */
export const FOCUS_GAP = 5000;

/** Runs `fn` when the window gets the focus, unless it last ran `minGapMs` or less ago. With `everyMs`, also every
 *  `everyMs` while the window isn't hidden, whatever the focus did; those runs count for the gap too. The first focus
 *  runs `fn` at once. Returns the cleanup. */
export function onWindowFocus(fn: () => void, minGapMs = FOCUS_GAP, everyMs?: number): () => void {
  let last = -Infinity;
  const run = () => { last = Date.now(); fn(); };
  // a clock set back doesn't hold the focus off until it catches up
  const focus = () => { const now = Date.now(); if (now - last > minGapMs || now < last) run(); };
  const t = everyMs ? setInterval(() => { if (!document.hidden) run(); }, everyMs) : undefined;
  window.addEventListener("focus", focus);
  return () => { clearInterval(t); window.removeEventListener("focus", focus); };
}
