// GA-46: Chat → Recent asks for 30 chats and has Archive under them; #/chats shows the Archive in the conversation's place,
// with its empty states and its rows (label, when, the snippet with the match marked). Rendered to HTML on the server: the
// data hooks are replaced so the page gets its data at once, and the Archive's search results are handed in as its state.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { ChatHit, ChatMessage, ChatStatus, ChatThread, Member } from "../types";

// What the mocked api answers, by function name; anything else never answers (its useData stays empty).
const answers: Record<string, (...a: unknown[]) => unknown> = {};
const calls: [string, unknown[]][] = [];
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) {
    out[k] = typeof v !== "function" ? v : (...a: unknown[]) => { calls.push([k, a]); return answers[k] ? answers[k](...a) : new Promise(() => {}); };
  }
  return out;
});
// useData hands over what the api answered at once (a pending promise: nothing yet).
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => unknown) => {
    let data: unknown = null;
    try { const v = fetch(); if (!(v instanceof Promise)) data = v; } catch { /* no data */ }
    return { data, error: null, reload: () => {}, setData: () => {} };
  },
}));
let live: ChatStatus[] = [];
vi.mock("../components/chat/useChat", () => ({ useChatLive: () => live }));
// The conversation itself is GA-46's neighbour: here only which chat it was handed.
vi.mock("../components/chat/ChatThread", () => ({
  ChatThread: ({ threadId, thread }: { threadId: string | null; thread?: ChatThread }) =>
    <section data-conversation={threadId ?? "new"}>{thread ? `title=${thread.title}` : "no thread"}</section>,
}));
// The Archive's search results: its first null-initialised states (what was found, the error) take these values.
const states: unknown[] = [];
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => R.useState(init === null && states.length > 0 ? states.shift() : init)) as typeof R.useState;
  return { ...R, default: { ...R, useState }, useState };
});

const { ChatPage } = await import("./ChatPage");
const { ChatArchive } = await import("../components/chat/ChatArchive");

const HOUR = 3_600_000;
const now = Date.now();
const lead = { actorId: "lead", name: "Team Lead", kind: "agent", roleKey: "lead", handle: "lead", status: "active", isLead: false, allowedTools: [],
  chatEnabled: true } as Member;
const chat = (n: number, more: Partial<ChatThread> = {}): ChatThread => ({
  id: `c${n}`, agentId: "lead", title: `Chat ${n}`, createdAt: now - n * HOUR, updatedAt: now - n * HOUR, costUsdMicros: 0, inputTokens: 0, outputTokens: 0, ...more,
});
const msg = (thread: string, role: string, body: string, authorName: string | null): ChatMessage => ({
  id: `m-${thread}`, threadId: thread, role, authorId: role === "user" ? "you" : "lead", authorName, bodyMd: body, runId: null, toolName: null, tool: null,
  createdAt: now,
});
// A thenable that answers at once, so `chatAgent().then(…)` gives useData its data during the render.
const atOnce = <T,>(v: T) => ({ then: (f: (x: T) => unknown) => f(v) });

const recent30 = Array.from({ length: 30 }, (_, i) => chat(i));
const page = (id?: string, archive = false) => renderToStaticMarkup(<ChatPage id={id} archive={archive} />);
const between = (html: string, from: string, to: string) => html.slice(html.indexOf(from), html.indexOf(to, html.indexOf(from)));

beforeEach(() => {
  for (const k of Object.keys(answers)) delete answers[k];
  calls.length = 0;
  states.length = 0;
  live = [];
  answers.chatAgent = () => atOnce(lead);
  answers.listChatThreads = () => recent30;
});

describe("Chat → Recent", () => {
  it("asks the backend for 30 chats and lists them newest first", () => {
    const html = page();
    expect(calls.filter(([k]) => k === "listChatThreads")).toEqual([["listChatThreads", [30]]]);
    const list = between(html, 'class="chat-threads-list"', 'class="chat-threads-foot"');
    const ids = [...list.matchAll(/href="#\/chat\/(c\d+)"/g)].map((m) => m[1]);
    expect(ids).toEqual(recent30.map((t) => t.id));
    expect(list).toContain(">Recent<");
  });

  it("keeps the GA-35 labels on Recent's rows", () => {
    answers.listChatThreads = () => [chat(1, { kind: "question", waiting: true }), chat(2, { kind: "approval", waiting: true }), chat(3, { kind: "question" }), chat(4)];
    const list = between(page(), 'class="chat-threads-list"', 'class="chat-threads-foot"');
    expect(list).toContain('class="badge needs"');
    expect(list).toContain(">Question<");
    expect(list).toContain(">Approval<");
    expect(list).toContain('class="badge outline"');
    expect(list).toContain(">Team Lead<");
  });

  it("has Archive under the list, with the History icon, outside the part that scrolls", () => {
    const html = page();
    const foot = html.slice(html.indexOf('class="chat-threads-foot"'), html.indexOf("</aside>"));
    expect(foot).toMatch(/<a class="th" href="#\/chats"[^>]*>/);
    expect(foot).toContain("lucide-history");
    expect(foot).not.toContain("lucide-archive");
    expect(foot).toContain("Archive</span>");
    expect(foot).not.toContain("aria-current");
    // the foot follows the scrolling list in the column; it isn't inside it
    expect(html.indexOf('class="chat-threads-foot"')).toBeGreaterThan(html.lastIndexOf('href="#/chat/c29"'));
    expect(between(html, 'class="chat-threads-list"', 'class="chat-threads-foot"')).not.toContain("#/chats");
  });

  it("has no Archive and no Recent while there are no chats", () => {
    answers.listChatThreads = () => [];
    const html = page();
    expect(html).toContain("New chat");
    expect(html).not.toContain("chat-threads-foot");
    expect(html).not.toContain("#/chats");
    expect(html).not.toContain(">Recent<");
  });
});

describe("Chat → Archive (#/chats)", () => {
  it("highlights Archive, says Chat / Archive and shows the Archive in the conversation's place", () => {
    const html = page(undefined, true);
    expect(html).toMatch(/<a class="th on" href="#\/chats" aria-current="page"/);
    // New chat isn't highlighted there
    expect(html).toMatch(/<a class="th" href="#\/chat"><span class="t">/);
    const crumbs = between(html, 'class="crumbs"', 'class="actions"');
    expect(crumbs).toContain('<a href="#/chat">Chat</a><span class="sep">/</span><span>Archive</span>');
    expect(html).not.toContain("data-conversation");
    expect(html).toContain('class="chat-main chat-archive"');
    // Recent stays on the left
    expect(html).toContain('href="#/chat/c0"');
  });

  it("puts the search box at the top, before the list", () => {
    states.push({ query: "", hits: recent30.map((thread) => ({ thread })) });
    const html = page(undefined, true);
    const archive = html.slice(html.indexOf('class="chat-main chat-archive"'));
    expect(archive).toMatch(/<input class="input archive-search" type="search"[^>]*aria-label="Search all chats"/);
    expect(archive.indexOf("archive-search")).toBeLessThan(archive.indexOf('class="archive-list"'));
  });
});

describe("a chat older than the 30 in Recent", () => {
  it("is asked for on its own: its title in the crumbs, and handed to the conversation", () => {
    answers.getChatThread = (id) => chat(99, { id: String(id), title: "An old plan" });
    const html = page("c99");
    expect(calls.filter(([k]) => k === "getChatThread")).toEqual([["getChatThread", ["c99"]]]);
    expect(between(html, 'class="crumbs"', 'class="actions"')).toContain("An old plan");
    expect(html).toContain('data-conversation="c99"');
    expect(html).toContain("title=An old plan");
    // it isn't in Recent, and nothing in Recent is highlighted
    expect(html).not.toContain('href="#/chat/c99"');
    expect(html).not.toContain('class="th on"');
  });

  it("isn't asked for when Recent has it", () => {
    const html = page("c3");
    expect(calls.filter(([k]) => k === "getChatThread")).toEqual([]);
    expect(between(html, 'class="crumbs"', 'class="actions"')).toContain("Chat 3");
    expect(html).toMatch(/<a class="th on" href="#\/chat\/c3" aria-current="page"/);
  });
});

describe("the Archive's list", () => {
  const archive = (found: { query: string; hits: ChatHit[] } | null) => {
    states.push(found, null);
    return renderToStaticMarkup(<ChatArchive live={live} />);
  };

  it("says No chats yet when there are none", () => {
    const html = archive({ query: "", hits: [] });
    expect(html).toContain("No chats yet.");
    expect(html).not.toContain("No chats match");
  });

  it("says no chats match the search", () => {
    const html = archive({ query: "zebra", hits: [] });
    expect(html).toContain("No chats match “zebra”.");
    expect(html).not.toContain("No chats yet");
  });

  it("shows no empty state while the first answer is on its way", () => {
    const html = archive(null);
    expect(html).not.toContain("No chats");
    expect(html).toContain("archive-search");
  });

  it("lists every chat as found, newest first, each with its title, when and label, opening the chat", () => {
    const threads = [chat(0, { kind: "question", waiting: true }), chat(5, { kind: "approval", waiting: true }), chat(30), chat(31, { kind: "question" }),
      chat(48)];
    const html = archive({ query: "", hits: threads.map((thread) => ({ thread, message: null })) });
    const rows = html.split('class="archive-row"').slice(1);
    expect(rows).toHaveLength(5);
    expect(rows.map((r) => r.match(/^ href="#\/chat\/(c\d+)"/)?.[1])).toEqual(["c0", "c5", "c30", "c31", "c48"]);
    expect(rows[0]).toContain(">Question<");
    expect(rows[1]).toContain(">Approval<");
    expect(rows[2]).not.toContain("badge");
    expect(rows[3]).toContain(">Team Lead<");
    expect(rows[0]).toContain(">Chat 0<");
    expect(rows[0]).toContain(">just now<");
    expect(rows[1]).toContain(">5h ago<");
    expect(rows[4]).toContain(">2d ago<");
    expect(html).not.toContain("snippet");
    expect(html).not.toContain("<mark>");
  });

  it("shows a found message's snippet with the match marked and who wrote it; a title-only hit has none", () => {
    const html = archive({ query: "warehouse", hits: [
      { thread: chat(1, { title: "Dashboard" }), message: msg("c1", "agent", "The **Warehouse** dashboard is ready.", "Team Lead") },
      { thread: chat(2, { title: "Stock" }), message: msg("c2", "user", "Please check the warehouse\n\nstock", "Jeffrey") },
      { thread: chat(40, { title: "Warehouse invoices" }), message: null },
    ] });
    const rows = html.split('class="archive-row"').slice(1);
    expect(rows).toHaveLength(3);
    expect(rows[0]).toContain('<span class="snippet"><span class="who">Team Lead:</span> The <mark>Warehouse</mark> dashboard is ready.</span>');
    expect(rows[1]).toContain('<span class="who">You:</span> Please check the <mark>warehouse</mark> stock</span>');
    expect(rows[2]).not.toContain("snippet");
    expect(rows[2]).toContain("<mark>Warehouse</mark> invoices");
    expect(rows[2]).toContain('href="#/chat/c40"');
  });

  it("marks the search as typed, % and _ included", () => {
    const html = archive({ query: "50%_", hits: [{ thread: chat(1, { title: "Plan" }), message: msg("c1", "user", "We are 50%_ done, not 500 done", "Jeffrey") }] });
    expect(html).toContain("We are <mark>50%_</mark> done, not 500 done");
  });

  it("shows a chat the Team Lead is answering in now as working", () => {
    live = [{ threadId: "c1", runId: "r", draft: "", seq: 0 }];
    const html = archive({ query: "", hits: [{ thread: chat(1) }, { thread: chat(2) }] });
    const rows = html.split('class="archive-row"').slice(1);
    expect(rows[0]).toContain('class="pulse"');
    expect(rows[1]).not.toContain("pulse");
  });
});
