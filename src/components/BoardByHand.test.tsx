// GA-115 QA: on the board, a card pinned to a person says "By hand" (Lucide keyboard) before the person's avatar, with
// whose it is as its tooltip, and never a working strip; a card that isn't by hand has no badge. Done is a collapsed rail
// until it's opened, which a server render can't click, so here every rail starts open (as in BoardLeadBadge.test).
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Task, WorkflowState } from "../types";

vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => R.useState(init instanceof Set && init.size === 0 ? { has: () => true } : init)) as typeof R.useState;
  return { ...R, default: { ...R, useState }, useState };
});

const { Board } = await import("./Board");

const states: WorkflowState[] = [
  { id: "s-prog", name: "In progress", category: "started", sortKey: "a2", auto: true, agentIds: ["backend"] },
  { id: "s-test", name: "Testing", category: "testing", sortKey: "a3", auto: true, agentIds: ["qa"] },
];
const task = (n: number, more: Partial<Task>): Task => ({
  id: `t${n}`, identifier: `KADE-${n}`, projectId: "p1", title: `Card ${n}`, descriptionMd: "", stateId: "s-prog", stateName: "In progress",
  stateCategory: "started", priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: `a${n}`, testing: true, createdAt: 0, updatedAt: 0,
  byHand: false, ...more,
} as Task);
const card = (html: string, id: string) => {
  const at = html.indexOf(`data-card="${id}"`);
  const next = html.indexOf("data-card=", at + 1);
  return html.slice(at, next < 0 ? undefined : next);
};

describe("By hand on the board's card (GA-115)", () => {
  const html = renderToStaticMarkup(<Board states={states} onMove={() => {}} onOpen={() => {}} working={new Map([["t3", "Backend Agent"]])} tasks={[
    task(1, { byHand: true, assigneeId: "you", assigneeName: "Jeffrey Sevinga", assigneeKind: "user" }),
    task(2, { assigneeId: "you", assigneeName: "Jeffrey Sevinga", assigneeKind: "user" }),
    task(3, { assigneeId: "backend", assigneeName: "Backend Agent", assigneeKind: "agent" }),
    task(4, { byHand: true, labels: [{ id: "l1", name: "bug", color: "#f00" }] as Task["labels"] }),
  ]} />);

  it("shows the badge with the keyboard icon and whose card it is, before the person's avatar", () => {
    const c = card(html, "KADE-1");
    expect(c).toContain('<span class="badge by-hand" title="Pinned to Jeffrey Sevinga: agents leave it alone until it&#x27;s handed over">');
    expect(c).toMatch(/class="badge by-hand"[^>]*><svg[^>]*lucide-keyboard[^>]*>.*?<\/svg>By hand<\/span>/);
    expect(c.indexOf("By hand")).toBeLessThan(c.indexOf('class="avatar'));
    expect(c).not.toContain('class="working"');
  });

  it("no badge on a card assigned to you that isn't by hand, nor on an agent's card", () => {
    expect(card(html, "KADE-2")).not.toContain("By hand");
    expect(card(html, "KADE-3")).not.toContain("By hand");
    expect(card(html, "KADE-3")).toContain("Backend Agent is working");
  });

  it("a card by hand without an assignee name still has the meta row, after its labels", () => {
    const c = card(html, "KADE-4");
    expect(c).toContain('<div class="meta">');
    expect(c.indexOf("bug")).toBeLessThan(c.indexOf("By hand"));
    expect(c).toContain("Pinned to a person");
  });
});
