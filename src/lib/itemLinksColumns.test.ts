// GA-88: `@task.` lists the tasks under a small heading per column. The columns in the board's order (then any that order
// doesn't have, in the list's order), in a column the order the list had (the best match first), the 50-row limit over all
// columns. A search across all kinds and the other kinds look as before, and a task's link doesn't change.
import { describe, expect, it } from "vitest";
import {
  clientItem, docItem, IN_KIND, itemLink, KIND_GROUP, pickRows, projectItem, rowHeading, taskItem, type PickItem, type PickRow,
} from "./itemLinks";
import type { Client, Doc, Project, Task } from "../types";

const BOARD = ["Backlog", "To do", "In progress", "Testing", "Review", "Deploy"];
const task = (identifier: string, title: string, stateName: string, projectName = "Giz AI") => ({ identifier, title, stateName, projectName }) as Task;

// The list's order mixes the columns up.
const TASKS: PickItem[] = [
  taskItem(task("GA-1", "Ship the release", "Deploy"), BOARD),
  taskItem(task("GA-2", "Fix the login", "In progress"), BOARD),
  taskItem(task("GA-3", "Write the docs", "Backlog"), BOARD),
  taskItem(task("KADE-9", "Header", "To do", "Login portal"), BOARD),
  taskItem(task("GA-4", "Login page", "To do"), BOARD),
  taskItem(task("GA-5", "Test the picker", "Testing"), BOARD),
  taskItem(task("GA-6", "Review the login", "Review"), BOARD),
  taskItem(task("GA-7", "Plan the sprint", "Backlog"), BOARD),
  taskItem(task("GA-8", "Old login bug", "In progress"), BOARD),
  taskItem(task("LOGIN-2", "Something", "In progress"), BOARD),
];

const keys = (rows: PickRow[]) => rows.map((r) => (r.type === "item" ? r.item.key : `kind:${r.kind}`));
/** The rows as the picker shows them: `# Column` for a heading, then the row's key. */
const shown = (query: string, rows: PickRow[]) =>
  rows.flatMap((r, i) => { const head = rowHeading(query, rows, i); const key = keys([r])[0]; return head ? [`# ${head}`, key] : [key]; });

describe("taskItem: a task's column", () => {
  it("carries the column's name and its place in the board's order", () => {
    expect(taskItem(task("GA-2", "Fix the login", "In progress"), BOARD).column).toEqual({ name: "In progress", place: 2 });
    expect(taskItem(task("GA-1", "Ship", "Backlog"), BOARD).column).toEqual({ name: "Backlog", place: 0 });
    expect(taskItem(task("GA-1", "Ship", "Deploy"), BOARD).column).toEqual({ name: "Deploy", place: 5 });
  });
  it("leaves the place out when the board's order doesn't have the column, and the column when the task has none", () => {
    expect(taskItem(task("GA-1", "Ship", "Design"), BOARD).column).toEqual({ name: "Design" });
    expect(taskItem(task("GA-1", "Ship", "To do")).column).toEqual({ name: "To do" });
    expect(taskItem(task("GA-1", "Ship", "")).column).toBeUndefined();
    expect(taskItem({ identifier: "GA-1", title: "Ship", projectName: "Giz AI" } as Task, BOARD).column).toBeUndefined();
  });
  it("writes the same row, link and link text as before", () => {
    const t = task("GA-5", "Fix [urgent] bug", "Testing");
    const withColumn = taskItem(t, BOARD);
    expect(withColumn).toMatchObject({ kind: "task", key: "GA-5", label: "GA-5 - Fix [urgent] bug", text: "GA-5 - Fix [urgent] bug", hint: "Giz AI" });
    expect(itemLink(withColumn)).toBe("[GA-5 - Fix \\[urgent\\] bug](gizai:task/GA-5)");
    expect(itemLink(withColumn)).toBe(itemLink(taskItem(t)));
  });
});

describe("pickRows: @task. grouped by column", () => {
  it("lists every open task column by column in the board's order, a column's tasks in the list's order", () => {
    expect(shown("task.", pickRows("task.", TASKS))).toEqual([
      "# Backlog", "GA-3", "GA-7",
      "# To do", "KADE-9", "GA-4",
      "# In progress", "GA-2", "GA-8", "LOGIN-2",
      "# Testing", "GA-5",
      "# Review", "GA-6",
      "# Deploy", "GA-1",
    ]);
  });
  it("is the same for @Task. and for the list in another order: only the order inside a column follows the list", () => {
    expect(shown("Task.", pickRows("Task.", TASKS))).toEqual(shown("task.", pickRows("task.", TASKS)));
    const reversed = pickRows("task.", [...TASKS].reverse());
    expect(shown("task.", reversed)).toEqual([
      "# Backlog", "GA-7", "GA-3",
      "# To do", "GA-4", "KADE-9",
      "# In progress", "LOGIN-2", "GA-8", "GA-2",
      "# Testing", "GA-5",
      "# Review", "GA-6",
      "# Deploy", "GA-1",
    ]);
  });
  it("with search words, heads only the columns with matching tasks, the best match first in its column", () => {
    // LOGIN-2 starts with the word, GA-2 and GA-8 have it in their name, KADE-9 only in its project (the faint hint).
    expect(shown("task.login", pickRows("task.login", TASKS))).toEqual([
      "# To do", "GA-4", "KADE-9",
      "# In progress", "LOGIN-2", "GA-2", "GA-8",
      "# Review", "GA-6",
    ]);
    expect(shown("task.fix login", pickRows("task.fix login", TASKS))).toEqual(["# In progress", "GA-2"]);
    expect(shown("task.ga-5", pickRows("task.ga-5", TASKS))).toEqual(["# Testing", "GA-5"]);
    expect(pickRows("task.zzz", TASKS)).toEqual([]);
  });
  it("puts columns the board's order lacks after it, in the list's order, and a task without a column first without a heading", () => {
    const items = [
      taskItem(task("GA-1", "One", "Design"), BOARD),
      taskItem(task("GA-2", "Two", "Review"), BOARD),
      taskItem(task("GA-3", "Three", "Ops"), BOARD),
      taskItem(task("GA-4", "Four", "Design"), BOARD),
      taskItem({ identifier: "GA-5", title: "Five", projectName: "Giz AI" } as Task, BOARD),
      taskItem(task("GA-6", "Six", "Backlog"), BOARD),
    ];
    expect(shown("task.", pickRows("task.", items))).toEqual(["GA-5", "# Backlog", "GA-6", "# Review", "GA-2", "# Design", "GA-1", "GA-4", "# Ops", "GA-3"]);
  });
  it("lists tasks without a board order (no columns given) by column in the list's order", () => {
    const items = [taskItem(task("GA-1", "One", "Review")), taskItem(task("GA-2", "Two", "To do")), taskItem(task("GA-3", "Three", "Review"))];
    expect(shown("task.", pickRows("task.", items))).toEqual(["# Review", "GA-1", "GA-3", "# To do", "GA-2"]);
  });
  it("keeps the 50-row limit over all columns: the same 50 tasks as before, only grouped", () => {
    const many = Array.from({ length: 60 }, (_, i) => taskItem(task(`GA-${i + 1}`, `Item ${i + 1}`, BOARD[(i * 5) % BOARD.length]), BOARD));
    for (const query of ["task.", "task.item"]) {
      const rows = pickRows(query, many);
      expect(rows).toHaveLength(IN_KIND);
      expect(new Set(keys(rows))).toEqual(new Set(Array.from({ length: IN_KIND }, (_, i) => `GA-${i + 1}`)));
      const places = rows.map((r) => (r.type === "item" ? r.item.column?.place ?? -1 : -1));
      expect(places).toEqual([...places].sort((a, b) => a - b));
      expect(rows.map((_, i) => rowHeading(query, rows, i)).filter(Boolean)).toEqual(BOARD);
      // In a column, the list's order.
      const backlog = keys(rows).filter((_, i) => places[i] === 0).map((k) => Number(k.slice(3)));
      expect(backlog).toEqual([...backlog].sort((a, b) => a - b));
    }
  });
});

describe("rowHeading", () => {
  it("is the column's name above each column's first task with @task., and nothing above the others", () => {
    const rows = pickRows("task.", TASKS);
    expect(rows.map((_, i) => rowHeading("task.", rows, i))).toEqual([
      "Backlog", null, "To do", null, "In progress", null, null, "Testing", "Review", "Deploy",
    ]);
  });
  it("heads the kinds as before when searching every kind: no columns, and the tasks in the order they had", () => {
    const others: PickItem[] = [
      projectItem({ id: "p1", key: "GA", name: "Giz AI", clientName: null } as Project),
      clientItem({ id: "c1", name: "Café Login", legalName: null } as Client),
      docItem({ id: "d1", title: "Login flow" } as Doc),
    ];
    const plain = TASKS.map((t) => ({ ...t, column: undefined }));
    const rows = pickRows("login", [...TASKS, ...others]);
    expect(keys(rows)).toEqual(keys(pickRows("login", [...plain, ...others])));
    // The best 5 tasks as before (grouped by column they would start with GA-4, in To do), then the client and the doc.
    expect(keys(rows)).toEqual(["LOGIN-2", "GA-2", "GA-4", "GA-6", "GA-8", "c1", "d1"]);
    expect(rows.map((_, i) => rowHeading("login", rows, i))).toEqual([KIND_GROUP.task, null, null, null, null, KIND_GROUP.client, KIND_GROUP.doc]);
  });
  it("is nothing for only an @ (the kinds) and for the other kinds", () => {
    const kinds = pickRows("", TASKS);
    expect(kinds.map((_, i) => rowHeading("", kinds, i))).toEqual([null, null, null, null, null, null]);
    const projects = pickRows("project.", [projectItem({ id: "p1", key: "GA", name: "Giz AI" } as Project), projectItem({ id: "p2", key: "KADE", name: "Kade" } as Project)]);
    expect(projects.map((_, i) => rowHeading("project.", projects, i))).toEqual([null, null]);
    // A task's column never heads rows outside @task. (a row with a column under another kind's dot).
    const odd: PickRow[] = [{ type: "item", item: { kind: "project", key: "GA", label: "GA", text: "GA", column: { name: "To do", place: 1 } } }];
    expect(rowHeading("project.", odd, 0)).toBeNull();
    expect(rowHeading("task.", [], 0)).toBeNull();
  });
});
