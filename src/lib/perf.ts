// The benchmark harness's counter (GA-188; research docs/research/performance.md §7). In a perf run
// (GIZAI_SELFTEST_MODE=perf:<scenario>, scripts/perf.sh) the backend sets window.__GIZAI_PERF__ before the page's own
// scripts run (src-tauri/src/lib.rs), and every backend call and every event the page hears is counted from the first one.
// Outside one, invoke and listen are Tauri's own: the app runs exactly as without this file.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type EventCallback, type EventName, type Options } from "@tauri-apps/api/event";

/** One backend call: when it started (ms since the page began), how long it took and its answer's size as JSON. Listening
 *  for an event counts as a call to "listen". */
export type PerfCall = { cmd: string; at: number; ms: number; bytes: number; error: boolean };
/** One event the page heard, with its payload's size as JSON. */
export type PerfHeard = { event: string; at: number; bytes: number };

export const perfOn = typeof window !== "undefined" && (window as { __GIZAI_PERF__?: boolean }).__GIZAI_PERF__ === true;
export const perfCalls: PerfCall[] = [];
export const perfHeard: PerfHeard[] = [];

const size = (v: unknown) => {
  if (v == null) return 0;
  try { return JSON.stringify(v)?.length ?? 0; } catch { return 0; }
};

export const invoke: typeof tauriInvoke = !perfOn ? tauriInvoke : (async (cmd, args, options) => {
  const call: PerfCall = { cmd, at: performance.now(), ms: 0, bytes: 0, error: false };
  perfCalls.push(call);
  try {
    const out = await tauriInvoke(cmd, args, options);
    call.bytes = size(out);
    return out;
  } catch (e) {
    call.error = true;
    throw e;
  } finally {
    call.ms = performance.now() - call.at;
  }
}) as typeof tauriInvoke;

export const listen: typeof tauriListen = !perfOn ? tauriListen : <T>(event: EventName, handler: EventCallback<T>, options?: Options) => {
  perfCalls.push({ cmd: "listen", at: performance.now(), ms: 0, bytes: 0, error: false });
  return tauriListen<T>(event, (e) => {
    perfHeard.push({ event, at: performance.now(), bytes: size(e.payload) });
    return handler(e);
  }, options);
};

/** The cards have come, and every open board column shows its own (at most 50 each). */
function boardDrawn() {
  const cols = [...document.querySelectorAll(".board .col:not(.rail)")];
  return cols.length > 0 && !!document.querySelector(".board .card") && cols.every((c) => {
    const n = parseInt(c.querySelector(".col-head .n")?.textContent ?? "0", 10) || 0;
    return c.querySelectorAll(".col-body .card").length >= Math.min(50, n);
  });
}
const WATCH: Record<string, () => boolean> = {
  sidebar: () => !!document.querySelector(".side .nav-item"),
  card: () => !!document.querySelector(".card[data-card]"),
  board: boardDrawn,
};
/** When (ms since the page began) the sidebar, the first card and the whole board were first drawn, watched from the start
 *  in a perf run: the probe itself loads only after app_info has answered. */
export const perfFirst: Record<string, number> = {};
if (perfOn) {
  const look = () => {
    for (const [name, drawn] of Object.entries(WATCH)) if (perfFirst[name] === undefined && drawn()) perfFirst[name] = performance.now();
    if (Object.keys(WATCH).every((name) => perfFirst[name] !== undefined)) watcher.disconnect();
  };
  const watcher = new MutationObserver(look);
  watcher.observe(document.documentElement, { childList: true, subtree: true });
  // only the start: later the scenario's own work shouldn't pay for the watching
  setTimeout(() => watcher.disconnect(), 15_000);
}

/** The calls and events between `from` and `to` (ms since the page began): per command (most bytes first), per event,
 *  and their totals. */
export function perfCount(from = 0, to = Infinity) {
  const by = new Map<string, { cmd: string; calls: number; ms: number; bytes: number; errors: number }>();
  let calls = 0, bytes = 0;
  for (const c of perfCalls) {
    if (c.at < from || c.at >= to) continue;
    const s = by.get(c.cmd) ?? by.set(c.cmd, { cmd: c.cmd, calls: 0, ms: 0, bytes: 0, errors: 0 }).get(c.cmd)!;
    s.calls += 1; s.ms += c.ms; s.bytes += c.bytes; s.errors += c.error ? 1 : 0;
    calls += 1; bytes += c.bytes;
  }
  const events: Record<string, { heard: number; bytes: number }> = {};
  let heard = 0;
  for (const h of perfHeard) {
    if (h.at < from || h.at >= to) continue;
    const e = (events[h.event] ??= { heard: 0, bytes: 0 });
    e.heard += 1; e.bytes += h.bytes;
    heard += 1;
  }
  const commands = [...by.values()].map((s) => ({ ...s, ms: Math.round(s.ms) })).sort((a, b) => b.bytes - a.bytes || b.calls - a.calls);
  return { calls, bytes, heard, commands, events };
}

/** `ms` of frames, doing `step` (a scroll, a keystroke) before each: frames a second and the longest frame, taken as the
 *  Changes probe takes them. `until` ends it sooner (a stream that finished). */
export async function perfFrames(ms: number, step?: (i: number) => void, until?: () => boolean) {
  const gaps: number[] = [];
  let last = performance.now();
  const end = last + ms;
  for (let i = 0; performance.now() < end && !until?.(); i++) {
    step?.(i);
    await new Promise((r) => requestAnimationFrame(r));
    const now = performance.now();
    gaps.push(now - last);
    last = now;
  }
  const total = gaps.reduce((n, g) => n + g, 0) || 1;
  return { fps: Math.round((gaps.length * 1000) / total), longestFrameMs: Math.round(Math.max(0, ...gaps)), frames: gaps.length };
}
