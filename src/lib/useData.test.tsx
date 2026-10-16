// GA-190 QA: useData after a write. Notices within one frame make one reload; an answer equal to the last one renders
// nothing and keeps the same objects (keep), in a list the cards that didn't change too; a write to a table a read shows
// reloads it, one to a table it doesn't show doesn't (a fetch the table map doesn't know reloads after any write; counts:
// false leaves out the counts' tables); while the window is hidden nothing reloads, and when it shows, one reload comes at
// once; an older answer that comes after a newer one doesn't overwrite it.
//
// React itself renders a small component that uses the hook (react-dom/client) into a stand-in DOM, as in
// components/StreamParses.test.tsx: there is no DOM library in the project. Only Tauri is faked: `invoke` of
// @tauri-apps/api/core records each backend call and answers it (or holds it until the test answers), `listen` of
// @tauri-apps/api/event keeps the listeners so the test sends `rows-changed` as the backend does. src/api.ts, lib/share.ts
// and lib/useData.ts are the real ones. Frames (requestAnimationFrame) run when the test says so; timers are fake.
import { act, memo, useEffect } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ---- a stand-in DOM, as much of it as React DOM uses (from components/StreamParses.test.tsx) ----
const HTML = "http://www.w3.org/1999/xhtml";
type Listener = { f: (e: unknown) => void; capture: boolean };
class FakeNode {
  childNodes: FakeNode[] = [];
  parentNode: FakeNode | null = null;
  attrs = new Map<string, string>();
  listeners = new Map<string, Listener[]>();
  style: Record<string, unknown> = {};
  nodeValue: string | null = null;
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
  removeAttribute(k: string) { this.attrs.delete(k); }
  getAttribute(k: string) { return this.attrs.get(k) ?? null; }
  addEventListener(type: string, f: (e: unknown) => void, opt?: boolean | { capture?: boolean }) {
    const capture = typeof opt === "boolean" ? opt : !!opt?.capture;
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), { f, capture }]);
  }
  removeEventListener(type: string, f: (e: unknown) => void) { this.listeners.set(type, (this.listeners.get(type) ?? []).filter((l) => l.f !== f)); }
  createElement(tag: string) { return new FakeNode(1, tag.toUpperCase(), this); }
  createElementNS(ns: string, tag: string) { return new FakeNode(1, tag, this, ns); }
  createTextNode(t: string) { const n = new FakeNode(3, "#text", this); n.nodeValue = t; return n; }
}
const doc = new FakeNode(9, "#document", null);
const body = doc.createElement("body");
doc.appendChild(body);
const win = { document: doc, event: undefined, HTMLIFrameElement: class {}, addEventListener() {}, removeEventListener() {} };
Object.assign(doc, { body, documentElement: body, activeElement: body, defaultView: win });
vi.stubGlobal("window", win);
vi.stubGlobal("document", doc);
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The page shows again, or hides: `document.hidden` and a `visibilitychange`, as WebKitGTK sends them. */
function setHidden(hidden: boolean) {
  doc.hidden = hidden;
  for (const l of [...(doc.listeners.get("visibilitychange") ?? [])]) l.f({ type: "visibilitychange" });
}

// ---- Tauri, faked ----
type Call = { cmd: string; args: unknown; answer: (v: unknown) => void; fail: (e: unknown) => void };
const h = vi.hoisted(() => ({
  /** Every backend call, in order. */
  calls: [] as Call[],
  /** What the backend answers, by command; held: the call waits until the test answers it. */
  answers: {} as Record<string, () => unknown>,
  hold: false,
  events: new Map<string, Set<(e: { payload: unknown }) => void>>(),
}));
vi.mock("@tauri-apps/api/core", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  return {
    ...real,
    invoke: (cmd: string, args?: unknown) => new Promise((answer, fail) => {
      h.calls.push({ cmd, args, answer, fail });
      if (!h.hold) answer(h.answers[cmd] ? h.answers[cmd]() : null);
    }),
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

let frames = new Map<number, (t: number) => void>();
let nextFrame = 1;
vi.stubGlobal("requestAnimationFrame", (f: (t: number) => void) => { const id = nextFrame++; frames.set(id, f); return id; });
vi.stubGlobal("cancelAnimationFrame", (id: number) => { frames.delete(id); });

const { createRoot } = await import("react-dom/client");
const { useData, keep } = await import("./useData");
const api = await import("../api");

/** A write on the backend sent `rows-changed` for `table`. */
function rowsChanged(...tables: string[]) {
  for (const table of tables) for (const f of [...(h.events.get("rows-changed") ?? [])]) f({ payload: { table } });
}
/** Lets the answers come in and React draw them (microtasks only: timers are fake). */
async function settle() {
  await act(async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); });
}
async function frame() {
  await act(async () => { const due = frames; frames = new Map(); for (const f of due.values()) f(0); });
  await settle();
}
const calls = (cmd?: string) => h.calls.filter((c) => !cmd || c.cmd === cmd).length;

// ---- a component that shows what the hook gives ----
type Seen = { renders: number; commits: number; childRenders: number; data: unknown; error: string | null; datas: unknown[]; fetches: number };
const Child = memo(function Child({ data, seen }: { data: unknown; seen: Seen }) { seen.childRenders++; return <i>{JSON.stringify(data)}</i>; });
function Probe({ fetch, deps, counts, seen }: { fetch: () => Promise<unknown>; deps?: unknown[]; counts?: boolean; seen: Seen }) {
  const { data, error } = useData(() => { seen.fetches++; return fetch(); }, deps, { counts });
  seen.renders++;
  seen.data = data;
  seen.error = error;
  useEffect(() => { seen.commits++; seen.datas.push(data); });
  return <Child data={data} seen={seen} />;
}
const newSeen = (): Seen => ({ renders: 0, commits: 0, childRenders: 0, data: undefined, error: null, datas: [], fetches: 0 });

let roots: { root: ReturnType<typeof createRoot>; el: FakeNode }[] = [];
async function mount(fetch: () => Promise<unknown>, opts: { deps?: unknown[]; counts?: boolean } = {}): Promise<Seen> {
  const seen = newSeen();
  const el = doc.createElement("div");
  body.appendChild(el);
  const root = createRoot(el as unknown as HTMLElement);
  roots.push({ root, el });
  await act(async () => { root.render(<Probe fetch={fetch} deps={opts.deps} counts={opts.counts} seen={seen} />); });
  await settle();
  return seen;
}

type T = { id: string; title: string; labels: { id: string; name: string }[] };
let tasks: T[] = [];
const tasksAnswer = () => tasks.map((t) => ({ ...t, labels: t.labels.map((l) => ({ ...l })) }));

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"] });
  h.calls = [];
  h.hold = false;
  doc.hidden = false;
  frames = new Map();
  tasks = [{ id: "a", title: "A", labels: [{ id: "l1", name: "bug" }] }, { id: "b", title: "B", labels: [] }, { id: "c", title: "C", labels: [] }];
  h.answers = {
    list_tasks: tasksAnswer,
    list_projects: () => [{ id: "p1", name: "Gizai", openTasks: 2 }],
    list_docs: () => [],
    app_info: () => ({ version: "1" }),
  };
});
afterEach(async () => {
  // answers still held come in, so nothing waits on to the next test (lib/share.ts keeps a read only while it is on its way)
  for (const c of h.calls) c.answer(null);
  await settle();
  await act(async () => { for (const r of roots) r.root.unmount(); });
  for (const r of roots) r.el.parentNode?.removeChild(r.el);
  roots = [];
  doc.hidden = false;
  await settle();
  vi.useRealTimers();
});

describe("notices within one frame make one reload", () => {
  it("5 notices for tables the hook reads: no read before the frame, exactly one at it", async () => {
    const seen = await mount(() => api.listTasks());
    expect(calls("list_tasks")).toBe(1);
    await act(async () => { rowsChanged("tasks", "runs", "labels", "tasks", "workflow_states"); });
    await settle();
    expect(calls("list_tasks")).toBe(1);
    await frame();
    expect(calls("list_tasks")).toBe(2);
    expect(seen.fetches).toBe(2);
    // nothing more later: no timer left behind
    await act(async () => { vi.advanceTimersByTime(1000); });
    await frame();
    expect(calls("list_tasks")).toBe(2);
  });

  it("frames that don't come (a window out of sight may draw none): one reload after 100 ms", async () => {
    await mount(() => api.listTasks());
    await act(async () => { rowsChanged("tasks", "tasks", "runs"); });
    await act(async () => { vi.advanceTimersByTime(99); });
    await settle();
    expect(calls("list_tasks")).toBe(1);
    await act(async () => { vi.advanceTimersByTime(1); });
    await settle();
    expect(calls("list_tasks")).toBe(2);
    // the frame that comes late reads nothing more
    await frame();
    expect(calls("list_tasks")).toBe(2);
  });

  it("notices in two frames: two reloads", async () => {
    await mount(() => api.listTasks());
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    expect(calls("list_tasks")).toBe(3);
  });

  it("three hooks on the same read: one notice, one backend call, and each shows the answer", async () => {
    const seen = [await mount(() => api.listTasks()), await mount(() => api.listTasks()), await mount(() => api.listTasks({}))];
    const before = calls("list_tasks");
    tasks = [...tasks, { id: "d", title: "D", labels: [] }];
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    expect(calls("list_tasks") - before).toBe(1);
    for (const s of seen) expect((s.data as T[]).map((t) => t.id)).toEqual(["a", "b", "c", "d"]);
  });
});

describe("an answer equal to the last one renders nothing", () => {
  it("identical answer: nothing commits, the memoised child doesn't draw, data keeps its identity; a changed answer: one render with it", async () => {
    const seen = await mount(() => api.listTasks());
    const first = seen.data as T[];
    expect(first.map((t) => t.title)).toEqual(["A", "B", "C"]);
    const renders = seen.renders;
    const commits = seen.commits;
    const childRenders = seen.childRenders;

    await act(async () => { rowsChanged("tasks"); });
    await frame();
    expect(calls("list_tasks")).toBe(2);
    // nothing commits and the memoised child doesn't draw; React may call the component itself once more before it bails
    // out (its documented early bailout, see "the same answer again and again" below)
    expect(seen.commits).toBe(commits);
    expect(seen.childRenders).toBe(childRenders);
    expect(seen.renders - renders).toBeLessThanOrEqual(1);
    expect(seen.data).toBe(first);
    const called = seen.renders;

    tasks = tasks.map((t) => (t.id === "b" ? { ...t, title: "B renamed" } : t));
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    expect(seen.renders).toBe(called + 1);
    expect(seen.commits).toBe(commits + 1);
    expect(seen.childRenders).toBe(childRenders + 1);
    const next = seen.data as T[];
    expect(next).not.toBe(first);
    expect(next.map((t) => t.title)).toEqual(["A", "B renamed", "C"]);
    // one card changed: the others are the objects they were (a memoised card skips)
    expect(next[0]).toBe(first[0]);
    expect(next[2]).toBe(first[2]);
    expect(next[1]).not.toBe(first[1]);
    // and the unchanged parts of the changed one too
    expect(next[1].labels).toBe(first[1].labels);
  });

  it("the same answer again and again: nothing commits; the component itself is called at most once (React's early bailout)", async () => {
    const seen = await mount(() => api.listTasks());
    const first = seen.data;
    const renders = seen.renders;
    const commits = seen.commits;
    const childRenders = seen.childRenders;
    for (let i = 0; i < 5; i++) {
      await act(async () => { rowsChanged("tasks"); });
      await frame();
    }
    expect(calls("list_tasks")).toBe(6);
    expect(seen.commits).toBe(commits);
    expect(seen.childRenders).toBe(childRenders);
    expect(seen.renders - renders).toBeLessThanOrEqual(1);
    expect(seen.data).toBe(first);
  });
});

describe("keep", () => {
  it("an equal answer is the last one itself, deep", () => {
    const prev = { a: [1, 2, { x: "y" }], b: null, c: "s" };
    expect(keep(prev, JSON.parse(JSON.stringify(prev)))).toBe(prev);
  });

  it("a list where one item changed: a new list, the other items the same objects", () => {
    const prev = [{ id: "a", n: 1, tags: ["x"] }, { id: "b", n: 2, tags: ["y"] }, { id: "c", n: 3, tags: [] as string[] }];
    const next = keep(prev, [{ id: "a", n: 1, tags: ["x"] }, { id: "b", n: 20, tags: ["y"] }, { id: "c", n: 3, tags: [] }]);
    expect(next).not.toBe(prev);
    expect(next[0]).toBe(prev[0]);
    expect(next[2]).toBe(prev[2]);
    expect(next[1]).not.toBe(prev[1]);
    expect(next[1]).toEqual({ id: "b", n: 20, tags: ["y"] });
    expect(next[1].tags).toBe(prev[1].tags);
  });

  it("a reorder keeps items by id; an added or removed item makes a new list", () => {
    const prev = [{ id: "a", n: 1 }, { id: "b", n: 2 }];
    const moved = keep(prev, [{ id: "b", n: 2 }, { id: "a", n: 1 }]);
    expect(moved).not.toBe(prev);
    expect(moved[0]).toBe(prev[1]);
    expect(moved[1]).toBe(prev[0]);
    const added = keep(prev, [{ id: "a", n: 1 }, { id: "b", n: 2 }, { id: "c", n: 3 }]);
    expect(added).not.toBe(prev);
    expect(added[0]).toBe(prev[0]);
    const removed = keep(prev, [{ id: "a", n: 1 }]);
    expect(removed).not.toBe(prev);
    expect(removed).toEqual([{ id: "a", n: 1 }]);
  });

  it("a key added or removed, or a value that differs, is a change", () => {
    const prev = { a: 1, b: 2 };
    expect(keep(prev, { a: 1, b: 2, c: undefined })).not.toBe(prev);
    expect(keep(prev, { a: 1 })).not.toBe(prev);
    expect(keep(prev, { a: 1, b: 3 })).toEqual({ a: 1, b: 3 });
    expect(keep([1, 2], [1, 2, 3])).toEqual([1, 2, 3]);
    expect(keep(null, [1])).toEqual([1]);
  });
});

describe("which writes reload which reads", () => {
  it("list_tasks: after \"tasks\" it reloads and shows the new data; after \"docs\" or \"contacts\" it doesn't reload", async () => {
    const seen = await mount(() => api.listTasks());
    await act(async () => { rowsChanged("docs", "contacts"); });
    await frame();
    await act(async () => { vi.advanceTimersByTime(500); });
    expect(calls("list_tasks")).toBe(1);
    expect(seen.fetches).toBe(1);
    tasks = [{ id: "z", title: "Z", labels: [] }];
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    expect(calls("list_tasks")).toBe(2);
    expect(seen.data).toEqual([{ id: "z", title: "Z", labels: [] }]);
    expect(seen.datas.at(-1)).toEqual([{ id: "z", title: "Z", labels: [] }]);
  });

  it("each of a card read's tables reloads it (list_tasks: tasks, workflow_states, projects, actors, runs, labels)", async () => {
    await mount(() => api.listTasks());
    const tables = ["tasks", "workflow_states", "projects", "actors", "runs", "labels"];
    for (const t of tables) {
      await act(async () => { rowsChanged(t); });
      await frame();
    }
    expect(calls("list_tasks")).toBe(1 + tables.length);
  });

  it("a fetch with a command the map doesn't know (app_info) reloads after any write", async () => {
    const seen = await mount(() => api.appInfo());
    await act(async () => { rowsChanged("contacts"); });
    await frame();
    await act(async () => { rowsChanged("docs"); });
    await frame();
    expect(calls("app_info")).toBe(3);
    expect(seen.fetches).toBe(3);
  });

  it("a fetch with a known and an unknown command reloads after any write", async () => {
    await mount(() => Promise.all([api.listTasks(), api.appInfo()]));
    await act(async () => { rowsChanged("contacts"); });
    await frame();
    expect(calls("list_tasks")).toBe(2);
  });

  it("a fetch that calls no command before it returns (Promise.resolve, a read after an await) reloads after any write", async () => {
    const plain = await mount(() => Promise.resolve(["x"]));
    const later = await mount(async () => { await Promise.resolve(); return api.listTasks(); });
    await act(async () => { rowsChanged("contacts"); });
    await frame();
    expect(plain.fetches).toBe(2);
    expect(later.fetches).toBe(2);
    expect(calls("list_tasks")).toBe(2);
  });

  it("counts: false on list_projects: not after \"tasks\" or \"runs\", after \"projects\" and \"clients\"; with counts after \"tasks\"", async () => {
    const without = await mount(() => api.listProjects(), { counts: false });
    await act(async () => { rowsChanged("tasks", "runs"); });
    await frame();
    expect(without.fetches).toBe(1);
    await act(async () => { rowsChanged("projects"); });
    await frame();
    expect(without.fetches).toBe(2);
    await act(async () => { rowsChanged("clients"); });
    await frame();
    expect(without.fetches).toBe(3);

    const withCounts = await mount(() => api.listProjects());
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    expect(withCounts.fetches).toBe(2);
    expect(without.fetches).toBe(3);
  });

  it("deps that change ask again with the new arguments, and the reads that follow use them", async () => {
    const seen = newSeen();
    const el = doc.createElement("div");
    body.appendChild(el);
    const root = createRoot(el as unknown as HTMLElement);
    roots.push({ root, el });
    const render = (projectId: string) => act(async () => {
      root.render(<Probe fetch={() => api.listTasks({ projectId })} deps={[projectId]} seen={seen} />);
    });
    await render("p1");
    await settle();
    await render("p2");
    await settle();
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    expect(h.calls.filter((c) => c.cmd === "list_tasks").map((c) => (c.args as { filter: { projectId: string } }).filter.projectId))
      .toEqual(["p1", "p2", "p2"]);
  });
});

describe("a hidden window", () => {
  it("hidden: notices read nothing, however long; shown: exactly one reload, at once", async () => {
    const seen = await mount(() => api.listTasks());
    setHidden(true);
    await act(async () => { rowsChanged("tasks", "runs", "tasks", "labels", "tasks"); });
    await frame();
    await act(async () => { vi.advanceTimersByTime(10_000); });
    await frame();
    expect(calls("list_tasks")).toBe(1);
    expect(seen.fetches).toBe(1);
    // shown: the read starts in the visibilitychange itself, no frame or timer needed
    await act(async () => { setHidden(false); });
    expect(calls("list_tasks")).toBe(2);
    await settle();
    await frame();
    await act(async () => { vi.advanceTimersByTime(1000); });
    expect(calls("list_tasks")).toBe(2);
  });

  it("a visibilitychange that leaves it hidden reads nothing; notices for tables it doesn't show don't reload it when shown", async () => {
    await mount(() => api.listTasks());
    setHidden(true);
    await act(async () => { rowsChanged("docs", "contacts"); });
    await act(async () => { setHidden(true); });
    expect(calls("list_tasks")).toBe(1);
    await act(async () => { setHidden(false); });
    await frame();
    await act(async () => { vi.advanceTimersByTime(1000); });
    expect(calls("list_tasks")).toBe(1);
  });

  it("hidden while a reload waits for its frame: the frame does nothing, the showing reloads once", async () => {
    await mount(() => api.listTasks());
    await act(async () => { rowsChanged("tasks"); });
    setHidden(true);
    await frame();
    await act(async () => { vi.advanceTimersByTime(1000); });
    expect(calls("list_tasks")).toBe(1);
    await act(async () => { setHidden(false); });
    await settle();
    expect(calls("list_tasks")).toBe(2);
  });
});

describe("answers out of order", () => {
  it("an older answer that comes after a newer one doesn't overwrite it", async () => {
    const seen = await mount(() => api.listTasks());
    h.hold = true;
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    const [, older, newer] = h.calls;
    expect(h.calls.map((c) => c.cmd)).toEqual(["list_tasks", "list_tasks", "list_tasks"]);
    // a notice drops the read on its way: the second reload didn't join the first
    expect(older).not.toBe(newer);
    await act(async () => { newer.answer([{ id: "n", title: "newer", labels: [] }]); });
    await settle();
    await act(async () => { older.answer([{ id: "o", title: "older", labels: [] }]); });
    await settle();
    expect(seen.data).toEqual([{ id: "n", title: "newer", labels: [] }]);
    expect(seen.datas.some((d) => JSON.stringify(d).includes("older"))).toBe(false);
  });

  it("an older failure after a newer answer shows no error", async () => {
    const seen = await mount(() => api.listTasks());
    h.hold = true;
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    const [, older, newer] = h.calls;
    await act(async () => { newer.answer([]); });
    await settle();
    await act(async () => { older.fail("database is locked"); });
    await settle();
    expect(seen.error).toBeNull();
    expect(seen.data).toEqual([]);
  });

  it("the read on its way when a notice comes isn't shared with the reload after it", async () => {
    h.hold = true;
    const seen = await mount(() => api.listTasks());
    await act(async () => { rowsChanged("tasks"); });
    await frame();
    expect(calls("list_tasks")).toBe(2);
    const [first, second] = h.calls;
    await act(async () => { second.answer([{ id: "after", title: "after the write", labels: [] }]); });
    await act(async () => { first.answer([{ id: "before", title: "before the write", labels: [] }]); });
    await settle();
    expect(seen.data).toEqual([{ id: "after", title: "after the write", labels: [] }]);
  });
});
