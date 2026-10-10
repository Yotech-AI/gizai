import { describe, expect, it } from "vitest";
import { href, parseHash, SETTINGS_TABS, type Route } from "../router";

describe("router", () => {
  it("round-trips routes", () => {
    for (const r of [{ page: "tasks" }, { page: "task", id: "0192-abc" }, { page: "client", id: "x" }, { page: "board" }] as const)
      expect(parseHash(href(r))).toEqual(r);
  });
  it("defaults to tasks for empty or unknown hashes", () => {
    expect(parseHash("")).toEqual({ page: "tasks" });
    expect(parseHash("#/nope")).toEqual({ page: "tasks" });
  });
  it("ignores an id on pages that don't take one", () => {
    expect(parseHash("#/clients/abc")).toEqual({ page: "clients" });
  });
});

describe("doc pages", () => {
  it("opens a doc by id and falls back to projects without one", () => {
    expect(parseHash("#/doc/abc")).toEqual({ page: "doc", id: "abc" });
    expect(parseHash("#/doc")).toEqual({ page: "projects" });
  });
});

describe("team page", () => {
  it("can select an agent and stays on the team page without one", () => {
    expect(parseHash("#/team/agent-1")).toEqual({ page: "team", id: "agent-1" });
    expect(parseHash("#/team")).toEqual({ page: "team" });
  });
});

describe("inbox", () => {
  it("is a page of its own", () => {
    expect(parseHash("#/inbox")).toEqual({ page: "inbox" });
  });
});

describe("agent pages", () => {
  it("open an agent by id and fall back to the team page", () => {
    expect(parseHash("#/agent/a1")).toEqual({ page: "agent", id: "a1" });
    expect(parseHash("#/agent")).toEqual({ page: "team" });
  });
  it("knows the chat page, with or without a thread", () => {
    expect(parseHash("#/chat")).toEqual({ page: "chat" });
    expect(parseHash("#/chat/abc")).toEqual({ page: "chat", id: "abc" });
  });
});

describe("chat archive (GA-46)", () => {
  it("is #/chats, a page of its own", () => {
    expect(parseHash("#/chats")).toEqual({ page: "chats" });
    expect(href({ page: "chats" })).toBe("#/chats");
    expect(parseHash(href({ page: "chats" }))).toEqual({ page: "chats" });
  });
  it("takes no id, and chat/<id> is still one chat", () => {
    expect(parseHash("#/chats/abc")).toEqual({ page: "chats" });
    expect(parseHash("#/chat/archive")).toEqual({ page: "chat", id: "archive" });
  });
});

describe("Settings tabs (GA-42)", () => {
  it("opens a tab from #/settings/<tab>, for each of the six tabs", () => {
    for (const tab of ["general", "appearance", "notifications", "agents", "mcp", "github"])
      expect(parseHash(`#/settings/${tab}`)).toEqual({ page: "settings", id: tab });
  });
  it("lists the tabs in the order Settings shows them", () => {
    expect(SETTINGS_TABS).toEqual(["general", "appearance", "notifications", "agents", "mcp", "github"]);
  });
  it("opens General (no tab) for #/settings, and for a tab it doesn't know", () => {
    expect(parseHash("#/settings")).toEqual({ page: "settings" });
    expect(parseHash("#/settings/")).toEqual({ page: "settings" });
    expect(parseHash("#/settings/nope")).toEqual({ page: "settings" });
    expect(parseHash("#/settings/Appearance")).toEqual({ page: "settings" });
  });
  it("round-trips a tab through href, the link a page or notice uses", () => {
    expect(href({ page: "settings", id: "appearance" })).toBe("#/settings/appearance");
    expect(parseHash(href({ page: "settings", id: "github" }))).toEqual({ page: "settings", id: "github" });
    expect(parseHash(href({ page: "settings" }))).toEqual({ page: "settings" });
  });
  it("leaves the other pages with an id as they were", () => {
    expect(parseHash("#/task/abc")).toEqual({ page: "task", id: "abc" });
    expect(parseHash("#/usage/total")).toEqual({ page: "usage" });
  });
});

describe("Memory page (GA-68)", () => {
  it("#/memory is every note, with a note's id after it to open that note", () => {
    expect(parseHash("#/memory")).toEqual({ page: "memory" });
    expect(parseHash("#/memory/")).toEqual({ page: "memory" });
    expect(parseHash("#/memory/n1")).toEqual({ page: "memory", id: "n1" });
  });
  it("#/memory/shared is the shared folders, with or without a note", () => {
    expect(parseHash("#/memory/shared")).toEqual({ page: "memory", scope: "shared" });
    expect(parseHash("#/memory/shared/n1")).toEqual({ page: "memory", scope: "shared", id: "n1" });
  });
  it("#/memory/agent/<agent id> is one agent's folder, with or without a note", () => {
    expect(parseHash("#/memory/agent/a1")).toEqual({ page: "memory", scope: "a1" });
    expect(parseHash("#/memory/agent/a1/n1")).toEqual({ page: "memory", scope: "a1", id: "n1" });
  });
  it("#/memory/agent without an agent's id is every note", () => {
    expect(parseHash("#/memory/agent")).toEqual({ page: "memory" });
    expect(parseHash("#/memory/agent/")).toEqual({ page: "memory" });
  });
  it("decodes percent-encoded ids", () => {
    expect(parseHash("#/memory/n%201")).toEqual({ page: "memory", id: "n 1" });
    expect(parseHash("#/memory/shared/n%2F1")).toEqual({ page: "memory", scope: "shared", id: "n/1" });
    expect(parseHash("#/memory/agent/a%2F1/n%201")).toEqual({ page: "memory", scope: "a/1", id: "n 1" });
  });
  it("writes each shape as its hash", () => {
    expect(href({ page: "memory" })).toBe("#/memory");
    expect(href({ page: "memory", id: "n1" })).toBe("#/memory/n1");
    expect(href({ page: "memory", scope: "shared" })).toBe("#/memory/shared");
    expect(href({ page: "memory", scope: "shared", id: "n1" })).toBe("#/memory/shared/n1");
    expect(href({ page: "memory", scope: "a1" })).toBe("#/memory/agent/a1");
    expect(href({ page: "memory", scope: "a1", id: "n1" })).toBe("#/memory/agent/a1/n1");
  });
  it("round-trips every shape through href, also ids that need encoding", () => {
    const routes: Route[] = [
      { page: "memory" },
      { page: "memory", id: "0192-abc" },
      { page: "memory", scope: "shared" },
      { page: "memory", scope: "shared", id: "0192-abc" },
      { page: "memory", scope: "0192-agent" },
      { page: "memory", scope: "0192-agent", id: "0192-abc" },
      { page: "memory", id: "n 1/x%y" },
      { page: "memory", scope: "shared", id: "n/1 é" },
      { page: "memory", scope: "a/1 #x", id: "n?1&2" },
    ];
    for (const r of routes) expect(parseHash(href(r)), JSON.stringify(r)).toEqual(r);
  });
  it("leaves #/doc/<id> as it was: a doc page by id", () => {
    expect(parseHash("#/doc/abc")).toEqual({ page: "doc", id: "abc" });
    expect(parseHash(href({ page: "doc", id: "a b" }))).toEqual({ page: "doc", id: "a b" });
    expect(parseHash("#/doc")).toEqual({ page: "projects" });
  });
});

describe("Memory graph (GA-69)", () => {
  it("#/memory/graph, #/memory/shared/graph and #/memory/agent/<id>/graph are the graph of those notes", () => {
    expect(parseHash("#/memory/graph")).toEqual({ page: "memory", view: "graph" });
    expect(parseHash("#/memory/shared/graph")).toEqual({ page: "memory", scope: "shared", view: "graph" });
    expect(parseHash("#/memory/agent/a1/graph")).toEqual({ page: "memory", scope: "a1", view: "graph" });
  });
  it("writes the graph's hash for each scope, without the note open last", () => {
    expect(href({ page: "memory", view: "graph" })).toBe("#/memory/graph");
    expect(href({ page: "memory", scope: "shared", view: "graph" })).toBe("#/memory/shared/graph");
    expect(href({ page: "memory", scope: "a1", view: "graph" })).toBe("#/memory/agent/a1/graph");
    expect(href({ page: "memory", scope: "a1", id: "n1", view: "graph" })).toBe("#/memory/agent/a1/graph");
  });
  it("round-trips the graph routes, and leaves a note's route a note's", () => {
    const routes: Route[] = [
      { page: "memory", view: "graph" },
      { page: "memory", scope: "shared", view: "graph" },
      { page: "memory", scope: "0192-agent", view: "graph" },
      { page: "memory", scope: "a/1 #x", view: "graph" },
      { page: "memory", id: "0192-graph" },
      { page: "memory", scope: "shared", id: "graphs" },
    ];
    for (const r of routes) expect(parseHash(href(r)), JSON.stringify(r)).toEqual(r);
    expect(parseHash("#/memory/agent/graph")).toEqual({ page: "memory", scope: "graph" });
  });
});
