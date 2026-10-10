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

import { asksToRun } from "./inbox";

describe("asksToRun (Run this for me, GA-31)", () => {
  const cmds = ["sudo pacman -S libayatana-appindicator"];
  it("is an open card on any hold whose agent asks you to run commands", () => {
    expect(asksToRun({ ...t({ hold: "needs_decision" }), runForMe: cmds })).toBe(true);
    expect(asksToRun({ ...t({ hold: "blocked" }), runForMe: cmds })).toBe(true);
    expect(asksToRun({ ...t({ hold: "needs_decision", stateCategory: "testing" }), runForMe: cmds })).toBe(true);
  });
  it("not without a hold, without commands, or for a card that is done or cancelled", () => {
    expect(asksToRun({ ...t({}), runForMe: cmds })).toBe(false);
    expect(asksToRun({ ...t({ hold: "needs_decision" }), runForMe: [] })).toBe(false);
    expect(asksToRun(t({ hold: "needs_decision" }))).toBe(false);
    expect(asksToRun({ ...t({ hold: "blocked", stateCategory: "done" }), runForMe: cmds })).toBe(false);
    expect(asksToRun({ ...t({ hold: "blocked", stateCategory: "cancelled" }), runForMe: cmds })).toBe(false);
  });
  it("counts once in the Inbox's count, like any held card", () => {
    expect(inboxCount([{ ...t({ hold: "needs_decision" }), runForMe: cmds }, t({ hold: "needs_decision" })], [], "me")).toBe(2);
  });
});

describe("needsYou with the Team Lead (GA-70)", () => {
  const card = (o: Partial<{ hold: string | null; stateCategory: string; assigneeId: string | null; withLead: boolean }>) => ({ ...t({}), ...o });
  it("leaves out a held card whose question is with the Team Lead, and counts it again once the Team Lead asks you", () => {
    expect(needsYou(card({ hold: "needs_decision", withLead: true }), "me")).toBe(false);
    expect(needsYou(card({ hold: "needs_decision", withLead: false }), "me")).toBe(true);
    expect(needsYou(card({ hold: "needs_decision" }), "me")).toBe(true);
  });
  it("still counts a Review card for you, and the Inbox count follows", () => {
    expect(needsYou(card({ hold: "needs_decision", withLead: true, stateCategory: "review", assigneeId: "me" }), "me")).toBe(true);
    expect(inboxCount([card({ hold: "needs_decision", withLead: true }), card({ hold: "needs_decision" }), card({ hold: "blocked" })], [], "me")).toBe(2);
  });
});

// The same rule in gizai-core: `tasks::needs_you` leaves out what `needsYou` leaves out.
import rustTasks from "../../crates/gizai-core/src/tasks.rs?raw";

describe("needsYou and tasks::needs_you (GA-70)", () => {
  it("both leave out a card with the Team Lead", () => {
    const fn = rustTasks.slice(rustTasks.indexOf("pub fn needs_you"), rustTasks.indexOf("pub fn needs_you") + 600);
    expect(fn).toContain("t.hold.is_some() && !t.with_lead");
    expect(fn).toMatch(/"review" \| "deploy"\) && t\.assignee_id\.as_deref\(\) == Some\(you_id\)/);
  });
});
