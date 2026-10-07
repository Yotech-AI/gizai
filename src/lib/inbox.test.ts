import { describe, expect, it } from "vitest";
import { needsYou } from "./inbox";

const t = (o: Partial<{ hold: string | null; stateCategory: string; assigneeId: string | null }>) =>
  ({ hold: null, stateCategory: "ready", assigneeId: null, ...o });

describe("needsYou", () => {
  it("counts cards on hold and cards waiting in Review for you", () => {
    expect(needsYou(t({ hold: "needs_decision" }), "me")).toBe(true);
    expect(needsYou(t({ stateCategory: "review", assigneeId: "me" }), "me")).toBe(true);
  });
  it("skips Review cards for someone else and ordinary cards", () => {
    expect(needsYou(t({ stateCategory: "review", assigneeId: "sanne" }), "me")).toBe(false);
    expect(needsYou(t({ stateCategory: "in_progress", assigneeId: "me" }), "me")).toBe(false);
    expect(needsYou(t({ hold: "blocked", stateCategory: "done" }), "me")).toBe(false);
  });
});
