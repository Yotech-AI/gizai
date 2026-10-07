import { describe, expect, it } from "vitest";
import { followsPull, pullAction, pullBadge, pullHint, pullLabel, pullNumber } from "./pulls";

const URL = "https://github.com/acme/shop/pull/12";
const card = (stateCategory: string, more: { prUrl?: string | null; prState?: "open" | "draft" | "merged" | "closed" | null; branch?: string | null } = {}) =>
  ({ stateCategory, branch: "gizai/shop-1-export", prUrl: null, prState: null, ...more });

describe("pull request links", () => {
  it("reads the number from the link", () => {
    expect(pullNumber(URL)).toBe(12);
    expect(pullNumber(URL + "/")).toBe(12);
    expect(pullNumber(" " + URL + " ")).toBe(12);
    expect(pullNumber("https://github.com/acme/shop")).toBeNull();
    expect(pullLabel(URL)).toBe("PR #12");
    expect(pullLabel("https://github.com/acme/shop/pulls")).toBe("PR");
  });
  it("shows each state as a badge, and an unknown one as open", () => {
    expect(pullBadge("open")).toEqual({ text: "Open", cls: "info" });
    expect(pullBadge("draft")).toEqual({ text: "Draft", cls: "outline" });
    expect(pullBadge("merged")).toEqual({ text: "Merged", cls: "ok" });
    expect(pullBadge("closed")).toEqual({ text: "Closed", cls: "fail" });
    expect(pullBadge(null)).toEqual({ text: "Open", cls: "info" });
    expect(pullBadge("weird")).toEqual({ text: "Open", cls: "info" });
  });
});

describe("the Open pull request button", () => {
  it("is on a card in Review only", () => {
    expect(pullAction(card("review"), false)).toEqual({ label: "Open pull request", why: null });
    for (const c of ["backlog", "ready", "in_progress", "testing", "done", "cancelled"]) expect(pullAction(card(c), false)).toBeNull();
  });
  it("becomes Push branch while the card has an open or draft pull request", () => {
    expect(pullAction(card("review", { prUrl: URL, prState: "open" }), false)?.label).toBe("Push branch");
    expect(pullAction(card("review", { prUrl: URL, prState: "draft" }), false)?.label).toBe("Push branch");
    expect(pullAction(card("review", { prUrl: URL, prState: null }), false)?.label).toBe("Push branch");
    expect(pullAction(card("review", { prUrl: URL, prState: "closed" }), false)?.label).toBe("Open pull request");
    expect(pullAction(card("review", { prUrl: URL, prState: "merged" }), false)?.label).toBe("Open pull request");
  });
  it("says why it can't be used yet", () => {
    expect(pullAction(card("review", { branch: null }), false)?.why).toMatch(/no branch yet/);
    expect(pullAction(card("review"), true)?.why).toMatch(/An agent is working on this card/);
  });
});

describe("following a pull request", () => {
  it("follows cards in Review and pull requests that aren't merged, like the PR check", () => {
    expect(followsPull(card("review"))).toBe(true);
    expect(followsPull(card("ready"))).toBe(false);
    expect(followsPull(card("ready", { prUrl: URL, prState: "open" }))).toBe(true);
    expect(followsPull(card("in_progress", { prUrl: URL, prState: "closed" }))).toBe(true);
    expect(followsPull(card("in_progress", { prUrl: URL, prState: "merged" }))).toBe(false);
    expect(followsPull(card("done", { prUrl: URL, prState: "open" }))).toBe(false);
    expect(followsPull(card("cancelled", { prUrl: URL, prState: "open" }))).toBe(false);
  });
  it("says in one line what happens next", () => {
    expect(pullHint(card("review"), "main")).toBe("Pushes gizai/shop-1-export to GitHub with your git login and opens a pull request into main with gh. When it is merged, the card moves to Done.");
    expect(pullHint(card("review", { prUrl: URL, prState: "open" }), "main")).toBe("When it is merged on GitHub, the card moves to Done and its worktree is removed. Push branch adds new commits from gizai/shop-1-export to it.");
    expect(pullHint(card("ready", { prUrl: URL, prState: "open" }), "main")).toBe("When it is merged on GitHub, the card moves to Done and its worktree is removed.");
    expect(pullHint(card("done", { prUrl: URL, prState: "merged" }), "main")).toBe("Merged on GitHub.");
    expect(pullHint(card("review", { prUrl: URL, prState: "closed" }), "main")).toMatch(/^Closed on GitHub without a merge\. Open pull request opens a new one\.$/);
  });
});
