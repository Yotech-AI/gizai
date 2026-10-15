// GA-206: in the @ picker an exact card ID comes first (`@SDH-4` lists SDH-4 above SDH-40 … SDH-49, case and accents
// ignored), and with `@task.` its column goes first with it as the first, selected row. Done and cancelled cards are listed
// too, after the open ones with the same score; under `@task.` they get the Done column's heading, and the 50-row cut still
// comes before the grouping. Without an exact match the order is as before.
import { describe, expect, it } from "vitest";
import {
  IN_KIND, itemLink, KIND_GROUP, PER_KIND, pickRows, projectItem, rowHeading, taskItem, type PickItem, type PickRow,
} from "./itemLinks";
import type { Project, Task } from "../types";

const BOARD = ["Backlog", "To do", "In progress", "Testing", "Review", "Deploy", "Done", "Cancelled"];
const CATEGORY: Record<string, string> = {
  Backlog: "backlog", "To do": "ready", "In progress": "in_progress", Testing: "testing", Review: "review", Deploy: "deploy",
  Done: "done", Cancelled: "cancelled",
};
const task = (identifier: string, title: string, stateName: string) =>
  ({ identifier, title, stateName, stateCategory: CATEGORY[stateName] ?? "work", projectName: "Stapsgewijs" }) as Task;
const card = (identifier: string, title: string, stateName: string) => taskItem(task(identifier, title, stateName), BOARD);

const keys = (rows: PickRow[]) => rows.map((r) => (r.type === "item" ? r.item.key : `kind:${r.kind}`));
const taskKeys = (rows: PickRow[]) => keys(rows.filter((r) => r.type === "item" && r.item.kind === "task"));
/** The rows as the picker shows them: `# Heading` for a heading, then the row's key. */
const shown = (query: string, rows: PickRow[]) =>
  rows.flatMap((r, i) => { const head = rowHeading(query, rows, i); const key = keys([r])[0]; return head ? [`# ${head}`, key] : [key]; });
const first = (rows: PickRow[]) => (rows[0] as Extract<PickRow, { type: "item" }>).item;

const range = (prefix: string, from: number, to: number) => Array.from({ length: to - from + 1 }, (_, i) => `${prefix}${from + i}`);
const FORTIES = range("SDH-", 40, 49);
const TWENTIES = range("SDH-", 20, 29);

// The list's order is the board's (tasks::list sorts by column): Backlog first, so SDH-40 … SDH-49 before SDH-4 in Review.
const SDH: PickItem[] = [
  ...FORTIES.map((id) => card(id, `Backlog item ${id.slice(4)}`, "Backlog")),
  card("SDH-4", "Fix the header", "Review"),
  projectItem({ id: "p1", key: "SDH", name: "Stapsgewijs", clientName: null } as Project),
];
// SDH-20 … SDH-29 open in Backlog, SDH-2 in Done (last in the list, as the board's order puts it).
const DONE: PickItem[] = [
  ...TWENTIES.map((id) => card(id, `Backlog item ${id.slice(4)}`, "Backlog")),
  card("SDH-2", "Release notes", "Done"),
];

describe("taskItem: closed (GA-206)", () => {
  it("marks a done or cancelled card closed, and leaves it out for an open one", () => {
    expect(card("SDH-2", "Release notes", "Done")).toMatchObject({ key: "SDH-2", closed: true, column: { name: "Done", place: 6 } });
    expect(card("SDH-3", "Dropped", "Cancelled")).toMatchObject({ key: "SDH-3", closed: true, column: { name: "Cancelled", place: 7 } });
    for (const column of ["Backlog", "To do", "In progress", "Testing", "Review", "Deploy"]) {
      expect("closed" in card("SDH-4", "Fix the header", column)).toBe(false);
    }
    // From the card's category, not its column's name or place: a done card in a column called otherwise is closed too.
    expect(taskItem({ ...task("SDH-5", "Shipped", "Live"), stateCategory: "done" } as Task, BOARD).closed).toBe(true);
    expect("closed" in taskItem({ ...task("SDH-6", "Waiting", "Done"), stateCategory: "review" } as Task, BOARD)).toBe(false);
  });
  it("writes the same row, link text and link for a done card", () => {
    const done = card("SDH-2", "Release [notes]", "Done");
    expect(done).toMatchObject({ kind: "task", label: "SDH-2 - Release [notes]", text: "SDH-2 - Release [notes]", hint: "Stapsgewijs" });
    expect(itemLink(done)).toBe("[SDH-2 - Release \\[notes\\]](gizai:task/SDH-2)");
  });
});

describe("pickRows: an exact ID first (GA-206)", () => {
  it("lists SDH-4 (in Review) as the first task for @SDH-4, above SDH-40 … SDH-49 in Backlog, and Enter writes its link", () => {
    const rows = pickRows("SDH-4", SDH);
    expect(taskKeys(rows)).toEqual(["SDH-4", "SDH-40", "SDH-41", "SDH-42", "SDH-43"]);
    expect(taskKeys(rows)).toHaveLength(PER_KIND);
    // The first row is the selected one: Enter picks it.
    expect(first(rows).key).toBe("SDH-4");
    expect(itemLink(first(rows))).toBe("[SDH-4 - Fix the header](gizai:task/SDH-4)");
    expect(rowHeading("SDH-4", rows, 0)).toBe(KIND_GROUP.task);
  });
  it("is the same in lower case, mixed case and with the accent left out", () => {
    for (const query of ["sdh-4", "Sdh-4", "sDH-4"]) expect(keys(pickRows(query, SDH))).toEqual(keys(pickRows("SDH-4", SDH)));
    const accents = [card("CAFÉ-10", "Ten", "Backlog"), card("CAFÉ-11", "Eleven", "Backlog"), card("CAFÉ-1", "One", "Deploy")];
    expect(keys(pickRows("cafe-1", accents))).toEqual(["CAFÉ-1", "CAFÉ-10", "CAFÉ-11"]);
    expect(keys(pickRows("CAFÉ-1", accents))).toEqual(["CAFÉ-1", "CAFÉ-10", "CAFÉ-11"]);
  });
  it("hits any card right of Backlog whose ID starts other IDs: SDH-1 in Deploy before SDH-10 … SDH-19", () => {
    const items = [...range("SDH-", 10, 19).map((id) => card(id, "Item", "Backlog")), card("SDH-1", "Ship it", "Deploy")];
    expect(keys(pickRows("SDH-1", items))).toEqual(["SDH-1", "SDH-10", "SDH-11", "SDH-12", "SDH-13"]);
  });
  it("only an exact ID counts: an ID that starts with the word keeps the list's order, as before", () => {
    expect(taskKeys(pickRows("SDH", SDH))).toEqual(["SDH-40", "SDH-41", "SDH-42", "SDH-43", "SDH-44"]);
    expect(taskKeys(pickRows("SDH-", SDH))).toEqual(["SDH-40", "SDH-41", "SDH-42", "SDH-43", "SDH-44"]);
    // A card whose title is the word isn't an exact ID match.
    const titled = [card("GA-1", "Something", "Backlog"), card("GA-2", "SDH-4", "Backlog"), card("SDH-4", "Fix", "Review")];
    expect(keys(pickRows("SDH-4", titled))).toEqual(["SDH-4", "GA-2"]);
  });
  it("still goes first with more search words when they are in its name, in one kind (@task.SDH-4 header)", () => {
    const items = [card("SDH-40", "Header colours", "Backlog"), ...SDH.slice(1)];
    expect(keys(pickRows("task.SDH-4 header", items))).toEqual(["SDH-4", "SDH-40"]);
    // When a later word isn't in its name the card isn't found at all.
    expect(keys(pickRows("task.SDH-4 colours", items))).toEqual(["SDH-40"]);
  });
  it("leaves the other kinds as they were: the project SDH isn't found by @SDH-4, and @sdh lists its tasks and the project", () => {
    expect(keys(pickRows("SDH-4", SDH)).filter((k) => !k.startsWith("SDH-"))).toEqual([]);
    expect(keys(pickRows("sdh", SDH))).toEqual(["SDH-40", "SDH-41", "SDH-42", "SDH-43", "SDH-44", "SDH"]);
  });
  it("puts a project whose key is the word first among the projects", () => {
    const projects = [
      projectItem({ id: "p2", key: "GAB", name: "Gabber", clientName: null } as Project),
      projectItem({ id: "p1", key: "GA", name: "Giz AI", clientName: null } as Project),
    ];
    expect(keys(pickRows("project.ga", projects))).toEqual(["GA", "GAB"]);
    expect(keys(pickRows("project.gab", projects))).toEqual(["GAB"]);
  });
});

describe("pickRows: @task. with an exact ID, its column first (GA-206)", () => {
  it("shows Review first with SDH-4 as its first (selected) row, then Backlog with SDH-40 … SDH-49", () => {
    const rows = pickRows("task.SDH-4", SDH);
    expect(shown("task.SDH-4", rows)).toEqual(["# Review", "SDH-4", "# Backlog", ...FORTIES]);
    expect(itemLink(first(rows))).toBe("[SDH-4 - Fix the header](gizai:task/SDH-4)");
    expect(shown("task.sdh-4", pickRows("task.sdh-4", SDH))).toEqual(shown("task.SDH-4", rows));
    expect(shown("Task.SDH-4", pickRows("Task.SDH-4", SDH))).toEqual(shown("task.SDH-4", rows));
  });
  it("keeps the board's order for the other columns, and the exact card above the others in its own column", () => {
    const items = [
      card("SDH-40", "One", "Backlog"),
      card("SDH-42", "Two", "To do"),
      card("SDH-401", "Three", "Review"),
      card("SDH-4", "Four", "Review"),
      card("SDH-402", "Five", "Deploy"),
      card("SDH-41", "Six", "Backlog"),
    ];
    expect(shown("task.SDH-4", pickRows("task.SDH-4", items))).toEqual([
      "# Review", "SDH-4", "SDH-401",
      "# Backlog", "SDH-40", "SDH-41",
      "# To do", "SDH-42",
      "# Deploy", "SDH-402",
    ]);
  });
  it("is unchanged when the exact card is in the first column anyway", () => {
    const items = [card("SDH-40", "One", "Backlog"), card("SDH-4", "Four", "Backlog"), card("SDH-41", "Two", "Review")];
    expect(shown("task.SDH-4", pickRows("task.SDH-4", items))).toEqual(["# Backlog", "SDH-4", "SDH-40", "# Review", "SDH-41"]);
  });
  it("puts the exact card first also in a column the board's order lacks, or without a column", () => {
    const odd = [...SDH.slice(0, 3), taskItem(task("SDH-4", "Draw it", "Design"), BOARD), card("SDH-45", "Check", "Review")];
    expect(shown("task.SDH-4", pickRows("task.SDH-4", odd))).toEqual(["# Design", "SDH-4", "# Backlog", "SDH-40", "SDH-41", "SDH-42", "# Review", "SDH-45"]);
    const none = [...SDH.slice(0, 2), taskItem({ identifier: "SDH-4", title: "Loose", projectName: "Stapsgewijs", stateCategory: "ready" } as Task, BOARD)];
    expect(shown("task.SDH-4", pickRows("task.SDH-4", none))).toEqual(["SDH-4", "# Backlog", "SDH-40", "SDH-41"]);
  });
  it("changes nothing without an exact match: the board's order", () => {
    expect(shown("task.SDH", pickRows("task.SDH", SDH))).toEqual(["# Backlog", ...FORTIES, "# Review", "SDH-4"]);
    expect(shown("task.header", pickRows("task.header", SDH))).toEqual(["# Review", "SDH-4"]);
    expect(shown("task.", pickRows("task.", SDH))).toEqual(["# Backlog", ...FORTIES, "# Review", "SDH-4"]);
    expect(pickRows("task.SDH-4 zzz", SDH)).toEqual([]);
  });
  it("keeps the 50-row cut before the grouping: the exact card is always in it, the rest in the list's order", () => {
    const many = [...range("SDH-", 400, 459).map((id) => card(id, "Item", "Backlog")), card("SDH-4", "Fix the header", "Review")];
    const rows = pickRows("task.SDH-4", many);
    expect(rows).toHaveLength(IN_KIND);
    expect(shown("task.SDH-4", rows)).toEqual(["# Review", "SDH-4", "# Backlog", ...range("SDH-", 400, 448)]);
  });
});

describe("pickRows: done cards (GA-206)", () => {
  it("lists a Done card first for its exact ID: @SDH-2 before SDH-20 … SDH-29", () => {
    const rows = pickRows("SDH-2", DONE);
    expect(keys(rows)).toEqual(["SDH-2", "SDH-20", "SDH-21", "SDH-22", "SDH-23"]);
    expect(itemLink(first(rows))).toBe("[SDH-2 - Release notes](gizai:task/SDH-2)");
    expect(keys(pickRows("sdh-2", DONE))).toEqual(keys(rows));
  });
  it("shows @task.SDH-2 under the Done heading first, then Backlog", () => {
    expect(shown("task.SDH-2", pickRows("task.SDH-2", DONE))).toEqual(["# Done", "SDH-2", "# Backlog", ...TWENTIES]);
  });
  it("puts open cards before closed ones with the same score, also when the closed one comes first in the list", () => {
    const items = [
      card("GA-1", "Login bug", "Done"),
      card("GA-3", "Login form", "Cancelled"),
      card("GA-2", "Login page", "Backlog"),
      card("GA-4", "Fix the login", "Review"),
    ];
    // GA-1 and GA-3 have login in their name like GA-2 and GA-4 (all score 1): the open ones first, then the list's order.
    expect(keys(pickRows("login", items))).toEqual(["GA-2", "GA-4", "GA-1", "GA-3"]);
    expect(keys(pickRows("task.login", items))).toEqual(["GA-2", "GA-4", "GA-1", "GA-3"]);
  });
  it("sorts by score before open or closed: a Done card with a better match still goes first", () => {
    const items = [card("GA-2", "Fix the login", "Backlog"), card("LOGIN-1", "Something", "Done")];
    expect(keys(pickRows("login", items))).toEqual(["LOGIN-1", "GA-2"]);
  });
  it("cuts a closed card first when the 5 rows per kind are full of open ones", () => {
    const items = [card("GA-1", "Login bug", "Done"), ...range("GA-", 2, 6).map((id) => card(id, "Login page", "To do"))];
    expect(keys(pickRows("login", items))).toEqual(["GA-2", "GA-3", "GA-4", "GA-5", "GA-6"]);
    expect(keys(pickRows("task.login", items))).toEqual(["GA-2", "GA-3", "GA-4", "GA-5", "GA-6", "GA-1"]);
  });
  it("lists done and cancelled cards with @task. under their headings after Deploy, when they fit in the 50 rows", () => {
    const items = [
      card("GA-7", "Done one", "Done"),
      card("GA-1", "Ship", "Deploy"),
      card("GA-8", "Dropped", "Cancelled"),
      card("GA-2", "Plan", "Backlog"),
      card("GA-3", "Check", "Review"),
      card("GA-9", "Done two", "Done"),
    ];
    const rows = pickRows("task.", items);
    expect(shown("task.", rows)).toEqual(["# Backlog", "GA-2", "# Review", "GA-3", "# Deploy", "GA-1", "# Done", "GA-7", "GA-9", "# Cancelled", "GA-8"]);
  });
  it("with more than 50 open cards shows a Done card under @task. only when the search finds it", () => {
    const many = [card("SDH-2", "Release notes", "Done"), ...range("SDH-", 200, 259).map((id) => card(id, "Item", "Backlog"))];
    const all = pickRows("task.", many);
    expect(all).toHaveLength(IN_KIND);
    expect(keys(all)).not.toContain("SDH-2");
    expect(all.map((_, i) => rowHeading("task.", all, i)).filter(Boolean)).toEqual(["Backlog"]);
    expect(shown("task.release", pickRows("task.release", many))).toEqual(["# Done", "SDH-2"]);
    // Its exact ID makes room for it: the Done card first, then the first 49 open ones.
    expect(shown("task.SDH-2", pickRows("task.SDH-2", many))).toEqual(["# Done", "SDH-2", "# Backlog", ...range("SDH-", 200, 248)]);
  });
});
