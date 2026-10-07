import { describe, expect, it } from "vitest";
import { groupMessages, toolCard, toolName } from "./chat";
import type { ChatMessage } from "../types";

const base: ChatMessage = { id: "m", threadId: "t", role: "tool", authorId: "a", authorName: "Team Lead", bodyMd: null, runId: "r", toolName: null, tool: null, createdAt: 1_000_000 };
const tool = (name: string, tool: Record<string, unknown>): ChatMessage => ({ ...base, toolName: name, tool });

describe("toolName", () => {
  it("drops the Gizai server prefix", () => {
    expect(toolName("mcp__gizai__create_task")).toBe("create_task");
    expect(toolName("Read")).toBe("Read");
  });
});

describe("toolCard", () => {
  it("links a write to what it made", () => {
    const c = toolCard(tool("mcp__gizai__create_task", { id: "t1", input: { title: "Export" },
      result: JSON.stringify({ ok: true, link: { page: "task", id: "abc", label: "KADE-14 Export" } }), isError: false }));
    expect(c).toMatchObject({ verb: "Created task", label: "KADE-14 Export", href: "#/task/abc", state: "ok" });
  });
  it("is running until the result arrives, and names its target from the input", () => {
    const c = toolCard(tool("mcp__gizai__get_task", { id: "t1", input: { task: "KADE-3" } }));
    expect(c).toMatchObject({ verb: "Opened task", label: "KADE-3", state: "running" });
    expect(c.href).toBeUndefined();
  });
  it("shows a failure with its reason", () => {
    const c = toolCard(tool("mcp__gizai__move_task", { id: "t1", input: { task: "KADE-3", column: "Nope" }, result: "No column called \"Nope\".", isError: true }));
    expect(c).toMatchObject({ verb: "Moved task", state: "error", detail: "No column called \"Nope\"." });
  });
  it("counts what a read found", () => {
    expect(toolCard(tool("mcp__gizai__read_inbox", { input: {}, result: JSON.stringify({ count: 3, items: [] }), isError: false })))
      .toMatchObject({ verb: "Read the inbox", label: "3 items", state: "ok" });
    expect(toolCard(tool("mcp__gizai__list_tasks", { input: {}, result: JSON.stringify({ count: 1, tasks: [] }), isError: false })).label).toBe("1 item");
    expect(toolCard(tool("mcp__gizai__get_overview", { input: {}, result: "{}", isError: false })).verb).toBe("Looked at the overview");
  });
  it("words Claude Code's own tools and passes unknown ones through", () => {
    expect(toolCard(tool("Read", { input: { file_path: "/srv/kade/README.md" }, result: "x", isError: false }))).toMatchObject({ verb: "Read a file", label: "README.md" });
    expect(toolCard(tool("mcp__other__thing", { input: {}, result: "x", isError: false })).verb).toBe("mcp__other__thing");
  });
});

describe("groupMessages", () => {
  const m = (id: string, role: string, at: number): ChatMessage => ({ ...base, id, role, createdAt: at, bodyMd: id });
  it("keeps the Team Lead's text and tool calls together and starts a new group per speaker", () => {
    const g = groupMessages([m("u1", "user", 0), m("a1", "agent", 1000), m("t1", "tool", 2000), m("a2", "agent", 3000), m("u2", "user", 4000)]);
    expect(g.map((x) => [x.side, x.items.map((i) => i.id)])).toEqual([["user", ["u1"]], ["agent", ["a1", "t1", "a2"]], ["user", ["u2"]]]);
  });
  it("starts a new group after five quiet minutes", () => {
    const g = groupMessages([m("a1", "agent", 0), m("a2", "agent", 6 * 60_000)]);
    expect(g).toHaveLength(2);
  });
  it("gives system notes their own group", () => {
    const g = groupMessages([m("a1", "agent", 0), m("s1", "system", 10), m("a2", "agent", 20)]);
    expect(g.map((x) => x.side)).toEqual(["agent", "system", "agent"]);
  });
});
