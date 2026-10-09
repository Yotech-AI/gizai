// GA-53: the Team page's screens, rendered to HTML on the server, so no data loads and nothing is clicked (the UI test,
// scripts/ui-test.sh, clicks them in the app): Team → Workflow's columns, the Labels section, the organisation chart,
// the agent form without a wake-up, "New label…" in the New task drawer, and the board's column notes.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { DndContext } from "@dnd-kit/core";
import type { ColumnRemoval, LabelInfo, Member, Team, Task, WorkflowState } from "../types";

// The bins ask the backend why they are off (columnRemoval) after the first render: here the answers are there at once.
let removals: Record<string, ColumnRemoval> = {};
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => R.useState(init !== null && typeof init === "object" && Object.getPrototypeOf(init) === Object.prototype
    && Object.keys(init as object).length === 0 && Object.keys(removals).length ? removals : init)) as typeof R.useState;
  return { ...R, default: { ...R, useState }, useState };
});
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));
let labels: LabelInfo[] = [];
vi.mock("../lib/useData", () => ({ useData: () => ({ data: labels, error: null, reload: () => {}, setData: () => {} }) }));
vi.mock("../lib/useClis", () => ({ useClis: () => [] }));

const { ColumnEditor } = await import("./ColumnEditor");
const { LabelsEditor } = await import("./LabelsEditor");
const { OrgChart } = await import("./OrgChart");
const { AgentDrawer } = await import("./AgentForm");
const { NewTaskDrawer } = await import("./NewTaskDrawer");
const { NewLabel } = await import("./NewLabel");
const { Board } = await import("./Board");

const agent = (actorId: string, name: string, roleKey: string, extra: Partial<Member> = {}): Member => ({
  actorId, name, kind: "agent", roleKey, handle: actorId, status: "active", isLead: false, allowedTools: [], chatEnabled: false, ...extra,
});
const members = [agent("lead", "Team Lead", "lead", { chatEnabled: true, isLead: true }), agent("be2", "Backend Agent 2", "backend"),
  agent("fe2", "Frontend Agent 2", "frontend"), agent("qa2", "QA Agent 2", "qa"), agent("ops2", "DevOps Agent 2", "devops"),
  { ...agent("you", "Jeffrey", "reviewer"), kind: "person" }];
// GA-49's table (the "Done when" of GA-53), plus an Auto column nobody is on yet.
const states: WorkflowState[] = [
  { id: "backlog", name: "Backlog", category: "backlog", sortKey: "a0" },
  { id: "todo", name: "To do", category: "ready", sortKey: "a1", auto: true, nextStateId: "doing", agentIds: ["be2", "fe2"] },
  { id: "doing", name: "In progress", category: "in_progress", sortKey: "a2", auto: true, nextStateId: "testing", agentIds: ["be2", "fe2"] },
  { id: "testing", name: "Testing", category: "testing", sortKey: "a3", auto: true, nextStateId: "review", agentIds: ["qa2"] },
  { id: "review", name: "Review", category: "review", sortKey: "a4", nextStateId: "deploy" },
  { id: "deploy", name: "Deploy", category: "deploy", sortKey: "a5", auto: false, nextStateId: "done", agentIds: ["ops2"] },
  { id: "design", name: "Design review", category: "testing", sortKey: "a55", auto: true, nextStateId: "done", agentIds: [] },
  { id: "done", name: "Done", category: "done", sortKey: "a6" },
  { id: "cancelled", name: "Cancelled", category: "cancelled", sortKey: "a7" },
];
const team: Team = { id: "team", name: "Software", members, states, labels: [] };
const order = states.map((s) => s.id);

const editor = (o: { adding?: boolean; error?: { id: string; text: string } | null } = {}) => renderToStaticMarkup(
  <DndContext><ColumnEditor team={team} order={order} error={o.error ?? null} onError={() => {}} overId={null} adding={!!o.adding} onAddingDone={() => {}} /></DndContext>);
/** One column's row in the editor's HTML. */
const row = (html: string, name: string) => {
  const at = html.indexOf(`data-column="${name}"`);
  expect(at, name).toBeGreaterThanOrEqual(0);
  const next = html.indexOf("data-column=", at + 1);
  return html.slice(at, next < 0 ? undefined : next);
};
const esc = (s: string) => s.replace(/'/g, "&#x27;");

describe("Team → Workflow", () => {
  const html = editor();

  it("shows the columns in board order", () => {
    const names = [...html.matchAll(/data-column="([^"]+)"/g)].map((m) => m[1]);
    expect(names).toEqual(["Backlog", "To do", "In progress", "Testing", "Review", "Deploy", "Design review", "Done", "Cancelled"]);
    for (const s of states) expect(row(html, s.name)).toContain(`aria-label="Move ${s.name}"`);
  });

  it("shows each column's agents as chips with ×, and + Agent", () => {
    const todo = row(html, "To do");
    expect(todo).toContain('aria-label="Take Backend Agent 2 off To do"');
    expect(todo).toContain('aria-label="Take Frontend Agent 2 off To do"');
    expect(todo).toContain('aria-label="Add an agent to To do"');
    // an agent can be on several columns
    expect(row(html, "In progress")).toContain('aria-label="Take Backend Agent 2 off In progress"');
    expect(row(html, "Testing")).toContain('aria-label="Take QA Agent 2 off Testing"');
    expect(row(html, "Deploy")).toContain('aria-label="Take DevOps Agent 2 off Deploy"');
  });

  it("gives Backlog, Review, Done and Cancelled no agents, no Auto and no + Agent", () => {
    for (const name of ["Backlog", "Review", "Done", "Cancelled"]) {
      const r = row(html, name);
      expect(r, name).not.toContain("Add an agent to");
      expect(r, name).not.toContain("Auto or Manual for");
      expect(r, name).not.toContain("agent-chip");
    }
    expect(row(html, "Backlog")).toContain("No agents");
  });

  it("shows Auto or Manual and the next column", () => {
    const pressed = (name: string, button: "Auto" | "Manual") =>
      new RegExp(`<button aria-pressed="(true|false)"[^>]*>${button}</button>`).exec(row(html, name))?.[1];
    expect(row(html, "To do")).toContain('aria-label="Auto or Manual for To do"');
    expect(pressed("To do", "Auto")).toBe("true");
    expect(pressed("To do", "Manual")).toBe("false");
    expect(pressed("Deploy", "Auto")).toBe("false");
    expect(pressed("Deploy", "Manual")).toBe("true");
    const todo = row(html, "To do");
    expect(todo).toContain('aria-label="Next column after To do"');
    expect(todo).toMatch(/<option value="doing" selected="">In progress<\/option>/);
    // a link to itself is on offer (the backend refuses it with its reason), marked as this column
    expect(todo).toContain('<option value="todo">To do (this column)</option>');
    expect(row(html, "Deploy")).toMatch(/<option value="done" selected="">Done<\/option>/);
  });

  it("shows on Review that you review and merge, and where merged cards go", () => {
    const review = row(html, "Review");
    expect(review).toContain("You review and merge");
    expect(review).toContain("Merged cards go to");
    expect(review).toContain('aria-label="Merged cards from Review go to"');
    expect(review).toMatch(/<option value="deploy" selected="">Deploy<\/option>/);
    expect(review).toContain('<option value="">Not set (Deploy)</option>');
    expect(review).toContain("You review and merge; merged cards go to Deploy");
  });

  it("has one line under each column that says what happens", () => {
    expect(row(html, "To do")).toContain('<div class="wf-line">Auto: Backend Agent 2 and Frontend Agent 2 take cards by priority and move them to In progress</div>');
    expect(row(html, "Testing")).toContain("Auto: QA Agent 2 tests cards by priority and moves them to Review when they pass; a fail goes back");
    expect(row(html, "Deploy")).toContain("Manual: press Run on a card to start DevOps Agent 2; deployed cards go to Done");
    expect(row(html, "Design review")).toContain("Auto, but no agent is on this column: its cards wait. Drag an agent here or use + Agent");
    for (const s of states) expect(row(html, s.name), s.name).toMatch(/<div class="wf-line">[^<]+<\/div>/);
  });

  it("shows the backend's reason under the column it belongs to", () => {
    const r = editor({ error: { id: "deploy", text: "a column can't link to itself" } });
    expect(row(r, "Deploy")).toContain('<div class="wf-err" role="alert">a column can&#x27;t link to itself</div>');
    expect(row(r, "To do")).not.toContain("wf-err");
  });

  it("turns the bin off, with the reason, on the last Backlog and the last Done column", () => {
    const bin = (name: string) => /<span class="wf-bin" title="([^"]*)"><button[^>]*aria-label="Remove [^"]*"([^>]*)>/.exec(row(html, name));
    expect(bin("Backlog")?.[1]).toBe(esc("Backlog is the team's last Backlog column: new cards need it"));
    expect(bin("Backlog")?.[2]).toContain('disabled=""');
    expect(bin("Done")?.[1]).toBe(esc("Done is the team's last Done column: finished cards need it"));
    expect(bin("Done")?.[2]).toContain('disabled=""');
    for (const name of ["To do", "In progress", "Testing", "Review", "Deploy", "Cancelled"]) {
      expect(bin(name)?.[1], name).toBe(`Remove ${name}`);
      expect(bin(name)?.[2], name).not.toContain("disabled");
    }
  });

  it("turns the bin off, with the backend's reason, on a column with a card an agent is working on", () => {
    removals = { doing: { cards: 2, archived: 0, defaultTarget: "todo", relinked: ["To do"], unlinked: [], blocked: "Backend Agent 2 is working on GA-7 in In progress" } };
    try {
      const r = row(editor(), "In progress");
      expect(r).toContain('<span class="wf-bin" title="Backend Agent 2 is working on GA-7 in In progress"><button class="btn ghost sm icon-only" aria-label="Remove In progress" disabled="">');
    } finally { removals = {}; }
  });

  it("adds a column with a name, a kind in plain words and the column it goes after", () => {
    const r = editor({ adding: true });
    const add = r.slice(r.indexOf('aria-label="Add column"'));
    expect(add).toContain('aria-label="Column name"');
    expect(add).toContain('placeholder="In progress"');
    const kinds = [...(/<select[^>]*aria-label="Kind of column"[^>]*>(.*?)<\/select>/.exec(add)?.[1] ?? "").matchAll(/<option[^>]*>([^<]*)<\/option>/g)].map((m) => m[1]);
    expect(kinds).toEqual(["Waiting, like To do", "Work, like In progress", "Testing", "Review", "Deploy", "Done", "Backlog"]);
    const after = /<select[^>]*aria-label="After column"[^>]*>(.*?)<\/select>/.exec(add)?.[1] ?? "";
    expect([...after.matchAll(/<option[^>]*>([^<]*)<\/option>/g)].map((m) => m[1])).toEqual(states.map((s) => s.name));
    // after Review by default
    expect(after).toContain('<option value="review" selected="">Review</option>');
  });
});

describe("Labels", () => {
  it("lists every label with its colour and number of cards, and how to rename, recolour and remove it", () => {
    labels = [{ id: "l1", name: "Must have", color: "#f2706b", cards: 3 }, { id: "l2", name: "Could have", color: "#3fb8a0", cards: 1 },
      { id: "l3", name: "Later", color: null, cards: 0 }];
    const html = renderToStaticMarkup(<LabelsEditor />);
    const at = (name: string) => html.slice(html.indexOf(`data-label="${name}"`), html.indexOf("</div></div>", html.indexOf(`data-label="${name}"`)));
    expect(at("Must have")).toContain('style="background:#f2706b"');
    expect(at("Must have")).toContain("3 cards");
    expect(at("Could have")).toContain("1 card<");
    expect(at("Later")).toContain("no cards");
    for (const l of labels) {
      expect(at(l.name)).toContain(`aria-label="Recolour ${l.name}"`);
      expect(at(l.name)).toContain(`aria-label="Rename ${l.name}" maxLength="40" value="${l.name}"`);
      expect(at(l.name)).toContain(`aria-label="Remove ${l.name}"`);
    }
    // create: a name and a colour from the palette, the first one no label has yet
    expect(html).toContain('aria-label="New label name"');
    expect(html).toContain("Create label");
    expect(html).toContain('aria-label="Colour #6f97ff" aria-pressed="true"');
  });

  it("says when there are none", () => {
    labels = [];
    expect(renderToStaticMarkup(<LabelsEditor />)).toContain("No labels yet. Create one below, like Must have or Could have.");
  });
});

describe("New label…", () => {
  it("is at the end of the New task drawer's labels", () => {
    const html = renderToStaticMarkup(<NewTaskDrawer onClose={() => {}} />);
    const labelsField = html.slice(html.indexOf(">Labels<"));
    expect(labelsField).toContain('<button type="button" class="label-pill new-label">');
    expect(labelsField).toContain("New label…");
    expect(html).not.toMatch(/rout(e|ing)/i);
  });

  it("is a menu option in Properties' label menu", () => {
    const html = renderToStaticMarkup(<NewLabel labels={[]} onCreated={() => {}} />);
    expect(html.startsWith('<button type="button" class="opt new-label">')).toBe(true);
    expect(html).toContain("New label…");
  });
});

describe("the organisation chart", () => {
  const chart = (ms: Member[], branches?: Team["branches"]) =>
    renderToStaticMarkup(<DndContext><OrgChart members={ms} teamId="team" branches={branches} working={() => false} /></DndContext>);

  it("shows the branches with Design first, each with one empty spot", () => {
    const html = chart([]);
    expect([...html.matchAll(/data-branch="([^"]+)"/g)].map((m) => m[1])).toEqual(["Design", "Development", "Quality", "Operations"]);
    for (const b of ["Design", "Development", "Quality", "Operations"]) expect(html.split(`aria-label="Add an agent to ${b}"`).length - 1, b).toBe(1);
    expect(html).toContain('aria-label="Add the Team Lead"');
    expect(html).toContain("Add branch");
  });

  it("puts the agents above their branch's empty spot, as cards that drag onto a column", () => {
    const html = chart(members);
    const dev = html.slice(html.indexOf('data-branch="Development"'), html.indexOf('data-branch="Quality"'));
    expect(dev.indexOf('data-agent="Backend Agent 2"')).toBeLessThan(dev.indexOf('aria-label="Add an agent to Development"'));
    expect(dev).toContain('data-agent="Frontend Agent 2"');
    expect(dev).toContain("drag onto a column in Workflow to put it there");
    expect(dev.split("Add an agent to Development").length - 1).toBe(1);
    expect(html).not.toContain('data-agent="Jeffrey"');
  });

  it("removes a branch only while it has no agents", () => {
    const html = chart(members, [{ key: "design", name: "Design", roles: ["design"] }, { key: "dev", name: "Development", roles: ["frontend", "backend"] },
      { key: "docs", name: "Docs", roles: ["docs"] }]);
    expect(html).toMatch(/title="Development has 2 agents: move or remove them first" class="org-dept-x"><button[^>]*aria-label="Remove the Development branch" disabled=""/);
    expect(html).toMatch(/title="Remove the Docs branch" class="org-dept-x"><button[^>]*aria-label="Remove the Docs branch">/);
    // an agent whose role no branch has goes under Specialists, which has no ×
    expect(html).toContain('<div class="org-dept-name">Specialists</div>');
    expect(html).toContain('aria-label="Add an agent to Docs"');
    expect(html).toContain('title="Add a Docs agent to Docs"');
  });
});

describe("the agent form", () => {
  it("has no Wake-up or heartbeat for a worker agent: the columns decide", () => {
    const html = renderToStaticMarkup(<AgentDrawer teamId="team" preset={{ name: "Frontend Agent", role: "frontend" }} onClose={() => {}} />);
    expect(html).not.toContain("Wakes up");
    expect(html).not.toContain("Wake-up");
    expect(html).not.toMatch(/heartbeat/i);
    expect(html).not.toContain('aria-label="Minutes"');
    expect(html).toContain("The columns it is on decide when it works (Team → Workflow)");
    expect(html).toContain('value="Frontend Agent"');
    expect(html).toMatch(/<option value="frontend" selected="">/);
  });

  it("keeps the Team Lead's board check", () => {
    const html = renderToStaticMarkup(<AgentDrawer teamId="team" preset={{ name: "Team Lead", role: "lead", chat: true }} onClose={() => {}} />);
    expect(html).toContain("Board check");
    expect(html).toContain('aria-label="Board check minutes"');
    expect(html).not.toContain("Wakes up");
  });
});

describe("the board's column notes", () => {
  it("follow the columns: the agents of an Auto column, Manual, or your review", () => {
    const task = { id: "t1", identifier: "GA-1", projectId: "p1", title: "A card", descriptionMd: "", stateId: "todo", stateName: "To do", stateCategory: "ready",
      priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: "a0", testing: true, createdAt: 0, updatedAt: 0 } as Task;
    const html = renderToStaticMarkup(<Board tasks={[task]} states={states} members={members} onMove={() => {}} onOpen={() => {}} />);
    const col = (name: string) => html.slice(html.indexOf(`data-col="${name}"`), html.indexOf("data-col=", html.indexOf(`data-col="${name}"`) + 1));
    expect(col("To do")).toContain('<div class="col-note">Auto: Backend Agent 2 and Frontend Agent 2</div>');
    expect(col("Testing")).toContain('<div class="col-note">Auto: QA Agent 2</div>');
    expect(col("Deploy")).toContain('<div class="col-note">Manual: press Run on a card</div>');
    expect(col("Review")).toContain('<div class="col-note">Waiting for your review</div>');
    expect(col("Design review")).toContain('<div class="col-note">Auto, but no agent is on this column</div>');
    expect(html.toLowerCase()).not.toContain("matching the label");
  });
});
