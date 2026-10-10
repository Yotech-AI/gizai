// GA-90 QA: the sidebar's Agents and Memory fold with a caret at the end of their heading. The sidebar is rendered to
// HTML on the server with its data handed in (useData, the team, the live runs and the drawers are replaced). React's
// useState keeps its values in `slots` by call order, so a caret's or a heading's click is tried on the element tree
// (components called as functions) and the next render shows what it did; emptying `slots` is a reload, which reads
// localStorage again (a Map here).
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { LiveRun, Member, MemoryNote, Project } from "../types";

const slots: unknown[] = [];
let slot = 0;
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init?: unknown) => {
    const k = slot++;
    if (!(k in slots)) slots[k] = typeof init === "function" ? (init as () => unknown)() : init;
    return [slots[k], (v: unknown) => { slots[k] = typeof v === "function" ? (v as (x: unknown) => unknown)(slots[k]) : v; }];
  }) as unknown as typeof R.useState;
  const useEffect = (() => {}) as typeof R.useEffect;
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});

const state = vi.hoisted(() => ({
  data: {} as Record<string, unknown>,
  live: [] as { runId: string; taskId: string; agentId: string }[],
  open: vi.fn(),
  go: vi.fn(),
}));
vi.mock("../api", async (orig) => {
  // Each list the sidebar loads names itself, so the replaced useData hands back its fixture at once.
  const as = (key: string) => () => { const p = { key, catch: () => p }; return p; };
  return { ...(await orig<object>()), listProjects: as("projects"), listTasks: as("tasks"), getTeam: as("team"), memoryNotes: as("notes"), listChatThreads: as("threads") };
});
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => { key: string }) => ({ data: state.data[fetch().key] ?? null, error: null, reload: () => {}, setData: () => {} }),
}));
vi.mock("../lib/team", async (orig) => ({ ...(await orig<object>()), useCurrentTeam: () => ["t1", () => {}] }));
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => state.live }));
vi.mock("../lib/drawers", async (orig) => ({ ...(await orig<object>()), useDrawer: () => state.open }));
vi.mock("../router", async (orig) => ({ ...(await orig<object>()), go: state.go }));
vi.mock("./chat/useChat", () => ({ useChatLive: () => [] }));
vi.mock("./UpdateNotice", () => ({ UpdateNotice: () => null }));

const { Sidebar } = await import("./Sidebar");

const store = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => { store.set(k, String(v)); },
  removeItem: (k: string) => { store.delete(k); },
});

const agent = (actorId: string, name: string, roleKey: string, more: Partial<Member> = {}): Member => ({
  actorId, name, kind: "agent", roleKey, handle: name.toLowerCase().replace(/ /g, "-"), status: "active", isLead: false,
  allowedTools: [], chatEnabled: false, ...more,
});
const lead = agent("lead", "Team Lead", "lead", { isLead: true, chatEnabled: true });
const backend = agent("be", "Backend Agent", "backend");
const frontend = agent("fe", "Frontend Agent", "frontend", { status: "paused" });
const qa = agent("qa", "QA Agent", "qa");
const you: Member = { ...agent("u1", "Jeffrey", "owner"), kind: "user" };
const project = (id: string, name: string, color: string): Project =>
  ({ id, name, color, status: "active", number: "1", key: id.toUpperCase(), defaultBranch: "main", openTasks: 0, doneTasks: 0, updatedAt: 1, aiCostUsdMicros: 0, aiUnknownCostRuns: 0, worktreeCopy: [], worktreeInstall: false });
const note = (id: string, path: string, scope: "shared" | "agent", ownerId?: string): MemoryNote =>
  ({ id, path, scope, ownerId, bodyMd: "", currentVersion: 1, updatedAt: 1, chars: 0 });
const run = (agentId: string, n: number): LiveRun => ({ runId: `r-${agentId}-${n}`, taskId: `t-${n}`, agentId });

function team(members: Member[]) {
  state.data = {
    projects: [project("kade", "Kade portal", "var(--c-yellow)")],
    tasks: [],
    threads: [],
    team: { id: "t1", name: "Software", members, states: [], labels: [] },
    notes: [
      note("n1", "shared/client.md", "shared"), note("n2", "agents/backend-agent/notes.md", "agent", "be"),
      note("n3", "agents/backend-agent/more.md", "agent", "be"), note("n4", "agents/qa-agent/notes.md", "agent", "qa"),
    ],
  };
}

const props = { route: { page: "tasks" } as const, youId: "u1", onSearch: () => {}, onNewTask: () => {} };
/** A render of the sidebar as it is now (its state kept). */
const html = () => { slot = 0; return renderToStaticMarkup(<Sidebar {...props} />); };
/** A reload: the state starts again from localStorage. */
const reload = () => { slots.length = 0; return html(); };

type Props = Record<string, unknown> & { children?: ReactNode };
/** The elements of the sidebar's tree (components called as functions, lucide's icons left as they are). */
function tree(): ReactElement<Props>[] {
  slot = 0;
  const all: ReactElement<Props>[] = [];
  const walk = (n: ReactNode): void => {
    if (!n || typeof n !== "object") return;
    if (Array.isArray(n)) { n.forEach(walk); return; }
    const el = n as ReactElement<Props>;
    if (typeof el.type === "function") { walk((el.type as (p: unknown) => ReactNode)(el.props)); return; }
    all.push(el);
    walk(el.props?.children);
  };
  walk(Sidebar(props));
  return all;
}
const caret = (section: "agents" | "memory") => {
  const found = tree().filter((el) => el.type === "button" && el.props["aria-controls"] === `side-${section}`);
  expect(found).toHaveLength(1);
  return found[0]!.props as { onClick: () => void };
};
const heading = (section: "agents" | "memory") => {
  const name = section === "agents" ? "Agents" : "Memory";
  const found = tree().filter((el) => el.type === "span" && el.props.className === "fold" && [el.props.children].flat().includes(name));
  expect(found).toHaveLength(1);
  return found[0]!.props as { onClick: () => void };
};

/** A section of the rendered sidebar, from its heading to the next section. */
function section(page: string, name: string): string {
  const start = page.indexOf(`<div class="nav-section"${name === "Memory" ? ' aria-label="Memory"' : ""}><div class="nav-label">${name === "Agents" || name === "Memory" ? '<span class="fold">' : ""}${name}`);
  expect(start, `the ${name} section`).toBeGreaterThan(-1);
  const end = page.indexOf('<div class="nav-section"', start + 1);
  return page.slice(start, end === -1 ? undefined : end);
}
/** The heading row of a section. */
const head = (page: string, name: string) => { const s = section(page, name); return s.slice(0, s.indexOf("</div>") + 6); };
const KEY = "gizai.sidebar.folded";

beforeEach(() => {
  slots.length = 0; store.clear(); state.live = []; state.open.mockReset(); state.go.mockReset();
  team([you, lead, backend, frontend, qa]);
});

describe("the carets on Agents and Memory (GA-90)", () => {
  it("are at the right end of both headings, chevron down while open, saying they fold the section", () => {
    const page = html();
    expect(head(page, "Agents")).toMatch(/^<div class="nav-section"><div class="nav-label"><span class="fold">Agents<\/span><button aria-expanded="true" aria-controls="side-agents" aria-label="Fold Agents" title="Fold Agents"><svg[^>]*class="lucide lucide-chevron-down icon sm"[^>]*>.*<\/svg><\/button><\/div>$/);
    expect(head(page, "Memory")).toMatch(/<span class="fold">Memory<\/span><button aria-expanded="true" aria-controls="side-memory" aria-label="Fold Memory" title="Fold Memory"><svg[^>]*class="lucide lucide-chevron-down icon sm"[^>]*>.*<\/svg><\/button><\/div>$/);
  });
  it("point at the section's list, which is there open or folded", () => {
    expect(html()).toContain('<div class="nav-list" id="side-agents">');
    expect(html()).toContain('<div class="nav-list" id="side-memory">');
    store.set(KEY, "agents,memory");
    const page = reload();
    expect(page).toContain('<div class="nav-list" id="side-agents" hidden=""></div>');
    expect(page).toContain('<div class="nav-list" id="side-memory" hidden=""></div>');
  });
  it("are only on Agents and Memory: Work, Projects and Company have none", () => {
    const page = html();
    expect(page.match(/aria-expanded=/g)).toHaveLength(2);
    expect(page.match(/lucide-chevron-(down|up)/g)).toHaveLength(2);
    expect(head(page, "Work")).toBe('<div class="nav-section"><div class="nav-label">Work</div>');
    expect(page).toContain('<div class="nav-label">Company</div>');
  });
  it("are buttons, so Tab reaches them and Enter or Space clicks them", () => {
    const page = html();
    expect(page).not.toMatch(/tabindex="-1"/);
    expect(tree().filter((el) => el.type === "button" && el.props["aria-controls"]).map((el) => typeof el.props.onClick)).toEqual(["function", "function"]);
  });
});

describe("folding and opening (GA-90)", () => {
  it("a click on Agents' caret leaves only its heading, with a chevron up that says Show Agents", () => {
    caret("agents").onClick();
    const agents = section(html(), "Agents");
    expect(agents).toMatch(/<button aria-expanded="false" aria-controls="side-agents" aria-label="Show Agents" title="Show Agents"><svg[^>]*class="lucide lucide-chevron-up icon sm"/);
    expect(agents).toContain('<div class="nav-list" id="side-agents" hidden=""></div>');
    expect(agents).not.toContain("nav-item");
    expect(agents).not.toContain("Backend Agent");
  });
  it("another click on it shows every agent again", () => {
    const before = section(html(), "Agents");
    caret("agents").onClick();
    caret("agents").onClick();
    expect(section(html(), "Agents")).toBe(before);
  });
  it("does the same for Memory: Fold Memory, then Show Memory, then every entry again", () => {
    const before = section(html(), "Memory");
    caret("memory").onClick();
    const memory = section(html(), "Memory");
    expect(memory).toMatch(/aria-expanded="false" aria-controls="side-memory" aria-label="Show Memory" title="Show Memory"><svg[^>]*class="lucide lucide-chevron-up icon sm"/);
    expect(memory).toContain('<div class="nav-list" id="side-memory" hidden=""></div>');
    expect(memory).not.toContain("nav-item");
    caret("memory").onClick();
    expect(section(html(), "Memory")).toBe(before);
  });
  it("a click on the heading's text does what the caret does", () => {
    const before = html();
    heading("agents").onClick();
    expect(head(html(), "Agents")).toContain('aria-label="Show Agents"');
    heading("memory").onClick();
    expect(head(html(), "Memory")).toContain('aria-label="Show Memory"');
    expect(store.get(KEY)).toBe("agents,memory");
    heading("agents").onClick();
    heading("memory").onClick();
    expect(html()).toBe(before);
  });
  it("folding one leaves the other as it was", () => {
    const open = html();
    caret("memory").onClick();
    const page = html();
    expect(section(page, "Agents")).toBe(section(open, "Agents"));
    caret("agents").onClick();
    caret("memory").onClick();
    const now = html();
    expect(section(now, "Memory")).toBe(section(open, "Memory"));
    expect(head(now, "Agents")).toContain('aria-expanded="false"');
  });
  it("Projects keeps its +, which opens New project, and Agents has no Add agent any more", () => {
    const page = html();
    expect(page).not.toContain("Add agent");
    expect(head(page, "Agents")).not.toContain("lucide-plus");
    expect(head(page, "Projects")).toMatch(/<div class="nav-label">Projects<button aria-label="New project" title="New project"><svg[^>]*class="lucide lucide-plus icon sm"/);
    const plus = tree().find((el) => el.type === "button" && el.props["aria-label"] === "New project")!;
    (plus.props.onClick as () => void)();
    expect(state.open).toHaveBeenCalledWith({ kind: "project" });
  });
});

describe("kept on this computer (GA-90)", () => {
  it("both start open on a computer that never folded them", () => {
    const page = reload();
    expect(head(page, "Agents")).toContain('aria-expanded="true"');
    expect(head(page, "Memory")).toContain('aria-expanded="true"');
    expect(store.has(KEY)).toBe(false);
  });
  it("keeps a fold in gizai.sidebar.folded, and a reload shows it folded", () => {
    caret("agents").onClick();
    expect(store.get(KEY)).toBe("agents");
    const page = reload();
    expect(head(page, "Agents")).toContain('aria-label="Show Agents"');
    expect(head(page, "Memory")).toContain('aria-label="Fold Memory"');
    caret("memory").onClick();
    expect(store.get(KEY)).toBe("agents,memory");
    const both = reload();
    expect(head(both, "Agents")).toContain('aria-expanded="false"');
    expect(head(both, "Memory")).toContain('aria-expanded="false"');
  });
  it("keeps an opened section open after a reload", () => {
    store.set(KEY, "agents,memory");
    reload();
    caret("agents").onClick();
    expect(store.get(KEY)).toBe("memory");
    const page = reload();
    expect(head(page, "Agents")).toContain('aria-expanded="true"');
    expect(section(page, "Agents")).toContain("Backend Agent");
    expect(head(page, "Memory")).toContain('aria-expanded="false"');
  });
  it("opens both, and still folds, when localStorage can't be used (private mode)", () => {
    vi.stubGlobal("localStorage", { getItem: () => { throw new Error("denied"); }, setItem: () => { throw new Error("denied"); } });
    try {
      const page = reload();
      expect(head(page, "Agents")).toContain('aria-expanded="true"');
      expect(() => caret("agents").onClick()).not.toThrow();
      expect(head(html(), "Agents")).toContain('aria-expanded="false"');
    } finally {
      vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, v); }, removeItem: (k: string) => { store.delete(k); } });
    }
  });
});

describe("the live tag on folded Agents (GA-90)", () => {
  const tag = (n: number) => `<span class="live-tag"><span class="pulse"></span>${n} live</span>`;
  it("shows the total of live runs while folded", () => {
    state.live = [run("be", 1), run("be", 2), run("qa", 3)];
    caret("agents").onClick();
    expect(head(html(), "Agents")).toContain(`<span class="fold">Agents${tag(3)}</span><button aria-expanded="false"`);
  });
  it("isn't on the heading while open: each agent shows its own", () => {
    state.live = [run("be", 1), run("be", 2), run("qa", 3)];
    const agents = section(html(), "Agents");
    expect(head(html(), "Agents")).not.toContain("live-tag");
    expect(agents).toMatch(new RegExp(`Backend Agent</span><span class="meta">${tag(2)}</span>`));
    expect(agents).toMatch(new RegExp(`QA Agent</span><span class="meta">${tag(1)}</span>`));
  });
  it("shows no tag when folded with nothing running", () => {
    caret("agents").onClick();
    const agents = section(html(), "Agents");
    expect(agents).toContain('aria-label="Show Agents"');
    expect(agents).not.toContain("live-tag");
    expect(agents).not.toContain("pulse");
  });
  it("counts only the agents the sidebar lists, like their tags, and never shows on Memory", () => {
    state.live = [run("be", 1), run("other-team", 2)];
    caret("agents").onClick();
    caret("memory").onClick();
    const page = html();
    expect(head(page, "Agents")).toContain(tag(1));
    expect(head(page, "Memory")).not.toContain("live-tag");
  });
});

describe("nothing else changes (GA-90)", () => {
  it("Agents lists the team's agents with their links, live tags and paused", () => {
    state.live = [run("be", 1)];
    const agents = section(html(), "Agents");
    expect(agents).toMatch(/<a class="nav-item" href="#\/agent\/lead"><svg[^>]*class="lucide lucide-crown icon"[^>]*>.*?<\/svg><span>Team Lead<\/span><span class="meta"><\/span><\/a>/);
    expect(agents).toMatch(/<a class="nav-item" href="#\/agent\/be"><svg[^>]*class="lucide lucide-server icon"[^>]*>.*?<\/svg><span>Backend Agent<\/span><span class="meta"><span class="live-tag"><span class="pulse"><\/span>1 live<\/span><\/span><\/a>/);
    expect(agents).toMatch(/<a class="nav-item" href="#\/agent\/fe">.*?<span>Frontend Agent<\/span><span class="meta"><span class="faint">paused<\/span><\/span><\/a>/);
    expect(agents).toContain("QA Agent");
    expect(agents).not.toContain("Jeffrey");
  });
  it("marks the agent page you are on", () => {
    slot = 0;
    const page = renderToStaticMarkup(<Sidebar {...props} route={{ page: "agent", id: "qa" }} />);
    expect(section(page, "Agents")).toContain('<a class="nav-item on" href="#/agent/qa">');
  });
  it("says No agents yet, with a link to Team, when the team has none", () => {
    team([you]);
    const agents = section(html(), "Agents");
    expect(agents).toContain('<a class="nav-item" href="#/team"><span class="faint">No agents yet</span></a>');
  });
  it("Memory has the Team Lead with every note first, then each agent's own count", () => {
    const memory = section(html(), "Memory");
    expect(memory).toMatch(/<a class="nav-item" href="#\/memory" title="Team Lead: every note \(its own, the shared folders and each agent&#x27;s\)">.*?<span>Team Lead<\/span><span class="meta"><span class="count" title="Notes">4<\/span><\/span><\/a>/);
    expect(memory).toMatch(/href="#\/memory\/agent\/be" title="Backend Agent: its own folder">.*?<span class="count" title="Notes">2<\/span>/);
    expect(memory).toMatch(/href="#\/memory\/agent\/qa" title="QA Agent: its own folder">.*?<span class="count" title="Notes">1<\/span>/);
    expect(memory).toMatch(/href="#\/memory\/agent\/fe" title="Frontend Agent: its own folder">.*?<span class="count" title="Notes">0<\/span>/);
    expect(memory).not.toContain("Set up the Team Lead");
  });
  it("Memory without a Team Lead has Shared notes and Set up the Team Lead, which opens the agent form on Team", () => {
    team([you, backend, qa]);
    const memory = section(html(), "Memory");
    expect(memory).toMatch(/<a class="nav-item" href="#\/memory\/shared" title="The shared folders">.*?<span>Shared notes<\/span><span class="meta"><span class="count" title="Notes">1<\/span><\/span><\/a>/);
    expect(memory).toMatch(/<button class="nav-item"><svg[^>]*class="lucide lucide-crown icon"[^>]*>.*?<\/svg><span>Set up the Team Lead<\/span><\/button>/);
    const setUp = tree().find((el) => el.type === "button" && [el.props.children].flat().some((c) => (c as ReactElement<Props>)?.props?.children === "Set up the Team Lead"))!;
    (setUp.props.onClick as () => void)();
    expect(state.go).toHaveBeenCalledWith({ page: "team" });
    expect(state.open).toHaveBeenCalledWith({ kind: "agent", teamId: "t1", preset: { name: "Team Lead", role: "lead", chat: true } });
  });
  it("keeps Memory's section label, folded or open", () => {
    expect(html()).toContain('<div class="nav-section" aria-label="Memory">');
    caret("memory").onClick();
    expect(html()).toContain('<div class="nav-section" aria-label="Memory">');
  });
  it("keeps Projects, Work and Company as they were", () => {
    const page = html();
    expect(section(page, "Projects")).toMatch(/<a class="nav-item" href="#\/project\/kade"><span class="dot" style="background:var\(--c-yellow\)"><\/span><span>Kade portal<\/span><\/a>/);
    expect(section(page, "Work")).toContain('href="#/tasks"');
    expect(page).toMatch(/<div class="side-foot"><div class="nav-section"><div class="nav-label">Company<\/div>/);
  });
});
