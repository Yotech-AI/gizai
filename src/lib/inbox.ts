type Card = { hold?: string | null; stateCategory: string; assigneeId?: string | null; runForMe?: string[]; withLead?: boolean };
type Chat = { kind?: string | null; waiting?: boolean; updatedAt: number; pendingActions?: number };

/** GA-102: calls of the Team Lead's wait for your approval in the chat (it couldn't make them after a tool from outside Gizai). */
export function hasPending(t: Chat): boolean {
  return (t.pendingActions ?? 0) > 0;
}

/** Whether a chat waits for you: one the Team Lead started that you haven't answered or dismissed, or one with calls of the Team
 *  Lead's that wait for your approval, whoever started it. */
export function chatWaits(t: Chat): boolean {
  return (!!t.kind && !!t.waiting) || hasPending(t);
}

/** The chats that wait for you (`chatWaits`), newest first: they head the Inbox. */
export function waitingChats<T extends Chat>(threads: T[]): T[] {
  return threads.filter(chatWaits).sort((a, b) => b.updatedAt - a.updatedAt);
}

/** The Inbox count: the cards that need you and the chats that wait for you. */
export function inboxCount(tasks: Card[], threads: Chat[], youId: string): number {
  return tasks.filter((t) => needsYou(t, youId)).length + waitingChats(threads).length;
}

/** "Approval" while calls of the Team Lead's wait for your approval in the chat; else "Question" or "Approval" while a chat the Team
 *  Lead started waits for you, "Team Lead" after; nothing for your own chats. */
export function chatLabel(t: Chat): string | null {
  if (hasPending(t)) return "Approval";
  if (!t.kind) return null;
  if (!t.waiting) return "Team Lead";
  return t.kind === "approval" ? "Approval" : "Question";
}

/** What the label says when you point at it. */
export function chatLabelTitle(t: Chat): string {
  const n = t.pendingActions ?? 0;
  if (n > 0) return n === 1 ? "A call of the Team Lead's waits for your approval here" : `${n} calls of the Team Lead's wait for your approval here`;
  return t.waiting ? "The Team Lead asks you this" : "The Team Lead started this chat";
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
