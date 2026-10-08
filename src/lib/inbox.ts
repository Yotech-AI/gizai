type Card = { hold?: string | null; stateCategory: string; assigneeId?: string | null };

/** The Inbox: open cards on hold (an agent or a gate needs a person), and cards waiting for you in Review or Deploy (merged,
 *  not deployed yet). */
export function needsYou(t: Card, youId: string): boolean {
  if (t.stateCategory === "done" || t.stateCategory === "cancelled") return false;
  return !!t.hold || ((t.stateCategory === "review" || t.stateCategory === "deploy") && t.assigneeId === youId);
}
