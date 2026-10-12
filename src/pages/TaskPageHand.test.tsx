// GA-115 QA: Open in editor in the top bar of a card assigned to you, and "By hand" in the card's head. Off, with who is
// at work as its tooltip, while an agent works on the card; not there on a card assigned to an agent, another person or
// nobody, nor on an archived or finished card. Rendered to HTML on the server with the data hooks answering at once (as
// in TaskPageChanges.test); the click itself is in HandFlow.test.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Task, Team } from "../types";

const answers: Record<string, (...a: unknown[]) => unknown> = {};
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) out[k] = typeof v !== "function" ? v : (...a: unknown[]) => (answers[k] ? answers[k](...a) : new Promise(() => {}));
  return out;
});
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => unknown) => {
    let data: unknown = null;
    try { const v = fetch(); if (!(v instanceof Promise)) data = v; } catch { /* no data */ }
    return { data, error: null, reload: () => {}, setData: () => {} };
  },
}));
let live: { taskId: string; agentId: string }[] = [];
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => live }));

const { TaskPage } = await import("./TaskPage");

const YOU = "u-you";
const team: Team = {
  id: "team", name: "Software", labels: [],
  members: [{ actorId: "a-backend", name: "Backend Agent", kind: "agent", roleKey: "backend", handle: "backend", status: "running", isLead: false, allowedTools: [], chatEnabled: false }],
  states: [{ id: "s-todo", name: "To do", category: "ready", sortKey: "a1" }, { id: "s-prog", name: "In progress", category: "started", sortKey: "a2" },
    { id: "s-done", name: "Done", category: "done", sortKey: "a5" }],
} as unknown as Team;
const base = {
  id: "t1", identifier: "KADE-41", projectId: "p1", projectName: "Kade portal", projectColor: "#36f", title: "Fix the invoice total", descriptionMd: "",
  acceptanceMd: "", stateId: "s-todo", stateName: "To do", stateCategory: "ready", priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: "a0",
  testing: true, createdAt: 0, updatedAt: 0, byHand: false, assigneeId: YOU, assigneeName: "Jeffrey", assigneeKind: "user",
} as unknown as Task;
let task: Task;

beforeEach(() => {
  task = { ...base };
  live = [];
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v), removeItem: (k: string) => void store.delete(k) });
  Object.assign(answers, { getTask: () => task, getTeam: () => team, listUsers: () => [], listComments: () => [], taskActivity: () => [], listRuns: () => [] });
});
afterEach(() => vi.unstubAllGlobals());

const page = (youId: string | undefined = YOU) => renderToStaticMarkup(<TaskPage id="t1" youId={youId} />);
const actions = (html: string) => html.slice(html.indexOf('<div class="actions">'), html.indexOf('<div class="tabs task-tabs"'));
const openButton = (html: string) => actions(html).match(/<button[^>]*>(?:(?!<\/button>).)*Open in editor<\/button>/)?.[0] ?? "";
const head = (html: string) => html.slice(html.indexOf('<div class="task-head">'), html.indexOf('class="title-input"'));

describe("Open in editor on the card's top bar (GA-115)", () => {
  it("on a card assigned to you in To do: on, with the square-code icon and what it does as its tooltip", () => {
    const b = openButton(page());
    expect(b).toContain('class="btn sm"');
    expect(b).not.toContain("disabled");
    expect(b).toContain("lucide-square-code");
    expect(b).toContain('title="Work on KADE-41 by hand: Gizai makes its worktree, pins the card to you so agents leave it alone, and moves it to In progress."');
  });

  it("not on a card assigned to an agent, another person or nobody, nor without knowing who you are", () => {
    task = { ...base, assigneeId: "a-backend", assigneeName: "Backend Agent", assigneeKind: "agent" };
    expect(actions(page())).not.toContain("Open in editor");
    task = { ...base, assigneeId: "u-other", assigneeName: "Sam" };
    expect(actions(page())).not.toContain("Open in editor");
    task = { ...base, assigneeId: null, assigneeName: null };
    expect(actions(page())).not.toContain("Open in editor");
    task = { ...base };
    expect(actions(page(""))).not.toContain("Open in editor");
    expect(actions(renderToStaticMarkup(<TaskPage id="t1" />))).not.toContain("Open in editor");
  });

  it("not on a card in Done, nor on an archived one", () => {
    task = { ...base, stateId: "s-done", stateName: "Done", stateCategory: "done" };
    expect(actions(page())).not.toContain("Open in editor");
    task = { ...base, stateId: "s-done", stateName: "Done", stateCategory: "done", archivedAt: 5 } as Task;
    expect(actions(page())).not.toContain("Open in editor");
  });

  it("off while an agent works on the card, with who and what to do as its tooltip", () => {
    task = { ...base, stateId: "s-prog", stateName: "In progress", stateCategory: "started" };
    live = [{ taskId: "t1", agentId: "a-backend" }];
    const b = openButton(page());
    expect(b).toContain('disabled=""');
    expect(b).toContain('title="Backend Agent is working on KADE-41: wait for its run to end, or stop it."');
    live = [{ taskId: "t1", agentId: "a-gone" }];
    expect(openButton(page())).toContain('title="An agent is working on KADE-41: wait for its run to end, or stop it."');
    live = [{ taskId: "t-other", agentId: "a-backend" }];
    expect(openButton(page())).not.toContain("disabled");
  });

  it("a card started by hand: By hand in its head, and Open in editor just opens its worktree", () => {
    task = { ...base, stateId: "s-prog", stateName: "In progress", stateCategory: "started", byHand: true };
    const html = page();
    expect(head(html)).toMatch(/<span class="badge" title="Pinned to Jeffrey: agents leave it alone until it&#x27;s handed over"><svg[^>]*lucide-keyboard[^>]*>.*?<\/svg>By hand<\/span>/);
    expect(openButton(html)).toContain('title="Open KADE-41&#x27;s worktree in the Editor"');
    expect(head(html)).not.toContain("Live");
  });

  it("someone else's card by hand: By hand in its head, but no Open in editor", () => {
    task = { ...base, stateId: "s-prog", stateName: "In progress", stateCategory: "started", byHand: true, assigneeId: "u-other", assigneeName: "Sam" };
    const html = page();
    expect(head(html)).toContain("Pinned to Sam");
    expect(actions(html)).not.toContain("Open in editor");
  });

  it("no By hand on a card that isn't pinned to a person", () => {
    expect(head(page())).not.toContain("By hand");
  });
});
