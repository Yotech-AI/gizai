import { useEffect, useState } from "react";

const KEY = "gizai-team";
const EVENT = "gizai-team-changed";

function read(): string | null { try { return localStorage.getItem(KEY); } catch { return null; } }

/** The team picked on the Team page, shared by the Team page and the sidebar (null = the first team). */
export function useCurrentTeam(): [string | null, (id: string | null) => void] {
  const [id, setId] = useState<string | null>(read);
  useEffect(() => {
    const on = () => setId(read());
    window.addEventListener(EVENT, on);
    return () => window.removeEventListener(EVENT, on);
  }, []);
  const set = (next: string | null) => {
    try { if (next) localStorage.setItem(KEY, next); else localStorage.removeItem(KEY); } catch { /* private mode */ }
    window.dispatchEvent(new Event(EVENT));
  };
  return [id, set];
}
