import { useCallback, useEffect, useRef, useState } from "react";
import { onRowsChanged } from "../api";
import { forgetReads, runFetch } from "./share";
import { tablesOf } from "./tables";

// GA-190 (research docs/research/performance.md F1, P2): a write reloads only the reads that show its table, once per
// frame however many notices came in it, not while the window is hidden, and an answer equal to the last one keeps the
// last one, so nothing renders again.

type Heard = (tables: ReadonlySet<string>) => void;
const hooks = new Set<Heard>();
let heard = new Set<string>();
let unlisten: (() => void) | null = null;
let listening = false;
let due: { frame: number | null; timer: ReturnType<typeof setTimeout> } | null = null;

const hidden = () => typeof document !== "undefined" && document.hidden;

/** Tells each hook which tables the writes since the last time changed. */
function tell() {
  if (due) {
    if (due.frame !== null && typeof cancelAnimationFrame === "function") cancelAnimationFrame(due.frame);
    clearTimeout(due.timer);
    due = null;
  }
  if (hidden() || heard.size === 0) return;
  const tables = heard;
  heard = new Set();
  for (const h of [...hooks]) h(tables);
}

/** At the next frame, or after 100 ms when frames don't come (a window out of sight may draw none). Hidden: when it
 *  shows again (onVisible). */
function soon() {
  if (due || hidden()) return;
  const frame = typeof requestAnimationFrame === "function" ? requestAnimationFrame(tell) : null;
  due = { frame, timer: setTimeout(tell, frame === null ? 16 : 100) };
}

function onVisible() { if (!hidden()) tell(); }

function listen() {
  if (listening) return;
  listening = true;
  if (typeof document !== "undefined") document.addEventListener("visibilitychange", onVisible);
  onRowsChanged((table) => {
    forgetReads(table);
    heard.add(table);
    soon();
  }).then((f) => { if (listening) unlisten = f; else f(); }).catch(() => {});
}

function stopListening() {
  listening = false;
  if (typeof document !== "undefined") document.removeEventListener("visibilitychange", onVisible);
  unlisten?.();
  unlisten = null;
  heard = new Set();
  if (due) { if (due.frame !== null && typeof cancelAnimationFrame === "function") cancelAnimationFrame(due.frame); clearTimeout(due.timer); due = null; }
}

/** Calls `fn` with the tables that writes changed, at most once a frame, never while the window is hidden; returns how to
 *  stop. One listener on the backend's notices serves every hook. */
export function onWrites(fn: Heard): () => void {
  hooks.add(fn);
  listen();
  return () => {
    hooks.delete(fn);
    if (hooks.size === 0) stopListening();
  };
}

const plain = (x: unknown): x is Record<string, unknown> =>
  typeof x === "object" && x !== null && Object.getPrototypeOf(x) === Object.prototype;
const idOf = (x: unknown) => (plain(x) && typeof x.id === "string" ? x.id : null);

/** `next` with every part that equals the same part of `prev` replaced by `prev`'s: an answer equal to the last one is
 *  the last one itself (React then renders nothing), and in a list where one card changed, the others stay the objects
 *  they were (a memoised row skips). List items with an `id` are matched by it, so a reorder keeps them too. */
export function keep<T>(prev: unknown, next: T): T {
  if (Object.is(prev, next)) return prev as T;
  if (Array.isArray(prev) && Array.isArray(next)) {
    const byId = new Map<string, unknown>();
    for (const x of prev) { const id = idOf(x); if (id !== null) byId.set(id, x); }
    let same = prev.length === next.length;
    const out = next.map((x, i) => {
      const id = idOf(x);
      const was = id !== null && byId.has(id) ? byId.get(id) : prev[i];
      const k = keep(was, x);
      if (k !== prev[i]) same = false;
      return k;
    });
    return (same ? prev : out) as T;
  }
  if (plain(prev) && plain(next)) {
    const keys = Object.keys(next);
    let same = keys.length === Object.keys(prev).length;
    const out: Record<string, unknown> = {};
    for (const k of keys) {
      const v = keep(prev[k], next[k]);
      out[k] = v;
      if (v !== prev[k] || !(k in prev)) same = false;
    }
    return (same ? prev : out) as T;
  }
  if (prev instanceof Set && next instanceof Set) {
    if (prev.size !== next.size) return next;
    for (const x of next) if (!prev.has(x)) return next;
    return prev as T;
  }
  return next;
}

/** Loads data, and reloads it after a write to a table it shows (lib/tables.ts; a fetch the map doesn't know reloads after
 *  any write). Notices within one frame make one reload; while the window is hidden they wait until it shows. Hooks that
 *  ask for the same read at the same time share one backend call (lib/share.ts). `counts: false`: the screen shows none of
 *  the read's counts and sums (a project's open cards, its cost), so writes that change only those don't reload it. */
export function useData<T>(fetch: () => Promise<T>, deps: unknown[] = [], { counts = true }: { counts?: boolean } = {}) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const fetchRef = useRef(fetch);
  fetchRef.current = fetch;
  // the tables the last fetch read; null: any write counts
  const tables = useRef<Set<string> | null>(null);
  const countsRef = useRef(counts);
  countsRef.current = counts;
  // answers can come back out of order: only the last one asked shows
  const asked = useRef(0);
  const reload = useCallback(() => {
    const n = ++asked.current;
    const { answer, commands } = runFetch(() => fetchRef.current());
    tables.current = tablesOf(commands, countsRef.current);
    answer.then((d) => { if (n === asked.current) { setData((was) => keep(was, d)); setError(null); } })
      .catch((e) => { if (n === asked.current) setError(String(e)); });
  }, []);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(reload, deps);
  useEffect(() => onWrites((changed) => {
    const mine = tables.current;
    if (!mine || [...changed].some((t) => mine.has(t))) reload();
  }), [reload]);
  return { data, error, reload, setData };
}
