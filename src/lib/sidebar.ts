// The sidebar's Agents and Memory sections fold (GA-90). Which are folded is kept on this computer in localStorage as a
// comma list ("agents,memory"), the way the Memory page keeps its panel's closed sections; both start open.
import { useState } from "react";

export const FOLDED_KEY = "gizai.sidebar.folded";
/** The sections that fold. */
export type Fold = "agents" | "memory";

/** The folded sections in what localStorage keeps (null: nothing kept, so none). */
export function foldedOf(kept: string | null): string[] {
  return (kept ?? "").split(",").map((x) => x.trim()).filter(Boolean);
}

/** What to keep after a click on a section's caret: that section folded if it was open and open if it was folded, the
 *  others as they were. */
export function toggleFold(kept: string | null, section: Fold): string {
  const list = foldedOf(kept);
  return (list.includes(section) ? list.filter((x) => x !== section) : [...list, section]).join(",");
}

/** The live runs of the agents the sidebar lists, together: folded Agents shows them on its heading. */
export function liveTotal(live: { agentId: string }[], agents: { actorId: string }[]): number {
  return live.filter((r) => agents.some((a) => a.actorId === r.agentId)).length;
}

function read(): string | null { try { return localStorage.getItem(FOLDED_KEY); } catch { return null; } }

/** Whether a section is folded, and the caret's click: it folds or opens the section and keeps that. */
export function useFolded(): [(section: Fold) => boolean, (section: Fold) => void] {
  const [kept, setKept] = useState<string | null>(read);
  const toggle = (section: Fold) => {
    const next = toggleFold(kept, section);
    setKept(next);
    try { localStorage.setItem(FOLDED_KEY, next); } catch { /* private mode */ }
  };
  return [(section) => foldedOf(kept).includes(section), toggle];
}
