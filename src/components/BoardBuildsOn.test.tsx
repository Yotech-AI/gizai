// GA-228 QA: on the board, a card that waits for cards it builds on (its waitsFor: those not merged yet) shows a "Waits for"
// badge: "Waits for GA-1" for one, "Waits for 2 cards" for several, with a tooltip naming the cards and their titles from
// the board. Once they are merged (waitsFor empty) the badge goes; a card in Done never shows it. Done is a collapsed rail
// until it's opened, which a server render can't click, so here every rail starts open (as in BoardByHand.test).
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
  { id: "s-todo", name: "To do", category: "ready", sortKey: "a1", auto: true, agentIds: ["backend"] },
  { id: "s-prog", name: "In progress", category: "in_progress", sortKey: "a2", auto: true, agentIds: ["backend"] },
  { id: "s-done", name: "Done", category: "done", sortKey: "a3", auto: false, agentIds: [] },
];
const task = (n: number, more: Partial<Task>): Task => ({
  id: `t${n}`, identifier: `GA-${n}`, projectId: "p1", title: `Card ${n}`, descriptionMd: "", stateId: "s-todo", stateName: "To do",
  stateCategory: "ready", priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: `a${n}`, testing: true, createdAt: 0, updatedAt: 0,
  byHand: false, screenshots: null, ...more,
} as Task);
const card = (html: string, id: string) => {
  const at = html.indexOf(`data-card="${id}"`);
  const next = html.indexOf("data-card=", at + 1);
  return html.slice(at, next < 0 ? undefined : next);
};
const text = (s: string) => s.replace(/&#x27;/g, "'").replace(/&amp;/g, "&").replace(/&#10;|&#xA;/gi, "\n");
/** The card's "Waits for" badge: its tooltip and its words, or null. */
const badge = (html: string, id: string) => {
  const m = /<span class="badge waits" title="([^"]*)">(.*?)<\/span>/s.exec(card(html, id));
  return m ? { title: text(m[1]), words: text(m[2].replace(/<svg[\s\S]*?<\/svg>/, "")) , icon: m[2] } : null;
};
const render = (tasks: Task[]) => renderToStaticMarkup(<Board states={states} onMove={() => {}} onOpen={() => {}} working={new Map()} tasks={tasks} />);

describe("the Waits for badge on the board (GA-228)", () => {
  const html = render([
    task(1, { title: "Team templates (backend)", stateId: "s-prog", stateName: "In progress", stateCategory: "in_progress" }),
    task(2, { title: "Team templates (screens)" }),
    task(3, { title: "Team templates (docs)", waitsFor: ["GA-1"] }),
    task(4, { title: "Team templates (release)", waitsFor: ["GA-1", "GA-2"], labels: [{ id: "l1", name: "bug", color: "#f00" }] as Task["labels"] }),
    task(5, { title: "Merged base", waitsFor: [] }),
    task(6, { title: "Already done", stateId: "s-done", stateName: "Done", stateCategory: "done", waitsFor: ["GA-1"] }),
    task(7, { title: "Builds on another project's card", waitsFor: ["KADE-9"] }),
  ]);

  it("names the one card it waits for, with an hourglass, and a tooltip naming it with its title", () => {
    const b = badge(html, "GA-3");
    expect(b).not.toBeNull();
    expect(b!.words).toBe("Waits for GA-1");
    expect(b!.icon).toMatch(/lucide-hourglass/);
    expect(b!.title).toBe("Waits for GA-1 Team templates (backend) to be merged");
  });

  it("counts several, and its tooltip names each on a line of its own, before the labels", () => {
    const b = badge(html, "GA-4");
    expect(b!.words).toBe("Waits for 2 cards");
    expect(b!.title).toBe("Waits for these cards to be merged:\nGA-1 Team templates (backend)\nGA-2 Team templates (screens)");
    const c = card(html, "GA-4");
    expect(c.indexOf("Waits for 2 cards")).toBeLessThan(c.indexOf("bug"));
  });

  it("names a card the board doesn't show by its ID", () => {
    expect(badge(html, "GA-7")).toEqual(expect.objectContaining({ words: "Waits for KADE-9", title: "Waits for KADE-9 to be merged" }));
  });

  it("shows no badge once the cards are merged, on a card without Builds on, nor on a card in Done", () => {
    expect(badge(html, "GA-5")).toBeNull();
    expect(card(html, "GA-5")).not.toContain("Waits for");
    expect(card(html, "GA-5")).not.toContain('class="meta"');
    expect(badge(html, "GA-1")).toBeNull();
    expect(badge(html, "GA-2")).toBeNull();
    expect(card(html, "GA-6")).toContain("Already done");
    expect(badge(html, "GA-6")).toBeNull();
  });

  it("goes when the same card renders again with its cards merged", () => {
    const before = render([task(1, { title: "Base" }), task(3, { title: "Next", waitsFor: ["GA-1"] })]);
    expect(badge(before, "GA-3")!.words).toBe("Waits for GA-1");
    const after = render([task(1, { title: "Base", stateId: "s-done", stateName: "Done", stateCategory: "done" }), task(3, { title: "Next", waitsFor: [] })]);
    expect(badge(after, "GA-3")).toBeNull();
    expect(card(after, "GA-3")).toContain("Next");
  });
});
