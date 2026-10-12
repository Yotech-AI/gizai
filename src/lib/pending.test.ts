// GA-102: where the Team Lead's calls that wait for your approval show in the chat (under the answer that tried them), how a
// preview's diff folds long unchanged runs, and the outside tool in words.
import { describe, expect, it } from "vitest";
import { foldDiff, outsideLabel, placePending } from "./pending";
import { groupMessages } from "./chat";
import type { ChatMessage, DiffLine, PendingAction } from "../types";

const at = 1_000_000;
const msg = (id: string, role: string, runId: string | null, minute: number): ChatMessage => ({
  id, threadId: "c1", role, authorId: role === "user" ? "you" : "lead", authorName: role === "user" ? "You" : "Team Lead", bodyMd: id, runId,
  toolName: role === "tool" ? "mcp__otus__search" : null, tool: null, createdAt: at + minute * 60_000,
});
const action = (id: string, runId: string | null): PendingAction => ({
  id, threadId: "c1", runId, actorId: "lead", tool: "memory_write", args: { path: "Lessons/Otus" }, outsideTool: "mcp__otus__search",
  status: "pending", createdAt: at,
});

describe("placePending", () => {
  // you ask; the answer (run r1) uses Otus, writes, is refused; Gizai notes something; you ask again; the answer r2
  const messages = [msg("u1", "user", null, 0), msg("t1", "tool", "r1", 1), msg("a1", "agent", "r1", 1), msg("n1", "system", "r1", 2),
    msg("a1b", "agent", "r1", 2), msg("u2", "user", null, 3), msg("a2", "agent", "r2", 4)];
  const groups = groupMessages(messages);

  it("puts each call under the last group of messages of the answer that tried it, in the order tried", () => {
    const placed = placePending(groups, [action("p1", "r1"), action("p2", "r2"), action("p3", "r1")]);
    // r1's messages make two groups (Gizai's note splits them): the calls go under the last one
    expect([...placed.keys()]).toEqual(["a1b", "a2"]);
    expect(placed.get("a1b")!.map((a) => a.id)).toEqual(["p1", "p3"]);
    expect(placed.get("a2")!.map((a) => a.id)).toEqual(["p2"]);
  });

  it("puts a call whose answer has no message here (or no run) at the end", () => {
    const placed = placePending(groups, [action("p1", "gone"), action("p2", null), action("p3", "r2")]);
    expect(placed.get("")!.map((a) => a.id)).toEqual(["p1", "p2"]);
    expect(placed.get("a2")!.map((a) => a.id)).toEqual(["p3"]);
  });

  it("never puts a call under your messages or Gizai's notes, and nothing when nothing waits", () => {
    const own = groupMessages([msg("u1", "user", "r9", 0), msg("n1", "system", "r9", 1)]);
    expect([...placePending(own, [action("p1", "r9")]).keys()]).toEqual([""]);
    expect(placePending(groups, []).size).toBe(0);
  });
});

const lines = (spec: string): DiffLine[] => spec.split("").map((c, i) => ({ kind: c === "+" ? "add" : c === "-" ? "del" : "same", text: `${c}${i}` }));
const shape = (parts: ReturnType<typeof foldDiff>) => parts.map((p) => (p.kind === "fold" ? `[${p.count}]` : p.kind === "add" ? "+" : p.kind === "del" ? "-" : "=")).join("");

describe("foldDiff", () => {
  it("keeps two unchanged lines on each side of a change and folds the rest into a count", () => {
    expect(shape(foldDiff(lines("=======-+=======")))).toBe("[5]==-+==[5]");
    // between two changes: two after the first, two before the second
    expect(shape(foldDiff(lines("+==========-")))).toBe("+==[6]==-");
  });

  it("doesn't fold a run that a count wouldn't make shorter", () => {
    expect(shape(foldDiff(lines("==-==")))).toBe("==-==");
    expect(shape(foldDiff(lines("+=====-")))).toBe("+=====-");
    expect(shape(foldDiff(lines("+======-")))).toBe("+==[2]==-");
    expect(shape(foldDiff(lines("===+")))).toBe("===+");
  });

  it("keeps every changed line, in order, and counts every line once", () => {
    const d = lines("====+-===-=====++=");
    const parts = foldDiff(d);
    expect(parts.filter((p) => p.kind === "add" || p.kind === "del")).toEqual(d.filter((l) => l.kind !== "same"));
    const shown = parts.reduce((n, p) => n + (p.kind === "fold" ? p.count : 1), 0);
    expect(shown).toBe(d.length);
  });

  it("shows the start of a text without changes", () => {
    expect(shape(foldDiff(lines("======")))).toBe("==[4]");
    expect(foldDiff([])).toEqual([]);
  });
});

describe("outsideLabel", () => {
  it("says the outside tool in words", () => {
    expect(outsideLabel("WebSearch")).toBe("web search");
    expect(outsideLabel("WebFetch")).toBe("fetching a web page");
    expect(outsideLabel("mcp__otus__search_notes")).toBe("search notes on the otus server");
    expect(outsideLabel("mcp__chrome-devtools__navigate_page")).toBe("navigate page on the chrome-devtools server");
    expect(outsideLabel("SomethingElse")).toBe("SomethingElse");
  });
});
