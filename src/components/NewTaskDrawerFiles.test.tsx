// GA-57: files in the New task drawer. Its Files section only lists the picked or dropped files; Create task (or Ctrl+Enter)
// creates the task, then adds them with addFiles("task", id, paths), then opens it. A file that can't be added never undoes
// or repeats the task. While the drawer is open, every dropped file goes to it, never also to the Files section of the page
// behind it. There is no DOM here, so a small hook runtime stands in for React: components are called as functions and keep
// their hooks between renders by their place in the tree, effects run after each render and clean up when a component goes.
// Drops come in through the real lib/useDropZone and lib/drop, from a fake Tauri webview; the api, the file picker and the
// router are fakes that record what they are given.
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import type { DragDropEvent } from "@tauri-apps/api/webview";
import type { FileOwner, FileRow, Project, Team } from "../types";

type Box = { left: number; top: number; right: number; bottom: number };
type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
const h = vi.hoisted(() => ({
  cur: null as Inst | null,
  dirty: false,
  effects: [] as (() => void)[],
  calls: [] as [string, unknown[]][],
  answers: {} as Record<string, (...a: unknown[]) => unknown>,
  picks: [] as (string[] | string | null)[],
  dragDrop: null as null | ((e: { payload: DragDropEvent }) => void),
  scrolled: 0,
}));

vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const slot = <T,>(make: () => T): T => {
    const c = h.cur!;
    if (c.i === c.slots.length) c.slots.push(make());
    return c.slots[c.i++] as T;
  };
  const changed = (deps?: unknown[], prev?: unknown[]) => !deps || !prev || deps.length !== prev.length || deps.some((d, i) => !Object.is(d, prev[i]));
  const useState = ((init: unknown) => {
    if (!h.cur) return R.useState(init);
    const box = slot(() => {
      const b = { v: typeof init === "function" ? (init as () => unknown)() : init, set: (x: unknown) => {
        const v = typeof x === "function" ? (x as (p: unknown) => unknown)(b.v) : x;
        if (!Object.is(v, b.v)) { b.v = v; h.dirty = true; }
      } };
      return b;
    });
    return [box.v, box.set];
  }) as typeof R.useState;
  const useRef = ((init: unknown) => (h.cur ? slot(() => ({ current: init })) : R.useRef(init))) as typeof R.useRef;
  const useMemo = ((f: () => unknown, deps: unknown[]) => {
    if (!h.cur) return R.useMemo(f, deps);
    const s = slot(() => ({ deps: undefined as unknown[] | undefined, v: undefined as unknown }));
    if (changed(deps, s.deps)) { s.v = f(); s.deps = deps; }
    return s.v;
  }) as typeof R.useMemo;
  const useCallback = ((f: unknown, deps: unknown[]) => (h.cur ? useMemo(() => f, deps) : R.useCallback(f as () => void, deps))) as typeof R.useCallback;
  const useEffect = ((f: () => void | (() => void), deps?: unknown[]) => {
    if (!h.cur) return R.useEffect(f, deps);
    const s = slot<EffectSlot>(() => ({ effect: true, first: true }));
    if (s.first || changed(deps, s.deps)) {
      s.first = false;
      s.deps = deps;
      h.effects.push(() => { s.cleanup?.(); const c = f(); s.cleanup = typeof c === "function" ? c : undefined; });
    }
  }) as typeof R.useEffect;
  const hooks = { useState, useRef, useMemo, useCallback, useEffect, useLayoutEffect: useEffect };
  return { ...R, default: { ...R, ...hooks }, ...hooks };
});
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent: (cb: (e: { payload: DragDropEvent }) => void) => { h.dragDrop = cb; return Promise.resolve(() => {}); } }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (opts: unknown) => { h.calls.push(["open", [opts]]); return Promise.resolve(h.picks.length ? h.picks.shift() : null); },
}));
vi.mock("../router", async (orig) => ({ ...(await orig<Record<string, unknown>>()), go: (r: unknown) => { h.calls.push(["go", [r]]); } }));
vi.mock("./MarkdownEditor", () => ({ MarkdownEditor: () => null }));
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) {
    out[k] = typeof v !== "function" ? v : (...a: unknown[]) => { h.calls.push([k, a]); return Promise.resolve().then(() => h.answers[k]?.(...a)); };
  }
  return out;
});
vi.stubGlobal("window", { devicePixelRatio: 1, addEventListener: () => {}, removeEventListener: () => {} });

const { NewTaskDrawer } = await import("./NewTaskDrawer");
const { FileDrop } = await import("./FileDrop");

const PROJECT = { id: "p1", key: "KADE", name: "Kade portal", status: "active" } as Project;
const TEAM = { id: "team", name: "Team", members: [], labels: [],
  states: [{ id: "s-todo", name: "To do", category: "ready", sortKey: "a1" }, { id: "s-backlog", name: "Backlog", category: "backlog", sortKey: "a0" }] } as Team;
const row = (path: string): FileRow => ({ id: `f-${path}`, name: path.split("/").pop()!, sizeBytes: 10, sha256: "x", createdAt: 0 });
const defaults: Record<string, (...a: unknown[]) => unknown> = {
  listProjects: () => [PROJECT], getTeam: () => TEAM, listUsers: () => [], createTask: () => "t-new", listFiles: () => [],
  addFiles: (_t, _id, paths) => ({ added: (paths as string[]).map(row), failed: [] }), onRowsChanged: () => () => {},
};

// What is on screen: the page's Files section (task or project page) and the New task drawer over it.
const ui = { page: null as null | { type: FileOwner; id: string; readOnly?: boolean }, drawer: false, closed: 0 };
// Where the boxes are, in CSS pixels: the page's Files section on the left, the drawer's on the right.
const PAGE: Box = { left: 0, top: 0, right: 800, bottom: 600 };
const DRAWER: Box = { left: 900, top: 0, right: 1400, bottom: 600 };
function Root() {
  return <>
    {ui.page && <FileDrop ownerType={ui.page.type} ownerId={ui.page.id} readOnly={ui.page.readOnly} />}
    {ui.drawer && <NewTaskDrawer onClose={() => { ui.closed++; ui.drawer = false; }} />}
  </>;
}

// ---- the hook runtime ----
const insts = new Map<string, Inst>();
const unmount = (inst: Inst) => { for (const s of inst.slots) if ((s as EffectSlot)?.effect) (s as EffectSlot).cleanup?.(); };
const keyOf = (n: unknown) => (n && typeof n === "object" && "key" in n ? (n as { key: string | null }).key : null);
function expand(node: ReactNode, path: string, seen: Set<string>): ReactNode {
  if (Array.isArray(node)) return node.map((n, i) => expand(n, `${path}.${keyOf(n) ?? i}`, seen));
  if (!node || typeof node !== "object" || !("props" in node)) return node;
  const el = node as ReactElement<Record<string, unknown>>;
  if (typeof el.type === "function") {
    const key = `${path}>${el.type.name}`;
    seen.add(key);
    let inst = insts.get(key);
    if (!inst) insts.set(key, (inst = { slots: [], i: 0 }));
    inst.i = 0;
    h.cur = inst;
    let out: ReactNode;
    try { out = (el.type as (p: unknown) => ReactNode)(el.props); } finally { h.cur = null; }
    return expand(out, key, seen);
  }
  const p = el.props;
  // A ref on an element gets a stand-in with a box: the drawer's Files section, or the page's.
  if (p.ref && typeof p.ref === "object") {
    const box = path.includes("PendingFiles") ? DRAWER : PAGE;
    (p.ref as { current: unknown }).current = { getBoundingClientRect: () => box, scrollIntoView: () => { h.scrolled++; } };
  }
  return { ...el, props: { ...p, children: expand(p.children as ReactNode, `${path}>${String(el.type)}`, seen) } } as ReactNode;
}
let tree: ReactNode = null;
function render() {
  for (let n = 0; n < 30; n++) {
    h.dirty = false;
    const seen = new Set<string>();
    tree = expand(<Root />, "", seen);
    for (const [k, inst] of insts) if (!seen.has(k)) { unmount(inst); insts.delete(k); }
    for (const f of h.effects.splice(0)) f();
    if (!h.dirty) return;
  }
  throw new Error("the render never settled");
}
async function settle() {
  for (let i = 0; i < 3; i++) { await new Promise((r) => setTimeout(r, 0)); render(); }
}

// ---- finding things on screen ----
type Found = { type: unknown; props: Record<string, unknown> };
function findAll(node: ReactNode, pred: (f: Found) => boolean, out: Found[] = []): Found[] {
  if (Array.isArray(node)) node.forEach((n) => findAll(n, pred, out));
  else if (node && typeof node === "object" && "props" in node) {
    const el = node as ReactElement<Record<string, unknown>>;
    const f = { type: el.type, props: el.props };
    if (pred(f)) out.push(f);
    findAll(el.props.children as ReactNode, pred, out);
  }
  return out;
}
function textOf(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return ` ${node} `;
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (node && typeof node === "object" && "props" in node) return textOf((node as ReactElement<{ children?: ReactNode }>).props.children);
  return "";
}
const text = () => textOf(tree).replace(/\s+/g, " ").trim();
const strings = (p: Record<string, unknown>) => [p.children].flat(Infinity).filter((c) => typeof c === "string").join("");
type Btn = { onClick: (e?: unknown) => unknown; disabled?: boolean; type?: string; autoFocus?: boolean };
/** Buttons whose text or aria-label is `name`. */
const buttons = (name: string) => findAll(tree, (f) => f.type === "button" && (strings(f.props) === name || f.props["aria-label"] === name)).map((f) => f.props as Btn);
const button = (name: string) => {
  const all = buttons(name);
  if (all.length !== 1) throw new Error(`${all.length} buttons called ${name}`);
  return all[0];
};
const ev = { preventDefault: () => {}, stopPropagation: () => {} };
async function click(name: string) { await button(name).onClick(ev); await settle(); }
const form = () => findAll(tree, (f) => f.type === "form")[0].props as { inert?: boolean; onKeyDown: (e: unknown) => void };
const ctrlEnter = () => form().onKeyDown({ key: "Enter", ctrlKey: true, metaKey: false, ...ev });
const typeTitle = (v: string) => { (findAll(tree, (f) => f.props.id === "t-title")[0].props.onChange as (e: unknown) => void)({ target: { value: v } }); render(); };
/** The drawer's Files section and the page's: their class, and the names listed in them. */
const zones = () => findAll(tree, (f) => f.type === "div" && /^filedrop( |$)/.test(String(f.props.className ?? "")));
const listed = () => findAll(tree, (f) => f.type === "span" && f.props.className === "file pending").map((f) => textOf(f.props.children as ReactNode).trim().split(/\s+/)[1]);
const called = (name: string) => h.calls.filter(([k]) => k === name).map(([, a]) => a);
const order = () => h.calls.map(([k]) => k).filter((k) => ["createTask", "addFiles", "go"].includes(k));
function drag(type: DragDropEvent["type"], x: number, y: number, paths: string[] = []) {
  const payload = (type === "leave" ? { type } : type === "over" ? { type, position: { x, y } } : { type, paths, position: { x, y } }) as DragDropEvent;
  h.dragDrop!({ payload });
  render();
}
const defer = <T,>() => { let resolve!: (v: T) => void, reject!: (e: unknown) => void; const promise = new Promise<T>((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };

async function openDrawer(page: typeof ui.page = null) {
  ui.page = page;
  ui.drawer = true;
  render();
  await settle();
}
async function pick(...paths: string[]) { h.picks.push(paths); await click("Add files"); }

beforeEach(() => {
  for (const inst of insts.values()) unmount(inst);
  insts.clear();
  h.effects.length = 0;
  h.calls.length = 0;
  h.picks.length = 0;
  h.scrolled = 0;
  h.answers = { ...defaults };
  Object.assign(ui,{ page: null, drawer: false, closed: 0 });
  tree = null;
});

const SPEC = "/home/jef/Downloads/spec.pdf", SHOT = "/home/jef/shots/login.png", CSV = "/home/jef/exports/invoices.csv";

describe("the Files section of the New task drawer", () => {
  it("comes after Acceptance criteria, with 'Drop files here, or' and Add files, like the task page's", async () => {
    await openDrawer();
    const sections = findAll(tree, (f) => f.type === "h3").map((f) => strings(f.props));
    expect(sections).toEqual(["Task", "Description", "Acceptance criteria", "Files"]);
    expect(text()).toContain("They are added when you create the task.");
    expect(zones().map((z) => z.props.className)).toEqual(["filedrop"]);
    expect(text()).toContain("Drop files here, or Add files");
    // A button in the form that doesn't submit it.
    expect(button("Add files").type).toBe("button");
  });

  it("Add files opens the system picker for several files at once and lists each by name, with an X; nothing is copied yet", async () => {
    await openDrawer();
    await pick(SPEC, SHOT);
    expect(called("open")).toEqual([[{ multiple: true, directory: false, title: "Add files" }]]);
    expect(listed()).toEqual(["spec.pdf", "login.png"]);
    expect(text()).toContain("/home/jef/Downloads");
    expect(button("Remove spec.pdf").type).toBe("button");
    expect(button("Remove login.png").disabled).toBe(false);
    expect(text()).toContain("Drop more files here, or");
    expect(called("addFiles")).toEqual([]);
    expect(called("createTask")).toEqual([]);
  });

  it("lists a file picked twice once", async () => {
    await openDrawer();
    await pick(SPEC);
    await pick(SPEC, SHOT);
    expect(listed()).toEqual(["spec.pdf", "login.png"]);
  });

  it("adds nothing when the picker is closed without a choice", async () => {
    await openDrawer();
    h.picks.push(null);
    await click("Add files");
    expect(listed()).toEqual([]);
    expect(text()).toContain("Drop files here, or");
  });

  it("X takes a file off the list, without a confirm and without touching anything", async () => {
    await openDrawer();
    await pick(SPEC, SHOT, CSV);
    await click("Remove login.png");
    expect(listed()).toEqual(["spec.pdf", "invoices.csv"]);
    expect(called("removeFile")).toEqual([]);
    expect(called("addFiles")).toEqual([]);
  });

  it("lists files dropped on it, also the same file dropped twice once, and copies nothing", async () => {
    await openDrawer();
    drag("enter", 1000, 100, [SPEC]);
    expect(zones()[0].props.className).toBe("filedrop hover");
    expect(text()).toContain("Drop to add");
    expect(h.scrolled).toBeGreaterThan(0); // brought into view, so the drop shows
    drag("drop", 1000, 100, [SPEC]);
    drag("drop", 1000, 100, [SPEC, CSV]);
    expect(zones()[0].props.className).toBe("filedrop");
    expect(listed()).toEqual(["spec.pdf", "invoices.csv"]);
    expect(called("addFiles")).toEqual([]);
  });
});

describe("Create task with files", () => {
  it("creates the task, then adds the files left on the list to it, then closes the drawer and opens the task", async () => {
    await openDrawer();
    typeTitle("Export invoices");
    await pick(SPEC, SHOT, CSV);
    await click("Remove login.png");
    await click("Create task");
    expect(called("createTask")).toHaveLength(1);
    expect(called("createTask")[0][0]).toMatchObject({ projectId: "p1", title: "Export invoices" });
    expect(called("addFiles")).toEqual([["task", "t-new", [SPEC, CSV]]]);
    expect(order()).toEqual(["createTask", "addFiles", "go"]);
    expect(called("go")).toEqual([[{ page: "task", id: "t-new" }]]);
    expect(ui.closed).toBe(1);
  });

  it("does the same on Ctrl+Enter", async () => {
    await openDrawer();
    typeTitle("Export invoices");
    await pick(SPEC);
    ctrlEnter();
    await settle();
    expect(order()).toEqual(["createTask", "addFiles", "go"]);
    expect(called("addFiles")).toEqual([["task", "t-new", [SPEC]]]);
    expect(ui.closed).toBe(1);
  });

  it("without files, creates the task as before: no addFiles, then opens it", async () => {
    await openDrawer();
    typeTitle("  Export invoices ");
    await click("Create task");
    expect(called("createTask")).toEqual([[{ projectId: "p1", title: "Export invoices", stateId: "s-backlog", labelIds: [], priority: 0,
      assigneeId: null, descriptionMd: "", acceptanceMd: null, testing: true }]]);
    expect(called("addFiles")).toEqual([]);
    expect(called("go")).toEqual([[{ page: "task", id: "t-new" }]]);
    expect(ui.closed).toBe(1);
  });

  it("without files, Ctrl+Enter creates the task as before", async () => {
    await openDrawer();
    typeTitle("Export invoices");
    ctrlEnter();
    await settle();
    expect(order()).toEqual(["createTask", "go"]);
    expect(ui.closed).toBe(1);
  });

  it("files taken off again are not added", async () => {
    await openDrawer();
    typeTitle("Export invoices");
    await pick(SPEC);
    await click("Remove spec.pdf");
    await click("Create task");
    expect(order()).toEqual(["createTask", "go"]);
  });

  it("still needs a title: no task and no files added, and the files stay listed", async () => {
    await openDrawer();
    await pick(SPEC);
    await click("Create task");
    expect(text()).toContain("Give the task a title.");
    expect(called("createTask")).toEqual([]);
    expect(called("addFiles")).toEqual([]);
    expect(listed()).toEqual(["spec.pdf"]);
  });

  it("while the task is made and its files added: Create task is off, the bar says Adding…, and nothing can create it twice", async () => {
    const task = defer<string>(), files = defer<unknown>();
    h.answers.createTask = () => task.promise;
    h.answers.addFiles = () => files.promise;
    await openDrawer();
    typeTitle("Export invoices");
    await pick(SPEC);
    const create = button("Create task");
    void create.onClick(ev);
    ctrlEnter(); // a fast double Ctrl+Enter, or a click and Ctrl+Enter
    void create.onClick(ev);
    await settle();
    expect(button("Create task").disabled).toBe(true);
    expect(text()).toContain("Adding…");
    expect(button("Add files").disabled).toBe(true);
    expect(button("Remove spec.pdf").disabled).toBe(true);
    // A file dropped now changes nothing.
    drag("drop", 1000, 100, [CSV]);
    expect(listed()).toEqual(["spec.pdf"]);
    task.resolve("t-new");
    await settle();
    expect(called("addFiles")).toEqual([["task", "t-new", [SPEC]]]);
    files.resolve({ added: [row(SPEC)], failed: [] });
    await settle();
    expect(called("createTask")).toHaveLength(1);
    expect(called("addFiles")).toHaveLength(1);
    expect(called("go")).toEqual([[{ page: "task", id: "t-new" }]]);
  });

  it("when the task can't be made, adds no files, says why, and Create task can be pressed again", async () => {
    h.answers.createTask = () => Promise.reject("the project is archived");
    await openDrawer();
    typeTitle("Export invoices");
    await pick(SPEC);
    await click("Create task");
    expect(text()).toContain("the project is archived");
    expect(called("addFiles")).toEqual([]);
    expect(listed()).toEqual(["spec.pdf"]);
    expect(button("Create task").disabled).toBe(false);
    h.answers.createTask = () => "t-new";
    await click("Create task");
    expect(order()).toEqual(["createTask", "createTask", "addFiles", "go"]);
    expect(called("addFiles")).toEqual([["task", "t-new", [SPEC]]]);
  });
});

describe("a file that can't be added", () => {
  const FAILED = ["setup.iso is larger than 1 GB", "shots is a folder, not a file"];
  async function createWithFailures() {
    h.answers.addFiles = () => ({ added: [row(SPEC)], failed: FAILED });
    await openDrawer();
    typeTitle("Export invoices");
    await pick(SPEC, "/home/jef/setup.iso", "/home/jef/shots");
    const create = button("Create task");
    await create.onClick(ev);
    await settle();
    return create;
  }

  it("keeps the drawer open and says the task was made and which files weren't added, and why", async () => {
    await createWithFailures();
    expect(ui.closed).toBe(0);
    expect(called("go")).toEqual([]);
    expect(findAll(tree, (f) => f.props.role === "alert").map((f) => textOf(f.props.children as ReactNode).trim()))
      .toEqual(["Task created. Not added: setup.iso is larger than 1 GB; shots is a folder, not a file"]);
  });

  it("offers Open task and Close, and Create task is gone", async () => {
    await createWithFailures();
    expect(buttons("Create task")).toEqual([]);
    expect(buttons("Cancel")).toEqual([]);
    expect(button("Open task").autoFocus).toBe(true);
    const foot = findAll(tree, (f) => f.props.className === "drawer-foot")[0];
    expect(textOf(foot.props.children as ReactNode)).not.toContain("Ctrl+Enter creates");
    expect(findAll(foot.props.children as ReactNode, (f) => f.type === "button").map((f) => strings(f.props))).toEqual(["Close", "Open task"]);
  });

  it("can't create the task again: not with Ctrl+Enter, nor the old Create task button", async () => {
    const create = await createWithFailures();
    ctrlEnter();
    await create.onClick(ev);
    await settle();
    expect(called("createTask")).toHaveLength(1);
    expect(called("addFiles")).toHaveLength(1);
    expect(text()).toContain("Task created. Not added:");
  });

  it("leaves the form to look at only: it is inert, Add files and the X buttons are off, and drops change nothing", async () => {
    await createWithFailures();
    expect(form().inert).toBe(true);
    expect(button("Add files").disabled).toBe(true);
    expect(button("Remove spec.pdf").disabled).toBe(true);
    drag("drop", 1000, 100, [CSV]);
    expect(listed()).toEqual(["spec.pdf", "setup.iso", "shots"]);
    expect(called("addFiles")).toHaveLength(1);
  });

  it("Open task opens the task once and closes the drawer", async () => {
    await createWithFailures();
    await click("Open task");
    expect(called("go")).toEqual([[{ page: "task", id: "t-new" }]]);
    expect(ui.closed).toBe(1);
    expect(called("createTask")).toHaveLength(1);
  });

  it("Close closes the drawer without asking to discard anything", async () => {
    await createWithFailures();
    const foot = findAll(tree, (f) => f.props.className === "drawer-foot")[0];
    const close = findAll(foot.props.children as ReactNode, (f) => f.type === "button" && strings(f.props) === "Close")[0].props as Btn;
    await close.onClick(ev);
    await settle();
    expect(ui.closed).toBe(1);
    expect(called("go")).toEqual([]);
  });

  it("when adding the files fails as a whole, the task still exists once and the drawer names the files and the error", async () => {
    h.answers.addFiles = () => Promise.reject("database is locked");
    await openDrawer();
    typeTitle("Export invoices");
    await pick(SPEC, SHOT);
    await click("Create task");
    expect(text()).toContain("Task created. Not added: spec.pdf, login.png: database is locked");
    expect(buttons("Create task")).toEqual([]);
    expect(button("Open task")).toBeTruthy();
    expect(called("createTask")).toHaveLength(1);
    expect(ui.closed).toBe(0);
  });
});

describe("drops while the New task drawer is open", () => {
  for (const type of ["task", "project"] as const) {
    it(`a drop on the ${type} page's Files section behind it goes only to the drawer`, async () => {
      await openDrawer({ type, id: `${type}-1` });
      expect(zones()).toHaveLength(2);
      drag("enter", 100, 100, [SPEC]);
      expect(zones().map((z) => z.props.className)).toEqual(["filedrop", "filedrop hover"]);
      drag("over", 400, 300);
      drag("drop", 400, 300, [SPEC, SHOT]);
      await settle();
      expect(listed()).toEqual(["spec.pdf", "login.png"]);
      expect(called("addFiles")).toEqual([]);
      expect(zones().map((z) => z.props.className)).toEqual(["filedrop", "filedrop"]);
    });
  }

  it("a drop on the drawer's own Files section goes to the drawer only", async () => {
    await openDrawer({ type: "task", id: "task-1" });
    drag("drop", 1000, 100, [SPEC]);
    await settle();
    expect(listed()).toEqual(["spec.pdf"]);
    expect(called("addFiles")).toEqual([]);
  });

  it("a drop on an archived task's page (no drop zone) still goes to the drawer", async () => {
    await openDrawer({ type: "task", id: "task-1", readOnly: true });
    drag("drop", 100, 100, [SPEC]);
    await settle();
    expect(listed()).toEqual(["spec.pdf"]);
    expect(called("addFiles")).toEqual([]);
  });

  it("with no open project, the drawer has no Files section, yet a drop still doesn't reach the page behind it", async () => {
    h.answers.listProjects = () => [];
    await openDrawer({ type: "project", id: "project-1" });
    expect(text()).toContain("No project yet.");
    expect(zones()).toHaveLength(1);
    drag("enter", 100, 100, [SPEC]);
    expect(zones()[0].props.className).toBe("filedrop");
    drag("drop", 100, 100, [SPEC]);
    await settle();
    expect(called("addFiles")).toEqual([]);
  });

  it("after the drawer closes, the page's Files section takes drops again", async () => {
    await openDrawer({ type: "task", id: "task-1" });
    drag("drop", 100, 100, [SPEC]);
    ui.drawer = false;
    render();
    expect(zones()).toHaveLength(1);
    drag("drop", 100, 100, [SHOT]);
    await settle();
    expect(called("addFiles")).toEqual([["task", "task-1", [SHOT]]]);
  });
});

describe("the Files sections on the task and project pages, without a drawer", () => {
  for (const type of ["task", "project"] as const) {
    it(`a ${type} page's Files section adds the files dropped on it, or anywhere when it is the only one`, async () => {
      ui.page = { type, id: `${type}-1` };
      render();
      await settle();
      drag("enter", 100, 100, [SPEC]);
      expect(zones()[0].props.className).toBe("filedrop hover");
      drag("drop", 100, 100, [SPEC]);
      await settle();
      drag("drop", 1000, 100, [SHOT]); // outside its box: the only zone on screen
      await settle();
      expect(called("addFiles")).toEqual([[type, `${type}-1`, [SPEC]], [type, `${type}-1`, [SHOT]]]);
      expect(zones()[0].props.className).toBe("filedrop");
    });
  }

  it("a task page's Add files adds the picked files at once", async () => {
    ui.page = { type: "task", id: "task-1" };
    render();
    await settle();
    h.picks.push([SPEC, SHOT]);
    await click("Add files");
    await settle();
    expect(called("open")).toEqual([[{ multiple: true, directory: false, title: "Add files" }]]);
    expect(called("addFiles")).toEqual([["task", "task-1", [SPEC, SHOT]]]);
  });

  it("an archived task's Files section takes no drops", async () => {
    ui.page = { type: "task", id: "task-1", readOnly: true };
    render();
    await settle();
    drag("drop", 100, 100, [SPEC]);
    await settle();
    expect(called("addFiles")).toEqual([]);
  });

  it("the task's Files section shows the files added to it", async () => {
    h.answers.listFiles = (_t, id) => (id === "t-new" ? [row(SPEC), row(CSV)] : []);
    ui.page = { type: "task", id: "t-new" };
    render();
    await settle();
    expect(called("listFiles")).toEqual([["task", "t-new"]]);
    expect(findAll(tree, (f) => f.type === "b").map((f) => strings(f.props))).toEqual(["spec.pdf", "invoices.csv"]);
  });
});
