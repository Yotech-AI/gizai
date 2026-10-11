// GA-96 QA: agents that share one memory folder, the UI's pure parts. The agent form's Shares memory with (the choices,
// what a pick stores, what is saved and loaded) in src/lib/agents.ts, and where an agent's memory is for the sidebar and
// the Memory page (memoryHome, sharersOf, and the scope and count they give) in src/lib/memory.ts.
import { describe, expect, it } from "vitest";
import { draftFrom, inputFrom, isLeadDraft, shareChoices, shareTarget } from "./agents";
import { inScope, memoryHome, memoryScope, noteCount, sharersOf } from "./memory";
import type { Member, MemoryNote } from "../types";

const agent = (actorId: string, name: string, more: Partial<Member> = {}): Member => ({
  actorId, name, kind: "agent", roleKey: "backend", handle: name.toLowerCase().replace(/ /g, "-"), status: "active", isLead: false,
  allowedTools: [], chatEnabled: false, ...more,
});
const lead = agent("lead", "Team Lead", { roleKey: "lead", isLead: true, chatEnabled: true });
const be = agent("be", "Backend Agent");
const be2 = agent("be2", "Backend Agent 2", { sharesMemoryWith: "be" });
const fe = agent("fe", "Frontend Agent", { roleKey: "frontend" });
const chat = agent("chat", "Chat Agent", { chatEnabled: true });
const leadNoChat = agent("lead2", "Second Lead", { roleKey: "lead", isLead: true });
const gone = agent("old", "Old Agent", { status: "archived" });
const team = [lead, be, be2, fe, chat, leadNoChat, gone];

describe("Shares memory with: the choices", () => {
  it("are every other agent by name, never the Team Lead (the lead role or Chat), itself or an archived one", () => {
    expect(shareChoices("fe", team).map((c) => c.agent.name)).toEqual(["Backend Agent", "Backend Agent 2"]);
    expect(shareChoices("be", team).map((c) => c.agent.name)).toEqual(["Backend Agent 2", "Frontend Agent"]);
    // a new agent has no id yet: every agent but the Team Lead
    expect(shareChoices(undefined, team).map((c) => c.agent.name)).toEqual(["Backend Agent", "Backend Agent 2", "Frontend Agent"]);
  });
  it("name, for an agent that shares, the agent whose folder it shares", () => {
    const c = shareChoices("fe", team);
    expect(c.map((x) => [x.agent.actorId, x.owner?.name ?? null])).toEqual([["be", null], ["be2", "Backend Agent"]]);
    // an agent that names itself has its own folder
    expect(shareChoices("fe", [agent("x", "X", { sharesMemoryWith: "x" })])[0]!.owner).toBeNull();
  });
});

describe("Shares memory with: what a pick stores", () => {
  it("is the agent picked, or its own folder for none", () => {
    expect(shareTarget("be", "fe", team)).toBe("be");
    expect(shareTarget("", "fe", team)).toBe("");
  });
  it("is the group's owner for an agent that shares: no chains", () => {
    expect(shareTarget("be2", "fe", team)).toBe("be");
    expect(shareTarget("be2", undefined, team)).toBe("be");
  });
  it("is its own folder when the owner of the group picked is the agent itself", () => {
    expect(shareTarget("be2", "be", team)).toBe("");
  });
});

describe("Shares memory with: saved and loaded", () => {
  it("is its own folder for a new agent and one saved before it existed", () => {
    expect(draftFrom(null).memoryWith).toBe("");
    expect(inputFrom(draftFrom(null)).sharesMemoryWith).toBe("");
    expect(draftFrom(be).memoryWith).toBe("");
    expect(draftFrom({ ...be, sharesMemoryWith: null }).memoryWith).toBe("");
  });
  it("loads the owner's id back into the form and saves it as it is, also when another field changes", () => {
    const d = draftFrom(be2);
    expect(d.memoryWith).toBe("be");
    expect(inputFrom(d).sharesMemoryWith).toBe("be");
    expect(inputFrom({ ...d, model: "sonnet" }).sharesMemoryWith).toBe("be");
    expect(inputFrom({ ...d, memoryWith: "" }).sharesMemoryWith).toBe(""); // back to its own folder
  });
  it("is never sent for the Team Lead: the lead role or Chat on keeps its own notes", () => {
    const d = { ...draftFrom(be2) };
    expect(isLeadDraft(d)).toBe(false);
    expect(isLeadDraft({ ...d, role: "lead" })).toBe(true);
    expect(isLeadDraft({ ...d, chat: true })).toBe(true);
    expect(inputFrom({ ...d, role: "lead" }).sharesMemoryWith).toBe("");
    expect(inputFrom({ ...d, chat: true }).sharesMemoryWith).toBe("");
  });
});

describe("where an agent's memory is (the sidebar and the Memory page)", () => {
  const note = (id: string, path: string, ownerId?: string): MemoryNote =>
    ({ id, path, scope: ownerId ? "agent" : "shared", ownerId, bodyMd: "", currentVersion: 1, updatedAt: 1, chars: 0 });
  const notes = [
    note("n1", "Agents/Backend Agent/Notes", "be"), note("n2", "Agents/Backend Agent/Gotchas", "be"),
    note("n3", "Agents/Frontend Agent/Notes", "fe"), note("n4", "Standards/Rust style"), note("n5", "Agents/Other Team Agent/Notes", "other"),
  ];

  it("is its own folder for an agent that shares with nobody, as before", () => {
    expect(memoryHome(be, team, notes)).toEqual({ ownerId: "be", name: "Backend Agent", shares: false });
    expect(memoryHome(fe, team, notes)).toEqual({ ownerId: "fe", name: "Frontend Agent", shares: false });
    expect(memoryHome({ ...fe, sharesMemoryWith: "fe" }, team, notes).shares).toBe(false);
  });
  it("is the group's folder for an agent that shares, by the owner's name", () => {
    expect(memoryHome(be2, team, notes)).toEqual({ ownerId: "be", name: "Backend Agent", shares: true });
  });
  it("finds the folder's name in the notes when the owner is on another team, else says Agent", () => {
    const x = agent("x", "Shop Agent", { sharesMemoryWith: "other" });
    expect(memoryHome(x, team, notes)).toEqual({ ownerId: "other", name: "Other Team Agent", shares: true });
    expect(memoryHome(agent("y", "Y", { sharesMemoryWith: "nobody" }), team, notes)).toEqual({ ownerId: "nobody", name: "Agent", shares: true });
  });
  it("names the agents that share an owner's folder", () => {
    expect(sharersOf("be", team)).toEqual(["Backend Agent 2"]);
    expect(sharersOf("fe", team)).toEqual([]);
    expect(sharersOf("be2", [...team, agent("be3", "Backend Agent 3", { sharesMemoryWith: "be" })])).toEqual([]);
  });
  it("gives every member of a group the group's folder and its count; the others keep theirs", () => {
    const scopeOf = (m: Member) => { const h = memoryHome(m, team, notes); return memoryScope(h.ownerId, h.name); };
    expect(scopeOf(be2)).toEqual(scopeOf(be));
    expect(scopeOf(be2)).toEqual({ kind: "agent", agentId: "be", folder: "Agents/Backend Agent" });
    expect(notes.filter((n) => inScope(n, scopeOf(be2))).map((n) => n.path)).toEqual(["Agents/Backend Agent/Notes", "Agents/Backend Agent/Gotchas"]);
    expect([noteCount(notes, scopeOf(be)), noteCount(notes, scopeOf(be2)), noteCount(notes, scopeOf(fe))]).toEqual([2, 2, 1]);
    // the Frontend Agent's and the shared notes are never in the group's view
    expect(notes.filter((n) => inScope(n, scopeOf(be2))).some((n) => n.ownerId === "fe" || n.scope === "shared")).toBe(false);
  });
});
