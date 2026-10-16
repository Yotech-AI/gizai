// GA-190 QA: what one write costs. A card moved on the board (move_task) or changed (update_task) sends one `rows-changed`
// with table "tasks" (src-tauri/src/commands.rs). Before GA-190 every data hook on screen read again after it: the board
// with the sidebar made 9 backend calls, the Inbox 10. Now a hook reads again only when the notice names a table it shows
// (lib/tables.ts), and two hooks that ask the same read at the same time share one call (lib/share.ts): at most 2 each.
//
// The real Sidebar and the real TasksPage (the board, and the Inbox through its inboxFor) render here with React itself
// (react-dom/client) into a small stand-in DOM, as in components/StreamParses.test.tsx: there is no DOM library in the
// project. Only Tauri is faked: `invoke` of @tauri-apps/api/core records each backend call and answers it, `listen` of
// @tauri-apps/api/event keeps the listeners so the test sends `rows-changed` as the backend does. src/api.ts, lib/share.ts
// and lib/useData.ts are the real ones. A frame (requestAnimationFrame) runs when the test says so.
import { act, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Member, Task, Team, WorkflowState } from "../types";

// ---- a stand-in DOM, as much of it as React DOM uses (from components/StreamParses.test.tsx) ----
const HTML = "http://www.w3.org/1999/xhtml";
type Listener = { f: (e: unknown) => void; capture: boolean };
class FakeNode {
  childNodes: FakeNode[] = [];
  parentNode: FakeNode | null = null;
  attrs = new Map<string, string>();
  listeners = new Map<string, Listener[]>();
  style: Record<string, unknown> = { setProperty(k: string, v: string) { (this as Record<string, unknown>)[k] = v; }, removeProperty() {} };
  nodeValue: string | null = null;
  scrollTop = 0;
  scrollHeight = 0;
  clientHeight = 0;
  hidden = false;
  constructor(public nodeType: number, public nodeName: string, public ownerDocument: FakeNode | null, public namespaceURI = HTML) {}
  get tagName() { return this.nodeName; }
  get firstChild(): FakeNode | null { return this.childNodes[0] ?? null; }
  get lastChild(): FakeNode | null { return this.childNodes[this.childNodes.length - 1] ?? null; }
  get nextSibling(): FakeNode | null { const p = this.parentNode; return p ? p.childNodes[p.childNodes.indexOf(this) + 1] ?? null : null; }
  get textContent(): string { return this.nodeType === 3 ? this.nodeValue ?? "" : this.childNodes.map((c) => c.textContent).join(""); }
  set textContent(t: string) {
    for (const c of this.childNodes) c.parentNode = null;
    this.childNodes = [];
    if (t) this.appendChild(doc.createTextNode(t));
  }
  get options() { return this.all().filter((n) => n.nodeName === "OPTION"); }
  appendChild(c: FakeNode) { return this.insertBefore(c, null); }
  insertBefore(c: FakeNode, ref: FakeNode | null) {
    c.parentNode?.removeChild(c);
    const i = ref ? this.childNodes.indexOf(ref) : -1;
    if (i < 0) this.childNodes.push(c); else this.childNodes.splice(i, 0, c);
    c.parentNode = this;
    return c;
  }
  removeChild(c: FakeNode) { this.childNodes.splice(this.childNodes.indexOf(c), 1); c.parentNode = null; return c; }
  setAttribute(k: string, v: unknown) { this.attrs.set(k, String(v)); }
  setAttributeNS(_ns: string, k: string, v: unknown) { this.attrs.set(k, String(v)); }
  removeAttribute(k: string) { this.attrs.delete(k); }
  removeAttributeNS(_ns: string, k: string) { this.attrs.delete(k); }
  getAttribute(k: string) { return this.attrs.get(k) ?? null; }
  hasAttribute(k: string) { return this.attrs.has(k); }
  addEventListener(type: string, f: (e: unknown) => void, opt?: boolean | { capture?: boolean }) {
    const capture = typeof opt === "boolean" ? opt : !!opt?.capture;
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), { f, capture }]);
  }
  removeEventListener(type: string, f: (e: unknown) => void) { this.listeners.set(type, (this.listeners.get(type) ?? []).filter((l) => l.f !== f)); }
  contains(n: FakeNode | null) { for (let x = n; x; x = x.parentNode) if (x === this) return true; return false; }
  hasFocus() { return true; }
  scrollIntoView() {}
  focus() {}
  blur() {}
  getBoundingClientRect() { return { top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 }; }
  /** This node and every node under it, in document order. */
  all(): FakeNode[] { return [this, ...this.childNodes.flatMap((c) => c.all())]; }
  createElement(tag: string) { return new FakeNode(1, tag.toUpperCase(), this); }
  createElementNS(ns: string, tag: string) { return new FakeNode(1, tag, this, ns); }
  createTextNode(t: string) { const n = new FakeNode(3, "#text", this); n.nodeValue = t; return n; }
}
const doc = new FakeNode(9, "#document", null);
const body = doc.createElement("body");
doc.appendChild(body);
const winListeners = new Map<string, ((e: unknown) => void)[]>();
const win = {
  document: doc, event: undefined, HTMLIFrameElement: class {}, devicePixelRatio: 1, innerWidth: 1280, innerHeight: 800,
  location: { hash: "#/tasks" },
  // dnd-kit (the board) measures its droppables, and tells nodes apart with instanceof
  Document: { [Symbol.hasInstance]: (x: unknown) => (x as FakeNode | null)?.nodeType === 9 },
  Node: { [Symbol.hasInstance]: (x: unknown) => x instanceof FakeNode },
  Element: { [Symbol.hasInstance]: (x: unknown) => (x as FakeNode | null)?.nodeType === 1 },
  HTMLElement: { [Symbol.hasInstance]: (x: unknown) => (x as FakeNode | null)?.nodeType === 1 && (x as FakeNode).namespaceURI === HTML },
  SVGElement: { [Symbol.hasInstance]: (x: unknown) => (x as FakeNode | null)?.nodeType === 1 && (x as FakeNode).namespaceURI !== HTML },
  Window: { [Symbol.hasInstance]: (x: unknown) => x === win },
  getComputedStyle: () => ({ transform: "none", transformOrigin: "0 0", overflow: "visible", overflowX: "visible", overflowY: "visible",
    position: "static", getPropertyValue: () => "" }),
  addEventListener(t: string, f: (e: unknown) => void) { winListeners.set(t, [...(winListeners.get(t) ?? []), f]); },
  removeEventListener(t: string, f: (e: unknown) => void) { winListeners.set(t, (winListeners.get(t) ?? []).filter((x) => x !== f)); },
  dispatchEvent(e: { type: string }) { for (const f of winListeners.get(e.type) ?? []) f(e); return true; },
};
Object.assign(doc, { body, documentElement: body, activeElement: body, defaultView: win });
const store = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, String(v)); },
  removeItem: (k: string) => { store.delete(k); }, clear: () => store.clear(), key: () => null, get length() { return store.size; },
});
vi.stubGlobal("window", win);
vi.stubGlobal("document", doc);
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// ---- Tauri, faked: the backend's calls and its events ----
const h = vi.hoisted(() => ({
  /** Every backend call, in order: the command and its arguments. */
  calls: [] as { cmd: string; args: unknown }[],
  /** What the backend answers, by command; a command without one answers []. */
  answers: {} as Record<string, (args: unknown) => unknown>,
  /** The event listeners, by event name. */
  events: new Map<string, Set<(e: { payload: unknown }) => void>>(),
}));
vi.mock("@tauri-apps/api/core", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  return {
    ...real,
    invoke: (cmd: string, args?: unknown) => {
      h.calls.push({ cmd, args });
      const a = h.answers[cmd];
      try { return Promise.resolve(a ? a(args) : []); } catch (e) { return Promise.reject(e); }
    },
  };
});
vi.mock("@tauri-apps/api/event", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  return {
    ...real,
    listen: (name: string, f: (e: { payload: unknown }) => void) => {
      const set = h.events.get(name) ?? new Set();
      h.events.set(name, set);
      set.add(f);
      return Promise.resolve(() => { set.delete(f); });
    },
  };
});

// ---- frames: requestAnimationFrame waits until the test runs a frame ----
let frames = new Map<number, (t: number) => void>();
let nextFrame = 1;
vi.stubGlobal("requestAnimationFrame", (f: (t: number) => void) => { const id = nextFrame++; frames.set(id, f); return id; });
vi.stubGlobal("cancelAnimationFrame", (id: number) => { frames.delete(id); });
function runFrame() { const due = frames; frames = new Map(); for (const f of due.values()) f(0); }

const { createRoot } = await import("react-dom/client");
const { Sidebar } = await import("../components/Sidebar");
const { TasksPage } = await import("./TasksPage");

// ---- the data a backend gives ----
const YOU = "you";
const state = (id: string, name: string, category: string, sortKey: string): WorkflowState => ({ id, name, category, sortKey });
const member = (actorId: string, name: string, kind: string, roleKey: string, isLead = false): Member => ({
  actorId, name, kind, roleKey, handle: name.toLowerCase(), status: "active", isLead, allowedTools: [], chatEnabled: isLead,
});
const TEAM: Team = {
  id: "team1", name: "Team", labels: [],
  states: [state("todo", "Todo", "unstarted", "a"), state("doing", "In Progress", "started", "b"), state("done", "Done", "completed", "c")],
  members: [member(YOU, "You", "human", "owner"), member("lead", "Team Lead", "agent", "lead", true), member("fe", "Frontend Agent", "agent", "frontend")],
};
const task = (n: number, stateId: string, over: Partial<Task> = {}): Task => ({
  id: `t${n}`, identifier: `GA-${n}`, projectId: "p1", projectName: "Gizai", title: `Card ${n}`, descriptionMd: "",
  stateId, stateName: TEAM.states.find((s) => s.id === stateId)!.name, stateCategory: TEAM.states.find((s) => s.id === stateId)!.category,
  priority: 2, labels: [], bounceCount: 0, failCount: 0, sortKey: `k${n}`, testing: true, createdAt: n, updatedAt: n, byHand: false,
  screenshots: null, ...over,
});
let tasks: Task[] = [];
function answer() {
  h.answers = {
    app_info: () => ({ version: "0", data_dir: "/tmp", selftest: false, you_id: YOU }),
    get_team: () => TEAM,
    list_teams: () => [{ id: TEAM.id, name: TEAM.name }],
    list_tasks: () => tasks.map((t) => ({ ...t })),
    list_projects: () => [{ id: "p1", number: "1", key: "GA", name: "Gizai", status: "active", defaultBranch: "main", openTasks: 3, doneTasks: 1,
      updatedAt: 1, aiCostUsdMicros: 0, aiUnknownCostRuns: 0, worktreeCopy: [], worktreeInstall: false,
      screenshots: { on: false, sizes: [], bothThemes: false, fullPage: false }, previewCommand: null, previewStartSecs: 120 }],
    list_users: () => [{ id: YOU, name: "You", kind: "human" }],
    memory_notes: () => [],
    list_chat_threads: () => [],
    live_runs: () => [],
    chat_live: () => [],
    update_status: () => null,
    editor_folders: () => [],
    list_terminals: () => [],
    move_task: () => null,
    update_task: () => null,
  };
}

/** What a write on the backend sends afterwards: `rows-changed` with the table it wrote. */
function rowsChanged(table: string) {
  for (const f of [...(h.events.get("rows-changed") ?? [])]) f({ payload: { table } });
}

let container: FakeNode;
let root: ReturnType<typeof createRoot> | null = null;
/** Lets the answers come in and React draw them. */
async function settle() {
  await act(async () => { for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0)); });
}
async function mount(el: ReactNode) {
  container = doc.createElement("div");
  body.appendChild(container);
  root = createRoot(container as unknown as HTMLElement);
  await act(async () => { root!.render(el); });
  await settle();
  await act(async () => { runFrame(); });
  await settle();
}
/** The backend calls one write leads to: its notice, a frame, and every answer in. */
async function callsAfterWrite(table: string): Promise<string[]> {
  const from = h.calls.length;
  await act(async () => { rowsChanged(table); });
  await settle();
  await act(async () => { runFrame(); });
  await settle();
  return h.calls.slice(from).map((c) => c.cmd);
}

const Board = () => (
  <>
    <Sidebar route={{ page: "tasks" }} youId={YOU} onSearch={() => {}} onNewTask={() => {}} />
    <TasksPage initialView="board" onNewTask={() => {}} />
  </>
);
const Inbox = () => (
  <>
    <Sidebar route={{ page: "inbox" }} youId={YOU} onSearch={() => {}} onNewTask={() => {}} />
    <TasksPage onNewTask={() => {}} inboxFor={YOU} />
  </>
);
const shown = () => container.textContent;
const sorted = (xs: string[]) => [...xs].sort();

beforeEach(() => {
  h.calls = [];
  store.clear();
  doc.hidden = false;
  tasks = [task(1, "todo"), task(2, "doing", { assigneeId: YOU, assigneeName: "You", assigneeKind: "human" }), task(3, "done"),
    task(4, "todo", { hold: "decision", holdReason: "Which colour?" })];
  answer();
  frames = new Map();
});
afterEach(async () => {
  await act(async () => { root?.unmount(); });
  root = null;
  container?.parentNode?.removeChild(container);
  doc.hidden = false;
  await settle();
});

describe("one write to the cards (rows-changed \"tasks\", as move_task and update_task send) with the sidebar beside", () => {
  it("on the board: 2 backend calls (was 9), list_tasks once and the sidebar's list_chat_threads, and the board shows the change", async () => {
    await mount(<Board />);
    expect(shown()).toContain("Card 1");
    // the backend moved and renamed card 1, then sent its notice
    tasks = tasks.map((t) => (t.id === "t1" ? { ...t, title: "Card 1 renamed", stateId: "doing", stateName: "In Progress", stateCategory: "started" } : t));
    const cmds = await callsAfterWrite("tasks");
    expect(cmds.length).toBeLessThanOrEqual(2);
    expect(sorted(cmds)).toEqual(["list_chat_threads", "list_tasks"]);
    expect(shown()).toContain("Card 1 renamed");
  });

  it("on the Inbox: 2 backend calls (was 10), list_tasks and list_chat_threads once each, shared by the sidebar and the page", async () => {
    await mount(<Inbox />);
    expect(shown()).toContain("Card 4");
    // card 4 decided: off hold, so it leaves the Inbox
    tasks = tasks.map((t) => (t.id === "t4" ? { ...t, hold: null, holdReason: null } : t));
    const cmds = await callsAfterWrite("tasks");
    expect(cmds.length).toBeLessThanOrEqual(2);
    expect(sorted(cmds)).toEqual(["list_chat_threads", "list_tasks"]);
    expect(shown()).not.toContain("Card 4");
  });

  it("two writes' notices before the next frame: still one call per read", async () => {
    await mount(<Board />);
    // a second write's notice right after the first, before the frame: still one reload of each read
    const from = h.calls.length;
    await act(async () => { rowsChanged("tasks"); rowsChanged("tasks"); });
    await act(async () => { runFrame(); });
    await settle();
    expect(sorted(h.calls.slice(from).map((c) => c.cmd))).toEqual(["list_chat_threads", "list_tasks"]);
  });
});

describe("writes the board doesn't show", () => {
  it("a contact's write: no backend call on the board or the Inbox", async () => {
    await mount(<Board />);
    expect(await callsAfterWrite("contacts")).toEqual([]);
    await act(async () => { root?.unmount(); });
    root = null;
    await mount(<Inbox />);
    expect(await callsAfterWrite("contacts")).toEqual([]);
  });

  it("a doc's write: only the sidebar's memory notes", async () => {
    await mount(<Board />);
    expect(await callsAfterWrite("docs")).toEqual(["memory_notes"]);
  });

  it("a project's write: list_projects and list_tasks (a card shows its project) once each, the sidebar and the page sharing them", async () => {
    await mount(<Board />);
    expect(sorted(await callsAfterWrite("projects"))).toEqual(["list_projects", "list_tasks"]);
  });
});

describe("a window out of sight", () => {
  it("hidden, a write's notice reads nothing, not after a frame nor after 100 ms; shown again, the reads come at once", async () => {
    await mount(<Board />);
    const from = h.calls.length;
    doc.hidden = true;
    await act(async () => { rowsChanged("tasks"); rowsChanged("runs"); });
    await act(async () => { runFrame(); });
    await act(async () => { await new Promise((r) => setTimeout(r, 250)); });
    await settle();
    expect(h.calls.slice(from)).toEqual([]);
    doc.hidden = false;
    await act(async () => { for (const l of doc.listeners.get("visibilitychange") ?? []) l.f({ type: "visibilitychange" }); });
    const cmds = h.calls.slice(from).map((c) => c.cmd);
    // at once: the reads started in the visibilitychange itself, before any frame
    expect(cmds).toContain("list_tasks");
    await settle();
    expect(sorted(h.calls.slice(from).map((c) => c.cmd))).toEqual(["list_chat_threads", "list_tasks", "memory_notes"]);
  });
});
