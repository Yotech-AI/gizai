// GA-185 QA: the screenshots sign. On the board only a card in Review whose newest round has screenshots shows the images
// sign with the count, right after its pull request's (GA-181's 174:3626); a card elsewhere shows none, also with a count.
// In the Inbox its row says "6 screenshots" and opens the card's Screenshots tab (174:4544). Rendered on the server;
// Done and the other rails start open, as in BoardArchive.test.tsx.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Task, WorkflowState } from "../../types";

vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => R.useState(init instanceof Set && init.size === 0 ? { has: () => true } : init)) as typeof R.useState;
  return { ...R, default: { ...R, useState }, useState };
});

const { Board } = await import("../Board");
const { TaskList } = await import("../TaskList");

const states: WorkflowState[] = [
  { id: "s-prog", name: "In progress", category: "started", sortKey: "a2" },
  { id: "s-test", name: "Testing", category: "testing", sortKey: "a3" },
  { id: "s-review", name: "Review", category: "review", sortKey: "a4" },
  { id: "s-done", name: "Done", category: "done", sortKey: "a5" },
];
const task = (id: string, s: WorkflowState, pr = false): Task => ({
  id, identifier: id.toUpperCase(), projectId: "p1", title: `Card ${id}`, descriptionMd: "", stateId: s.id, stateName: s.name, stateCategory: s.category,
  priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: "a0", testing: true, createdAt: 0, updatedAt: 0, byHand: false, screenshots: null,
  ...(pr ? { prUrl: "https://github.com/o/r/pull/41", prState: "open" } : {}),
} as Task);
const [prog, test, review, reviewNone, done] = [task("gw-1", states[0]), task("gw-2", states[1]), task("gw-8", states[2], true), task("ga-176", states[2], true),
  task("gw-4", states[3])];
const tasks = [prog, test, review, reviewNone, done];
// counts for every column: the board still shows only Review's
const shots = new Map([["gw-1", 3], ["gw-2", 2], ["gw-8", 6], ["gw-4", 5]]);
const card = (html: string, id: string) => {
  const at = html.indexOf(`data-card="${id}"`);
  const next = html.indexOf("data-card=", at + 1);
  return html.slice(at, next < 0 ? undefined : next);
};

describe("the sign on the board", () => {
  const html = renderToStaticMarkup(<Board tasks={tasks} states={states} onMove={() => {}} onOpen={() => {}} shots={shots} />);

  it("is on a Review card with screenshots: the images icon and the count, right after the pull request's sign", () => {
    const c = card(html, "GW-8");
    expect(c).toMatch(/<button type="button" class="shot-sign" title="6 screenshots: open the Screenshots tab" aria-label="6 screenshots: open the Screenshots tab"><svg[^>]*lucide-images[^>]*>.*?<\/svg>6<\/button>/);
    expect(c.indexOf("shot-sign")).toBeGreaterThan(c.indexOf("PR #41"));
    expect(c.slice(c.indexOf("PR #41"), c.indexOf("shot-sign"))).not.toMatch(/<span class="(?!badge)/);
  });
  it("is on no Review card without screenshots, and on no card in another column", () => {
    for (const id of ["GA-176", "GW-1", "GW-2", "GW-4"]) expect(card(html, id), id).not.toContain("shot-sign");
    expect(html.match(/shot-sign/g)).toHaveLength(1);
  });
  it("is not there without counts (the board as before)", () => {
    expect(renderToStaticMarkup(<Board tasks={tasks} states={states} onMove={() => {}} onOpen={() => {}} />)).not.toContain("shot-sign");
  });
});

describe("the sign in the Inbox", () => {
  // without group heads: the open-rails stand-in above would fold every group
  const groups = [{ key: "review", label: "Review", tasks: [review, reviewNone] }];
  const html = renderToStaticMarkup(<TaskList groups={groups as never} showHeads={false} live={new Map()} shots={new Map([["gw-8", 6]])} />);
  const row = (id: string) => html.match(new RegExp(`<a id="row-${id}"[^>]*>[\\s\\S]*?</a>`))?.[0] ?? "";

  it("says 6 screenshots in the row, after the title, and the row opens the Screenshots tab", () => {
    expect(row("gw-8")).toContain('href="#/task/gw-8/screenshots"');
    expect(row("gw-8")).toMatch(/<span class="title"><span>Card gw-8<\/span><span class="shot-sign" title="6 screenshots: open the Screenshots tab"><svg[^>]*lucide-images[^>]*>.*?<\/svg>6 screenshots<\/span><\/span>/);
  });
  it("a row without screenshots has no sign and opens the card as before", () => {
    expect(row("ga-176")).toContain('href="#/task/ga-176"');
    expect(row("ga-176")).not.toContain("shot-sign");
  });
});
