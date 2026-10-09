// GA-53: what Team → Workflow says about each column, the board's column notes, Add column's kinds, a dragged column's
// new place, the bins of the last Backlog and Done columns, the label palette and what an empty spot in a branch fills in.
import { describe, expect, it } from "vitest";
import {
  afterIdAt, agentNames, andList, boardNote, boardOrder, cardCount, columnLine, KINDS, LABEL_COLORS, labelTaken, lastOfKind,
  mergedTarget, nextLabelColor, branchPreset, takesAgents,
} from "./columns";
import { DEFAULT_BRANCHES } from "./org";
import type { Member, WorkflowState } from "../types";

const agent = (actorId: string, name: string, roleKey: string, extra: Partial<Member> = {}): Member => ({
  actorId, name, kind: "agent", roleKey, handle: actorId, status: "active", isLead: false, allowedTools: [], chatEnabled: false, ...extra,
});
const members = [agent("be2", "Backend Agent 2", "backend"), agent("fe2", "Frontend Agent 2", "frontend"), agent("qa2", "QA Agent 2", "qa"),
  agent("ops2", "DevOps Agent 2", "devops")];

// GA-49's table: To do and In progress with the builders on Auto, Testing with QA Agent 2 on Auto, Review linked to Deploy,
// Deploy with DevOps Agent 2 on Manual.
const states: WorkflowState[] = [
  { id: "done", name: "Done", category: "done", sortKey: "a6" },
  { id: "backlog", name: "Backlog", category: "backlog", sortKey: "a0" },
  { id: "todo", name: "To do", category: "ready", sortKey: "a1", auto: true, nextStateId: "doing", agentIds: ["be2", "fe2"] },
  { id: "doing", name: "In progress", category: "in_progress", sortKey: "a2", auto: true, nextStateId: "testing", agentIds: ["be2", "fe2"] },
  { id: "testing", name: "Testing", category: "testing", sortKey: "a3", auto: true, nextStateId: "review", agentIds: ["qa2"] },
  { id: "review", name: "Review", category: "review", sortKey: "a4", nextStateId: "deploy" },
  { id: "deploy", name: "Deploy", category: "deploy", sortKey: "a5", auto: false, nextStateId: "done", agentIds: ["ops2"] },
  { id: "cancelled", name: "Cancelled", category: "cancelled", sortKey: "a7" },
];
const col = (id: string, change: Partial<WorkflowState> = {}) => ({ ...states.find((s) => s.id === id)!, ...change });
const line = (s: WorkflowState) => columnLine(s, states, members);

describe("the columns", () => {
  it("come in board order", () => {
    expect(boardOrder(states).map((s) => s.name)).toEqual(["Backlog", "To do", "In progress", "Testing", "Review", "Deploy", "Done", "Cancelled"]);
  });

  it("take agents, except Backlog, Review, Done and Cancelled", () => {
    expect(["ready", "in_progress", "testing", "deploy"].every(takesAgents)).toBe(true);
    expect(["backlog", "review", "done", "cancelled"].some(takesAgents)).toBe(false);
  });

  it("name their agents in the column's order and leave out agents no longer on the team", () => {
    expect(agentNames(col("todo", { agentIds: ["fe2", "gone", "be2"] }), members)).toEqual(["Frontend Agent 2", "Backend Agent 2"]);
    expect(andList([])).toBe("");
    expect(andList(["A"])).toBe("A");
    expect(andList(["A", "B"])).toBe("A and B");
    expect(andList(["A", "B", "C"])).toBe("A, B and C");
  });
});

describe("the line under each column", () => {
  it("says who takes the cards of an Auto column and where they go", () => {
    expect(line(col("todo"))).toBe("Auto: Backend Agent 2 and Frontend Agent 2 take cards by priority and move them to In progress");
    expect(line(col("doing"))).toBe("Auto: Backend Agent 2 and Frontend Agent 2 take cards by priority and move them to Testing when done");
    expect(line(col("testing"))).toBe("Auto: QA Agent 2 tests cards by priority and moves them to Review when they pass; a fail goes back");
    expect(line(col("deploy", { auto: true }))).toBe("Auto: DevOps Agent 2 takes cards by priority and moves them to Done once deployed");
  });

  it("says Run starts a card on a Manual column", () => {
    expect(line(col("deploy"))).toBe("Manual: press Run on a card to start DevOps Agent 2; deployed cards go to Done");
    expect(line(col("todo", { auto: false, agentIds: [] }))).toBe("Manual: press Run on a card and pick an agent; started cards go to In progress");
    expect(line(col("doing", { auto: false, nextStateId: null }))).toBe("Manual: press Run on a card to start Backend Agent 2");
  });

  it("says so when an Auto column has no agents, or no next column", () => {
    expect(line(col("testing", { agentIds: [] }))).toBe("Auto, but no agent is on this column: its cards wait. Drag an agent here or use + Agent");
    expect(line(col("testing", { nextStateId: null }))).toBe("Auto needs a next column: pick where QA Agent 2 moves the cards");
  });

  it("says you review and merge on Review, and where merged cards go", () => {
    expect(line(col("review"))).toBe("You review and merge; merged cards go to Deploy");
    // without a next column: the team's Deploy column, else its Done column
    expect(line(col("review", { nextStateId: null }))).toBe("You review and merge; merged cards go to Deploy");
    expect(mergedTarget(col("review", { nextStateId: null }), states.filter((s) => s.category !== "deploy"))?.name).toBe("Done");
    expect(mergedTarget(col("review", { nextStateId: "done" }), states)?.name).toBe("Done");
  });

  it("says no agent takes the cards of Backlog, Done and Cancelled", () => {
    expect(line(col("backlog"))).toBe("New cards wait here; no agent takes them. Run on a card starts it in In progress");
    expect(line(col("done"))).toBe("Finished cards; no agent takes them");
    expect(line(col("cancelled"))).toBe("Cards nobody will work on; no agent takes them");
  });
});

describe("the board's column note", () => {
  it("follows the column: its agents when Auto, Run when Manual, your review on Review", () => {
    expect(boardNote(col("todo"), members)).toBe("Auto: Backend Agent 2 and Frontend Agent 2");
    expect(boardNote(col("testing"), members)).toBe("Auto: QA Agent 2");
    expect(boardNote(col("testing"))).toBe("Auto: picked up by the agents on this column");
    expect(boardNote(col("testing", { agentIds: [] }), members)).toBe("Auto, but no agent is on this column");
    expect(boardNote(col("deploy"), members)).toBe("Manual: press Run on a card");
    expect(boardNote(col("deploy", { agentIds: [] }), members)).toBeUndefined();
    expect(boardNote(col("review"), members)).toBe("Waiting for your review");
    for (const id of ["backlog", "done", "cancelled"]) expect(boardNote(col(id), members), id).toBeUndefined();
  });

  it("never says 'the agent matching the label'", () => {
    const notes = states.flatMap((s) => [boardNote(s, members), boardNote({ ...s, auto: !s.auto }, members), line(s)]);
    expect(notes.join(" ").toLowerCase()).not.toContain("matching the label");
  });
});

describe("Add column", () => {
  it("offers the kinds in plain words, each with its usual name", () => {
    expect(KINDS.map((k) => k.label)).toEqual(["Waiting, like To do", "Work, like In progress", "Testing", "Review", "Deploy", "Done", "Backlog"]);
    expect(KINDS.map((k) => k.kind)).toEqual(["waiting", "work", "testing", "review", "deploy", "done", "backlog"]);
    expect(KINDS.find((k) => k.kind === "work")?.name).toBe("In progress");
  });
});

describe("a dragged column", () => {
  it("goes after the column before its new place, or to the front", () => {
    const order = ["backlog", "testing", "todo", "doing"];
    expect(afterIdAt(order, 1)).toBe("backlog");
    expect(afterIdAt(order, 3)).toBe("todo");
    expect(afterIdAt(order, 0)).toBe("");
  });
});

describe("the bin", () => {
  it("is off on the team's last Backlog and last Done column, with the reason", () => {
    expect(lastOfKind(col("backlog"), states)).toBe("Backlog is the team's last Backlog column: new cards need it");
    expect(lastOfKind(col("done"), states)).toBe("Done is the team's last Done column: finished cards need it");
    for (const id of ["todo", "doing", "testing", "review", "deploy", "cancelled"]) expect(lastOfKind(col(id), states), id).toBeNull();
  });

  it("is on for a Backlog or Done column when the team has another one", () => {
    const more = [...states, { id: "shipped", name: "Shipped", category: "done", sortKey: "a65" }, { id: "ideas", name: "Ideas", category: "backlog", sortKey: "a05" }];
    expect(lastOfKind(col("done"), more)).toBeNull();
    expect(lastOfKind(col("backlog"), more)).toBeNull();
  });
});

describe("labels", () => {
  it("count their cards in words", () => {
    expect(cardCount(0)).toBe("no cards");
    expect(cardCount(1)).toBe("1 card");
    expect(cardCount(4)).toBe("4 cards");
  });

  it("get the first palette colour no label has yet, then go round", () => {
    expect(nextLabelColor([])).toBe(LABEL_COLORS[0]);
    expect(nextLabelColor([{ color: LABEL_COLORS[0].toUpperCase() }, { color: null }])).toBe(LABEL_COLORS[1]);
    const all = LABEL_COLORS.map((color) => ({ color }));
    expect(nextLabelColor(all)).toBe(LABEL_COLORS[0]);
    expect(nextLabelColor([...all, { color: "#000000" }])).toBe(LABEL_COLORS[1]);
    expect(new Set(LABEL_COLORS).size).toBe(LABEL_COLORS.length);
  });

  it("know a name in use, in any case, except the label's own", () => {
    const labels = [{ id: "l1", name: "Must have" }, { id: "l2", name: "Could have" }];
    expect(labelTaken("must HAVE ", labels)).toBe(true);
    expect(labelTaken("Must have", labels, "l1")).toBe(false);
    expect(labelTaken("Should have", labels)).toBe(false);
  });
});

describe("an empty spot in a branch", () => {
  const branch = (key: string) => DEFAULT_BRANCHES.find((b) => b.key === key)!;

  it("fills in the branch's first role no agent has yet, named like the usual agent", () => {
    expect(branchPreset(branch("design"), [])).toEqual({ name: "Design Agent", role: "design" });
    expect(branchPreset(branch("dev"), [])).toEqual({ name: "Frontend Agent", role: "frontend" });
    expect(branchPreset(branch("dev"), [agent("f", "Frontend Agent", "frontend")])).toEqual({ name: "Backend Agent", role: "backend" });
    expect(branchPreset(branch("qa"), [])).toEqual({ name: "QA Agent", role: "qa" });
    expect(branchPreset(branch("ops"), [])).toEqual({ name: "DevOps Agent", role: "devops" });
  });

  it("makes the name unique when every role is taken", () => {
    expect(branchPreset(branch("qa"), [agent("q", "QA Agent", "qa")])).toEqual({ name: "QA Agent 2", role: "qa" });
    expect(branchPreset(branch("qa"), [agent("q", "QA Agent", "qa"), agent("q2", "qa agent 2", "qa")])).toEqual({ name: "QA Agent 3", role: "qa" });
    // people don't count
    expect(branchPreset(branch("qa"), [agent("p", "QA Agent", "qa", { kind: "person" })])).toEqual({ name: "QA Agent", role: "qa" });
  });

  it("uses a branch you added: its own role and name", () => {
    expect(branchPreset({ key: "docs", name: "Docs", roles: ["docs"] }, [])).toEqual({ name: "Docs Agent", role: "docs" });
  });
});
