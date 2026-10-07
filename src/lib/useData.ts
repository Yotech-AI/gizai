import { useCallback, useEffect, useRef, useState } from "react";
import { onRowsChanged } from "../api";

/** Loads data and reloads it whenever the backend reports a write. */
export function useData<T>(fetch: () => Promise<T>, deps: unknown[] = []) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const fetchRef = useRef(fetch);
  fetchRef.current = fetch;
  const reload = useCallback(() => {
    fetchRef.current().then((d) => { setData(d); setError(null); }).catch((e) => setError(String(e)));
  }, []);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(reload, deps);
  useEffect(() => {
    let un: (() => void) | undefined;
    let alive = true;
    onRowsChanged(() => alive && reload()).then((f) => (alive ? (un = f) : f()));
    return () => { alive = false; un?.(); };
  }, [reload]);
  return { data, error, reload, setData };
}
