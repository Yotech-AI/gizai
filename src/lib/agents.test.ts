import { describe, expect, it } from "vitest";
import { dollarsToMicros, microsToDollars, parseTools, ruleSentence, wakeupLabel } from "./agents";

describe("wakeupLabel", () => {
  it("names the three wake-ups", () => {
    expect(wakeupLabel("manual", null)).toBe("Manual");
    expect(wakeupLabel("on_assign", null)).toBe("When assigned");
    expect(wakeupLabel("heartbeat", 15)).toBe("Every 15 min");
    expect(wakeupLabel("heartbeat", 60)).toBe("Every hour");
    expect(wakeupLabel("heartbeat", 90)).toBe("Every 90 min");
  });
});

describe("money", () => {
  it("converts dollars and micros both ways", () => {
    expect(dollarsToMicros("2.5")).toBe(2_500_000);
    expect(dollarsToMicros("")).toBeNull();
    expect(dollarsToMicros("abc")).toBeNull();
    expect(microsToDollars(420_000)).toBe("0.42");
    expect(microsToDollars(null)).toBe("");
  });
});

describe("parseTools", () => {
  it("takes one tool per line and drops blanks", () => {
    expect(parseTools("Bash(npm test:*)\n\n  Read \n")).toEqual(["Bash(npm test:*)", "Read"]);
  });
});

describe("ruleSentence", () => {
  const team = { labels: [{ id: "L1", name: "frontend" }], states: [{ id: "S4", name: "Testing" }] };
  it("reads like the mockup", () => {
    expect(ruleSentence({ kind: "label", matchLabelId: "L1", targetRole: "frontend" }, team)).toBe("A card labelled frontend goes to a frontend agent");
    expect(ruleSentence({ kind: "column", matchStateId: "S4", targetRole: "qa" }, team)).toBe("A card that enters Testing goes to a qa agent");
    expect(ruleSentence({ kind: "label", matchLabelId: "gone", targetRole: "x" }, team)).toBe("A card labelled (deleted label) goes to a x agent");
  });
});

import { draftFrom, inputFrom, ROLES, roleLabel } from "./agents";

describe("agent drafts", () => {
  it("start from a preset: the Chat page's Team Lead", () => {
    const d = draftFrom(null, { name: "Team Lead", role: "lead", chat: true });
    expect(d).toMatchObject({ name: "Team Lead", role: "lead", chat: true, wakeup: "manual" });
    expect(inputFrom(d)).toMatchObject({ name: "Team Lead", roleKey: "lead", chatEnabled: true });
  });
  it("keep an agent's chat setting and send it back", () => {
    const m = { actorId: "a", name: "Backend Agent", kind: "agent", roleKey: "backend", handle: "b", status: "active", isLead: false, allowedTools: [], chatEnabled: false };
    expect(draftFrom(m).chat).toBe(false);
    expect(inputFrom(draftFrom(m)).chatEnabled).toBe(false);
  });
  it("offer the software roles with readable names", () => {
    expect(ROLES).toEqual(["lead", "frontend", "backend", "design", "qa", "devops"]);
    expect(ROLES.map(roleLabel)).toEqual(["Team Lead", "Frontend", "Backend", "Design", "QA", "DevOps"]);
    expect(roleLabel("api")).toBe("Api");
  });
});

describe("agent effort", () => {
  it("round-trips, and empty means Claude Code's default", () => {
    const m = { actorId: "a", name: "Frontend Agent", kind: "agent", roleKey: "frontend", handle: "f", status: "active", isLead: false, allowedTools: [], chatEnabled: false, effort: "xhigh" };
    expect(draftFrom(m).effort).toBe("xhigh");
    expect(inputFrom(draftFrom(m)).effort).toBe("xhigh");
    expect(inputFrom({ ...draftFrom(m), effort: "" }).effort).toBeNull();
    expect(draftFrom(null).effort).toBe("");
  });
});

describe("cards at once", () => {
  it("starts at one for a new agent and round-trips a saved number", () => {
    expect(draftFrom(null).maxRuns).toBe("1");
    expect(inputFrom({ ...draftFrom(null), maxRuns: "6" }).maxRuns).toBe(6);
  });
  it("sends nothing for an empty or bad number, so the agent keeps its setting", () => {
    expect(inputFrom({ ...draftFrom(null), maxRuns: "" }).maxRuns).toBeNull();
    expect(inputFrom({ ...draftFrom(null), maxRuns: "x" }).maxRuns).toBeNull();
  });
});

describe("the board check setting", () => {
  const lead = { actorId: "a", name: "Team Lead", kind: "agent", roleKey: "lead", handle: "l", status: "active", isLead: true, allowedTools: [], chatEnabled: true };
  it("is off for an existing agent, 15 min when turned on, and sent as 0 when off", () => {
    const d = draftFrom(lead);
    expect(d).toMatchObject({ boardCheck: false, boardMinutes: "15" });
    expect(inputFrom(d).boardCheckMinutes).toBe(0);
    expect(inputFrom({ ...d, boardCheck: true }).boardCheckMinutes).toBe(15);
  });
  it("keeps the agent's interval, and goes off with Chat", () => {
    const d = draftFrom({ ...lead, boardCheckMinutes: 30 });
    expect(d).toMatchObject({ boardCheck: true, boardMinutes: "30" });
    expect(inputFrom(d).boardCheckMinutes).toBe(30);
    expect(inputFrom({ ...d, chat: false }).boardCheckMinutes).toBe(0);
  });
});

describe("a Team Lead with both a board check and folders (GA-35 and GA-45, merged by GA-47)", () => {
  const lead = { actorId: "a", name: "Team Lead", kind: "agent", roleKey: "lead", handle: "l", status: "active", isLead: true, allowedTools: [],
    chatEnabled: true, boardCheckMinutes: 30, folders: [{ path: "/srv/shared", access: "read" as const }] };
  it("loads and sends both, and changing one keeps the other", () => {
    const d = draftFrom(lead);
    expect(d).toMatchObject({ boardCheck: true, boardMinutes: "30", folders: [{ path: "/srv/shared", access: "read" }] });
    expect(inputFrom(d)).toMatchObject({ boardCheckMinutes: 30, folders: [{ path: "/srv/shared", access: "read" }] });
    expect(inputFrom({ ...d, boardMinutes: "60" })).toMatchObject({ boardCheckMinutes: 60, folders: [{ path: "/srv/shared", access: "read" }] });
    const more = { ...d, folders: [...d.folders, { path: " /srv/out ", access: "change" as const }] };
    expect(inputFrom(more)).toMatchObject({ boardCheckMinutes: 30, folders: [{ path: "/srv/shared", access: "read" }, { path: "/srv/out", access: "change" }] });
    // Chat off turns the check off, not the folders
    expect(inputFrom({ ...d, chat: false })).toMatchObject({ boardCheckMinutes: 0, folders: [{ path: "/srv/shared", access: "read" }] });
  });
});
