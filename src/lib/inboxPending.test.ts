// GA-102: a chat with calls of the Team Lead's that wait for your approval waits for you in the Inbox, whoever started it,
// labelled Approval, and counts once.
import { describe, expect, it } from "vitest";
import { chatLabel, chatLabelTitle, chatWaits, hasPending, inboxCount, waitingChats } from "./inbox";

const chat = (o: Partial<{ id: string; kind: string | null; waiting: boolean; updatedAt: number; pendingActions: number }>) =>
  ({ id: "c", kind: null, waiting: false, updatedAt: 0, ...o });

describe("chats with calls that wait for your approval", () => {
  it("wait for you, your own chats too, until each call was approved or discarded", () => {
    expect(hasPending(chat({ pendingActions: 1 }))).toBe(true);
    expect(hasPending(chat({ pendingActions: 0 }))).toBe(false);
    expect(hasPending(chat({}))).toBe(false);
    expect(chatWaits(chat({ pendingActions: 2 }))).toBe(true);
    expect(chatWaits(chat({ kind: "approval", waiting: false, pendingActions: 1 }))).toBe(true);
    expect(chatWaits(chat({}))).toBe(false);
    expect(chatWaits(chat({ kind: "question", waiting: true }))).toBe(true);
    expect(chatWaits(chat({ kind: "question", waiting: false }))).toBe(false);
  });

  it("head the Inbox with the Team Lead's questions, newest first, each once", () => {
    const threads = [
      chat({ id: "mine", updatedAt: 9 }),
      chat({ id: "mine-pending", updatedAt: 4, pendingActions: 1 }),
      chat({ id: "question", kind: "question", waiting: true, updatedAt: 6 }),
      chat({ id: "both", kind: "approval", waiting: true, updatedAt: 8, pendingActions: 3 }),
      chat({ id: "answered-pending", kind: "question", waiting: false, updatedAt: 2, pendingActions: 1 }),
      chat({ id: "answered", kind: "approval", waiting: false, updatedAt: 7 }),
    ];
    expect(waitingChats(threads).map((t) => t.id)).toEqual(["both", "question", "mine-pending", "answered-pending"]);
    expect(inboxCount([{ hold: "blocked", stateCategory: "ready" }], threads, "you")).toBe(5);
  });

  it("are labelled Approval, and the label says how many calls wait", () => {
    expect(chatLabel(chat({ pendingActions: 1 }))).toBe("Approval");
    expect(chatLabel(chat({ kind: "question", waiting: true, pendingActions: 1 }))).toBe("Approval");
    expect(chatLabel(chat({ kind: "question", waiting: true }))).toBe("Question");
    expect(chatLabel(chat({ kind: "question", waiting: false }))).toBe("Team Lead");
    expect(chatLabel(chat({}))).toBeNull();
    expect(chatLabelTitle(chat({ pendingActions: 1 }))).toBe("A call of the Team Lead's waits for your approval here");
    expect(chatLabelTitle(chat({ pendingActions: 3 }))).toBe("3 calls of the Team Lead's wait for your approval here");
    expect(chatLabelTitle(chat({ kind: "question", waiting: true }))).toBe("The Team Lead asks you this");
    expect(chatLabelTitle(chat({ kind: "question", waiting: false }))).toBe("The Team Lead started this chat");
  });
});
