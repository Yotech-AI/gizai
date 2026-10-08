import { describe, expect, it } from "vitest";
import { needsYou } from "./inbox";

const t = (o: Partial<{ hold: string | null; stateCategory: string; assigneeId: string | null }>) =>
  ({ hold: null, stateCategory: "ready", assigneeId: null, ...o });

describe("needsYou", () => {
  it("counts cards on hold and cards waiting in Review for you", () => {
    expect(needsYou(t({ hold: "needs_decision" }), "me")).toBe(true);
    expect(needsYou(t({ stateCategory: "review", assigneeId: "me" }), "me")).toBe(true);
  });
  it("skips Review cards for someone else and ordinary cards", () => {
    expect(needsYou(t({ stateCategory: "review", assigneeId: "sanne" }), "me")).toBe(false);
    expect(needsYou(t({ stateCategory: "in_progress", assigneeId: "me" }), "me")).toBe(false);
    expect(needsYou(t({ hold: "blocked", stateCategory: "done" }), "me")).toBe(false);
  });
});

import { chatLabel, inboxCount, waitingChats } from "./inbox";

const chat = (o: Partial<{ id: string; kind: string | null; waiting: boolean; updatedAt: number }>) =>
  ({ id: "c", kind: null, waiting: false, updatedAt: 0, ...o });

describe("Team Lead chats in the Inbox", () => {
  it("lists only the Team Lead's chats that still wait for you, newest first", () => {
    const threads = [
      chat({ id: "mine", updatedAt: 9 }),
      chat({ id: "old", kind: "question", waiting: true, updatedAt: 1 }),
      chat({ id: "answered", kind: "approval", waiting: false, updatedAt: 8 }),
      chat({ id: "new", kind: "approval", waiting: true, updatedAt: 5 }),
    ];
    expect(waitingChats(threads).map((t) => t.id)).toEqual(["new", "old"]);
  });
  it("counts the cards that need you and the waiting chats", () => {
    const cards = [t({ hold: "needs_decision" }), t({ stateCategory: "in_progress" })];
    const threads = [chat({ kind: "question", waiting: true }), chat({ kind: "approval", waiting: false }), chat({})];
    expect(inboxCount(cards, threads, "me")).toBe(2);
    expect(inboxCount([], threads, "me")).toBe(1);
    expect(inboxCount([], [], "me")).toBe(0);
  });
});

describe("chatLabel (Chat → Recent)", () => {
  it("says Question or Approval while the chat waits for you, Team Lead after, nothing for your own chats", () => {
    expect(chatLabel(chat({ kind: "question", waiting: true }))).toBe("Question");
    expect(chatLabel(chat({ kind: "approval", waiting: true }))).toBe("Approval");
    expect(chatLabel(chat({ kind: "approval", waiting: false }))).toBe("Team Lead");
    expect(chatLabel(chat({ kind: "question", waiting: false }))).toBe("Team Lead");
    expect(chatLabel(chat({}))).toBeNull();
  });
});
