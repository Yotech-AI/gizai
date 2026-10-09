import { describe, expect, it } from "vitest";
import { applyDraft, chatRunsOn, groupMessages, highlight, hitAuthor, RECENT_MAX, snippet, toolCard, toolName, withSnapshot, type DraftChange,
  type LiveDraft } from "./chat";
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

// GA-46: Chat → Recent shows 30 chats; the Archive's rows show a piece of the matching message with the match marked.
describe("Recent", () => {
  it("shows 30 chats", () => {
    expect(RECENT_MAX).toBe(30);
  });
});

describe("the Archive's snippet", () => {
  // 80 short words, ~520 characters: "w0 w1 w2 …"; `at` puts the match in place of word `at`.
  const words = (at: number, match = "needle") => Array.from({ length: 80 }, (_, i) => (i === at ? match : `word${i}`)).join(" ");
  const isWordAt = (text: string, piece: string) => {
    const i = text.indexOf(piece);
    return i >= 0 && (i === 0 || text[i - 1] === " ") && (i + piece.length === text.length || text[i + piece.length] === " ");
  };

  it("is a short message whole, on one line", () => {
    expect(snippet("Hello\n\n  there ", "there")).toBe("Hello there");
    expect(snippet("Hello there", "")).toBe("Hello there");
  });

  it("drops Markdown's marks: links and images keep their text", () => {
    expect(snippet("**Bold** and `code` and [the docs](https://x.y/z) ![pic](a.png)", "code")).toBe("Bold and code and the docs pic");
    expect(snippet("## Plan\n- one\n* two\n> quoted\n1. first\n2) second", "one")).toBe("Plan one two quoted first second");
  });

  it("keeps the marks when the match is in them, like a link's address", () => {
    expect(snippet("See [the docs](https://example.com/guide)", "EXAMPLE.com")).toBe("See [the docs](https://example.com/guide)");
  });

  it("cuts a long message around the match, on words, with … where text was left out", () => {
    for (const at of [0, 1, 5, 20, 40, 60, 75, 79]) {
      const text = words(at);
      const s = snippet(text, "NEEDLE");
      expect(s, `at ${at}`).toContain("needle");
      expect(s.length, `at ${at}`).toBeLessThanOrEqual(142);
      expect(s.length, `at ${at}`).toBeGreaterThan(100);
      expect(s.startsWith("…"), `at ${at}: ${s}`).toBe(!text.startsWith(s.replace(/…$/, "")));
      expect(s.endsWith("…"), `at ${at}: ${s}`).toBe(!text.endsWith(s.replace(/^…/, "")));
      expect(isWordAt(text, s.replace(/^…|…$/g, "")), `at ${at}: ${s}`).toBe(true);
      expect(highlight(s, "needle").filter((p) => p.hit).map((p) => p.text), `at ${at}`).toEqual(["needle"]);
    }
  });

  it("leaves some text before a match in the middle, to read it in context", () => {
    const s = snippet(words(40), "needle");
    expect(s.indexOf("needle")).toBeGreaterThan(20);
  });

  it("cuts inside a very long word (a link) instead of dropping the text around the match", () => {
    const link = `https://example.com/${"a".repeat(200)}/needle/${"b".repeat(200)}`;
    const s = snippet(`Read ${link} please`, "needle");
    expect(s).toContain("/needle/");
    expect(s.length).toBeLessThanOrEqual(142);
    expect(s.startsWith("…") && s.endsWith("…")).toBe(true);
    expect(s).toContain("aaaa");
    expect(s).toContain("bbbb");
  });

  it("is the message's start when there is nothing to search for", () => {
    const s = snippet(words(-1), "");
    expect(s.startsWith("word0 word1")).toBe(true);
    expect(s.endsWith("…")).toBe(true);
    expect(s.length).toBeLessThanOrEqual(141);
  });

  it("reads the search as typed: spaces as one, regex characters literal", () => {
    expect(snippet(words(70, "a.b*(c)"), "a.b*(c)")).toContain("a.b*(c)");
    expect(snippet(`${words(-1)} the  warehouse`, "the   warehouse")).toContain("the warehouse");
  });
});

describe("the Archive's highlight", () => {
  const marked = (text: string, q: string) => highlight(text, q).filter((p) => p.hit).map((p) => p.text);

  it("marks every match, case ignored, and keeps the text as it was", () => {
    const parts = highlight("Warehouse and the WAREHOUSE.", "warehouse");
    expect(parts).toEqual([
      { text: "Warehouse", hit: true }, { text: " and the ", hit: false }, { text: "WAREHOUSE", hit: true }, { text: ".", hit: false },
    ]);
    expect(parts.map((p) => p.text).join("")).toBe("Warehouse and the WAREHOUSE.");
  });

  it("marks nothing for an empty search or no match", () => {
    expect(highlight("Plain title", "")).toEqual([{ text: "Plain title", hit: false }]);
    expect(highlight("Plain title", "   ")).toEqual([{ text: "Plain title", hit: false }]);
    expect(highlight("Plain title", "zebra")).toEqual([{ text: "Plain title", hit: false }]);
    expect(highlight("", "zebra")).toEqual([{ text: "", hit: false }]);
  });

  it("takes %, _ and regex characters literally", () => {
    expect(marked("50% done, 500 done", "50%")).toEqual(["50%"]);
    expect(marked("user_id and userXid", "user_id")).toEqual(["user_id"]);
    expect(marked("a.b and axb", "a.b")).toEqual(["a.b"]);
    expect(marked("(see) [x] C:\\temp $1 ^a", "C:\\t")).toEqual(["C:\\t"]);
    expect(marked("(see) [x] $1 ^a", "(see) [x] $1 ^a")).toEqual(["(see) [x] $1 ^a"]);
  });

  it("marks a match at the start and at the end", () => {
    expect(highlight("kade portal kade", "KADE")).toEqual([{ text: "kade", hit: true }, { text: " portal ", hit: false }, { text: "kade", hit: true }]);
  });
});

describe("who wrote an Archive hit", () => {
  it("is You for your message, the Team Lead by its name otherwise", () => {
    expect(hitAuthor({ role: "user", authorName: "Jeffrey" })).toBe("You");
    expect(hitAuthor({ role: "agent", authorName: "Kees" })).toBe("Kees");
    expect(hitAuthor({ role: "agent", authorName: null })).toBe("Team Lead");
  });
});
