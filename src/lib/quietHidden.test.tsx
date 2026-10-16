// GA-190 QA: quiet while nobody looks, with the sidebar's Editor section and the Editor page mounted together (as in the
// app), the run panel's clock, and the live runs' and the chat's reloads. A fake of contract C3 records each call
// ("git <folder>", "main <folder>", "versions <folder>", ...); its answers can take a while (`latency`), as editor_main's
// 15 to 30 git processes do, so an ask can be on its way when another comes. The document says whether the window is
// hidden (with visibilitychange) and whether it has the focus. There is no DOM: a small hook runtime stands in for React
// (as in SidebarBehind.test.tsx and EditorPageBehind.test.tsx): components are called as functions, keep their hooks by
// their place in the tree, and effects run after each render.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import type { EditorState, Transaction, TransactionSpec } from "@codemirror/state";
import type { DirEntry, EditorFolder, FolderGit, FolderMain, LiveRun, OpenedFile, Run, Task, Team } from "../types";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
const h = vi.hoisted(() => ({ cur: null as Inst | null, dirty: false, effects: [] as (() => void)[] }));

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
  const useSyncExternalStore = ((sub: (f: () => void) => () => void, get: () => unknown, server?: () => unknown) =>
    (h.cur ? get() : R.useSyncExternalStore(sub, get, server))) as typeof R.useSyncExternalStore;
  const hooks = { useState, useRef, useMemo, useCallback, useEffect, useLayoutEffect: useEffect, useSyncExternalStore };
  return { ...R, default: { ...R, ...hooks }, ...hooks };
});
vi.mock("react-dom", async (orig) => ({ ...(await orig<object>()), createPortal: (children: ReactNode) => children }));
vi.mock("@codemirror/view", async (orig) => {
  const real = await orig<typeof import("@codemirror/view")>();
  const { EditorState: State, Transaction: Tr } = await import("@codemirror/state");
  class View {
    state = State.create({ doc: "" });
    setState(s: EditorState) { this.state = s; }
    dispatch(...specs: (Transaction | TransactionSpec)[]) {
      const tr = specs[0] instanceof Tr ? specs[0] : this.state.update(...(specs as TransactionSpec[]));
      this.state = tr.state;
    }
    focus() {}
    destroy() {}
  }
  function EditorView() { return new View(); }
  Object.setPrototypeOf(EditorView, real.EditorView);
  return { ...real, EditorView };
});
vi.mock("../components/editor/FolderSearch", () => ({ FolderSearch: () => null }));
vi.mock("../components/editor/GoToFile", () => ({ GoToFile: () => null }));
vi.mock("../components/editor/FolderPicker", async (orig) => ({ ...(await orig<object>()), FolderPicker: () => null }));
vi.mock("../components/editor/Menus", async (orig) => ({ ...(await orig<object>()), OpenInMenu: () => null }));
vi.mock("../components/UpdateNotice", () => ({ UpdateNotice: () => null }));
vi.mock("../router", async (orig) => ({ ...(await orig<object>()), go: () => {} }));

// ---- two folders under ~/Herd; contract C3 as a fake that records each call ----
const NOW = Date.UTC(2026, 9, 16, 12, 0, 0);
const MIN = 60_000;
const KADE = "/home/you/Herd/kade-portal";
const API = "/home/you/Herd/invoice-api";
const folderOf = (id: string, name: string): EditorFolder => ({ id, path: id, name, kind: "root", projectId: null, taskId: null, readOnly: false, readOnlyWhy: null });
const FOLDERS = [folderOf(KADE, "kade-portal"), folderOf(API, "invoice-api")];
const state = vi.hoisted(() => ({
  data: {} as Record<string, unknown>, live: [] as unknown[], calls: [] as string[], latency: 0,
  runsChanged: [] as (() => void)[], chatChanged: [] as (() => void)[],
}));
const main = (more: Partial<FolderMain> = {}): FolderMain => ({
  mainLabel: "origin/main", fetchedAt: NOW - 2 * MIN, fetchError: null, branch: "main", onDefault: true, behind: 3, ahead: 0, uncommitted: 0,
  canUpdate: true, updateSays: "move main 3 commits to 319bf0b and run npm ci", updating: false, ...more,
});
const README: DirEntry = { name: "README.md", path: "README.md", dir: false, size: 14, ignored: false, git: null };

vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) if (typeof real[k] === "function") out[k] = () => new Promise(() => {});
  const as = (key: string) => () => { const p = { key, then: () => p, catch: () => p }; return p; };
  /** The answer, after `latency` ms (git takes a while), or at once. */
  const later = <T,>(v: () => T) => (state.latency ? new Promise<T>((ok) => setTimeout(() => ok(v()), state.latency)) : Promise.resolve().then(v));
  const call = (what: string) => { state.calls.push(what); };
  return {
    ...out,
    listProjects: as("projects"), listTasks: as("tasks"), getTeam: as("team"), memoryNotes: as("notes"), listChatThreads: as("threads"), listRuns: as("runs"),
    editorFolders: () => { call("folders"); return later(() => FOLDERS); },
    editorApps: () => Promise.resolve([]),
    editorGit: (id: string) => { call(`git ${id}`); return later((): FolderGit => ({ branch: "main", files: {}, uncommitted: 0 })); },
    editorMain: (id: string) => { call(`main ${id}`); return later(() => main()); },
    editorFetch: (id: string) => { call(`fetch ${id}`); return later(() => main({ fetchedAt: Date.now() })); },
    editorList: (id: string) => { call(`list ${id}`); return later(() => [README]); },
    editorVersions: (id: string, paths: string[]) => { call(`versions ${id}`); return later(() => paths.map(() => "v1")); },
    editorRead: (id: string, path: string) => {
      call(`read ${id}`);
      return later((): OpenedFile => ({ path, text: "# Kade portal\n", binary: false, tooLarge: false, readOnly: false, readOnlyWhy: null, crlf: false, bom: false, version: "v1" }));
    },
    onEditorUpdated: () => Promise.resolve(() => {}),
    liveRuns: () => { call("liveRuns"); return Promise.resolve([]); },
    onRunsChanged: (cb: () => void) => { state.runsChanged.push(cb); return Promise.resolve(() => { state.runsChanged.splice(state.runsChanged.indexOf(cb), 1); }); },
    chatLive: () => { call("chatLive"); return Promise.resolve([]); },
    onChatChanged: (cb: () => void) => { state.chatChanged.push(cb); return Promise.resolve(() => { state.chatChanged.splice(state.chatChanged.indexOf(cb), 1); }); },
  };
});
vi.mock("./useData", async (orig) => ({
  ...(await orig<object>()),
  useData: (fetch: () => { key?: string }) => ({ data: state.data[fetch()?.key ?? ""] ?? null, error: null, reload: () => {}, setData: () => {} }),
  onWrites: () => () => {},
}));
vi.mock("./team", async (orig) => ({ ...(await orig<object>()), useCurrentTeam: () => ["t1", () => {}] }));
vi.mock("./useLiveRuns", () => ({ useLiveRuns: () => state.live }));
vi.mock("./drawers", async (orig) => ({ ...(await orig<object>()), useDrawer: () => () => {} }));
vi.mock("../components/chat/useChat", async (orig) => ({ ...(await orig<object>()), useChatLive: () => [] }));

// ---- the browser bits: the window, and a document that can be hidden and lose the focus ----
const store = new Map<string, string>();
vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, String(v)); }, removeItem: (k: string) => { store.delete(k); } });
vi.stubGlobal("navigator", { userAgent: "Linux", platform: "Linux x86_64" });
const win = new EventTarget();
vi.stubGlobal("window", win);
const docEvents = new EventTarget();
let focused = true;
const doc = {
  hidden: false, hasFocus: () => focused, querySelector: () => null, body: { style: {} },
  documentElement: { style: {}, setAttribute: () => {}, removeAttribute: () => {} },
  addEventListener: docEvents.addEventListener.bind(docEvents), removeEventListener: docEvents.removeEventListener.bind(docEvents),
};
vi.stubGlobal("document", doc);
vi.stubGlobal("requestAnimationFrame", (f: () => void) => setTimeout(f, 0));
/** The window is hidden (closed to the tray) or shows again: WebKitGTK's visibilitychange. */
const setHidden = (on: boolean) => { doc.hidden = on; docEvents.dispatchEvent(new Event("visibilitychange")); };
const focus = () => { focused = true; win.dispatchEvent(new Event("focus")); };
const blur = () => { focused = false; win.dispatchEvent(new Event("blur")); };

const { Sidebar } = await import("../components/Sidebar");
const { EditorPage } = await import("../pages/EditorPage");
const { RunPanel } = await import("../components/RunPanel");

// ---- the hook runtime ----
let root: () => ReactNode = () => null;
const insts = new Map<string, Inst>();
const unmount = (inst: Inst) => { for (const s of inst.slots) if ((s as EffectSlot)?.effect) (s as EffectSlot).cleanup?.(); };
const HOST = { tagName: "DIV" };
function expand(node: ReactNode, path: string, seen: Set<string>): ReactNode {
  if (Array.isArray(node)) return node.map((n, i) => expand(n, `${path}.${(n as { key?: string | null })?.key ?? i}`, seen));
  if (!node || typeof node !== "object" || !("props" in node)) return node;
  const el = node as ReactElement<Record<string, unknown>>;
  if (typeof el.type === "function") {
    const key = `${path}>${el.type.name}${el.key !== null ? `#${el.key}` : ""}`;
    seen.add(key);
    let inst = insts.get(key);
    if (!inst) insts.set(key, (inst = { slots: [], i: 0 }));
    inst.i = 0;
    h.cur = inst;
    let out: ReactNode;
    try { out = (el.type as (p: unknown) => ReactNode)(el.props); } finally { h.cur = null; }
    return expand(out, key, seen);
  }
  const ref = el.props.ref as { current: unknown } | undefined;
  if (el.props.className === "ed-host" && ref && typeof ref === "object" && !ref.current) ref.current = HOST;
  return { ...el, props: { ...el.props, children: expand(el.props.children as ReactNode, `${path}>${String(el.type)}`, seen) } } as ReactNode;
}
let tree: ReactNode = null;
function render() {
  for (let n = 0; n < 40; n++) {
    h.dirty = false;
    const seen = new Set<string>();
    tree = expand(root(), "", seen);
    for (const [k, inst] of insts) if (!seen.has(k)) { unmount(inst); insts.delete(k); }
    for (const f of h.effects.splice(0)) f();
    if (!h.dirty) return;
  }
  throw new Error("the render never settled");
}
async function settle() { for (let i = 0; i < 4; i++) { await vi.advanceTimersByTimeAsync(0); render(); } }
async function wait(ms: number) { await vi.advanceTimersByTimeAsync(ms); render(); await settle(); }

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
  if (typeof node === "string" || typeof node === "number") return `${node}`;
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (node && typeof node === "object" && "props" in node) return textOf((node as ReactElement<{ children?: ReactNode }>).props.children);
  return "";
}

// ---- what was asked ----
const calls = state.calls;
/** How many calls of each kind came since `n`: { "main /home/you/Herd/kade-portal": 1, ... }. */
function countSince(n: number): Record<string, number> {
  const out: Record<string, number> = {};
  for (const c of calls.slice(n)) out[c] = (out[c] ?? 0) + 1;
  return out;
}
const GIT_KINDS = /^(folders|git|main|fetch|versions|list|read)\b/;
/** The calls since `n` that start git or read the folder: none of them should come while nobody looks. */
const gitSince = (n: number) => calls.slice(n).filter((c) => GIT_KINDS.test(c));
const n = () => calls.length;

const sidebar = () => <Sidebar key="side" route={{ page: "editor", id: KADE } as never} youId="u1" onSearch={() => {}} onNewTask={() => {}} />;
const editor = () => <EditorPage key={KADE} id={KADE} youId="u1" />;
/** Both mounted at once, as when Gizai opens on #/editor/<kade-portal>. */
async function both() { root = () => [sidebar(), editor()]; render(); await settle(); }

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0;
  store.clear(); calls.length = 0; state.latency = 0; state.live = []; state.runsChanged.length = 0; state.chatChanged.length = 0;
  doc.hidden = false; focused = true;
  state.data = { projects: [], tasks: [], threads: [], notes: [], runs: [], team: { id: "t1", name: "Software", members: [], states: [], labels: [] } };
  store.set("gizai.editor.opened", JSON.stringify([KADE, API]));
  // README.md is open in kade-portal, so the page checks its version every 3 seconds
  store.set("gizai.editor.tabs", JSON.stringify({ [KADE]: { paths: ["README.md"], active: "README.md" } }));
});
// The answers still on their way settle before the fake clock goes: askMain/askGit keep an ask on its way per folder for
// the whole module (src/lib/behind.ts), so one that never settled would answer the next test's asks.
afterEach(async () => { root = () => null; render(); await vi.advanceTimersByTimeAsync(5000); vi.useRealTimers(); });

describe("GA-190 criterion: with the window hidden, no git calls and no reloads happen", () => {
  it("the sidebar's Editor section and the Editor page, hidden for 10 minutes: no editor_git, editor_main, editor_versions or editor_folders", async () => {
    await both();
    // in sight, they do ask: the page's 3-second look at the open file's version, its 10-second refresh
    let at = n();
    await wait(10_000);
    expect(countSince(at)).toMatchObject({ [`versions ${KADE}`]: 3, [`git ${KADE}`]: 1, [`main ${KADE}`]: 1, folders: 1 });
    at = n();
    setHidden(true);
    await wait(10 * MIN);
    expect(gitSince(at)).toEqual([]);
  });

  it("hidden from the start (opened from the tray, then hidden): nothing after the first load", async () => {
    await both();
    const at = n();
    doc.hidden = true; // no visibilitychange needed: the timers look at document.hidden
    await wait(5 * MIN);
    expect(gitSince(at)).toEqual([]);
  });

  it("shown but without the focus (another Hyprland workspace, another window in front): the timers make no git calls either", async () => {
    await both();
    const at = n();
    blur();
    await wait(10 * MIN);
    expect(gitSince(at)).toEqual([]);
    // the focus back asks at once
    focus();
    await settle();
    expect(countSince(at)).toMatchObject({ [`git ${KADE}`]: 1, [`git ${API}`]: 1, [`main ${KADE}`]: 1, [`main ${API}`]: 1, [`versions ${KADE}`]: 1 });
  });

  it("the sidebar alone, hidden for 10 minutes: nothing", async () => {
    root = () => sidebar();
    render();
    await settle();
    const at = n();
    setHidden(true);
    await wait(10 * MIN);
    expect(gitSince(at)).toEqual([]);
  });

  it("the Editor page alone, hidden for 30 minutes: no fetch either (the 10-minute fetch waits)", async () => {
    root = () => editor();
    render();
    await settle();
    const at = n();
    setHidden(true);
    await wait(30 * MIN);
    expect(gitSince(at)).toEqual([]);
  });
});

describe("GA-190 criterion: the sidebar's timer asks no editor_main; with the Editor page, editor_main once per folder, not twice", () => {
  it("the sidebar alone, in use for 5 minutes: editor_git 10 times per folder, editor_main not once", async () => {
    root = () => sidebar();
    render();
    await settle();
    expect(countSince(0)).toMatchObject({ [`main ${KADE}`]: 1, [`main ${API}`]: 1 }); // when the folders open
    const at = n();
    await wait(5 * MIN);
    const c = countSince(at);
    expect(c[`main ${KADE}`] ?? 0).toBe(0);
    expect(c[`main ${API}`] ?? 0).toBe(0);
    expect(c[`git ${KADE}`]).toBe(10);
    expect(c[`git ${API}`]).toBe(10);
  });

  it("both open at once (Gizai opens on the Editor): one editor_main and one editor_git per folder", async () => {
    state.latency = 150; // editor_main's git processes take a while
    await both();
    await wait(500);
    const c = countSince(0);
    expect([c[`main ${KADE}`], c[`main ${API}`]]).toEqual([1, 1]);
    expect([c[`git ${KADE}`], c[`git ${API}`]]).toEqual([1, 1]);
    expect(c[`fetch ${KADE}`]).toBe(1); // the page's fetch on opening, apart from editor_main
  });

  it("opening a folder in the Editor with the sidebar showing: one editor_main per folder", async () => {
    state.latency = 150;
    store.set("gizai.editor.opened", JSON.stringify([API]));
    root = () => sidebar();
    render();
    await wait(1000);
    expect(countSince(0)).toMatchObject({ [`main ${API}`]: 1, [`git ${API}`]: 1 });
    await wait(MIN);
    const at = n();
    root = () => [sidebar(), editor()]; // the Editor opens kade-portal: it joins the sidebar's list
    render();
    await wait(500);
    const c = countSince(at);
    expect(JSON.parse(store.get("gizai.editor.opened")!)).toEqual([API, KADE]);
    // kade-portal: the page's ask and the sidebar's are one; invoice-api: the sidebar's, as its list changed
    expect([c[`main ${KADE}`], c[`main ${API}`]]).toEqual([1, 1]);
    expect([c[`git ${KADE}`], c[`git ${API}`]]).toEqual([1, 1]);
  });

  it("a window focus with both mounted: one editor_main and one editor_git per folder", async () => {
    state.latency = 150;
    await both();
    await wait(6000);
    const at = n();
    focus();
    await wait(500);
    const c = countSince(at);
    expect([c[`main ${KADE}`], c[`main ${API}`]]).toEqual([1, 1]);
    expect([c[`git ${KADE}`], c[`git ${API}`]]).toEqual([1, 1]);
  });

  it("both mounted for 5 minutes in use: the sidebar adds no editor_main; its 30-second editor_git shares the page's", async () => {
    state.latency = 150;
    await both();
    await wait(1000);
    const at = n();
    await wait(5 * MIN - 1000);
    const c = countSince(at);
    // kade-portal: the page's own 10-second look (unchanged by GA-190), 30 in 5 minutes; invoice-api: none
    expect(c[`main ${KADE}`]).toBe(30);
    expect(c[`main ${API}`] ?? 0).toBe(0);
    // git: the page's 10-second refresh for kade-portal (the sidebar's every 30 s is the same ask); invoice-api: the sidebar's
    expect(c[`git ${KADE}`]).toBe(30);
    expect(c[`git ${API}`]).toBe(10);
  });
});

describe("GA-190 criterion: when the window shows again, everything shows the current state at once", () => {
  it("the sidebar asks git and main, and the Editor page refreshes, as the window shows (no focus event needed), once per folder", async () => {
    await both();
    await wait(1000);
    setHidden(true);
    await wait(5 * MIN);
    const at = n();
    setHidden(false);
    await settle();
    const c = countSince(at);
    expect(c).toMatchObject({ [`git ${KADE}`]: 1, [`git ${API}`]: 1, [`main ${KADE}`]: 1, [`main ${API}`]: 1, [`versions ${KADE}`]: 1, [`list ${KADE}`]: 1 });
    expect(c[`fetch ${KADE}`] ?? 0).toBe(0); // fetched 5 minutes ago: not due
    // the focus that comes with the window, a moment later, asks nothing more
    const after = n();
    await wait(1000);
    focus();
    await settle();
    expect(gitSince(after)).toEqual([]);
  });

  it("hidden for 20 minutes: as the window shows, the page fetches main (due) and the sidebar asks editor_main once per folder", async () => {
    await both();
    await wait(1000);
    setHidden(true);
    await wait(20 * MIN);
    const at = n();
    setHidden(false);
    await settle();
    const c = countSince(at);
    expect(c[`fetch ${KADE}`]).toBe(1);
    expect([c[`main ${KADE}`], c[`main ${API}`], c[`git ${KADE}`], c[`git ${API}`]]).toEqual([1, 1, 1, 1]);
  });

  it("the run panel's clock rests while hidden and shows the time at once when the window shows", async () => {
    const run = { id: "r1", agentId: "a1", agentName: "Frontend Agent", taskId: "t9", trigger: "manual", status: "running", createdAt: NOW - 20_000,
      startedAt: NOW - 10_000, costUsdMicros: 0, inputTokens: 0, outputTokens: 0, logPath: "", sessionId: null } as Run;
    state.live = [{ runId: "r1", taskId: "t9", agentId: "a1" } satisfies LiveRun];
    state.data.runs = [run];
    const task = { id: "t9", identifier: "GA-9", title: "x", stateId: "s1", assigneeId: null } as unknown as Task;
    const team = { id: "t1", name: "Software", members: [], states: [], labels: [] } as unknown as Team;
    root = () => <RunPanel task={task} team={team} />;
    render();
    await settle();
    const clock = () => {
      const right = findAll(tree, (f) => f.props.className === "right")[0];
      if (!right) throw new Error("no live run head");
      return textOf((right.props.children as ReactNode[])[0]);
    };
    expect(clock()).toBe("10s");
    await wait(1000);
    expect(clock()).toBe("11s");
    setHidden(true);
    await wait(2 * MIN);
    expect(clock()).toBe("11s"); // it rested
    setHidden(false);
    render();
    expect(clock()).toBe("2m 11s"); // at once, not a second later
    await wait(1000);
    expect(clock()).toBe("2m 12s");
  });

  it("useLiveRuns: events while hidden reload once when the window shows; none if no event came", async () => {
    const { useLiveRuns } = await vi.importActual<typeof import("./useLiveRuns")>("./useLiveRuns");
    function Live() { useLiveRuns(); return null; }
    root = () => <Live />;
    render();
    await settle();
    const loads = () => calls.filter((c) => c === "liveRuns").length;
    expect(loads()).toBe(1);
    expect(state.runsChanged.length).toBe(1);
    state.runsChanged[0]!(); // shown: at once
    expect(loads()).toBe(2);
    setHidden(true);
    for (let i = 0; i < 12; i++) state.runsChanged[0]!();
    expect(loads()).toBe(2);
    setHidden(false);
    expect(loads()).toBe(3);
    setHidden(true);
    setHidden(false);
    expect(loads()).toBe(3);
    // gone: a hidden event never reloads it
    setHidden(true);
    state.runsChanged[0]?.();
    root = () => null;
    render();
    setHidden(false);
    expect(loads()).toBe(3);
  });

  it("useChatLive: the same, once when the window shows", async () => {
    const { useChatLive } = await vi.importActual<typeof import("../components/chat/useChat")>("../components/chat/useChat");
    function Chat() { useChatLive(); return null; }
    root = () => <Chat />;
    render();
    await settle();
    const loads = () => calls.filter((c) => c === "chatLive").length;
    expect(loads()).toBe(1);
    setHidden(true);
    for (let i = 0; i < 7; i++) state.chatChanged[0]!();
    expect(loads()).toBe(1);
    setHidden(false);
    expect(loads()).toBe(2);
    state.chatChanged[0]!();
    expect(loads()).toBe(3);
  });
});
