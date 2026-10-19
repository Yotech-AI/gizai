// GA-191 QA: list_tasks answers every card with acceptanceMd null and get_team every member with instructionsMd null, so
// the screens that show them read the full row: the card page from get_task, the agent page from get_agent, and the
// agent form loads the agent with get_agent. Rendered to HTML on the server with the data hooks answering at once (as in
// TaskPageHand.test), the lists answering lean the way the backend does now.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Member, Task, Team } from "../types";
import { draftFrom } from "../lib/agents";
import agentForm from "../components/AgentForm.tsx?raw";

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
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));

const { TaskPage } = await import("./TaskPage");
const { AgentPage } = await import("./AgentPage");

const ACCEPTANCE = "The invoice total includes VAT on every line";
const INSTRUCTIONS = "Run the PHP tests before every hand-over";

const task = {
  id: "t1", identifier: "KADE-41", projectId: "p1", projectName: "Kade portal", projectColor: "#36f", title: "Fix the invoice total",
  descriptionMd: "The total is off by the VAT.", acceptanceMd: `- [ ] ${ACCEPTANCE}`, stateId: "s-todo", stateName: "To do",
  stateCategory: "ready", priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: "a0", testing: true, createdAt: 0, updatedAt: 0,
  byHand: false,
} as unknown as Task;
const agent = {
  actorId: "a-backend", name: "Backend Agent", kind: "agent", roleKey: "backend", handle: "backend", status: "idle", isLead: false,
  allowedTools: [], chatEnabled: false, model: null, adapter: "claude_code", instructionsMd: INSTRUCTIONS,
} as unknown as Member;
// What list_tasks and get_team answer now (tasks::list_lean, team::get_lean): the same rows without the two texts.
const leanTask = { ...task, acceptanceMd: null } as Task;
const leanAgent = { ...agent, instructionsMd: null } as Member;
const team = {
  id: "team", name: "Software", labels: [], members: [leanAgent],
  states: [{ id: "s-todo", name: "To do", category: "ready", sortKey: "a1" }],
} as unknown as Team;

beforeEach(() => {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v), removeItem: (k: string) => void store.delete(k) });
  Object.assign(answers, {
    getTask: () => task, getAgent: () => agent, listTasks: () => [leanTask], getTeam: () => team,
    listUsers: () => [], listComments: () => [], taskActivity: () => [], listRuns: () => [], agentStats: () => [], agentRuns: () => [],
  });
});
afterEach(() => vi.unstubAllGlobals());

describe("the screens show what the lean lists leave out (GA-191)", () => {
  it("the card page shows the acceptance criteria from get_task while list_tasks has none", () => {
    const html = renderToStaticMarkup(<TaskPage id="t1" youId="u-you" />);
    expect(html).toContain("Acceptance criteria");
    expect(html).toContain(ACCEPTANCE);
  });

  it("the agent page shows the instructions from get_agent while get_team has none", () => {
    const html = renderToStaticMarkup(<AgentPage id="a-backend" />);
    const instructions = html.slice(html.indexOf("<h3>Instructions</h3>"));
    expect(instructions).toContain(INSTRUCTIONS);
    expect(instructions).not.toContain("The role&#x27;s standard instructions.");
  });

  it("the agent form loads the agent with get_agent, whose instructions fill the form; get_team's member would leave it empty", () => {
    expect(agentForm).toMatch(/getAgent\(agentId!?\)\.then\(\(m\) => \{[^}]*draftFrom\(m\)/);
    expect(draftFrom(agent).instructions).toBe(INSTRUCTIONS);
    expect(draftFrom(leanAgent).instructions).toBe("");
  });
});
