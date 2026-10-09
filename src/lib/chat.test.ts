import { describe, expect, it } from "vitest";
import { applyDraft, chatRunsOn, groupMessages, toolCard, toolName, withSnapshot, type DraftChange, type LiveDraft } from "./chat";
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

// GA-50 (was GA-23): the text being written must not drop words when a snapshot (chat_live) and live changes cross.
describe("the live text", () => {
  const delta = (text: string, seq: number): DraftChange => ({ kind: "delta", text, seq });
  const block = (seq: number): DraftChange => ({ kind: "block", seq });

  it("adds each change once, in order", () => {
    const d = [delta("Hello ", 1), delta("wor", 2), delta("ld", 3)].reduce(applyDraft, { text: "", seq: 0 } as LiveDraft);
    expect(d).toEqual({ text: "Hello world", seq: 3 });
    expect(applyDraft(d, delta("ld", 3))).toBe(d);
    expect(applyDraft(d, block(4))).toEqual({ text: "", seq: 4 });
  });

  it("keeps the words heard while a snapshot was asked for", () => {
    // The snapshot was taken after change 2; changes 3 and 4 arrived before it came back.
    const heard = [block(1), delta("Hello ", 2), delta("wor", 3), delta("ld", 4)];
    expect(withSnapshot({ text: "Hello ", seq: 2 }, heard)).toEqual({ text: "Hello world", seq: 4 });
  });

  it("doesn't add words twice when the snapshot holds them already", () => {
    const heard = [delta("Hello ", 2), delta("wor", 3)];
    expect(withSnapshot({ text: "Hello wor", seq: 3 }, heard)).toEqual({ text: "Hello wor", seq: 3 });
    // out of order events are put in order first
    expect(withSnapshot({ text: "", seq: 0 }, [delta("b", 2), delta("a", 1)]).text).toBe("ab");
  });

  it("starts again at a new block that came after the snapshot", () => {
    expect(withSnapshot({ text: "Old block", seq: 5 }, [block(6), delta("New", 7)])).toEqual({ text: "New", seq: 7 });
  });
});

describe("chatRunsOn", () => {
  const clis = [{ id: "claude_code" }, { id: "cc2" }];
  it("is the chat's own pick while it is still in Settings, else the Team Lead's", () => {
    expect(chatRunsOn("cc2", "claude_code", clis)).toBe("cc2");
    expect(chatRunsOn(null, "cc2", clis)).toBe("cc2");
    expect(chatRunsOn("gone", "claude_code", clis)).toBe("claude_code");
    expect(chatRunsOn(undefined, null, clis)).toBe("claude_code");
    expect(chatRunsOn("cc2", "claude_code", null)).toBe("cc2");
  });
});
