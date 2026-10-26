// GA-225 QA: the board's Team select, the Team page and the sidebar's agents follow the same pick (useCurrentTeam, kept on
// this device). With "Research team" picked, the Team page and the sidebar ask for it and show its agents, not the first
// team's. Rendered to HTML on the server: each api call names itself, so the replaced useData hands back its fixture.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Team, TeamSummary } from "../types";

const page = vi.hoisted(() => ({ data: {} as Record<string, unknown>, asked: [] as (string | null)[] }));
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  // The sidebar's Terminal section reads the open terminals from their store: the snapshot, outside React too
  const useSyncExternalStore = ((_subscribe: unknown, snapshot: () => unknown) => snapshot()) as typeof R.useSyncExternalStore;
  return { ...R, default: { ...R, useSyncExternalStore }, useSyncExternalStore };
});
// any other call (the labels, …) answers nothing
vi.mock("@tauri-apps/api/core", () => ({ invoke: () => Promise.resolve(null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));
vi.mock("../api", async (orig) => {
  const as = (key: string) => { const p = { key, catch: () => p }; return p; };
  return { ...(await orig<object>()), listProjects: () => as("projects"), listTasks: () => as("tasks"), listChatThreads: () => as("threads"),
    memoryNotes: () => as("notes"), listTeams: () => as("teams"),
    getTeam: (id?: string | null) => { page.asked.push(id ?? null); return as(`team:${id ?? "my"}`); } };
});
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => { key?: string }) => ({ data: page.data[fetch()?.key ?? ""] ?? null, error: null, reload: () => {}, setData: () => {} }),
}));
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));
vi.mock("../lib/drawers", async (orig) => ({ ...(await orig<object>()), useDrawer: () => () => {} }));
vi.mock("../components/chat/useChat", () => ({ useChatLive: () => [] }));
vi.mock("../components/UpdateNotice", () => ({ UpdateNotice: () => null }));

const { TeamPage } = await import("./TeamPage");
const { Sidebar } = await import("../components/Sidebar");

const member = (actorId: string, name: string) => ({ actorId, name, kind: "agent", roleKey: "backend", handle: actorId, status: "active", isLead: false,
  allowedTools: [], chatEnabled: false });
const states = [{ id: "s1", name: "To do", category: "ready", sortKey: "a1", auto: true, agentIds: [] }];
const my = { id: "my", name: "My team", labels: [], branches: [], members: [member("be", "Backend Agent")], states } as unknown as Team;
const rt = { id: "rt", name: "Research team", labels: [], branches: [], members: [member("ra", "Research Agent")], states } as unknown as Team;
const teams: TeamSummary[] = [{ id: "my", name: "My team" }, { id: "rt", name: "Research team" }];

let store: Map<string, string>;
beforeEach(() => {
  store = new Map();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k) });
  page.asked = [];
  page.data = { "team:my": my, "team:rt": rt, teams, projects: [], tasks: [], threads: [], notes: [] };
});
afterEach(() => vi.unstubAllGlobals());

const sidebar = () => renderToStaticMarkup(<Sidebar route={{ page: "tasks" }} youId="you" onSearch={() => {}} onNewTask={() => {}} />);

describe("the team picked on the board", () => {
  it("is the one the Team page shows", () => {
    store.set("gizai-team", "rt");
    const html = renderToStaticMarkup(<TeamPage />);
    expect(page.asked).toEqual(["rt"]);
    expect(html).toContain("Research Agent");
    expect(html).not.toContain("Backend Agent");
  });

  it("is the one whose agents the sidebar shows", () => {
    store.set("gizai-team", "rt");
    const html = sidebar();
    expect(page.asked).toEqual(["rt"]);
    expect(html).toContain("Research Agent");
    expect(html).not.toContain("Backend Agent");
  });

  it("none picked: the first team, on both", () => {
    const html = sidebar() + renderToStaticMarkup(<TeamPage />);
    expect(page.asked).toEqual([null, null]);
    expect(html).toContain("Backend Agent");
    expect(html).not.toContain("Research Agent");
  });
});
