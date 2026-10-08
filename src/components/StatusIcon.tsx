// Status glyphs from the Gizai design system: one shape and one colour per column category (colour-blind safe).
// Same paths as design/gen-design-system.mjs (GLYPH). A hold overrides the category.
import type { ReactNode } from "react";
import type { StateCategory } from "../types";

const GLYPH: Record<string, ReactNode> = {
  backlog: <circle cx="8" cy="8" r="6.25" stroke="currentColor" strokeWidth="1.6" strokeDasharray="2.4 2.1" />,
  ready: <circle cx="8" cy="8" r="6.25" stroke="currentColor" strokeWidth="1.6" />,
  in_progress: <><circle cx="8" cy="8" r="6.25" stroke="currentColor" strokeWidth="1.6" /><path d="M8 4.25a3.75 3.75 0 0 1 0 7.5z" fill="currentColor" /></>,
  testing: <><circle cx="8" cy="8" r="6.25" stroke="currentColor" strokeWidth="1.6" /><path d="M8 8V4.25a3.75 3.75 0 1 1-3.75 3.75z" fill="currentColor" /></>,
  review: <><circle cx="8" cy="8" r="6.25" stroke="currentColor" strokeWidth="1.6" /><circle cx="8" cy="8" r="2.4" fill="currentColor" /></>,
  deploy: <><circle cx="8" cy="8" r="6.25" stroke="currentColor" strokeWidth="1.6" /><path d="M8 10.8V5.2M5.6 7.6 8 5.2l2.4 2.4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" /></>,
  done: <><circle cx="8" cy="8" r="7" fill="currentColor" /><path d="m5.1 8.2 2 1.9 3.8-3.9" stroke="var(--bg)" strokeWidth="1.7" fill="none" strokeLinecap="round" strokeLinejoin="round" /></>,
  hold: <><circle cx="8" cy="8" r="6.25" stroke="currentColor" strokeWidth="1.6" /><path d="M5.2 8h5.6" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" /></>,
  cancelled: <><circle cx="8" cy="8" r="6.25" stroke="currentColor" strokeWidth="1.6" /><path d="m4 12 8-8" stroke="currentColor" strokeWidth="1.6" /></>,
};

export const CATEGORY_NAMES: Record<string, string> = {
  backlog: "Backlog", ready: "To do", in_progress: "In progress", testing: "Testing", review: "Review", deploy: "Deploy", done: "Done", cancelled: "Cancelled",
  hold: "On hold",
};

/** The column categories in workflow order. */
export const CATEGORIES: StateCategory[] = ["backlog", "ready", "in_progress", "testing", "review", "deploy", "done", "cancelled"];

export function StatusIcon({ category, hold, title }: { category: string; hold?: string | null; title?: string }) {
  const cat = hold ? "hold" : GLYPH[category] ? category : "ready";
  const label = title ?? (hold ? `On hold (${hold.replaceAll("_", " ")})` : CATEGORY_NAMES[cat]);
  return <svg className={`st st-${cat}`} viewBox="0 0 16 16" fill="none" role="img" aria-label={label}><title>{label}</title>{GLYPH[cat]}</svg>;
}

export const PRIORITY_NAMES = ["None", "Urgent", "High", "Medium", "Low"];

/** Grey bars; only Urgent is coloured. */
export function PriorityIcon({ priority }: { priority: number }) {
  if (priority === 1) return <span className="pri urgent" title="Urgent">!</span>;
  const cls = priority === 2 ? "high" : priority === 3 ? "medium" : priority === 4 ? "low" : "";
  return <span className={`pri ${cls}`} title={PRIORITY_NAMES[priority] ?? "None"}><i /><i /><i /></span>;
}
