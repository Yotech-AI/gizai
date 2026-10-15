// GA-178 QA: how Gizai starts. A normal start never loads the self-test (src/selftest.ts); a test run loads it before its
// first step (GIZAI_SELFTEST_MODE=steps: and appearance:, the tour's capture) or probe (GIZAI_SELFTEST, each mode
// scripts/ui-test.sh and close-save-test.sh use), and the probes run and report as before. Every route shows its page as
// before: the board and the Inbox (TasksPage) at once, every other page through React.lazy, in one Suspense whose
// fallback is nothing (no spinner). A hash change sets the route in a transition, so React keeps the page you came from
// on screen while the next one loads. A small hook runtime stands in for React (App runs as a function, its effects by
// hand); the api, the components, the pages and the self-test are fakes, and the fake self-test notes when it is loaded.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Suspense, type ReactElement, type ReactNode } from "react";

const h = vi.hoisted(() => ({
  effects: [] as (() => unknown)[],
  log: [] as string[],
  sets: [] as { value: unknown; transition: boolean }[],
  transition: false,
  info: {} as Record<string, unknown>,
  listeners: new Map<string, () => void>(),
}));

vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  return {
    ...R,
    useState: (init: unknown) => [typeof init === "function" ? (init as () => unknown)() : init, (value: unknown) => { h.sets.push({ value, transition: h.transition }); }],
    useEffect: (f: () => unknown) => { h.effects.push(f); },
    startTransition: (f: () => void) => { h.transition = true; try { f(); } finally { h.transition = false; } },
  };
});
vi.mock("./api", () => {
  const ok = (v: unknown = null) => () => Promise.resolve(v);
  return {
    appInfo: () => { h.log.push("appInfo"); return Promise.resolve(h.info); },
    exitApp: (code: number) => { h.log.push(`exitApp ${code}`); return Promise.resolve(); },
    selftestReport: (json: { ready?: boolean; errors?: string[] }) => { h.log.push(`report ready=${json.ready} errors=${JSON.stringify(json.errors ?? null)}`); return Promise.resolve(); },
    getDoc: ok(), getTask: ok({ descriptionMd: "" }), getTeam: ok({ states: [] }), listRuns: ok([]), saveDoc: ok(), updateTask: ok(),
    listClients: ok([]), listProjects: ok([]), listTasks: ok([]), tourStill: ok(), tourSteps: ok(),
  };
});
// the self-test, made again for each test (beforeEach), so each test sees whether its start loaded it
const fakeSelftest = () => {
  h.log.push("selftest loaded");
  const probe = (name: string) => async (...args: unknown[]) => {
    h.log.push(`${name} ${JSON.stringify(args.filter((a) => typeof a === "string" || Array.isArray(a)))}`);
    return { ok: true, moved: true, saved: true, editor_closed: true };
  };
  return Object.fromEntries(["runSteps", "appearanceProbe", "changesProbe", "chatArchiveProbe", "chatProbe", "closeSaveProbe", "docProbe", "dragProbe",
    "editorProbe", "memoryEmptyProbe", "memoryProbe", "runProbe", "teamProbe", "terminalProbe", "usageProbe"].map((n) => [n, probe(n)]));
};
vi.mock("./lib/appearance", () => ({ appearanceOf: (spec: string) => ({ spec }), setAppearance: (a: unknown) => { h.log.push(`setAppearance ${JSON.stringify(a)}`); }, toggleDensity: () => {}, toggleTheme: () => {} }));
vi.mock("./lib/terminals", () => ({ startTerminals: () => { h.log.push("startTerminals"); } }));
vi.mock("./lib/drawers", () => ({ DrawerHost: ({ children }: { children: ReactNode }) => children }));
vi.mock("./components/Sidebar", () => ({ Sidebar: () => null }));
vi.mock("./components/Band", () => ({ Band: () => null }));
vi.mock("./components/CommandPalette", () => ({ CommandPalette: () => null }));
vi.mock("./components/CloseGuard", () => ({ CloseGuard: () => null }));
vi.mock("./pages/TasksPage", () => ({ TasksPage: function TasksPage() { return null; } }));
vi.mock("./pages/Placeholder", () => ({ Placeholder: function Placeholder() { return null; } }));
vi.mock("./pages/ClientsPage", () => ({ ClientsPage: function ClientsPage() { return null; } }));
vi.mock("./pages/ClientPage", () => ({ ClientPage: function ClientPage() { return null; } }));
vi.mock("./pages/UsersPage", () => ({ UsersPage: function UsersPage() { return null; } }));
vi.mock("./pages/ProjectsPage", () => ({ ProjectsPage: function ProjectsPage() { return null; } }));
vi.mock("./pages/ProjectPage", () => ({ ProjectPage: function ProjectPage() { return null; } }));
vi.mock("./pages/TaskPage", () => ({ TaskPage: function TaskPage() { return null; } }));
vi.mock("./pages/DocPage", () => ({ DocPage: function DocPage() { return null; } }));
vi.mock("./pages/TeamPage", () => ({ TeamPage: function TeamPage() { return null; } }));
vi.mock("./pages/SettingsPage", () => ({ SettingsPage: function SettingsPage() { return null; } }));
vi.mock("./pages/AgentPage", () => ({ AgentPage: function AgentPage() { return null; } }));
vi.mock("./pages/ChatPage", () => ({ ChatPage: function ChatPage() { return null; } }));
vi.mock("./pages/UsagePage", () => ({ UsagePage: function UsagePage() { return null; } }));
vi.mock("./pages/MemoryPage", () => ({ MemoryPage: function MemoryPage() { return null; } }));
vi.mock("./pages/EditorPage", () => ({ EditorPage: function EditorPage() { return null; } }));
vi.mock("./pages/TerminalPage", () => ({ TerminalPage: function TerminalPage() { return null; } }));

const ready: string[] = [];
const fakeWindow = {
  location: { hash: "" },
  addEventListener: (type: string, f: () => void) => { h.listeners.set(type, f); },
  removeEventListener: (type: string) => { h.listeners.delete(type); },
};
const fakeDocument = {
  body: { dataset: {} as Record<string, string> },
  documentElement: { dataset: {} as Record<string, string> },
  querySelector: () => ({}),
};
Object.defineProperty(fakeDocument.body.dataset, "ready", { set: (v: string) => { ready.push(v); h.log.push(`ready ${v}`); }, get: () => ready.at(-1) });
vi.stubGlobal("window", fakeWindow);
vi.stubGlobal("document", fakeDocument);

beforeEach(() => {
  vi.resetModules();
  vi.doMock("./selftest", fakeSelftest);
  h.effects = []; h.log = []; h.sets = []; h.listeners.clear();
  fakeWindow.location.hash = "";
});

/** App's first render at `hash`: its tree; its effects ran, and the start went as far as it goes for `info`. */
async function start(info: Record<string, unknown>, hash = "", done = (log: string[]) => log.includes("ready 1")) {
  h.log = []; h.sets = []; ready.length = 0;
  h.info = { version: "0.0.0", data_dir: "/data", you_id: "you", selftest: false, start_route: null, selftest_mode: null, ...info };
  fakeWindow.location.hash = hash;
  const { default: App } = await import("./App");
  // as React calls it: with its props, none here (main.tsx passes onboarding: false when the onboarding doesn't wait, GA-164)
  const tree = (App as (props: object) => ReactElement)({});
  for (const f of h.effects.splice(0)) f();
  await vi.waitFor(() => { if (!done(h.log)) throw new Error(`not done: ${h.log.join(" | ")} ${JSON.stringify(h.sets.at(-1))}`); }, { timeout: 3000, interval: 10 });
  await new Promise((r) => setTimeout(r, 50)); // anything a start does after that, it does by now
  return tree;
}
const ended = (log: string[]) => log.some((l) => l.startsWith("exitApp"));

/** The Suspense element in the page area, and the element it shows. */
type Area = ReactElement<{ fallback: ReactNode; children: ReactElement }>;
function pageArea(tree: ReactElement): { suspense: Area; page: ReactElement } {
  const find = (n: ReactNode): Area | null => {
    if (!n || typeof n !== "object") return null;
    if (Array.isArray(n)) return n.map(find).find(Boolean) ?? null;
    const el = n as ReactElement<{ children?: ReactNode }>;
    return el.type === Suspense ? (el as Area) : find(el.props?.children);
  };
  const suspense = find(tree);
  expect(suspense, "a Suspense in the page area").not.toBeNull();
  return { suspense: suspense!, page: suspense!.props.children };
}
/** Each named element of the tree (a component by its name, a tag by itself): whether it sits inside the page area's Suspense. */
function placed(tree: ReactElement): Map<string, boolean> {
  const at = new Map<string, boolean>();
  const walk = (n: ReactNode, inside: boolean) => {
    if (!n || typeof n !== "object") return;
    if (Array.isArray(n)) { n.forEach((c) => walk(c, inside)); return; }
    const el = n as ReactElement<{ children?: ReactNode }>;
    const name = el.type === Suspense ? "Suspense" : typeof el.type === "function" ? el.type.name : typeof el.type === "string" ? el.type : "";
    if (name) at.set(name, inside);
    walk(el.props?.children, inside || el.type === Suspense);
  };
  walk(tree, false);
  return at;
}
type Lazy = { $$typeof: symbol; _payload: unknown; _init: (p: unknown) => (...a: unknown[]) => unknown };
const LAZY = Symbol.for("react.lazy");
/** The component a lazy page resolves to (React's lazy holds its loader in _payload and resolves with _init). */
async function resolveLazy(type: unknown): Promise<string> {
  const lazy = type as Lazy;
  try { return lazy._init(lazy._payload).name; } catch (thrown) { await thrown; }
  return lazy._init(lazy._payload).name;
}

describe("a normal start", () => {
  it("never loads the self-test, and marks the page ready", async () => {
    await start({});
    expect(h.log).toEqual(["startTerminals", "appInfo", "ready 1"]);
  });

  it("with a start route goes there, still without the self-test", async () => {
    await start({ start_route: "memory/graph" });
    expect(fakeWindow.location.hash).toBe("#/memory/graph");
    expect(h.log).not.toContain("selftest loaded");
  });

  it("opens a drawer for open:<kind> or sets the light theme without the self-test", async () => {
    await start({ selftest_mode: "open:project" });
    expect(h.sets).toContainEqual({ value: { kind: "project" }, transition: false });
    expect(h.log).not.toContain("selftest loaded");
    await start({ selftest_mode: "theme:light" });
    expect(fakeDocument.documentElement.dataset.theme).toBe("light");
    expect(h.log).not.toContain("selftest loaded");
  });
});

describe("a test run", () => {
  it("steps: loads the self-test before its first step, then marks the page ready", async () => {
    await start({ selftest_mode: "steps:#/settings/github;#s-bb-email" });
    expect(h.log).toEqual(["startTerminals", "appInfo", "selftest loaded", 'runSteps [["#/settings/github","#s-bb-email"]]', "ready 1"]);
  });

  it("appearance:<spec>;<steps> sets the appearance, then loads the self-test for the steps", async () => {
    await start({ selftest_mode: "appearance:theme=light;#/board;wait:200" });
    expect(h.log).toEqual(["startTerminals", "appInfo", 'setAppearance {"spec":"theme=light"}', "selftest loaded", 'runSteps [["#/board","wait:200"]]', "ready 1"]);
  });

  it("appearance:<spec> without steps loads no self-test", async () => {
    await start({ selftest_mode: "appearance:theme=light" });
    expect(h.log).not.toContain("selftest loaded");
  });

  // each start in scripts/ui-test.sh and scripts/close-save-test.sh: its route, its mode and the probe it runs
  it.each([
    ["board", "", "dragProbe []"],
    ["task/t1", "", 'editorProbe ["t1"]'],
    ["doc/d1", "", 'docProbe ["d1"]'],
    ["team", "", "teamProbe []"],
    ["task/t1", "run", 'runProbe ["t1"]'],
    ["chat", "chat", "chatProbe []"],
    ["usage", "", "usageProbe []"],
    ["chat", "archive", "chatArchiveProbe []"],
    ["settings/appearance", "appearance-set", 'appearanceProbe ["set"]'],
    ["settings/appearance", "appearance-kept", 'appearanceProbe ["kept"]'],
    ["memory/n1", "memory:a1", 'memoryProbe ["n1","a1"]'],
    ["memory/shared", "memory-empty", "memoryEmptyProbe []"],
    ["memory/graph", "memory:graph-a x y", 'memoryProbe ["graph","graph-a x y"]'],
    ["task/t1/changes", "changes-a:t1 t2", 'changesProbe ["a",["t1","t2"]]'],
    ["task/t1", "changes-b:t1 t2", 'changesProbe ["b",["t1","t2"]]'],
    ["terminal", "terminal:KADE-1", 'terminalProbe ["KADE-1"]'],
  ])("GIZAI_SELFTEST on %s with mode '%s' loads the self-test before its probe (%s) and reports", async (route, mode, probe) => {
    await start({ selftest: true, start_route: route, selftest_mode: mode || null }, "", ended);
    expect(fakeWindow.location.hash).toBe(`#/${route}`);
    const loaded = h.log.indexOf("selftest loaded");
    expect(loaded).toBeGreaterThan(h.log.indexOf("ready 1"));
    expect(h.log.indexOf(probe)).toBeGreaterThan(loaded);
    expect(h.log.slice(-2)).toEqual(["report ready=true errors=[]", "exitApp 0"]);
    expect(h.log.filter((l) => l === "selftest loaded")).toHaveLength(1);
  });

  it("close-save:<step> on a doc runs its probe and leaves quitting to the script", async () => {
    await start({ selftest: true, start_route: "doc/d1", selftest_mode: "close-save:type" }, "", (log) => log.some((l) => l.startsWith("closeSaveProbe")));
    expect(h.log.slice(1)).toEqual(["appInfo", "ready 1", "selftest loaded", 'closeSaveProbe ["type","d1"]']);
  });
});

describe("the page area", () => {
  it("waits for a page's code with nothing in it (no spinner)", async () => {
    const { suspense } = pageArea(await start({}, "#/settings"));
    expect(suspense.props.fallback).toBeNull();
  });

  it.each(["", "#/", "#/board", "#/tasks", "#/inbox", "#/nothing-here"])("shows the board or the Inbox at once on '%s'", async (hash) => {
    const { page } = pageArea(await start({}, hash));
    expect((page.type as { name?: string }).name).toBe("TasksPage");
    expect((page.type as { $$typeof?: symbol }).$$typeof).toBeUndefined();
  });

  it.each([
    ["#/task/t1", "TaskPage", { id: "t1", view: "card" }],
    ["#/task/t1/changes/run/r1", "TaskPage", { id: "t1", view: "changes", run: "r1" }],
    ["#/chat/c1", "ChatPage", { id: "c1", archive: false }],
    ["#/chats", "ChatPage", { archive: true }],
    ["#/editor/%2Fhome%2Fa", "EditorPage", { id: "/home/a" }],
    ["#/memory/graph", "MemoryPage", {}],
    ["#/memory/agent/a1/n1", "MemoryPage", {}],
    ["#/settings/appearance", "SettingsPage", { tab: "appearance" }],
    ["#/doc/d1", "DocPage", { id: "d1" }],
    ["#/agent/a1", "AgentPage", { id: "a1" }],
    ["#/team", "TeamPage", {}],
    ["#/clients", "ClientsPage", {}],
    ["#/client/k1", "ClientPage", { id: "k1" }],
    ["#/users", "UsersPage", {}],
    ["#/usage", "UsagePage", {}],
    ["#/terminal/s1", "TerminalPage", { id: "s1" }],
    ["#/projects", "ProjectsPage", {}],
    ["#/project/p1", "ProjectPage", { id: "p1" }],
  ])("opens %s as a lazy %s with its props", async (hash, name, props) => {
    const { page } = pageArea(await start({}, hash));
    expect((page.type as { $$typeof?: symbol }).$$typeof).toBe(LAZY);
    expect(await resolveLazy(page.type)).toBe(name);
    expect(page.props).toMatchObject(props);
  });

  // GA-178 QA: a page whose code is still loading suspends only the page area; the drawers (DrawerHost renders them beside
  // the app), the sidebar, Ctrl+K's command palette and the close guard stay on screen and work as before.
  it.each(["#/board", "#/task/t1", "#/memory/graph", "#/settings"])("leaves the drawers, the sidebar, the command palette and the close guard outside it on '%s'", async (hash) => {
    const tree = await start({}, hash);
    expect((tree.type as { name?: string }).name).toBe("DrawerHost");
    const at = placed(tree);
    expect(at.get("Suspense")).toBe(false);
    expect(at.get("main")).toBe(false);
    for (const name of ["Band", "Sidebar", "CommandPalette", "CloseGuard"]) expect(at.get(name), name).toBe(false);
  });
});

describe("a route change", () => {
  it("sets the new route in a transition, so the page you came from stays until the next one is ready", async () => {
    const { useRoute } = await import("./router");
    fakeWindow.location.hash = "#/board";
    expect(useRoute()).toEqual({ page: "board" });
    for (const f of h.effects.splice(0)) f();
    fakeWindow.location.hash = "#/memory/graph";
    h.listeners.get("hashchange")!();
    expect(h.sets).toEqual([{ value: { page: "memory", view: "graph" }, transition: true }]);
  });
});
