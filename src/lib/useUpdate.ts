import { useEffect, useState } from "react";
import { onUpdateChanged, updateStatus } from "../api";
import type { UpdateStatus } from "../types";

/** The release check and the update; refreshed whenever either moves on. The setter takes a status an action returned. */
export function useUpdate(): [UpdateStatus | null, (s: UpdateStatus) => void] {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  useEffect(() => {
    let alive = true;
    let un: (() => void) | undefined;
    const load = () => { updateStatus().then((s) => { if (alive) setStatus(s); }).catch(() => {}); };
    load();
    onUpdateChanged(load).then((f) => (alive ? (un = f) : f())).catch(() => {});
    return () => { alive = false; un?.(); };
  }, []);
  return [status, setStatus];
}
