// GA-43: the Archive button on the board shows only on cards in a Done column. Done is a collapsed rail until it's
// opened, which a server render can't click, so here every rail starts open.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Task, WorkflowState } from "../types";

vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  // The board's open rails start as an empty Set: here, a set that holds every column (all rails open).
  const useState = ((init: unknown) => R.useState(init instanceof Set && init.size === 0 ? { has: () => true } : init)) as typeof R.useState;
  return { ...R, default: { ...R, useState }, useState };
});

const { Board } = await import("./Board");

const states: WorkflowState[] = [
  { id: "s-todo", name: "To do", category: "ready", sortKey: "a1" },
  { id: "s-review", name: "Review", category: "review", sortKey: "a4" },
  { id: "s-deploy", name: "Deploy", category: "deploy", sortKey: "a4V" },
  { id: "s-done", name: "Done", category: "done", sortKey: "a5" },
  { id: "s-cancelled", name: "Cancelled", category: "cancelled", sortKey: "a6" },
];
const task = (n: number, s: WorkflowState): Task => ({
  id: `t${n}`, identifier: `GA-${n}`, projectId: "p1", title: `Card ${n}`, descriptionMd: "", stateId: s.id, stateName: s.name, stateCategory: s.category,
  priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: "a0", testing: true, createdAt: 0, updatedAt: 0,
} as Task);
const tasks = states.map((s, i) => task(i + 1, s)); // GA-1 To do … GA-4 Done, GA-5 Cancelled
const card = (html: string, id: string) => {
  const at = html.indexOf(`data-card="${id}"`);
  const next = html.indexOf("data-card=", at + 1);
  return html.slice(at, next < 0 ? undefined : next);
};

describe("the Archive button on the board", () => {
  const html = renderToStaticMarkup(<Board tasks={tasks} states={states} onMove={() => {}} onOpen={() => {}} onArchive={() => {}} />);

  it("is on a card in Done, labelled with its ID", () => {
    expect(html).toContain('data-col="Done"');
    const done = card(html, "GA-4");
    expect(done).toContain('<button class="btn ghost sm icon-only card-archive" aria-label="Archive GA-4" title="Archive">');
  });

  it("is on no other card, Deploy and Cancelled included", () => {
    for (const id of ["GA-1", "GA-2", "GA-3", "GA-5"]) {
      expect(card(html, id), id).toContain(`data-card="${id}"`);
      expect(card(html, id), id).not.toContain("card-archive");
    }
    expect(html.match(/card-archive/g)).toHaveLength(1);
  });

  it("is not there when the page doesn't offer Archive", () => {
    const plain = renderToStaticMarkup(<Board tasks={tasks} states={states} onMove={() => {}} onOpen={() => {}} />);
    expect(plain).toContain('data-card="GA-4"');
    expect(plain).not.toContain("card-archive");
  });
});
