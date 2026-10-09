// The loading state of buttons that start something slow (Run, Continue, Stop); BusyButton shows it.
import { useEffect, useRef, useState } from "react";

/** How long a button waits, after its call returned, for the screen to show what it started: a missed event never
 *  leaves it spinning. */
export const PENDING_MS = 30_000;

export type Pending<K extends string> = {
  /** The button that is busy now, or null. */
  busy: K | null;
  /** Button `key` was clicked: runs `call` unless a button is busy already. A failure goes to `onError` and ends it. */
  act: (key: K, call: () => Promise<unknown>, onError: (e: unknown) => void) => void;
};

/**
 * Busy from the click until `done(key, value)` says the screen shows what the click started (the live run, or that it
 * ended; `value` is what the call resolved with, undefined until then), until the call fails, or PENDING_MS after the
 * call returned. The call itself can take minutes (a card's first run prepares its worktree): the button spins
 * meanwhile. `scope`: what the buttons act on, when the screen can switch to another (a chat thread); a click only
 * keeps its own busy.
 */
export function usePending<K extends string>(done: (key: K, value: unknown) => boolean, scope?: unknown): Pending<K> {
  const [state, setState] = useState<{ key: K; n: number; scope: unknown; returned: boolean; value?: unknown } | null>(null);
  const clicks = useRef(0);
  const current = useRef(0); // the click being waited for (0: none): a late answer to an earlier click changes nothing
  const end = (n: number) => { if (current.current === n) { current.current = 0; setState(null); } };
  const busy = state && state.scope === scope && !done(state.key, state.value) ? state.key : null;
  useEffect(() => { if (state && !busy) end(state.n); }, [state, busy]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!state?.returned) return;
    const t = setTimeout(() => end(state.n), PENDING_MS);
    return () => clearTimeout(t);
  }, [state?.n, state?.returned]); // eslint-disable-line react-hooks/exhaustive-deps
  const act = (key: K, call: () => Promise<unknown>, onError: (e: unknown) => void) => {
    if (current.current) return;
    const n = current.current = ++clicks.current;
    setState({ key, n, scope, returned: false });
    call().then((value) => setState((s) => (s?.n === n ? { ...s, returned: true, value } : s)),
      (e) => { end(n); onError(e); });
  };
  return { busy, act };
}
