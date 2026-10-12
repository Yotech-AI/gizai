// GA-102, the Inbox: a chat with calls of the Team Lead's that wait for your approval heads it under From the Team Lead,
// labelled Approval with how many wait, whoever started the chat, and has no ×: it stays until you approved or discarded
// each call in the chat. The Team Lead's own questions keep their ×. Rendered to HTML on the server: useData answers in
// TasksPage's order.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { ChatThread } from "../types";

const page = vi.hoisted(() => ({ data: [] as unknown[] }));
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));
// getTeam, listProjects, listUsers, listTasks, listChatThreads, in that order
vi.mock("../lib/useData", () => ({ useData: () => ({ data: page.data.shift() ?? null, error: null, reload: () => {}, setData: () => {} }) }));

const { TasksPage } = await import("./TasksPage");

const team = { id: "team", name: "Software", labels: [], members: [], states: [{ id: "ip", name: "In progress", category: "in_progress", sortKey: "b" }] };
const thread = (id: string, title: string, o: Partial<ChatThread>): ChatThread => ({ id, agentId: "lead", title, createdAt: 0, updatedAt: 0,
  costUsdMicros: 0, inputTokens: 0, outputTokens: 0, ...o });

let store: Map<string, string>;
beforeEach(() => {
  store = new Map();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k) });
});
afterEach(() => vi.unstubAllGlobals());

const inbox = (threads: ChatThread[]) => {
  page.data = [team, [{ id: "p1", name: "Gizai", key: "GA" }], [{ id: "you", name: "Jeffrey" }], [], threads];
  try { return renderToStaticMarkup(<TasksPage inboxFor="you" onNewTask={() => {}} />); } finally { page.data = []; }
};

describe("chats with calls that wait for your approval in the Inbox", () => {
  const html = inbox([
    thread("mine", "Remember the release notes", { updatedAt: 5, pendingActions: 2 }),
    thread("asks", "Which date format?", { updatedAt: 9, kind: "question", waiting: true }),
    thread("quiet", "Just chatting", { updatedAt: 7 }),
  ]);
  const section = html.slice(html.indexOf('aria-label="From the Team Lead"'));

  it("head the Inbox with the Team Lead's questions, newest first, and count in its total", () => {
    expect(section).toContain("<h3>From the Team Lead</h3>");
    expect(section.indexOf("Which date format?")).toBeLessThan(section.indexOf("Remember the release notes"));
    expect(section).not.toContain("Just chatting");
    expect(html).toMatch(/<b>Inbox<\/b><span class="faint">2<\/span>/);
  });

  it("are labelled Approval with how many calls wait, open the chat, and have no ×", () => {
    const row = section.slice(section.indexOf('href="#/chat/mine"') - 200, section.indexOf("Remember the release notes") + 400);
    expect(row).toContain('<span class="badge needs" title="2 calls of the Team Lead&#x27;s wait for your approval here">Approval</span> Remember the release notes');
    expect(row).toContain("2 to approve");
    expect(section).not.toContain('aria-label="Dismiss Remember the release notes"');
    // the Team Lead's question keeps its ×
    expect(section).toContain('aria-label="Dismiss Which date format?"');
  });

  it("says one call in the singular", () => {
    const h = inbox([thread("one", "One thing", { updatedAt: 1, pendingActions: 1 })]);
    expect(h).toContain("1 to approve");
    expect(h).toContain('title="A call of the Team Lead&#x27;s waits for your approval here"');
  });

  it("leave the Inbox once nothing waits", () => {
    const h = inbox([thread("done", "All decided", { updatedAt: 1, pendingActions: 0 })]);
    expect(h).not.toContain('aria-label="From the Team Lead"');
    expect(h).toContain("the calls it waits for your OK on show up here");
  });
});
