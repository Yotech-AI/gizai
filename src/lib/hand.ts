// A person's card (GA-115, F4 of the Code Editor; research §3.2 Q8, design design/code-editor/editor.html §6): what Open in
// editor, the card bar in the Editor (Commit…, Push, Hand over) and "By hand" say, worked out from the card and the
// answers of contract C4, kept apart from the screen so it can be tested. When you last looked at a card (so the bar can
// say QA sent it back until you open it) is kept in localStorage, read inside try/catch: the tests run in Node, which has none.
import type { Run, Task, Team } from "../types";
import { agentNames, andList } from "./columns";

/** "1 file", "2 files". */
export const filesWord = (n: number) => `${n.toLocaleString("en-US")} ${n === 1 ? "file" : "files"}`;
/** "1 commit", "2 commits". */
export const commitsWord = (n: number) => `${n.toLocaleString("en-US")} ${n === 1 ? "commit" : "commits"}`;

/** Where a card you do by hand stands: files with uncommitted changes, commits the remote doesn't have, the branch's own
 *  commits (those main doesn't have; null while Gizai doesn't know yet), and whether its project has a link to push to. */
export type HandStand = { changed: number; unpushed: number; own: number | null; link: boolean };

/** How far it is, in the card bar: changed files first, then commits not pushed, then all done or nothing yet. */
export function handState(s: HandStand): string {
  if (s.changed > 0) return `${filesWord(s.changed)} changed`;
  if (s.unpushed > 0) return `${commitsWord(s.unpushed)} not pushed`;
  if (s.own === 0) return "No changes yet";
  return s.link ? "Everything committed and pushed" : "Everything committed";
}

/** A button of the card bar: off with why (its tooltip), or on with what it does; the next step's is primary. */
export type HandButton = { off: boolean; title: string; primary: boolean };

/** Hand over's label: to Testing, or to Review when the card's Testing switch is off. */
export const handOverLabel = (testing: boolean) => (testing ? "Hand over to Testing" : "Hand over to Review");

/** The agents that take a card in the team's first Testing column, when it is Auto: "QA Agent". "" when none does. */
export function testersOf(team: Team | null): string {
  const s = [...(team?.states ?? [])].sort((a, b) => (a.sortKey < b.sortKey ? -1 : 1)).find((x) => x.category === "testing");
  return s && s.auto ? andList(agentNames(s, team!.members)) : "";
}

/** Commit…, Push and Hand over for the card `id` (on branch `branch`) as it stands. */
export function handButtons(id: string, branch: string, s: HandStand, o: { testing: boolean; testers: string; project: string }): {
  commit: HandButton; push: HandButton; over: HandButton;
} {
  const nothingYet = s.changed === 0 && s.unpushed === 0 && s.own === 0;
  const commit: HandButton = s.changed > 0
    ? { off: false, primary: true, title: `Commit every change in ${id}'s worktree: ${filesWord(s.changed)}` }
    : { off: true, primary: false, title: "Nothing to commit" };
  const push: HandButton = !s.link
    ? { off: true, primary: false, title: `${o.project} has no GitHub, Bitbucket or git link to push to` }
    : s.unpushed > 0
      ? { off: false, primary: false, title: `Push ${branch}: ${commitsWord(s.unpushed)} the remote doesn't have` }
      : { off: true, primary: false, title: "Nothing to push" };
  const to = o.testing ? `Testing${o.testers ? ` for ${o.testers}` : ""}` : "Review for you";
  const over: HandButton = s.changed > 0
    ? { off: true, primary: false, title: `Commit your changes first: ${filesWord(s.changed)} ${s.changed === 1 ? "isn't" : "aren't"} committed.` }
    : nothingYet
      ? { off: true, primary: false, title: "Commit a change first" }
      : { off: false, primary: true, title: `${s.link ? "Pushes, moves" : "Moves"} ${id} to ${to} and takes the pin off` };
  return { commit, push, over };
}

/** What Commit did: "Committed 2 files as 3f5c1a2." */
export const committedSay = (files: number, sha: string) => `Committed ${files > 0 ? `${filesWord(files)} ` : ""}as ${sha.slice(0, 7)}.`;

/** What the card bar says after Hand over, from the column the card is in now (`handOver`'s answer). */
export function handedSay(column: string, team: Team | null): string {
  const s = team?.states.find((x) => x.name === column);
  const ro = "Its worktree is read-only while an agent works on it.";
  if (s?.category === "review") return `Handed over to ${column}: it waits for your review.`;
  const who = s?.category === "testing" && s.auto ? andList(agentNames(s, team!.members)) : "";
  return who ? `Handed over to ${column}: ${who} ${who.includes(" and ") ? "take" : "takes"} it from here. ${ro}` : `Handed over to ${column}. ${ro}`;
}

/** An error from the backend as a sentence: without Tauri's "Error: ", with a full stop. */
export function plainError(e: unknown): string {
  const s = String(e).replace(/^Error:\s*/, "").trim();
  return /[.!?]$/.test(s) ? s : `${s}.`;
}

/** Open in editor refused or failed: "Couldn't open KADE-41 in the editor: npm ci failed in its new worktree (exit code 1)." */
export function couldntOpen(id: string, e: unknown): string {
  const s = plainError(e);
  return `Couldn't open ${id} in the editor: ${/^[A-Z][a-z]/.test(s) ? s.charAt(0).toLowerCase() + s.slice(1) : s}`;
}

/** Open in editor's tooltip on the card's top bar: off while an agent works on it (`agent`, its name), or what it does. */
export function openInEditorTitle(task: Pick<Task, "identifier" | "byHand" | "stateCategory">, agent: string | null): string {
  if (agent) return `${agent} is working on ${task.identifier}: wait for its run to end, or stop it.`;
  if (task.byHand) return `Open ${task.identifier}'s worktree in the Editor`;
  const moves = task.stateCategory === "backlog" || task.stateCategory === "ready" ? ", and moves it to In progress" : "";
  return `Work on ${task.identifier} by hand: Gizai makes its worktree, pins the card to you so agents leave it alone${moves}.`;
}

/** Whether Open in editor shows on the card: assigned to you, not archived, and in a column where work is done. */
export function canDoByHand(task: Pick<Task, "assigneeId" | "archivedAt" | "stateCategory">, youId: string | undefined): boolean {
  return !!youId && task.assigneeId === youId && !task.archivedAt && !["deploy", "done", "cancelled"].includes(task.stateCategory);
}

/** "By hand"'s tooltip, on the board and the card. */
export const byHandTip = (task: Pick<Task, "assigneeName">) => `Pinned to ${task.assigneeName ?? "a person"}: agents leave it alone until it's handed over`;

// ---- when you last looked at a card: QA sent it back shows in the card bar until you open the card ----

export const SEEN_KEY = "gizai.cards.seen";
const SEEN_MAX = 100;

/** The cards you looked at, by id, when (ms); nothing kept, or something unreadable, is none. */
export function seenOf(kept: string | null): Record<string, number> {
  try {
    const v: unknown = JSON.parse(kept ?? "{}");
    if (!v || typeof v !== "object" || Array.isArray(v)) return {};
    return Object.fromEntries(Object.entries(v).filter((e): e is [string, number] => typeof e[1] === "number"));
  } catch { return {}; }
}

/** What to keep after looking at card `id` at `at`: the latest 100 cards. */
export function withSeen(kept: string | null, id: string, at: number): string {
  const all = Object.entries({ ...seenOf(kept), [id]: at }).sort((a, b) => b[1] - a[1]).slice(0, SEEN_MAX);
  return JSON.stringify(Object.fromEntries(all));
}

function readKey(key: string): string | null { try { return localStorage.getItem(key); } catch { return null; } }
function writeKey(key: string, value: string) { try { localStorage.setItem(key, value); } catch { /* private mode */ } }

/** You look at card `id` now (its page is open). */
export const markCardSeen = (id: string, at = Date.now()) => writeKey(SEEN_KEY, withSeen(readKey(SEEN_KEY), id, at));
/** When you last looked at card `id`; 0 for never. */
export const cardSeenAt = (id: string) => seenOf(readKey(SEEN_KEY))[id] ?? 0;

/** The QA run that sent your card by hand back to you, while you haven't opened the card since: the card's latest run
 *  failed it, and the card is yours by hand again. Else null. */
export function backFromQa(task: Pick<Task, "byHand">, runs: Run[] | null, seenAt: number): Run | null {
  if (!task.byHand || !runs?.length) return null;
  const latest = [...runs].sort((a, b) => b.createdAt - a.createdAt)[0]!;
  if (latest.outcome !== "qa_fail") return null;
  return (latest.endedAt ?? latest.createdAt) > seenAt ? latest : null;
}

/** What the card bar says when QA sent the card back. */
export const backSay = (run: Pick<Run, "agentName">, id: string) => `${run.agentName} sent ${id} back to you: what it found is on the card.`;
