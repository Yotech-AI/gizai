type Card = { hold?: string | null; stateCategory: string; assigneeId?: string | null; runForMe?: string[]; withLead?: boolean };
type Chat = { kind?: string | null; waiting?: boolean; updatedAt: number };

/** The chats the Team Lead started that still wait for you, newest first: they head the Inbox. */
export function waitingChats<T extends Chat>(threads: T[]): T[] {
  return threads.filter((t) => !!t.kind && !!t.waiting).sort((a, b) => b.updatedAt - a.updatedAt);
}

/** The Inbox count: the cards that need you and the Team Lead's chats that wait for you. */
export function inboxCount(tasks: Card[], threads: Chat[], youId: string): number {
  return tasks.filter((t) => needsYou(t, youId)).length + waitingChats(threads).length;
}

/** "Question" or "Approval" while the chat waits for you; "Team Lead" after. */
export function chatLabel(t: Chat): string | null {
  if (!t.kind) return null;
  if (!t.waiting) return "Team Lead";
  return t.kind === "approval" ? "Approval" : "Question";
}

/** The Inbox: open cards on hold (an agent or a gate needs a person), except a card whose question is with the Team Lead
 *  (GA-70: it answers, or asks you first), and cards waiting for you in Review or Deploy (merged, not deployed yet). Same
 *  rule as gizai-core's `tasks::needs_you`. */
export function needsYou(t: Card, youId: string): boolean {
  if (t.stateCategory === "done" || t.stateCategory === "cancelled") return false;
  return (!!t.hold && !t.withLead) || ((t.stateCategory === "review" || t.stateCategory === "deploy") && t.assigneeId === youId);
}

/** Run this for me: an open card on hold whose agent asks you to run commands it may not run. The Inbox shows these on
 *  top, each with its commands and Done, continue, instead of in the list. */
export function asksToRun(t: Card): boolean {
  return needsYou(t, "") && !!t.hold && (t.runForMe?.length ?? 0) > 0;
}
