type Card = { hold?: string | null; stateCategory: string; assigneeId?: string | null };
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

/** The Inbox: open cards on hold (an agent or a gate needs a person), and cards waiting for you in Review or Deploy (merged,
 *  not deployed yet). */
export function needsYou(t: Card, youId: string): boolean {
  if (t.stateCategory === "done" || t.stateCategory === "cancelled") return false;
  return !!t.hold || ((t.stateCategory === "review" || t.stateCategory === "deploy") && t.assigneeId === youId);
}
