// GA-225 QA, the board one team at a time: with two teams a Team select in the toolbar; the board and the list show the
// picked team's columns and the cards of its projects (the first team's also those of a project without a team); the
// Inbox shows every team's cards that need you and has no select. With one team: no select, and every card as before.
// Rendered to HTML on the server: each api call names itself, so the replaced useData hands back its fixture, and the
// page's task list starts with the cards.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Project, Task, Team, TeamSummary } from "../types";

const page = vi.hoisted(() => ({ data: {} as Record<string, unknown>, tasks: null as unknown[] | null, asked: [] as (string | null)[] }));
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
vi.mock("../api", async (orig) => {
  const as = (key: string) => { const p = { key, catch: () => p }; return p; };
  return { ...(await orig<object>()), listProjects: () => as("projects"), listUsers: () => as("users"), listTasks: () => as("tasks"),
    listChatThreads: () => as("threads"), listTeams: () => as("teams"),
    // no team asked: the first team, as the backend answers
    getTeam: (id?: string | null) => { page.asked.push(id ?? null); return as(`team:${id ?? "my"}`); } };
});
// Every team's boards for the Inbox come as one Promise.all, which names nothing: "all"
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => { key?: string }) => ({ data: page.data[fetch()?.key ?? "all"] ?? null, error: null, reload: () => {}, setData: () => {} }),
}));

const { TasksPage } = await import("./TasksPage");

const member = (actorId: string, name: string, kind = "agent") => ({ actorId, name, kind, roleKey: "backend", handle: actorId, status: "active", isLead: false,
  allowedTools: [], chatEnabled: false });
const my: Team = { id: "my", name: "My team", labels: [], branches: [], members: [member("be", "Backend Agent"), member("you", "Jeffrey", "person")],
  states: [{ id: "my-todo", name: "To do", category: "ready", sortKey: "a1" }, { id: "my-ip", name: "In progress", category: "in_progress", sortKey: "a2" },
           { id: "my-rv", name: "Review", category: "review", sortKey: "a3" }] } as unknown as Team;
const rt: Team = { id: "rt", name: "Research team", labels: [], branches: [], members: [member("ra", "Research Agent")],
  states: [{ id: "rt-todo", name: "To do", category: "ready", sortKey: "a1" }, { id: "rt-design", name: "Design", category: "in_progress", sortKey: "a2" },
           { id: "rt-rv", name: "Review", category: "review", sortKey: "a3" }] } as unknown as Team;
const teams: TeamSummary[] = [{ id: "my", name: "My team" }, { id: "rt", name: "Research team" }];
const project = (id: string, name: string, teamId: string | null) => ({ id, name, key: id.toUpperCase(), teamId, teamName: null, number: "1", status: "active",
  defaultBranch: "main", openTasks: 0, doneTasks: 0, updatedAt: 0 }) as unknown as Project;
const projects = [project("ga", "Giz AI", "my"), project("rov", "Rover", "rt"), project("old", "Old", null)];
const card = (identifier: string, projectId: string, stateId: string, over: Partial<Task> = {}): Task => {
  const s = [...my.states, ...rt.states].find((x) => x.id === stateId)!;
  return { id: identifier.toLowerCase(), identifier, title: `Card ${identifier}`, projectId, descriptionMd: "", stateId, stateName: s.name, stateCategory: s.category,
    priority: 0, labels: [], hold: null, bounceCount: 0, failCount: 0, sortKey: "a0", testing: true, createdAt: 0, updatedAt: 0, byHand: false, screenshots: null,
    ...over } as Task;
};
const yours = { assigneeId: "you", assigneeName: "Jeffrey", assigneeKind: "person" };
const cards = [card("GA-1", "ga", "my-todo"), card("OLD-1", "old", "my-ip"), card("GA-2", "ga", "my-rv", yours),
               card("ROV-1", "rov", "rt-todo"), card("ROV-2", "rov", "rt-rv", yours), card("ROV-3", "rov", "rt-design")];

let store: Map<string, string>;
beforeEach(() => {
  store = new Map();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k) });
  page.asked = [];
});
afterEach(() => vi.unstubAllGlobals());

function render(o: { teams: TeamSummary[]; picked?: string; view?: "board" | "list"; inbox?: boolean }) {
  if (o.picked) store.set("gizai-team", o.picked);
  if (o.view) store.set("gizai-tasks-view", JSON.stringify(o.view));
  page.data = { "team:my": my, "team:rt": rt, teams: o.teams, projects, users: [{ id: "you", name: "Jeffrey" }], tasks: cards, threads: [], all: [my, rt] };
  page.tasks = cards;
  try {
    return renderToStaticMarkup(o.inbox ? <TasksPage inboxFor="you" onNewTask={() => {}} /> : <TasksPage onNewTask={() => {}} />);
  } finally { page.data = {}; page.tasks = null; }
}
/** The cards on the board, in the fixture's order. */
const onBoard = (html: string) => cards.map((c) => c.identifier).filter((id) => html.includes(`data-card="${id}"`));
const select = (html: string) => /<select[^>]*aria-label="Team"[^>]*>([\s\S]*?)<\/select>/.exec(html)?.[1] ?? null;

describe("the board with two teams", () => {
  it("has a Team select in the toolbar, on the first team when none is picked, with its columns and its projects' cards", () => {
    const html = render({ teams });
    expect(select(html)).toBe('<option value="my" selected="">My team</option><option value="rt">Research team</option>');
    expect(onBoard(html)).toEqual(["GA-1", "OLD-1", "GA-2"]);
    expect(html).toContain('data-col="In progress"');
    expect(html).not.toContain('data-col="Design"');
  });

  it("shows the picked team (the same pick as the Team page and the sidebar): its columns and only its projects' cards", () => {
    const html = render({ teams, picked: "rt" });
    expect(page.asked).toContain("rt");
    expect(select(html)).toBe('<option value="my">My team</option><option value="rt" selected="">Research team</option>');
    expect(onBoard(html)).toEqual(["ROV-1", "ROV-2", "ROV-3"]);
    expect(html).toContain('data-col="Design"');
    expect(html).not.toContain('data-col="In progress"');
  });

  it("the list shows the picked team's cards too", () => {
    const html = render({ teams, picked: "rt", view: "list" });
    expect(select(html)).not.toBeNull();
    for (const id of ["ROV-1", "ROV-2", "ROV-3"]) expect(html).toContain(id);
    for (const id of ["GA-1", "GA-2", "OLD-1"]) expect(html).not.toContain(id);
  });
});

describe("the board with one team", () => {
  it("has no Team select and shows every card, as before", () => {
    const html = render({ teams: [teams[0]] });
    expect(select(html)).toBeNull();
    expect(html).not.toContain('aria-label="Team"');
    expect(onBoard(html)).toEqual(["GA-1", "OLD-1", "GA-2"]);
  });
});

describe("the Inbox with two teams", () => {
  it("shows the cards that need you from both teams, and no Team select", () => {
    const html = render({ teams, picked: "my", inbox: true });
    expect(select(html)).toBeNull();
    expect(html).toContain("GA-2");
    expect(html).toContain("ROV-2");
    for (const id of ["GA-1", "OLD-1", "ROV-1", "ROV-3"]) expect(html).not.toContain(id);
  });
});
