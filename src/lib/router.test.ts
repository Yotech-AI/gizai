import { describe, expect, it } from "vitest";
import { href, parseHash, SETTINGS_TABS } from "../router";

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
