// GA-2: the Inbox is always the list, with no board/list toggle; the Tasks page keeps both.
// Rendered to HTML on the server, so no data loads: this checks the toolbar and which view the page picks.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { TasksPage } from "./TasksPage";

let store: Map<string, string>;
beforeEach(() => {
  store = new Map();
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  });
});
afterEach(() => vi.unstubAllGlobals());

const inbox = () => renderToStaticMarkup(<TasksPage inboxFor="you" onNewTask={() => {}} />);
const tasks = (initialView?: "list" | "board") => renderToStaticMarkup(<TasksPage initialView={initialView} onNewTask={() => {}} />);
// The Group button only shows in the list view, so it tells which view the page is in.
const hasGroup = (html: string) => html.includes("</svg>Group</button>");
const toggle = (html: string) => ({
  shown: html.includes('aria-label="View"'),
  board: /aria-pressed="true" aria-label="Board"/.test(html),
  list: /aria-pressed="true" aria-label="List"/.test(html),
});

describe("Inbox", () => {
  it("has no board/list toggle and shows the list", () => {
    const html = inbox();
    expect(html).toContain("<b>Inbox</b>");
    expect(toggle(html).shown).toBe(false);
    expect(html).not.toContain('aria-label="Board"');
    expect(html).not.toContain('aria-label="List"');
    expect(html).not.toContain("(Ctrl+B)");
    expect(hasGroup(html)).toBe(true);
  });

  it("keeps New task, search, Filters and Sort", () => {
    const html = inbox();
    expect(html).toContain("New task</button>");
    expect(html).toContain('aria-label="Search tasks"');
    expect(html).toContain("Filters</button>");
    expect(html).toContain("Sort</button>");
  });

  it("stays a list when a board was saved for the Inbox or the Tasks page", () => {
    store.set("gizai-inbox-view", JSON.stringify("board"));
    store.set("gizai-tasks-view", JSON.stringify("board"));
    const html = inbox();
    expect(toggle(html).shown).toBe(false);
    expect(hasGroup(html)).toBe(true);
  });

  it("doesn't save a view choice", () => {
    inbox();
    expect(store.has("gizai-inbox-view")).toBe(false);
    expect(store.has("gizai-tasks-view")).toBe(false);
  });
});

describe("Tasks page", () => {
  it("keeps the toggle, with the board as the default", () => {
    const html = tasks();
    expect(html).toContain("<b>Tasks</b>");
    expect(toggle(html)).toEqual({ shown: true, board: true, list: false });
    expect(html).toContain('title="Board (Ctrl+B)"');
    expect(html).toContain('title="List (Ctrl+B)"');
    expect(hasGroup(html)).toBe(false);
  });

  it("remembers a saved list choice", () => {
    store.set("gizai-tasks-view", JSON.stringify("list"));
    const html = tasks();
    expect(toggle(html)).toEqual({ shown: true, board: false, list: true });
    expect(hasGroup(html)).toBe(true);
  });

  it("ignores an old Inbox choice", () => {
    store.set("gizai-inbox-view", JSON.stringify("list"));
    expect(toggle(tasks())).toEqual({ shown: true, board: true, list: false });
  });

  it("opens #/board on the board", () => {
    expect(toggle(tasks("board"))).toEqual({ shown: true, board: true, list: false });
  });
});
