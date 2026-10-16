import { useEffect, useState } from "react";
import { liveRuns, onRunsChanged } from "../api";
import type { LiveRun } from "../types";
import { whenShown } from "./windowFocus";

/** Runs in progress right now; refreshed whenever a run starts or ends (while the window is hidden, once when it shows
 *  again, GA-190). */
export function useLiveRuns(): LiveRun[] {
  const [runs, setRuns] = useState<LiveRun[]>([]);
  useEffect(() => {
    let alive = true;
    let un: (() => void) | undefined;
    const load = () => { liveRuns().then((r) => { if (alive) setRuns(r); }).catch(() => {}); };
    load();
    const later = whenShown(load);
    onRunsChanged(later.run).then((f) => (alive ? (un = f) : f())).catch(() => {});
    return () => { alive = false; un?.(); later.stop(); };
  }, []);
  return runs;
}
