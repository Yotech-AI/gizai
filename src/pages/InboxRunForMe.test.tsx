// GA-31, Run this for me in the Inbox: held cards whose agent asks you to run commands show on top, each with its
// commands exactly (a Copy button each) and Done, continue, and not again in the list below; the count includes them.
// Rendered to HTML on the server: useData answers in TasksPage's order, and the page's task list starts with the cards.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Task } from "../types";

const page = vi.hoisted(() => ({ data: [] as unknown[], tasks: null as unknown[] | null }));
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  // TasksPage keeps the loaded cards in useState<Task[]>([]) and fills it in an effect, which a server render doesn't run
  const useState = ((init: unknown) => {
    if (Array.isArray(init) && init.length === 0 && page.tasks) {
      const tasks = page.tasks;
      page.tasks = null;
      return R.useState(tasks);
    }
    return R.useState(init);
  }) as typeof R.useState;
  return { ...R, useState, default: { ...R, useState } };
});
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));
// getTeam, listProjects, listUsers, listTasks, listChatThreads, in that order
vi.mock("../lib/useData", () => ({ useData: () => ({ data: page.data.shift() ?? null, error: null, reload: () => {}, setData: () => {} }) }));

const { TasksPage } = await import("./TasksPage");

const CMDS = ["sudo pacman -S libayatana-appindicator",
  'echo "fs.inotify.max_user_watches=524288" | sudo tee /etc/sysctl.d/40-watches.conf && sudo sysctl --system'];
const team = { id: "team", name: "Software", labels: [],
  members: [{ actorId: "be2", name: "Backend Agent 2", kind: "agent", roleKey: "backend", status: "active", handle: "be2", isLead: false, allowedTools: [], chatEnabled: false }],
  states: [{ id: "ip", name: "In progress", category: "in_progress", sortKey: "b" }, { id: "rv", name: "Review", category: "review", sortKey: "c" },
           { id: "dn", name: "Done", category: "done", sortKey: "d" }] };
const card = (id: string, over: Partial<Task>): Task => ({ id, identifier: id.toUpperCase(), title: `Card ${id}`, descriptionMd: "", stateId: "ip",
  stateName: "In progress", stateCategory: "in_progress", priority: 0, assigneeId: "be2", assigneeName: "Backend Agent 2", assigneeKind: "agent",
  labels: [], hold: null, bounceCount: 0, failCount: 0, sortKey: "a", testing: true, createdAt: 0, updatedAt: 0, projectId: "p1", ...over });
const cards = [
  card("ga-31", { title: "Tray icon", hold: "needs_decision", holdReason: "The tray needs a system library.", runForMe: CMDS, updatedAt: 5 }),
  // the push after the run failed: held blocked, the commands still asked
  card("ga-50", { title: "Push failed", hold: "blocked", holdReason: "Couldn't push", runForMe: ["sudo make install"], updatedAt: 9 }),
  card("ga-12", { title: "Pick a format", hold: "needs_decision", holdReason: "CSV or JSON?", updatedAt: 7 }),
  card("ga-40", { title: "Review me", stateId: "rv", stateName: "Review", stateCategory: "review", assigneeId: "you", assigneeKind: "person", updatedAt: 3 }),
  // not in the Inbox: not held (an older list), or done
  card("ga-60", { title: "Working", runForMe: ["sudo stale"], updatedAt: 2 }),
  card("ga-70", { title: "Finished", stateId: "dn", stateName: "Done", stateCategory: "done", hold: "blocked", runForMe: ["sudo done"], updatedAt: 1 }),
];

let store: Map<string, string>;
beforeEach(() => {
  store = new Map();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k) });
});
afterEach(() => vi.unstubAllGlobals());

const inbox = (tasks: Task[]) => {
  page.data = [team, [{ id: "p1", name: "Gizai", key: "GA" }], [{ id: "you", name: "Jeffrey" }], tasks, []];
  page.tasks = tasks;
  try { return renderToStaticMarkup(<TasksPage inboxFor="you" onNewTask={() => {}} />); } finally { page.data = []; page.tasks = null; }
};
const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
const count = (html: string, s: string) => html.split(s).length - 1;

describe("Run this for me in the Inbox", () => {
  it("shows the held cards whose agent asks for commands on top, newest first, with each command, Copy and Done, continue", () => {
    const html = inbox(cards);
    const section = html.slice(html.indexOf('<section class="lead-chats" aria-label="Run this for me">'));
    expect(section.length).toBeLessThan(html.length);
    expect(section).toContain('<h3>Run this for me</h3><span class="faint">2</span>');
    expect(section.indexOf(">GA-50<")).toBeLessThan(section.indexOf(">GA-31<"));
    expect(section).toContain('href="#/task/ga-31"');
    const codes = [...section.matchAll(/<code class="mono">([^<]*)<\/code>/g)].map((m) => text(m[1] ?? ""));
    expect(codes).toEqual(["sudo make install", ...CMDS]);
    expect(count(section, 'aria-label="Copy ')).toBe(3);
    expect(count(section, "</svg>Done, continue</button>")).toBe(2);
    expect(text(section)).toContain("Backend Agent 2 asks you to run these commands:");
  });

  it("lists those cards only there: the others that need you are in the list below, and the count has them all", () => {
    const html = inbox(cards);
    expect(count(html, ">GA-31<")).toBe(1);
    expect(count(html, ">GA-50<")).toBe(1);
    expect(count(html, ">GA-12<")).toBe(1);
    expect(count(html, ">GA-40<")).toBe(1);
    expect(html).not.toContain(">GA-60<");
    expect(html).not.toContain(">GA-70<");
    expect(html).not.toContain("sudo stale");
    expect(html.indexOf('aria-label="Run this for me"')).toBeLessThan(html.indexOf(">GA-12<"));
    // two in the list, two on top
    expect(html).toContain('<b>Inbox</b><span class="faint">4</span>');
  });

  it("with only such cards, it doesn't say nothing needs you", () => {
    const html = inbox([cards[0]!]);
    expect(html).toContain('aria-label="Run this for me"');
    expect(html).not.toContain("Nothing needs you.");
    expect(html).toContain('<b>Inbox</b><span class="faint">1</span>');
    expect(inbox([cards[4]!])).toContain("Nothing needs you.");
  });
});
