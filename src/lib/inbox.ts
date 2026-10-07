type Card = { hold?: string | null; stateCategory: string; assigneeId?: string | null };

/** The Inbox: open cards on hold (an agent or a gate needs a person) and cards waiting in Review for you. */
export function needsYou(t: Card, youId: string): boolean {
  if (t.stateCategory === "done" || t.stateCategory === "cancelled") return false;
  return !!t.hold || (t.stateCategory === "review" && t.assigneeId === youId);
}
