// GA-90 QA: which of the sidebar's Agents and Memory are folded, as kept in localStorage, and the live runs folded
// Agents shows on its heading.
import { describe, expect, it } from "vitest";
import { FOLDED_KEY, foldedOf, liveTotal, toggleFold } from "./sidebar";

describe("the sidebar's folded sections in localStorage (GA-90)", () => {
  it("is kept under gizai.sidebar.folded", () => {
    expect(FOLDED_KEY).toBe("gizai.sidebar.folded");
  });
  it("starts with nothing folded on a computer that never folded them", () => {
    expect(foldedOf(null)).toEqual([]);
    expect(foldedOf("")).toEqual([]);
  });
  it("reads the comma list, leaving out blanks and spaces", () => {
    expect(foldedOf("agents,memory")).toEqual(["agents", "memory"]);
    expect(foldedOf(" memory , ,agents,")).toEqual(["memory", "agents"]);
  });
  it("folds an open section and opens a folded one", () => {
    expect(toggleFold(null, "agents")).toBe("agents");
    expect(toggleFold("agents", "agents")).toBe("");
    expect(toggleFold("", "memory")).toBe("memory");
  });
  it("leaves the other section as it was", () => {
    expect(toggleFold("agents", "memory")).toBe("agents,memory");
    expect(toggleFold("agents,memory", "agents")).toBe("memory");
    expect(toggleFold("agents,memory", "memory")).toBe("agents");
    expect(foldedOf(toggleFold(toggleFold(null, "memory"), "agents"))).toEqual(["memory", "agents"]);
  });
  it("folds a section only once, even when it was kept twice", () => {
    expect(toggleFold("agents,agents", "agents")).toBe("");
  });
});

describe("the live runs on folded Agents' heading (GA-90)", () => {
  const agents = [{ actorId: "a1" }, { actorId: "a2" }, { actorId: "a3" }];
  it("adds up every live run of the agents the sidebar lists", () => {
    expect(liveTotal([{ agentId: "a1" }, { agentId: "a1" }, { agentId: "a3" }], agents)).toBe(3);
  });
  it("is 0 with nothing running", () => {
    expect(liveTotal([], agents)).toBe(0);
    expect(liveTotal([], [])).toBe(0);
  });
  it("leaves out runs of agents the sidebar doesn't list (another team's), like the per-agent tags", () => {
    expect(liveTotal([{ agentId: "a2" }, { agentId: "elsewhere" }], agents)).toBe(1);
  });
});
