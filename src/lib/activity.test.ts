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
  it("describes a card's pull request on GitHub and the clean-up after its merge", () => {
    const pr = "https://github.com/acme/shop/pull/7";
    expect(describeChange(e("tasks", "update", { pullRequest: pr, prState: "open", opened: true }))).toBe("opened pull request #7 on GitHub");
    expect(describeChange(e("tasks", "update", { pullRequest: pr, prState: "open" }))).toBe("saw pull request #7 open on GitHub");
    expect(describeChange(e("tasks", "update", { pullRequest: pr, prState: "draft" }))).toBe("saw pull request #7 as a draft on GitHub");
    expect(describeChange(e("tasks", "update", { pullRequest: pr, prState: "merged" }))).toBe("saw pull request #7 merged on GitHub");
    expect(describeChange(e("tasks", "update", { pullRequest: "https://github.com/acme/shop/pulls", prState: "closed" }))).toBe("saw a pull request closed on GitHub");
    expect(describeChange(e("tasks", "update", { cleanup: "removed its worktree and deleted branch gizai/kade-1-x after the merge" })))
      .toBe("removed its worktree and deleted branch gizai/kade-1-x after the merge");
  });
});

// GA-60: the activity names Bitbucket for a Bitbucket pull request (its link says which).
describe("describeChange on Bitbucket", () => {
  it("describes a card's pull request on Bitbucket", () => {
    const pr = "https://bitbucket.org/acme/shop/pull-requests/12";
    expect(describeChange(e("tasks", "update", { pullRequest: pr, prState: "open", opened: true }))).toBe("opened pull request #12 on Bitbucket");
    expect(describeChange(e("tasks", "update", { pullRequest: pr, prState: "open" }))).toBe("saw pull request #12 open on Bitbucket");
    expect(describeChange(e("tasks", "update", { pullRequest: pr, prState: "draft" }))).toBe("saw pull request #12 as a draft on Bitbucket");
    expect(describeChange(e("tasks", "update", { pullRequest: pr, prState: "merged" }))).toBe("saw pull request #12 merged on Bitbucket");
    expect(describeChange(e("tasks", "update", { pullRequest: pr, prState: "closed" }))).toBe("saw pull request #12 closed on Bitbucket");
    expect(describeChange(e("tasks", "update", { pullRequest: "https://bitbucket.org/acme/shop/pull-requests", opened: true }))).toBe("opened a pull request on Bitbucket");
  });
});
