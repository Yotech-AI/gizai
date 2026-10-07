import { useEffect, useState } from "react";
import { liveRuns, onRunsChanged } from "../api";
import type { LiveRun } from "../types";

/** Runs in progress right now; refreshed whenever a run starts or ends. */
export function useLiveRuns(): LiveRun[] {
  const [runs, setRuns] = useState<LiveRun[]>([]);
  useEffect(() => {
    let alive = true;
    let un: (() => void) | undefined;
    const load = () => { liveRuns().then((r) => { if (alive) setRuns(r); }).catch(() => {}); };
    load();
    onRunsChanged(load).then((f) => (alive ? (un = f) : f())).catch(() => {});
    return () => { alive = false; un?.(); };
  }, []);
  return runs;
}
