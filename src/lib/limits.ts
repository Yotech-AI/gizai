// The Usage page's Subscription tab (GA-62): each coding CLI's subscription limits, as its runs and chat turns last reported
// them (gizai-core limits.rs). A number is only shown with the time it was read, and not once its window has reset since.
// Times are local: a reset is a moment on your clock, not a UTC day.
import type { CliLimits, LimitReading, SubscriptionLimit } from "../types";

/** From this share used, a limit shows as near its end (Claude Code warns at about the same point). */
export const NEAR_PERCENT = 80;

/** Under the Subscription tab. */
export const LIMITS_NOTE = "Gizai reads only what the coding CLIs report in your agents' runs and chat turns; it never asks Anthropic or OpenAI. "
  + "Each number shows with the time it was read, and not after its window has reset.";

const DAY = 86_400_000;

/** Where a limit stands now: `unread` (no run has reported it), `reset` (its window has reset since the reading), or as read:
 *  `ok`, `near` (80% or more, or Claude Code's warning), `reached`. */
export type LimitState = "unread" | "reset" | "ok" | "near" | "reached";

export function limitState(l: SubscriptionLimit, now: number): LimitState {
  const r = l.reading;
  if (!r) return "unread";
  if (r.resetsAt != null && r.resetsAt <= now) return "reset";
  if (r.status === "rejected" || (r.usedPercent != null && r.usedPercent >= 100)) return "reached";
  if (r.status === "allowed_warning" || (r.usedPercent != null && r.usedPercent >= NEAR_PERCENT)) return "near";
  return "ok";
}

/** How much is used: "42%", "<1%", or "Limit reached" when the CLI said so without a number; "" without a reading, and once the
 *  window has reset since (the number is old then). */
export function usedLabel(l: SubscriptionLimit, now: number): string {
  const r = l.reading;
  const state = limitState(l, now);
  if (!r || state === "reset") return "";
  if (r.usedPercent == null) return state === "reached" ? "Limit reached" : "";
  if (r.usedPercent > 0 && r.usedPercent < 1) return "<1%";
  return `${Math.round(r.usedPercent)}%`;
}

/** The bar's width in % (a number past the cap fills it). */
export function usedWidth(r: LimitReading | null | undefined): number {
  return r?.usedPercent == null ? 0 : Math.max(0, Math.min(100, r.usedPercent));
}

const pad = (n: number) => String(n).padStart(2, "0");
const clock = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;
const sameDay = (a: Date, b: Date) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
const shift = (d: Date, days: number) => { const x = new Date(d); x.setDate(x.getDate() + days); return x; };

/** A moment in local time, as short as `now` allows: "14:02" today, "tomorrow 09:00", "yesterday 14:02", "Mon 13 Oct, 09:00" within a
 *  week, else "13 Oct, 09:00" (with the year when it isn't this year's). */
export function whenLabel(ms: number, now: number): string {
  const d = new Date(ms), n = new Date(now);
  if (sameDay(d, n)) return clock(d);
  if (sameDay(d, shift(n, 1))) return `tomorrow ${clock(d)}`;
  if (sameDay(d, shift(n, -1))) return `yesterday ${clock(d)}`;
  const date = d.toLocaleDateString("en-GB", { day: "numeric", month: "short", ...(d.getFullYear() !== n.getFullYear() && { year: "numeric" }) });
  if (Math.abs(ms - now) < 6 * DAY) return `${d.toLocaleDateString("en-GB", { weekday: "short" })} ${date}, ${clock(d)}`;
  return `${date}, ${clock(d)}`;
}

/** A moment in full, for a title: "Thursday 9 October 2026 at 14:02". */
export function fullWhen(ms: number): string {
  const d = new Date(ms);
  return `${d.toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long", year: "numeric" })} at ${clock(d)}`;
}

/** When the limit resets: "15:00", "tomorrow 09:00", "Mon 13 Oct, 09:00"; as the CLI wrote it ("3pm (Europe/Amsterdam)"); "" when
 *  it didn't say. */
export function resetLabel(r: LimitReading | null | undefined, now: number): string {
  if (!r) return "";
  if (r.resetsAt != null) return whenLabel(r.resetsAt, now);
  return r.resetsText?.trim() ?? "";
}

/** When the number was read: "as of 14:02", "as of yesterday 14:02", "as of 8 Oct, 14:02". */
export function asOfLabel(r: LimitReading, now: number): string {
  return `as of ${whenLabel(r.observedAt, now)}`;
}

/** A window's length: "5 hours", "7 days", "90 minutes"; "" when unknown. */
export function windowLabel(minutes: number | null | undefined): string {
  if (!minutes || minutes <= 0) return "";
  if (minutes % 1440 === 0) return minutes === 1440 ? "1 day" : `${minutes / 1440} days`;
  if (minutes % 60 === 0) return minutes === 60 ? "1 hour" : `${minutes / 60} hours`;
  return `${minutes} minutes`;
}

/** A limit no run has reported yet: what the row says, and why on hover. */
export function unreadLabel(c: CliLimits, l: SubscriptionLimit): { text: string; title: string } {
  if (c.kind === "codex") return { text: "Not read yet", title: `Gizai reads it from Codex's session log when a run on ${c.name} ends.` };
  if (l.key === "seven_day_overage_included")
    return { text: "Not reported yet", title: "Claude Code reports the Fable limit only for an account that has one, once a run or chat turn on it has used Claude." };
  return { text: "Not reported yet", title: `Claude Code reports it in the runs and chat turns on ${c.name}, on a Claude subscription (not with an API key).` };
}

/** Where a block's numbers come from, under it. */
export function sourceNote(c: CliLimits): string {
  if (c.kind === "claude_code")
    return `Claude Code reports these limits in the runs and chat turns on ${c.name} when they change, on a Claude subscription only. Gizai keeps the newest numbers.`;
  if (c.kind === "codex")
    return `Codex writes these limits in its session log${c.accountDir ? ` (${c.accountDir}/sessions)` : ""}. Gizai reads the log of each run on ${c.name} when the run ends.`;
  return cantRead(c);
}

/** A block whose limits Gizai can't read (Gemini, Other). */
export function cantRead(c: CliLimits): string {
  return c.kind === "gemini" ? "Gizai can't read Gemini's limits yet." : `Gizai can't read the limits of ${c.name} yet: it reads Claude Code's and Codex's.`;
}

/** Whether any limit of a block has a reading. */
export function anyRead(c: CliLimits): boolean {
  return c.limits.some((l) => l.reading != null);
}

/** Which chats run on a CLI: "The Team Lead's chat runs here." (its Runs on), "2 chats picked it under Runs on." (their own), both;
 *  "" for none. */
export function chatsLine(c: CliLimits): string {
  const picked = c.chats > 0 ? `${c.chats === 1 ? "1 chat" : `${c.chats} chats`} picked it under Runs on.` : "";
  if (c.leadChat) return picked ? `The Team Lead's chat runs here, and ${picked}` : "The Team Lead's chat runs here.";
  return picked;
}
