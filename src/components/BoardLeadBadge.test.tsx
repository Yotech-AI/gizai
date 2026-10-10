// GA-70 QA: on the board, a held card whose question is with the Team Lead shows a teal Team Lead badge instead of the
// magenta On hold (it doesn't need you yet); a card the Team Lead escalated is On hold again. Done is a collapsed rail
// until it's opened, which a server render can't click, so here every rail starts open (as in BoardArchive.test).
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Task, WorkflowState } from "../types";

vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => R.useState(init instanceof Set && init.size === 0 ? { has: () => true } : init)) as typeof R.useState;
  return { ...R, default: { ...R, useState }, useState };
});

const { Board } = await import("./Board");

const states: WorkflowState[] = [{ id: "s-prog", name: "In progress", category: "in_progress", sortKey: "a2" }];
const task = (n: number, more: Partial<Task>): Task => ({
  id: `t${n}`, identifier: `GA-${n}`, projectId: "p1", title: `Card ${n}`, descriptionMd: "", stateId: "s-prog", stateName: "In progress",
  stateCategory: "in_progress", priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: `a${n}`, testing: true, createdAt: 0, updatedAt: 0,
  ...more,
} as Task);
const card = (html: string, id: string) => {
  const at = html.indexOf(`data-card="${id}"`);
  const next = html.indexOf("data-card=", at + 1);
  return html.slice(at, next < 0 ? undefined : next);
};

describe("a card with the Team Lead on the board (GA-70)", () => {
  const html = renderToStaticMarkup(<Board states={states} onMove={() => {}} onOpen={() => {}} tasks={[
    task(1, { hold: "needs_decision", holdReason: "CSV or JSON?", withLead: true }),
    task(2, { hold: "needs_decision", holdReason: "Team Lead escalated to you: money", withLead: false }),
    task(3, {}),
  ]} />);

  it("shows the teal Team Lead badge, not On hold", () => {
    const c = card(html, "GA-1");
    expect(c).toContain('<span class="badge live" title="Its question is with the Team Lead">Team Lead</span>');
    expect(c).not.toContain("On hold");
  });
  it("shows On hold once the Team Lead asked you, and nothing on a card without a hold", () => {
    expect(card(html, "GA-2")).toContain('<span class="badge needs">On hold</span>');
    expect(card(html, "GA-2")).not.toContain("badge live");
    expect(card(html, "GA-3")).not.toContain("badge needs");
    expect(card(html, "GA-3")).not.toContain("Team Lead");
  });
});
