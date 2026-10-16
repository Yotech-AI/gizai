// A card's screenshots (GA-185, GA-181's design: design/screenshots/README.md): what the Screenshots tab, its larger view,
// the board's sign and the Inbox row show of the card's sets (contract CS, `listScreenshots`). No React here.
import type { Screenshot, ScreenshotSet } from "../types";

/** The newest round: the newest builder's set (the newest that isn't QA's) and QA's sets after it, newest first. */
export function newestRound(sets: ScreenshotSet[]): ScreenshotSet[] {
  const builder = sets.findIndex((s) => s.role !== "qa");
  return builder < 0 ? sets : sets.slice(0, builder + 1);
}

/** The tab's count, the board's sign and the Inbox row's: the pictures of the newest round (4 + 2 = 6). */
export function roundCount(sets: ScreenshotSet[] | null | undefined): number {
  return newestRound(sets ?? []).reduce((n, s) => n + s.shots.length, 0);
}

/** "6 screenshots", "1 screenshot", "no screenshots". */
export function shotCount(n: number): string {
  return n === 0 ? "no screenshots" : n === 1 ? "1 screenshot" : `${n} screenshots`;
}

/** When, as the design writes it: "just now", "5 min ago", "2 h ago", "yesterday", "2 days ago". */
export function when(ms: number, now = Date.now()): string {
  const m = Math.round((now - ms) / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  return d === 1 ? "yesterday" : `${d} days ago`;
}

/** A set's answer in words, with its badge's kind: Ready for testing, QA passed, QA failed, Needs a decision, Stopped at
 *  the time limit. */
export function answerOf(s: Pick<ScreenshotSet, "outcome" | "status">): { cls: string; text: string } {
  if (s.status === "timed_out") return { cls: "warn", text: "Stopped at the time limit" };
  if (s.status === "cancelled") return { cls: "", text: "Stopped" };
  switch (s.outcome) {
    case "ready_for_testing": return { cls: "info", text: "Ready for testing" };
    case "qa_pass": return { cls: "ok", text: "QA passed" };
    case "qa_fail": return { cls: "fail", text: "QA failed" };
    case "needs_decision": return { cls: "needs", text: "Needs a decision" };
    case "deployed": return { cls: "ok", text: "Deployed" };
  }
  return s.status === "failed" ? { cls: "fail", text: "Failed" } : { cls: "", text: "Ended without a result" };
}

/** The commit in a run's heading: 7 characters. */
export const shortCommit = (sha: string | null | undefined) => (sha ? sha.slice(0, 7) : null);

/** "Desktop" or "Mobile", from the file name (the agents name their files …-desktop-dark.png), else from the size. */
export function formatOf(s: Pick<Screenshot, "name" | "width" | "height">): string {
  const parts = s.name.toLowerCase().replace(/\.[a-z]+$/, "").split(/[-_.\s]+/);
  for (const [word, label] of [["desktop", "Desktop"], ["mobile", "Mobile"], ["phone", "Mobile"], ["tablet", "Tablet"], ["window", "Window"], ["small", "Small"]]) {
    if (parts.includes(word)) return label;
  }
  return s.width < 600 ? "Mobile" : "Desktop";
}

/** "Dark" or "Light", from the file name; Dark when it doesn't say (dark is the default). */
export function themeOf(s: Pick<Screenshot, "name">): string {
  return /(^|[-_.\s])light([-_.\s]|$)/i.test(s.name.replace(/\.[a-z]+$/i, "")) ? "Light" : "Dark";
}

/** "1280 × 800". */
export const sizeOf = (s: Pick<Screenshot, "width" | "height">) => `${s.width} × ${s.height}`;

export type Pairing = "changed" | "unchanged" | "new";
export const PAIRING_TEXT: Record<Pairing, string> = { changed: "Changed", unchanged: "Unchanged", new: "New" };

/** The run before a set, for Compare: the newest older set of the same agent (else of the same role) with pictures. */
export function runBefore(sets: ScreenshotSet[], set: ScreenshotSet): ScreenshotSet | null {
  const older = sets.slice(sets.findIndex((s) => s.runId === set.runId) + 1).filter((s) => s.shots.length > 0 && s.runId !== set.runId);
  return older.find((s) => s.agentId === set.agentId) ?? (set.role ? older.find((s) => s.role === set.role) : undefined) ?? null;
}

/** A set's pictures paired by file name with the run before's: null when they share no file name (then a set shows no
 *  summary, badges or Compare; QA's additions are like that). */
export type Compared = {
  before: ScreenshotSet;
  /** Per picture of this set (by file id): Changed, Unchanged (the same hash) or New (a name the run before didn't have). */
  pairing: Map<string, Pairing>;
  /** The run before's picture with the same file name, by this set's file id. */
  pair: Map<string, Screenshot>;
  /** The names the run before had and this set hasn't: "Not taken this time". */
  notTaken: string[];
  counts: Record<Pairing, number>;
};

export function compare(sets: ScreenshotSet[], set: ScreenshotSet): Compared | null {
  const before = runBefore(sets, set);
  if (!before) return null;
  const byName = new Map(before.shots.map((s) => [s.name, s]));
  if (!set.shots.some((s) => byName.has(s.name))) return null;
  const pairing = new Map<string, Pairing>(), pair = new Map<string, Screenshot>();
  const counts: Record<Pairing, number> = { changed: 0, unchanged: 0, new: 0 };
  for (const s of set.shots) {
    const b = byName.get(s.name);
    const p: Pairing = !b ? "new" : b.sha256 === s.sha256 ? "unchanged" : "changed";
    pairing.set(s.fileId, p);
    if (b) pair.set(s.fileId, b);
    counts[p]++;
  }
  const names = new Set(set.shots.map((s) => s.name));
  return { before, pairing, pair, counts, notTaken: before.shots.map((s) => s.name).filter((n) => !names.has(n)) };
}

/** "2 changed, 1 unchanged, 1 new": the parts that aren't 0. */
export function summaryOf(c: Compared): string {
  return (["changed", "unchanged", "new"] as const).filter((k) => c.counts[k] > 0).map((k) => `${c.counts[k]} ${k}`).join(", ");
}

/** "Not taken this time: a.png, b.png." */
export const notTakenText = (names: string[]) => `Not taken this time: ${names.join(", ")}.`;

/** One of the backend's "<name>: <why>" lines as the design writes it: "notes.txt isn’t a PNG, JPEG or WebP image",
 *  "13-board.png is over the limit of 12 a run"; any other why stays after its colon. */
function leftOutLine(line: string): string {
  const l = line.trim().replace(/\.$/, "");
  const at = l.indexOf(": ");
  if (at < 0) return l;
  const name = l.slice(0, at), why = l.slice(at + 2);
  if (why.startsWith("not ")) return `${name} isn’t ${why.slice(4)}`;
  if (/^(over|under|too) /.test(why)) return `${name} is ${why}`;
  return l;
}

/** "Left out 2: notes.txt isn’t a PNG, JPEG or WebP image; 05-….png is over 16,384 pixels high." */
export function leftOutText(lines: string[]): string {
  return `Left out ${lines.length}: ${lines.map(leftOutLine).join("; ")}.`;
}

/** Why a run that took none took none: its preview didn't start, else no screen changed. Null for a set with pictures. */
export function noneText(s: ScreenshotSet): { kind: "preview" | "none"; text: string } | null {
  if (s.shots.length > 0) return null;
  if (s.preview) {
    const why = s.preview.trim().replace(/\.$/, "");
    return { kind: "preview", text: `The preview didn’t start: ${why[0].toLowerCase()}${why.slice(1)}, so the agent took none. Its last lines are in the run.` };
  }
  if (s.leftOut.length > 0) return null;
  // a run that ended early took none because it ended, not because nothing changed
  if (s.status === "timed_out") return { kind: "none", text: "It stopped at the time limit before it saved any." };
  if (s.status === "failed") return { kind: "none", text: "It failed before it saved any." };
  if (s.status === "cancelled") return { kind: "none", text: "It was stopped before it saved any." };
  return { kind: "none", text: "No screen changed, the agent says, so it took none." };
}

/** Older runs fold after the newest two. */
export const SHOWN_SETS = 2;

/** "Yesterday and 2 days ago: 2 sets of screenshots and 2 runs that took none". */
export function olderText(older: ScreenshotSet[], now = Date.now()): string {
  const days = [...new Set(older.map((s) => when(s.endedAt, now)))];
  const at = days.length <= 1 ? days.join("") : `${days.slice(0, -1).join(", ")} and ${days[days.length - 1]}`;
  const withShots = older.filter((s) => s.shots.length > 0).length, none = older.length - withShots;
  const parts = [withShots ? (withShots === 1 ? "1 set of screenshots" : `${withShots} sets of screenshots`) : "",
    none ? (none === 1 ? "1 run that took none" : `${none} runs that took none`) : ""].filter(Boolean);
  return `${at[0]?.toUpperCase() ?? ""}${at.slice(1)}: ${parts.join(" and ")}`;
}

/** "4 older runs", "1 older run". */
export const olderCount = (n: number) => (n === 1 ? "1 older run" : `${n} older runs`);

/** An agent's name in a sentence: "the Frontend Agent", "the Team Lead"; a name of its own stays as it is. */
export function agentInText(name: string): string {
  return /\b(agent|lead)$/i.test(name.trim()) ? `the ${name.trim()}` : name.trim();
}

/** Why a card's tab is empty (the design's four reasons), in the order they are checked. */
export type EmptyReason =
  | { kind: "card-off"; project: string; projectOn: boolean }
  | { kind: "no-preview"; project: string }
  | { kind: "agent-off"; agentId: string; agent: string; browser: boolean }
  | { kind: "no-run"; agent: string | null };

export function emptyReason(o: {
  cardSwitch: boolean | null; project: { name: string; on: boolean; previewCommand: string | null };
  agent: { id: string; name: string; browser: boolean; screenshots: boolean } | null;
}): EmptyReason {
  if (o.cardSwitch === false) return { kind: "card-off", project: o.project.name, projectOn: o.project.on };
  if (!o.project.previewCommand?.trim()) return { kind: "no-preview", project: o.project.name };
  if (o.agent && (!o.agent.browser || !o.agent.screenshots)) return { kind: "agent-off", agentId: o.agent.id, agent: o.agent.name, browser: o.agent.browser };
  return { kind: "no-run", agent: o.agent?.name ?? null };
}

/** The empty tab's title and text (GA-181's words). */
export function emptyTexts(r: EmptyReason): { title: string; text: string } {
  switch (r.kind) {
    case "card-off": return { title: "No screenshots: they’re off for this card.",
      text: `Screenshots before Review is off in this card’s Properties, so its agents take none here. ${r.project} has them ${r.projectOn ? "on" : "off too"}.` };
    case "no-preview": return { title: `No screenshots: ${r.project} has no preview yet.`,
      text: "Agents take screenshots of the card’s branch, served by the project’s Preview command with demo data. Without one they take none." };
    case "agent-off": {
      const who = agentInText(r.agent);
      return { title: `No screenshots: ${who} doesn’t take them.`,
        text: r.browser ? "Its Take screenshots of what it built is off, so it builds this card’s screens without keeping pictures of them."
          : "It has no browser, so it builds this card’s screens without keeping pictures of them." };
    }
    case "no-run": return { title: "No screenshots yet.",
      text: "Agents with the browser save screenshots of the screens they changed at the end of their run."
        + (r.agent ? ` ${agentInText(r.agent).replace(/^the/, "The")} takes them when it has built this card.` : "") };
  }
}

/** The MIME type of a screenshot, from its name, for its blob: URL. */
export function mimeOf(name: string): string {
  const ext = name.toLowerCase().split(".").pop();
  return ext === "jpg" || ext === "jpeg" ? "image/jpeg" : ext === "webp" ? "image/webp" : "image/png";
}
