// GA-190 QA: the Tasks page keeps its filter on this device (localStorage) once typing in the search pauses, not per
// keystroke; a pick in Filters writes at once; leaving the page writes what still waits, so nothing is lost. By design the
// search text itself is kept only while the page shows: what is stored is the filter without its text, and the project
// under a key of its own. React itself renders (react-dom/client) into a small stand-in DOM (there is no DOM library in
// the project), with the real search box, Filters popover and task list; the data hooks answer at once, and timers are
// fake. Typing goes through React's own listeners, an input event per character.
import { act, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "../types";

// ---- a stand-in DOM, as much of it as React DOM uses (as in components/StreamParses.test.tsx) ----
const HTML = "http://www.w3.org/1999/xhtml";
type Listener = { f: (e: unknown) => void; capture: boolean };
class FakeNode {
  childNodes: FakeNode[] = [];
  parentNode: FakeNode | null = null;
  attrs = new Map<string, string>();
  listeners = new Map<string, Listener[]>();
  style: Record<string, unknown> = { setProperty(k: string, v: string) { (this as Record<string, unknown>)[k] = v; }, removeProperty() {} };
  nodeValue: string | null = null;
  // an <input>'s value and type, as React sets and reads them
  declare value?: string;
  declare type?: string;
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
  closest() { return null; }
  scrollIntoView() {}
  focus() {}
  blur() {}
  getBoundingClientRect() { return { top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 }; }
  /** This node and every node under it, in document order. */
  all(): FakeNode[] { return [this, ...this.childNodes.flatMap((c) => c.all())]; }
  createElement(tag: string) { return new FakeNode(1, tag.toUpperCase(), this); }
  createElementNS(ns: string, tag: string) { return new FakeNode(1, tag, this, ns); }
  createTextNode(t: string) { const n = new FakeNode(3, "#text", this); n.nodeValue = t; return n; }
  getElementById(id: string) { return this.all().find((n) => n.getAttribute("id") === id) ?? null; }
}
const doc = new FakeNode(9, "#document", null);
const body = doc.createElement("body");
doc.appendChild(body);
// React DOM checks, as it loads, whether the browser has the input event (`"oninput" in document`): it does here
(doc as unknown as { oninput: null }).oninput = null;
const win = { document: doc, event: undefined, HTMLIFrameElement: class {}, devicePixelRatio: 1, addEventListener() {}, removeEventListener() {},
  matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }) };
Object.assign(doc, { body, documentElement: body, activeElement: body, defaultView: win, hidden: false });
vi.stubGlobal("window", win);
vi.stubGlobal("document", doc);
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** An event at `target`, through the listeners on the way as a browser calls them (capture down, then bubble up). */
function fire(target: FakeNode, type: string) {
  const path: FakeNode[] = [];
  for (let x: FakeNode | null = target; x; x = x.parentNode) path.push(x);
  let stopped = false;
  const e = { type, target, bubbles: true, button: 0, defaultPrevented: false, timeStamp: 0,
    preventDefault() { e.defaultPrevented = true; }, stopPropagation() { stopped = true; } };
  for (const n of [...path].reverse()) for (const l of n.listeners.get(type) ?? []) if (l.capture && !stopped) l.f(e);
  for (const n of path) for (const l of n.listeners.get(type) ?? []) if (!l.capture && !stopped) l.f(e);
}
const cls = (n: FakeNode) => (n.getAttribute("class") ?? "").split(" ");
const byLabel = (root: FakeNode, label: string) => root.all().filter((n) => n.getAttribute("aria-label") === label);
const buttonsNamed = (root: FakeNode, text: string) => root.all().filter((n) => n.nodeName === "BUTTON" && n.textContent.trim() === text);

// ---- what the page reads ----
const h = vi.hoisted(() => ({
  /** What the data hooks answer, by the api call they make. */
  data: {} as Record<string, unknown>,
  calls: [] as [string, unknown[]][],
}));
// The api: the reads the page makes answer with a tag the data hooks look up; anything else never settles.
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const tags: Record<string, string> = { getTeam: "team", listProjects: "projects", listUsers: "people", listTasks: "tasks" };
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) {
    out[k] = typeof v !== "function" ? v : (...a: unknown[]) => { h.calls.push([k, a]); return tags[k] ? { tag: tags[k] } : new Promise(() => {}); };
  }
  return out;
});
// The data hooks answer at once, with the same objects on each render.
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => unknown) => {
    const r = fetch() as { tag?: string } | null;
    const tag = r && typeof r === "object" && "tag" in r ? r.tag : undefined;
    return { data: tag ? h.data[tag] ?? null : null, error: null, reload: () => {}, setData: () => {} };
  },
}));
vi.mock("../lib/useLiveRuns", () => { const none: never[] = []; return { useLiveRuns: () => none }; });
vi.mock("../router", async (orig) => ({ ...(await orig<Record<string, unknown>>()), go: (...a: unknown[]) => { h.calls.push(["go", a]); } }));

const { createRoot } = await import("react-dom/client");
const { TasksPage, KEEP_AFTER_MS } = await import("./TasksPage");

// ---- the data ----
const team = { id: "team", name: "Software", labels: [{ id: "l1", name: "Bug", color: null }],
  members: [{ actorId: "be", name: "Backend Agent", kind: "agent", roleKey: "backend", status: "active", handle: "be", isLead: false, allowedTools: [], chatEnabled: false }],
  states: [{ id: "todo", name: "Todo", category: "unstarted", sortKey: "a" }, { id: "ip", name: "In progress", category: "in_progress", sortKey: "b" }] };
const projects = [{ id: "p1", name: "Gizai", key: "GA", color: "#4f46e5" }, { id: "p2", name: "Website", key: "WEB", color: null }];
const people = [{ id: "u1", name: "Jeffrey" }];
const card = (id: string, title: string, over: Partial<Task>): Task => ({ id, identifier: id.toUpperCase(), title, descriptionMd: "", stateId: "todo",
  stateName: "Todo", stateCategory: "unstarted", priority: 0, assigneeId: null, assigneeName: null, assigneeKind: null, labels: [], hold: null,
  bounceCount: 0, failCount: 0, sortKey: "a", testing: true, createdAt: 0, updatedAt: 0, projectId: "p1", byHand: false, screenshots: null, ...over } as Task);
const tasks = [
  card("ga-1", "Fix the login page", { assigneeId: "u1", assigneeName: "Jeffrey", assigneeKind: "person", updatedAt: 6 }),
  card("ga-2", "Login page copy", { stateId: "ip", stateName: "In progress", stateCategory: "in_progress", updatedAt: 5 }),
  card("web-3", "Landing page", { projectId: "p2", updatedAt: 4 }),
  card("ga-4", "Logout button", { assigneeId: "be", assigneeName: "Backend Agent", assigneeKind: "agent", updatedAt: 3 }),
  card("web-5", "Login page for partners", { projectId: "p2", assigneeId: "u1", assigneeName: "Jeffrey", assigneeKind: "person", updatedAt: 2 }),
];

// ---- localStorage, every write recorded ----
let store: Map<string, string>;
let ls: { getItem: ReturnType<typeof vi.fn>; setItem: ReturnType<typeof vi.fn>; removeItem: ReturnType<typeof vi.fn> };
const writes = () => [...ls.setItem.mock.calls.map(([k, v]) => ["set", k, v]), ...ls.removeItem.mock.calls.map(([k]) => ["remove", k])];
const setsOf = (k: string) => ls.setItem.mock.calls.filter(([key]) => key === k).map(([, v]) => v);
const removesOf = (k: string) => ls.removeItem.mock.calls.filter(([key]) => key === k).length;

beforeEach(() => {
  store = new Map();
  ls = {
    getItem: vi.fn((k: string) => store.get(k) ?? null),
    setItem: vi.fn((k: string, v: string) => void store.set(k, String(v))),
    removeItem: vi.fn((k: string) => void store.delete(k)),
  };
  vi.stubGlobal("localStorage", ls);
  h.data = { team, projects, people, tasks };
  h.calls.length = 0;
  // on before the page mounts, so its setTimeout is the fake one
  vi.useFakeTimers();
});
afterEach(async () => {
  if (root) await unmount();
  vi.useRealTimers();
});

// ---- the page ----
let container: FakeNode;
let root: ReturnType<typeof createRoot> | null = null;
function mount(el: ReactNode) {
  container = doc.createElement("div");
  body.appendChild(container);
  root = createRoot(container as unknown as HTMLElement);
  act(() => { root!.render(el); });
}
const mountTasks = () => mount(<TasksPage initialView="list" onNewTask={() => {}} />);
async function unmount() {
  await act(async () => { root!.unmount(); });
  root = null;
  container.parentNode?.removeChild(container);
}
const search = () => byLabel(container, "Search tasks")[0]!;
/** Types one character more: the box holds `value`, and the browser sends an input event. */
function key(value: string) {
  const box = search();
  act(() => { box.type = "search"; box.value = value; fire(box, "input"); });
}
const wait = (ms: number) => act(() => { vi.advanceTimersByTime(ms); });
/** The task rows shown, by identifier. */
const rows = () => container.all().filter((n) => cls(n).includes("task-row")).map((n) => n.all().find((c) => cls(c).includes("id"))!.textContent);
/** What the page's search should show for `q` (as lib/taskView matches it: identifier and title, any case). */
const matching = (q: string, among = tasks) => among.filter((t) => `${t.identifier} ${t.title}`.toLowerCase().includes(q.toLowerCase())).map((t) => t.identifier);
const filtersButton = () => container.all().find((n) => n.nodeName === "BUTTON" && /^Filters/.test(n.textContent.trim()))!;
const openFilters = () => act(() => { fire(filtersButton(), "click"); });
const pick = (name: string) => act(() => { fire(buttonsNamed(byLabel(container, "Filters").find((n) => n.getAttribute("role") === "menu")!, name)[0]!, "click"); });

const TEXT = "login page";
const GAP = 50;
/** Types `text` a character at a time, `gap` ms apart; checks after each character that nothing was written yet and the list
 *  shows what matches. */
function typeQuickly(text: string, gap = GAP, among = tasks) {
  for (let i = 1; i <= text.length; i++) {
    if (i > 1) wait(gap);
    key(text.slice(0, i));
    expect(search().value).toBe(text.slice(0, i));
    expect(rows().sort()).toEqual(matching(text.slice(0, i), among).sort());
    expect(writes(), `after ${i} characters`).toEqual([]);
  }
}

describe("the Tasks page keeps its filter once typing pauses (GA-190)", () => {
  it("writes nothing while 10 characters are typed 50 ms apart, the list filtering at each; once typing pauses for KEEP_AFTER_MS it writes once, the filter without its text", () => {
    expect(KEEP_AFTER_MS).toBe(400);
    mountTasks();
    expect(rows().sort()).toEqual(tasks.map((t) => t.identifier).sort());
    // opening the page writes nothing
    expect(writes()).toEqual([]);

    expect(TEXT).toHaveLength(10);
    typeQuickly(TEXT);
    expect(rows().sort()).toEqual(["GA-1", "GA-2", "WEB-5"]);
    // 450 ms of typing, nothing written

    // a pause just short of KEEP_AFTER_MS: still nothing
    wait(KEEP_AFTER_MS - 1);
    expect(writes()).toEqual([]);
    // then the write, once: the filter without its search text (nothing else is on: an empty filter), and no project kept
    wait(1);
    expect(setsOf("gizai-task-filter")).toEqual(["{}"]);
    expect(removesOf("gizai-task-project")).toBe(1);
    expect(setsOf("gizai-task-project")).toEqual([]);
    expect(writes()).toHaveLength(2);
    expect(store.get("gizai-task-filter")).not.toContain("login");
    // nothing more as time goes on, and the search still shows what was typed
    wait(10_000);
    expect(writes()).toHaveLength(2);
    expect(search().value).toBe(TEXT);
    expect(rows().sort()).toEqual(["GA-1", "GA-2", "WEB-5"]);
  });

  it("with a project and an assignee kept, typing writes those again once it pauses (and never the text), the project under its own key", () => {
    store.set("gizai-task-filter", JSON.stringify({ assigneeId: "u1" }));
    store.set("gizai-task-project", "p1");
    mountTasks();
    expect(filtersButton().textContent).toBe("Filters: 2");
    expect(rows()).toEqual(["GA-1"]);
    expect(writes()).toEqual([]);

    typeQuickly("fix the l", 30, tasks.filter((t) => t.projectId === "p1" && t.assigneeId === "u1"));
    expect(writes()).toEqual([]);
    wait(KEEP_AFTER_MS);
    expect(setsOf("gizai-task-filter")).toEqual([JSON.stringify({ assigneeId: "u1" })]);
    expect(setsOf("gizai-task-project")).toEqual(["p1"]);
    expect(removesOf("gizai-task-project")).toBe(0);
    expect(writes()).toHaveLength(2);
    wait(10_000);
    expect(writes()).toHaveLength(2);
  });

  it("a pick in Filters writes at once, without waiting for the timer, and leaves no write waiting", () => {
    mountTasks();
    openFilters();
    expect(byLabel(container, "Filters").filter((n) => n.getAttribute("role") === "menu")).toHaveLength(1);
    expect(writes()).toEqual([]);

    // a project: at once
    pick("Gizai");
    expect(setsOf("gizai-task-filter")).toEqual(["{}"]);
    expect(setsOf("gizai-task-project")).toEqual(["p1"]);
    expect(writes()).toHaveLength(2);
    expect(rows().sort()).toEqual(["GA-1", "GA-2", "GA-4"]);
    expect(filtersButton().textContent).toBe("Filters: 1");

    // an assignee: at once
    pick("Jeffrey");
    expect(setsOf("gizai-task-filter")).toEqual(["{}", JSON.stringify({ assigneeId: "u1" })]);
    expect(setsOf("gizai-task-project")).toEqual(["p1", "p1"]);
    expect(writes()).toHaveLength(4);
    expect(rows()).toEqual(["GA-1"]);
    expect(filtersButton().textContent).toBe("Filters: 2");

    // the timer that a change starts was flushed with it: nothing more later
    wait(10_000);
    expect(writes()).toHaveLength(4);
    expect(store.get("gizai-task-filter")).toBe(JSON.stringify({ assigneeId: "u1" }));
    expect(store.get("gizai-task-project")).toBe("p1");
  });

  it("a pick in Filters while typed text waits writes at once, once, and no second write follows when the timer would have run", () => {
    mountTasks();
    typeQuickly("log", 20);
    expect(writes()).toEqual([]);
    openFilters();
    pick("Website");
    expect(setsOf("gizai-task-filter")).toEqual(["{}"]);
    expect(setsOf("gizai-task-project")).toEqual(["p2"]);
    expect(writes()).toHaveLength(2);
    // Website's cards with "log" in them
    expect(rows()).toEqual(["WEB-5"]);
    wait(10_000);
    expect(writes()).toHaveLength(2);
    expect(search().value).toBe("log");
  });

  it("leaving the page while a write waits writes it then (nothing is lost), and nothing more when the timers run", async () => {
    store.set("gizai-task-project", "p1");
    mountTasks();
    typeQuickly(TEXT, GAP, tasks.filter((t) => t.projectId === "p1"));
    wait(KEEP_AFTER_MS - 100);
    expect(writes()).toEqual([]);

    await unmount();
    expect(setsOf("gizai-task-filter")).toEqual(["{}"]);
    expect(setsOf("gizai-task-project")).toEqual(["p1"]);
    expect(removesOf("gizai-task-project")).toBe(0);
    expect(writes()).toHaveLength(2);
    // the timer was cleared with it
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(10_000);
    expect(writes()).toHaveLength(2);
  });

  it("leaving the page right after a single keystroke writes too, and with nothing waiting, leaving writes nothing", async () => {
    mountTasks();
    key("x");
    await unmount();
    expect(setsOf("gizai-task-filter")).toEqual(["{}"]);
    expect(removesOf("gizai-task-project")).toBe(1);
    expect(writes()).toHaveLength(2);

    ls.setItem.mockClear();
    ls.removeItem.mockClear();
    mountTasks();
    typeQuickly("lo");
    wait(KEEP_AFTER_MS);
    expect(writes()).toHaveLength(2);
    await unmount();
    expect(writes()).toHaveLength(2);
    vi.advanceTimersByTime(10_000);
    expect(writes()).toHaveLength(2);
  });

  it("a page opened again reads back what was kept: the picks in Filters, the project; not the search text", async () => {
    mountTasks();
    openFilters();
    pick("Gizai");
    pick("Jeffrey");
    key("f");
    key("fi");
    key("fix");
    expect(rows()).toEqual(["GA-1"]);
    await unmount();
    expect(store.get("gizai-task-filter")).toBe(JSON.stringify({ assigneeId: "u1" }));
    expect(store.get("gizai-task-project")).toBe("p1");
    const before = writes().length;

    mountTasks();
    expect(search().value).toBe("");
    expect(filtersButton().textContent).toBe("Filters: 2");
    // the project in the crumbs, the list filtered by project and assignee
    expect(container.all().find((n) => cls(n).includes("crumbs"))!.textContent).toContain("Gizai/");
    expect(rows()).toEqual(["GA-1"]);
    openFilters();
    const menu = byLabel(container, "Filters").find((n) => n.getAttribute("role") === "menu")!;
    const ticked = menu.all().filter((n) => n.nodeName === "BUTTON" && n.all().some((c) => (c.getAttribute("class") ?? "").includes("tick"))).map((n) => n.textContent);
    expect(ticked).toEqual(["Gizai", "Jeffrey"]);
    // opening it writes nothing
    expect(writes()).toHaveLength(before);
  });
});
