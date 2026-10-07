import { describe, expect, it } from "vitest";
import { describeChange } from "./activity";
const e = (table: string, op: string, diff: unknown) => ({ at: 0, actorName: "Jeffrey", table, op, diff });
describe("describeChange", () => {
  it("describes creating, moving and labelling a task", () => {
    expect(describeChange(e("tasks", "insert", { identifier: "KADE-1", title: "x" }))).toBe("created the task");
    expect(describeChange(e("tasks", "update", { column: ["To do", "In progress"] }))).toBe("moved it from To do to In progress");
    expect(describeChange(e("tasks", "update", { labels: ["frontend", "bug"] }))).toBe("set labels to frontend, bug");
    expect(describeChange(e("tasks", "update", { labels: [] }))).toBe("removed all labels");
  });
  it("lists only the fields an edit actually changed", () => {
    expect(describeChange(e("tasks", "update", { title: "New", descriptionMd: null, priority: 2, hold: null }))).toBe("changed the title and priority");
    expect(describeChange(e("tasks", "update", { descriptionMd: "long text" }))).toBe("changed the description");
    expect(describeChange(e("tasks", "update", { hold: "" }))).toBe("cleared the hold");
  });
  it("describes comments", () => {
    expect(describeChange(e("comments", "insert", { task_id: "t", chars: 12 }))).toBe("commented");
  });
});
