// Team → Workflow: what a column says about itself (its line under the column, the board's note), the kinds Add column
// offers, a column's new place after a drag, and the colours labels pick from. The backend (gizai-core columns.rs)
// decides; these only put it into words.
import type { Branch, LabelInfo, Member, WorkflowState } from "../types";
import { ROLES, roleLabel } from "./agents";

/** Backlog, Review, Done and Cancelled take no agents and are never Auto (gizai-core `columns::NO_AGENTS`). */
export const NO_AGENTS = ["backlog", "review", "done", "cancelled"];
export const takesAgents = (category: string) => !NO_AGENTS.includes(category);

/** Add column's kinds in plain words, as `addState` takes them, with the name a column of that kind usually has. */
export const KINDS: { kind: string; label: string; name: string }[] = [
  { kind: "waiting", label: "Waiting, like To do", name: "To do" },
  { kind: "work", label: "Work, like In progress", name: "In progress" },
  { kind: "testing", label: "Testing", name: "Testing" },
  { kind: "review", label: "Review", name: "Review" },
  { kind: "deploy", label: "Deploy", name: "Deploy" },
  { kind: "done", label: "Done", name: "Done" },
  { kind: "backlog", label: "Backlog", name: "Backlog" },
];

/** The columns in board order. */
export const boardOrder = (states: WorkflowState[]) => [...states].sort((a, b) => (a.sortKey < b.sortKey ? -1 : a.sortKey > b.sortKey ? 1 : 0));

/** "A", "A and B", "A, B and C". */
export function andList(names: string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** The names of the agents on a column, in its order (agents no longer on the team are left out). */
export function agentNames(s: WorkflowState, members: Member[]): string[] {
  return (s.agentIds ?? []).map((id) => members.find((m) => m.actorId === id)?.name).filter((n): n is string => !!n);
}

/** Where a merged card goes from Review: its next column, else the team's first Deploy column, else its first Done column. */
export function mergedTarget(review: WorkflowState, states: WorkflowState[]): WorkflowState | undefined {
  const order = boardOrder(states);
  return order.find((s) => s.id === review.nextStateId) ?? order.find((s) => s.category === "deploy") ?? order.find((s) => s.category === "done");
}

/** The line under a column in Team → Workflow: who takes its cards and where they go next. */
export function columnLine(s: WorkflowState, states: WorkflowState[], members: Member[]): string {
  const next = states.find((x) => x.id === s.nextStateId)?.name;
  if (s.category === "review") return `You review and merge; merged cards go to ${mergedTarget(s, states)?.name ?? "the next column"}`;
  if (s.category === "backlog") {
    const work = boardOrder(states).find((x) => x.category === "in_progress");
    return work ? `New cards wait here; no agent takes them. Run on a card starts it in ${work.name}` : "New cards wait here; no agent takes them";
  }
  if (s.category === "done") return "Finished cards; no agent takes them";
  if (s.category === "cancelled") return "Cards nobody will work on; no agent takes them";
  const names = agentNames(s, members);
  const one = names.length === 1;
  if (s.auto) {
    if (names.length === 0) return "Auto, but no agent is on this column: its cards wait. Drag an agent here or use + Agent";
    if (!next) return `Auto needs a next column: pick where ${andList(names)} ${one ? "moves" : "move"} the cards`;
    const who = `Auto: ${andList(names)}`;
    if (s.category === "ready") return `${who} ${one ? "takes" : "take"} cards by priority and ${one ? "moves" : "move"} them to ${next}`;
    if (s.category === "testing") return `${who} ${one ? "tests" : "test"} cards by priority and ${one ? "moves" : "move"} them to ${next} when they pass; a fail goes back`;
    if (s.category === "deploy") return `${who} ${one ? "takes" : "take"} cards by priority and ${one ? "moves" : "move"} them to ${next} once deployed`;
    return `${who} ${one ? "takes" : "take"} cards by priority and ${one ? "moves" : "move"} them to ${next} when done`;
  }
  const done = s.category === "ready" ? "started" : s.category === "testing" ? "passed" : s.category === "deploy" ? "deployed" : "done";
  const onward = next ? `; ${done} cards go to ${next}` : "";
  if (names.length === 0) return `Manual: press Run on a card and pick an agent${onward}`;
  return `Manual: press Run on a card to start ${names[0]}${onward}`;
}

/** The note under a column's name on the board: your review, or who starts its cards (its agents, or Manual). */
export function boardNote(s: WorkflowState, members?: Member[]): string | undefined {
  if (s.category === "review") return "Waiting for your review";
  if (!takesAgents(s.category)) return undefined;
  const n = s.agentIds?.length ?? 0;
  if (s.auto) {
    if (n === 0) return "Auto, but no agent is on this column";
    const names = members ? agentNames(s, members) : [];
    return names.length ? `Auto: ${andList(names)}` : "Auto: picked up by the agents on this column";
  }
  return n ? "Manual: press Run on a card" : undefined;
}

/** After a column was dragged to `index` in `order` (board order with it already in place): the column it now goes after,
 * "" for the front (as `setColumn`'s afterId takes it). */
export function afterIdAt(order: string[], index: number): string {
  return index > 0 ? order[index - 1] : "";
}

/** Why a column's bin is off, as far as the screen can tell before asking: the last Backlog or Done column. The backend's
 * `columnRemoval` also knows about a card an agent is working on. */
export function lastOfKind(s: WorkflowState, states: WorkflowState[]): string | null {
  if (s.category !== "backlog" && s.category !== "done") return null;
  if (states.filter((x) => x.category === s.category).length > 1) return null;
  return s.category === "backlog" ? `${s.name} is the team's last Backlog column: new cards need it` : `${s.name} is the team's last Done column: finished cards need it`;
}

/** The colours labels pick from (the design system's c-* colours). */
export const LABEL_COLORS = ["#6f97ff", "#3fb8a0", "#e3bd3c", "#a98bfa", "#f29a4a", "#e77ab3", "#f2706b", "#8a8e98"];

/** A colour for a new label: the first of the palette no label has yet, else the next one round. */
export function nextLabelColor(labels: { color?: string | null }[]): string {
  const used = new Set(labels.map((l) => l.color?.toLowerCase()));
  return LABEL_COLORS.find((c) => !used.has(c)) ?? LABEL_COLORS[labels.length % LABEL_COLORS.length];
}

/** "4 cards", "1 card", "no cards". */
export const cardCount = (n: number) => (n === 0 ? "no cards" : n === 1 ? "1 card" : `${n} cards`);

/** A label's name is already in use, ignoring case (the backend refuses it too, with its reason). */
export const labelTaken = (name: string, labels: LabelInfo[] | { id: string; name: string }[], except?: string) =>
  labels.some((l) => l.id !== except && l.name.toLowerCase() === name.trim().toLowerCase());

/** What an empty spot in a branch fills in: the first of the branch's usual roles no agent has yet (else its first
 * role), named like the usual agent and made unique ("Frontend Agent 2"). */
export function branchPreset(b: Branch, members: Member[]): { name: string; role: string } {
  const agents = members.filter((m) => m.kind === "agent");
  const known = b.roles.filter((r) => ROLES.includes(r));
  const role = known.find((r) => !agents.some((m) => m.roleKey === r)) ?? known[0] ?? b.roles[0] ?? b.key;
  const base = `${ROLES.includes(role) ? roleLabel(role) : b.name} Agent`;
  const taken = new Set(agents.map((m) => m.name.toLowerCase()));
  let name = base;
  for (let i = 2; taken.has(name.toLowerCase()); i++) name = `${base} ${i}`;
  return { name, role };
}
