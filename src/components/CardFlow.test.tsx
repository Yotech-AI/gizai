// GA-32: the Testing switch in the New task drawer and in Properties, and how a Deploy card and a deployed run show.
// Rendered to HTML on the server, so no data loads.
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { NewTaskDrawer } from "./NewTaskDrawer";
import { Properties } from "./Properties";
import { Board } from "./Board";
import { CATEGORIES, CATEGORY_NAMES, StatusIcon } from "./StatusIcon";
import { badgeOf } from "../lib/runs";
import { needsYou } from "../lib/inbox";
import type { Run, Task, Team } from "../types";

const team: Team = {
  id: "team", name: "Software", members: [], labels: [],
  states: [
    { id: "s-review", name: "Review", category: "review", sortKey: "a4", nextStateId: "s-deploy" },
    // GA-49: the column decides: Deploy is Manual, with the DevOps Agent on it.
    { id: "s-deploy", name: "Deploy", category: "deploy", sortKey: "a4V", auto: false, agentIds: ["devops"], nextStateId: "s-done" },
    { id: "s-done", name: "Done", category: "done", sortKey: "a5" },
  ],
};
const task = (o: Partial<Task>): Task => ({
  id: "t1", identifier: "KADE-1", projectId: "p1", title: "Fix a typo", stateId: "s-deploy", stateName: "Deploy", stateCategory: "deploy",
  priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: "a0", testing: true, createdAt: 0, updatedAt: 0, ...o,
} as Task);

describe("the Testing switch", () => {
  it("is in the New task drawer's Task section, on by default, with its hint", () => {
    const html = renderToStaticMarkup(<NewTaskDrawer onClose={() => {}} />);
    expect(html).toMatch(/<input id="t-testing" type="checkbox" checked=""\/>Test before Review/);
    expect(html).toContain("On: the QA Agent tests the card before Review. Turn it off for a small UI fix or a bug fix");
    expect(html.indexOf("t-testing")).toBeLessThan(html.indexOf("Description"));
  });

  it("shows in Properties as the card has it", () => {
    const on = renderToStaticMarkup(<Properties task={task({ testing: true })} team={team} people={[]} onError={() => {}} onClose={() => {}} />);
    expect(on).toMatch(/Testing<\/span><label[^>]*><input type="checkbox" checked=""\/>Test before Review/);
    const off = renderToStaticMarkup(<Properties task={task({ testing: false })} team={team} people={[]} onError={() => {}} onClose={() => {}} />);
    expect(off).toMatch(/Testing<\/span><label[^>]*><input type="checkbox"\/>Test before Review/);
    expect(off).toContain("Off: the card skips Testing and goes straight to Review");
  });
});

describe("Deploy", () => {
  it("has its own glyph and name, between Review and Done", () => {
    expect(CATEGORY_NAMES.deploy).toBe("Deploy");
    expect(CATEGORIES.indexOf("deploy")).toBe(CATEGORIES.indexOf("review") + 1);
    expect(CATEGORIES.indexOf("done")).toBe(CATEGORIES.indexOf("deploy") + 1);
    const deploy = renderToStaticMarkup(<StatusIcon category="deploy" />);
    const todo = renderToStaticMarkup(<StatusIcon category="ready" />);
    expect(deploy).not.toBe(todo);
    expect(deploy).toContain("Deploy");
  });

  it("shows on the board between Review and Done with its own note, not 'Waiting for your review'", () => {
    const html = renderToStaticMarkup(<Board tasks={[task({})]} states={team.states} onMove={() => {}} onOpen={() => {}} />);
    const at = (name: string) => html.indexOf(`data-col="${name}"`);
    expect(at("Review")).toBeGreaterThanOrEqual(0);
    expect(at("Review")).toBeLessThan(at("Deploy"));
    expect(at("Deploy")).toBeLessThan(at("Done"));
    const deployCol = html.slice(at("Deploy"), at("Done"));
    // GA-49: the note follows the column (Manual, its agents), not GA-32's "press Run for the DevOps Agent".
    expect(deployCol).toContain('<div class="col-note">Manual: press Run on a card</div>');
    expect(deployCol).not.toContain("Waiting for your review");
    expect(deployCol).toContain("Fix a typo");
    expect(html.slice(at("Review"), at("Deploy"))).toContain("Waiting for your review");
    // a Deploy column without agents says nothing about Run or review
    const bare = renderToStaticMarkup(<Board tasks={[task({})]} states={team.states.map((s) => s.id === "s-deploy" ? { ...s, agentIds: [] } : s)} onMove={() => {}} onOpen={() => {}} />);
    const bareDeploy = bare.slice(bare.indexOf('data-col="Deploy"'), bare.indexOf('data-col="Done"'));
    expect(bareDeploy).not.toContain("col-note");
  });

  it("puts a Deploy card assigned to you in the inbox, like a Review card", () => {
    expect(needsYou({ stateCategory: "deploy", assigneeId: "me", hold: null }, "me")).toBe(true);
    expect(needsYou({ stateCategory: "deploy", assigneeId: "devops", hold: null }, "me")).toBe(false);
    expect(needsYou({ stateCategory: "deploy", assigneeId: null, hold: "needs_decision" }, "me")).toBe(true);
  });

  it("shows a deployed run as Deployed", () => {
    const run = { id: "r1", agentId: "a", agentName: "DevOps Agent", trigger: "manual", status: "succeeded", outcome: "deployed",
      createdAt: 0, costUsdMicros: 0, inputTokens: 0, outputTokens: 0, logPath: "" } as Run;
    expect(badgeOf(run)).toEqual({ cls: "ok", text: "Deployed" });
  });
});
