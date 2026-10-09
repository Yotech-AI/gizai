// GA-43: Archive on a Done card's page, the read-only archived card with its badge and Restore, the bin icon on the Tasks
// page (not the Inbox), the bin's list and search, and the activity lines. Rendered to HTML on the server: the data
// hooks are replaced so each page gets its data at once.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Task, Team } from "../types";

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
let live: { runId: string; taskId: string; agentId: string }[] = [];
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => live }));

const { TaskPage } = await import("./TaskPage");
const { TasksPage } = await import("./TasksPage");
const { ArchivedCards } = await import("../components/ArchivedCards");
const { filterTasks } = await import("../lib/taskView");
const { describeChange } = await import("../lib/activity");

const team: Team = {
  id: "team", name: "Software", members: [], labels: [],
  states: [
    { id: "s-todo", name: "To do", category: "ready", sortKey: "a1" },
    { id: "s-review", name: "Review", category: "review", sortKey: "a4" },
    { id: "s-deploy", name: "Deploy", category: "deploy", sortKey: "a4V" },
    { id: "s-done", name: "Done", category: "done", sortKey: "a5" },
  ],
};
const task = (o: Partial<Task>): Task => ({
  id: "t1", identifier: "GA-40", projectId: "p1", projectName: "Gizai", projectColor: "#36f", title: "Ship the release", descriptionMd: "Notes",
  acceptanceMd: "- [ ] it works", stateId: "s-done", stateName: "Done", stateCategory: "done", priority: 0, labels: [], bounceCount: 0,
  failCount: 0, sortKey: "a0", testing: true, createdAt: 0, updatedAt: 0, ...o,
} as Task);

let store: Map<string, string>;
beforeEach(() => {
  store = new Map();
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  });
  for (const k of Object.keys(answers)) delete answers[k];
  calls.length = 0;
  live = [];
  answers.getTeam = () => team;
  answers.listUsers = () => [];
  answers.listComments = () => [];
  answers.taskActivity = () => [];
  answers.listRuns = () => [];
});
afterEach(() => vi.unstubAllGlobals());

const page = (t: Task) => { answers.getTask = () => t; return renderToStaticMarkup(<TaskPage id={t.id} />); };
const archiveButton = (html: string) => html.match(/<button[^>]*>(?:(?!<\/button>).)*Archive<\/button>/)?.[0];
const restoreButton = (html: string) => html.match(/<button[^>]*>(?:(?!<\/button>).)*Restore<\/button>/)?.[0];

describe("a card's page", () => {
  it("has Archive in its actions for a card in Done", () => {
    const html = page(task({}));
    const b = archiveButton(html);
    expect(b).toBeDefined();
    expect(b).not.toContain("disabled");
    expect(b).toContain('title="Take it off the board; the bin on the Tasks page lists it"');
    expect(restoreButton(html)).toBeUndefined();
    expect(html).not.toContain("Archived</span>");
  });

  it("has no Archive for a card in another column", () => {
    for (const [stateId, stateName, stateCategory] of [["s-todo", "To do", "ready"], ["s-review", "Review", "review"], ["s-deploy", "Deploy", "deploy"]]) {
      const html = page(task({ stateId, stateName, stateCategory }));
      expect(archiveButton(html), stateName).toBeUndefined();
      expect(restoreButton(html), stateName).toBeUndefined();
    }
  });

  it("can't be archived while an agent works on it", () => {
    live = [{ runId: "r1", taskId: "t1", agentId: "a1" }];
    const b = archiveButton(page(task({})));
    expect(b).toContain('disabled=""');
    expect(b).toContain('title="An agent is working on this card"');
  });

  it("shows an archived card read-only, with the Archived badge, who archived it and Restore", () => {
    const html = page(task({ archivedAt: Date.now() - 60_000, archivedBy: "Jeffrey" }));
    expect(html).toMatch(/<span class="badge"><svg[^>]*>.*?<\/svg>Archived<\/span>/);
    expect(restoreButton(html)).toContain('title="Put it back at the bottom of Done"');
    expect(archiveButton(html)).toBeUndefined();
    expect(html).toContain("by Jeffrey. It is read-only: Restore puts it back at the bottom of Done.");
    expect(html).toMatch(/<input class="title-input" aria-label="Title" readOnly=""/);
    expect(html).not.toContain('class="composer"');
    expect(html).not.toContain(">Comment</button>");
    expect(html).not.toContain('aria-label="Agent run"'); // no Run, Continue or pull request
    expect(html).toContain('<div class="filedrop read-only">');
    expect(html).not.toContain("md-click"); // the description and criteria only show
    expect(html).not.toContain("Add files</button>");
    // its properties show, but nothing opens a picker
    expect(html).toContain('aria-label="Properties"');
    expect(html).not.toContain('aria-haspopup="menu"');
    expect(html).not.toContain('class="v editable" role="button"');
    expect(html).toMatch(/<input type="checkbox" disabled="" checked=""\/>Test before Review/);
    expect(html).toContain("Archived</span><span class=\"v\"");
  });

  it("is editable as usual when it isn't archived", () => {
    const html = page(task({}));
    expect(html).toContain('class="composer"');
    expect(html).toContain("md-click");
    expect(html).toContain('aria-haspopup="menu"');
    expect(html).toContain('aria-label="Agent run"');
    expect(html).toContain("Add files</button>");
    expect(html).not.toMatch(/<input class="title-input"[^>]*readOnly/);
  });
});

describe("the bin icon", () => {
  const tasksPage = (inboxFor?: string) => renderToStaticMarkup(<TasksPage inboxFor={inboxFor} onNewTask={() => {}} />);
  const bin = '<button class="btn ghost icon-only" aria-label="Archived cards" title="Archived cards">';

  it("is on the Tasks page at the top right, after Filters and Sort, in both views", () => {
    const html = tasksPage();
    expect(html).toContain(bin);
    expect(html.indexOf(bin)).toBeGreaterThan(html.indexOf("Sort</button>"));
    expect(html.indexOf(bin)).toBeGreaterThan(html.indexOf("Filters</button>"));
    expect(html.indexOf(bin)).toBeGreaterThan(html.indexOf('aria-label="View"'));
    store.set("gizai-tasks-view", JSON.stringify("list"));
    expect(tasksPage()).toContain(bin);
  });

  it("is not on the Inbox", () => {
    expect(tasksPage("you")).not.toContain('aria-label="Archived cards"');
  });
});

describe("the bin", () => {
  const archived = [
    task({ id: "t2", identifier: "GA-41", title: "Newest archived", archivedAt: Date.now() - 2 * 3600_000, archivedBy: "Jeffrey" }),
    task({ id: "t1", identifier: "GA-12", title: "Export invoices", projectName: "Kade portal", archivedAt: Date.now() - 3 * 86400_000, archivedBy: "Team Lead" }),
  ];

  it("lists the archived cards in the order the backend gives (newest first), with ID, title, project, when and by whom", () => {
    answers.listArchivedTasks = () => archived;
    const html = renderToStaticMarkup(<ArchivedCards onClose={() => {}} />);
    expect(html).toContain("Archived cards");
    expect(html).toContain("All projects, the most recently archived first");
    expect(html).toContain('aria-label="Search archived cards"');
    expect(html).toContain('placeholder="Search ID or title"');
    const rows = [...html.matchAll(/<a role="listitem" class="bin-row" href="([^"]+)"[^>]*>(.*?)<\/a>/g)];
    expect(rows.map((r) => r[1])).toEqual(["#/task/t2", "#/task/t1"]);
    expect(rows[0][2]).toContain('<span class="id">GA-41</span>');
    expect(rows[0][2]).toContain("Newest archived");
    expect(rows[0][2]).toContain("Gizai");
    expect(rows[0][2]).toMatch(/Archived 2 ?h(ours?)? ago by Jeffrey|Archived .* by Jeffrey/);
    expect(rows[1][2]).toContain("Kade portal");
    expect(rows[1][2]).toContain("by Team Lead");
  });

  it("asks for the Tasks page's project, or all projects when none is set", () => {
    answers.listArchivedTasks = () => [];
    renderToStaticMarkup(<ArchivedCards onClose={() => {}} />);
    renderToStaticMarkup(<ArchivedCards projectId="p9" projectName="Kade portal" onClose={() => {}} />);
    expect(calls.filter(([k]) => k === "listArchivedTasks").map(([, a]) => a)).toEqual([[null], ["p9"]]);
  });

  it("says when there are no archived cards", () => {
    answers.listArchivedTasks = () => [];
    expect(renderToStaticMarkup(<ArchivedCards onClose={() => {}} />)).toContain("No archived cards.");
    const html = renderToStaticMarkup(<ArchivedCards projectId="p9" projectName="Kade portal" onClose={() => {}} />);
    expect(html).toContain("No archived cards in Kade portal.");
    expect(html).toContain("Kade portal, the most recently archived first");
  });

  it("searches by ID or by words in the title (the bin's filter)", () => {
    expect(filterTasks(archived, { text: "ga-12" }).map((t) => t.identifier)).toEqual(["GA-12"]);
    expect(filterTasks(archived, { text: "invoices" }).map((t) => t.identifier)).toEqual(["GA-12"]);
    expect(filterTasks(archived, { text: "archived" }).map((t) => t.identifier)).toEqual(["GA-41"]);
    expect(filterTasks(archived, { text: "nothing like it" })).toEqual([]);
  });
});

describe("the activity", () => {
  const e = (op: string, diff: unknown) => ({ at: 0, actorName: "Jeffrey", table: "tasks", op, diff });
  it("says who archived and who restored the card", () => {
    expect(describeChange(e("delete", null))).toBe("archived the card");
    expect(describeChange(e("update", { archived: false }))).toBe("restored the card");
  });
});
