import { describe, expect, it } from "vitest";
import { filterTasks, groupTasks, sortTasks, type ViewTask } from "./taskView";

const t = (o: Partial<ViewTask> & { id: string }): ViewTask => ({
  identifier: o.id.toUpperCase(), title: o.id, stateId: "s-todo", stateName: "To do", stateCategory: "ready", priority: 0, sortKey: "a0",
  updatedAt: 0, labels: [], assigneeId: null, assigneeName: null, projectId: "p1", projectName: "Kade portal", projectColor: null, hold: null, ...o,
});
const states = [{ id: "s-back", name: "Backlog", category: "backlog" }, { id: "s-todo", name: "To do", category: "ready" }, { id: "s-prog", name: "In progress", category: "in_progress" }];

describe("groupTasks", () => {
  it("groups by column in workflow order and keeps empty columns out", () => {
    const g = groupTasks([t({ id: "a", stateId: "s-prog", stateName: "In progress" }), t({ id: "b" })], "status", states);
    expect(g.map((x) => [x.label, x.tasks.map((y) => y.id)])).toEqual([["To do", ["b"]], ["In progress", ["a"]]]);
    expect(g[1].category).toBe("in_progress");
  });
  it("groups by assignee with Unassigned last, by priority from Urgent to None, and not at all", () => {
    const tasks = [t({ id: "a" }), t({ id: "b", assigneeId: "u1", assigneeName: "Sanne" }), t({ id: "c", priority: 1 }), t({ id: "d", priority: 3 })];
    expect(groupTasks(tasks, "assignee", states).map((x) => x.label)).toEqual(["Sanne", "Unassigned"]);
    expect(groupTasks(tasks, "priority", states).map((x) => x.label)).toEqual(["Urgent", "Medium", "None"]);
    expect(groupTasks(tasks, "none", states).map((x) => x.tasks.length)).toEqual([4]);
  });
});

describe("sortTasks", () => {
  it("sorts by update time (newest first), priority (urgent first, none last), title and ID", () => {
    const tasks = [t({ id: "a", updatedAt: 1, priority: 0, identifier: "KADE-10" }), t({ id: "b", updatedAt: 3, priority: 4, identifier: "KADE-2" }), t({ id: "c", updatedAt: 2, priority: 1, identifier: "KADE-3" })];
    expect(sortTasks(tasks, "updated").map((x) => x.id)).toEqual(["b", "c", "a"]);
    expect(sortTasks(tasks, "priority").map((x) => x.id)).toEqual(["c", "b", "a"]);
    expect(sortTasks(tasks, "id").map((x) => x.id)).toEqual(["b", "c", "a"]);
  });
});

describe("filterTasks", () => {
  it("filters by project, label, assignee (or nobody) and text", () => {
    const tasks = [t({ id: "a", labels: [{ id: "L1", name: "frontend" }] }), t({ id: "b", projectId: "p2", assigneeId: "u1" }), t({ id: "c", title: "Café export" })];
    expect(filterTasks(tasks, { projectId: "p2" }).map((x) => x.id)).toEqual(["b"]);
    expect(filterTasks(tasks, { labelIds: ["L1"] }).map((x) => x.id)).toEqual(["a"]);
    expect(filterTasks(tasks, { assigneeId: "none" }).map((x) => x.id)).toEqual(["a", "c"]);
    expect(filterTasks(tasks, { text: "cafe" }).map((x) => x.id)).toEqual(["c"]);
  });
});
