// GA-113 QA: the Editor page (#/editor/<folder>) against the card's acceptance criteria. A folder under a test root is
// opened on a fake disk that behaves like contract C2 (each file has a version, a save with an old version is refused with
// "changed on disk", git's status follows the text), a file is opened from the tree, typed in and saved with Ctrl+S; files
// change "outside" and the 3-second poll is run with fake timers; search in the folder and Go to file hand the page a hit
// or a path; read-only files and folders are opened. There is no DOM here, so a small hook runtime stands in for React
// (as in DocPageSave.test.tsx): components are called as functions and keep their hooks by their place in the tree, refs
// to the code's host get a stand-in element, effects run after each render. The CodeMirror view is a stand-in that keeps
// the real EditorState the page gives it and tells the page's update listener what each transaction did, as the real
// view does. Search in the folder and Go to file are stand-ins that keep their props (EditorParts tests test them).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { EditorState, Transaction, type TransactionSpec } from "@codemirror/state";
import type { DirEntry, EditorFolder, FolderGit, GitMark, OpenedFile, SearchHit, Task, TerminalSession } from "../types";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
type Update = { state: EditorState; startState: EditorState; docChanged: boolean; selectionSet: boolean; transactions: Transaction[]; view: unknown };
const h = vi.hoisted(() => ({
  cur: null as Inst | null,
  dirty: false,
  effects: [] as (() => void)[],
  views: [] as unknown[],
  search: null as null | { onOpen: (hit: unknown) => void; folder: string },
  goto: null as null | { onOpen: (path: string) => void; folder: string },
  calls: [] as string[],
  /** TF0's store as the terminal panel reads it, and the props of the terminal's view (GA-137 QA) */
  terms: [] as TerminalSession[],
  termView: null as null | { id: string; place: unknown; autoFocus?: boolean },
  /** the terminals placed in the folder's panel (Show in Editor), one id per line, as useShownHere gives them */
  placed: "",
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
  // a store read through useSyncExternalStore (the terminal panel reads TF0's, GA-137): its snapshot as it is now
  const useSyncExternalStore = ((sub: (f: () => void) => () => void, get: () => unknown, server?: () => unknown) =>
    (h.cur ? get() : R.useSyncExternalStore(sub, get, server))) as typeof R.useSyncExternalStore;
  const hooks = { useState, useRef, useMemo, useCallback, useEffect, useLayoutEffect: useEffect, useSyncExternalStore };
  return { ...R, default: { ...R, ...hooks }, ...hooks };
});

// The code view: the real EditorView's statics (its facets and theme) with a stand-in instance that keeps the state and
// tells the update listeners (the page's) about each transaction.
vi.mock("@codemirror/view", async (orig) => {
  const real = await orig<typeof import("@codemirror/view")>();
  const { EditorState: State, Transaction: Tr } = await import("@codemirror/state");
  class View {
    state = State.create({ doc: "" });
    focused = 0;
    setState(s: EditorState) { this.state = s; }
    dispatch(...specs: (Transaction | TransactionSpec)[]) {
      const tr = specs[0] instanceof Tr ? specs[0] : this.state.update(...(specs as TransactionSpec[]));
      this.state = tr.state;
      const u: Update = { state: tr.state, startState: tr.startState, docChanged: tr.docChanged, selectionSet: !!tr.selection, transactions: [tr], view: this };
      for (const l of tr.state.facet(real.EditorView.updateListener)) l(u as never);
    }
    focus() { this.focused++; }
    destroy() {}
  }
  function EditorView() { const v = new View(); h.views.push(v); return v; }
  Object.setPrototypeOf(EditorView, real.EditorView);
  return { ...real, EditorView };
});
vi.mock("@codemirror/search", async (orig) => ({ ...(await orig<object>()), openSearchPanel: () => { h.calls.push("find"); return true; }, gotoLine: () => true }));
vi.mock("../components/editor/FolderSearch", () => ({ FolderSearch: (p: { onOpen: (hit: unknown) => void; folder: string }) => { h.search = p; return null; } }));
vi.mock("../components/editor/GoToFile", () => ({ GoToFile: (p: { onOpen: (path: string) => void; folder: string }) => { h.goto = p; return null; } }));
vi.mock("../components/editor/FolderPicker", () => ({ FolderPicker: () => null }));
vi.mock("../components/editor/Menus", async (orig) => ({ ...(await orig<object>()), OpenInMenu: () => null }));
vi.mock("../router", async (orig) => ({ ...(await orig<object>()), go: (r: unknown) => { h.calls.push(`go ${JSON.stringify(r)}`); } }));
// the question before closing a tab that isn't saved is put on the body: here it shows where it is made
vi.mock("react-dom", async (orig) => ({ ...(await orig<object>()), createPortal: (children: ReactNode) => children }));
// the terminals' store (TF0) and the terminal's view (GA-137 QA): the panel reads h.terms, a terminal it opens joins them
vi.mock("../lib/terminals", async (orig) => ({
  ...(await orig<object>()),
  useTerminals: () => h.terms,
  useTerminalsLoaded: () => true,
  useShownHere: () => h.placed,
  useShownIn: () => null,
  noteOpened: (s: TerminalSession) => { if (!h.terms.some((x) => x.id === s.id)) h.terms = [...h.terms, s]; },
  showIn: (id: string, place: unknown) => { h.calls.push(`showIn ${id} ${JSON.stringify(place)}`); },
}));
vi.mock("../components/terminal/TerminalView", () => ({ TerminalView: (p: { id: string; place: unknown; autoFocus?: boolean }) => { h.termView = p; return null; } }));

// ---- the fake disk: a folder under a test root, in a git repository ----
const ROOT = "/srv/test-root/Herd";
const FOLDER = `${ROOT}/kade-portal`;
type OnDisk = { text: string; v: number; size?: number; readOnlyWhy?: string };
const disk = new Map<string, OnDisk>();
/** The text of each file in git's last commit. */
const head = new Map<string, string>();
let nextV = 1;
let folderInfo: EditorFolder;
let task: Task | null = null;
const IGNORED = ["vendor"];

const put = (path: string, text: string, more: Partial<OnDisk> = {}) => { disk.set(path, { text, v: nextV++, ...more }); };
const version = (path: string) => { const f = disk.get(path); return f ? `v${f.v}` : ""; };
const ignored = (path: string) => IGNORED.some((d) => path === d || path.startsWith(`${d}/`));
function gitFiles(): Record<string, GitMark> {
  const out: Record<string, GitMark> = {};
  for (const [p, f] of disk) if (!ignored(p)) { if (!head.has(p)) out[p] = "untracked"; else if (head.get(p) !== f.text) out[p] = "modified"; }
  for (const p of head.keys()) if (!disk.has(p)) out[p] = "deleted";
  return out;
}
function list(dir: string): DirEntry[] {
  const pre = dir ? `${dir}/` : "";
  const dirs = new Set<string>();
  const files: string[] = [];
  for (const p of disk.keys()) {
    if (!p.startsWith(pre)) continue;
    const rest = p.slice(pre.length);
    if (rest.includes("/")) dirs.add(pre + rest.slice(0, rest.indexOf("/")));
    else files.push(p);
  }
  const git = gitFiles();
  const name = (p: string) => p.slice(p.lastIndexOf("/") + 1);
  return [
    ...[...dirs].sort().map((p): DirEntry => ({ name: name(p), path: p, dir: true, size: 0, ignored: ignored(p), git: Object.keys(git).some((g) => g.startsWith(`${p}/`)) ? "modified" : null })),
    ...files.sort().map((p): DirEntry => ({ name: name(p), path: p, dir: false, size: disk.get(p)!.size ?? disk.get(p)!.text.length, ignored: ignored(p), git: git[p] ?? null })),
  ];
}
const opened = (path: string): OpenedFile => {
  const f = disk.get(path)!;
  const why = f.readOnlyWhy ?? (ignored(path) && path.includes("/") ? "Ignored by git" : null);
  return { path, text: f.text, binary: false, tooLarge: false, readOnly: !!why || folderInfo.readOnly, readOnlyWhy: why ?? folderInfo.readOnlyWhy, crlf: false, bom: false, version: version(path) };
};
const writes: { path: string; text: string; version: string }[] = [];

vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) if (typeof real[k] === "function") out[k] = () => new Promise(() => {});
  const mine = (folder: string) => { if (folder !== FOLDER) throw new Error(`The Editor can't open ${folder}`); };
  return {
    ...out,
    editorFolders: () => Promise.resolve([folderInfo]),
    editorList: async (folder: string, dir: string) => { mine(folder); return list(dir); },
    editorRead: async (folder: string, path: string) => {
      mine(folder);
      h.calls.push(`read ${path}`);
      if (!disk.has(path)) throw new Error(`${path} isn't there any more`);
      return opened(path);
    },
    editorWrite: async (folder: string, path: string, text: string, v: string) => {
      mine(folder);
      writes.push({ path, text, version: v });
      if (version(path) !== v) throw new Error(`${path} changed on disk since you opened it${disk.has(path) ? "" : ": it isn't there any more"}`);
      put(path, text);
      return version(path);
    },
    editorVersions: async (folder: string, paths: string[]) => { mine(folder); return paths.map(version); },
    editorGit: async (folder: string): Promise<FolderGit> => { mine(folder); const files = gitFiles(); return { branch: "main", files, uncommitted: Object.keys(files).length }; },
    editorFiles: async () => [...disk.keys()].filter((p) => !ignored(p)).sort(),
    editorApps: () => Promise.resolve([]),
    getTask: () => (task ? Promise.resolve(task) : Promise.reject(new Error("no card"))),
    terminalOpen: async (folder: string, cols: number, rows: number, name: string | null): Promise<TerminalSession> => {
      h.calls.push(`terminalOpen ${folder} ${cols} ${rows} ${name}`);
      const n = h.terms.length + 1;
      return { id: `term-${n}`, name: n === 1 ? "kade-portal" : `kade-portal ${n}`, folder, folderPath: folder, taskId: null, shell: "sh", busy: null, ended: null, cols: 80, rows: 24, openedAt: n };
    },
    terminalClose: async (id: string) => { h.calls.push(`terminalClose ${id}`); h.terms = h.terms.filter((s) => s.id !== id); },
  };
});

// ---- the browser bits the page uses ----
const store = new Map<string, string>();
vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, v); }, removeItem: (k: string) => { store.delete(k); } });
vi.stubGlobal("navigator", { userAgent: "Linux", platform: "Linux x86_64" });
const win = new EventTarget();
vi.stubGlobal("window", win);
vi.stubGlobal("document", { hidden: false, querySelector: () => null, body: { style: {} }, documentElement: { style: {} } });
vi.stubGlobal("requestAnimationFrame", (f: () => void) => setTimeout(f, 0));

const { EditorPage } = await import("./EditorPage");

// ---- the hook runtime ----
let root: () => ReactNode = () => null;
const insts = new Map<string, Inst>();
const unmount = (inst: Inst) => { for (const s of inst.slots) if ((s as EffectSlot)?.effect) (s as EffectSlot).cleanup?.(); };
const keyOf = (n: unknown) => (n && typeof n === "object" && "key" in n ? (n as { key: string | null }).key : null);
const HOST = { tagName: "DIV" };
function expand(node: ReactNode, path: string, seen: Set<string>): ReactNode {
  if (Array.isArray(node)) return node.map((n, i) => expand(n, `${path}.${keyOf(n) ?? i}`, seen));
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
  // the code's host element is there before the effects run, as in React
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
  if (typeof node === "string" || typeof node === "number") return `${node}`;
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (node && typeof node === "object" && "props" in node) {
    const p = (node as ReactElement<{ children?: ReactNode; "aria-label"?: string }>).props;
    return textOf(p.children);
  }
  return "";
}
const one = (pred: (f: Found) => boolean, what: string) => {
  const all = findAll(tree, pred);
  if (all.length !== 1) throw new Error(`${all.length} of ${what}`);
  return all[0]!;
};
const row = (path: string) => one((f) => f.props.role === "treeitem" && f.props["data-path"] === path, `tree rows ${path}`);
/** A tree row's mark letter (M, A, U…), or "" without one. */
const rowMark = (path: string) => findAll(row(path).props.children as ReactNode, (f) => String(f.props.className ?? "").startsWith("ed-mark")).map((f) => textOf(f.props.children as ReactNode)).join("");
const rowDot = (path: string) => findAll(row(path).props.children as ReactNode, (f) => f.props.className === "ed-dot").length > 0;
const tabs = () => findAll(tree, (f) => f.props.role === "tab" && typeof f.props["data-path"] === "string");
const tab = (path: string) => one((f) => f.props.role === "tab" && f.props["data-path"] === path, `tabs ${path}`);
const tabMark = (path: string) => findAll(tab(path).props.children as ReactNode, (f) => String(f.props.className ?? "").startsWith("ed-mark")).map((f) => textOf(f.props.children as ReactNode)).join("");
const statusBar = () => textOf(one((f) => f.props["aria-label"] === "Status bar", "status bars").props.children as ReactNode);
/** The bar under the tabs about the open file (or the folder's under the top bar), as text; "" when there is none. */
const fileBar = () => findAll(tree, (f) => String(f.props.className ?? "").startsWith("ed-bar file")).map((f) => textOf(f.props.children as ReactNode)).join(" | ");
const folderBar = () => findAll(tree, (f) => f.props.className === "ed-bar").map((f) => textOf(f.props.children as ReactNode)).join(" | ");
const barButtons = () => findAll(tree, (f) => f.type === "button" && findAll(tree, (b) => String(b.props.className ?? "").startsWith("ed-bar file")).length > 0
  && String(f.props.className ?? "").startsWith("btn sm")).map((f) => textOf(f.props.children as ReactNode).trim());
function button(name: string) {
  const all = findAll(tree, (f) => f.type === "button" && textOf(f.props.children as ReactNode).trim() === name);
  if (all.length !== 1) throw new Error(`${all.length} buttons called ${name}`);
  return all[0]!.props as { onClick: (e?: unknown) => unknown; disabled?: boolean; title?: string };
}
const ev = { preventDefault: () => {}, stopPropagation: () => {}, currentTarget: { getBoundingClientRect: () => ({ left: 0, bottom: 0 }), parentElement: null } };
async function click(name: string) { await button(name).onClick(ev); await settle(); }
async function clickRow(path: string) { (row(path).props.onClick as () => void)(); await settle(); }

type View = { state: EditorState; dispatch: (spec: TransactionSpec) => void; focused: number };
const view = () => { const v = h.views.at(-1) as View | undefined; if (!v) throw new Error("no code view"); return v; };
/** Types at the end of the open file, as a person does in the code. */
async function typeAtEnd(text: string) { const v = view(); v.dispatch({ changes: { from: v.state.doc.length, insert: text }, selection: { anchor: v.state.doc.length + text.length }, userEvent: "input.type" }); await settle(); }
/** A key on the window, as the browser sends it. */
async function key(k: string, mods: { ctrl?: boolean; shift?: boolean } = { ctrl: true }) {
  const e = Object.assign(new Event("keydown", { cancelable: true }), { key: k, ctrlKey: !!mods.ctrl, metaKey: false, altKey: false, shiftKey: !!mods.shift });
  win.dispatchEvent(e);
  await settle();
}
/** Something outside Gizai changes a file (a git pull, another editor). */
const outside = (path: string, text: string) => put(path, text);

const folder = (more: Partial<EditorFolder> = {}): EditorFolder => ({
  id: FOLDER, path: FOLDER, name: "kade-portal", kind: "root", projectId: null, taskId: null, readOnly: false, readOnlyWhy: null, ...more,
});
async function open(id = FOLDER) {
  root = () => <EditorPage key={id} id={id} youId="u1" />;
  render();
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0; h.views.length = 0; h.calls.length = 0; h.search = null; h.goto = null;
  store.clear(); disk.clear(); head.clear(); writes.length = 0; nextV = 1; task = null;
  folderInfo = folder();
  // the folder's files, committed
  put("README.md", "# Kade portal\n");
  put("src/app.ts", "export const one = 1;\n");
  put("src/lib/util.ts", "export function add(a: number, b: number) {\n  return a + b;\n}\n\n// the needle is here\nexport const needle = add(1, 2);\n");
  put("vendor/laravel/framework.php", "<?php\n");
  put("storage/big.log", "x\n", { size: 1_300_000, readOnlyWhy: "Over 1 MB" });
  for (const [p, f] of disk) if (!ignored(p)) head.set(p, f.text);
});
afterEach(() => { root = () => null; render(); vi.useRealTimers(); });

describe("open a folder under the test root, open a file, type, Ctrl+S (GA-113)", () => {
  it("shows the folder's tree, folders first, with nothing marked while nothing changed", async () => {
    await open();
    expect(findAll(tree, (f) => f.props.role === "treeitem").map((f) => f.props["data-path"])).toEqual(["src", "storage", "vendor", "README.md"]);
    expect(rowMark("README.md")).toBe("");
    expect(rowDot("src")).toBe(false);
    expect(store.get("gizai.editor.opened")).toBe(JSON.stringify([FOLDER]));
  });
  // GA-157 QA: the page of a folder already in the sidebar's list leaves the list as it was; a new one joins at the bottom
  it("opened from the sidebar, leaves the folder where it is in the sidebar's list, every row too", async () => {
    const list = [`${ROOT}/one`, FOLDER, `${ROOT}/two`];
    store.set("gizai.editor.opened", JSON.stringify(list));
    await open();
    expect(JSON.parse(store.get("gizai.editor.opened")!)).toEqual(list);
    expect(Object.keys(JSON.parse(store.get("gizai.editor.openedAt")!))).toEqual([FOLDER]);
  });
  it("adds a folder that isn't in the sidebar's list yet at the bottom", async () => {
    store.set("gizai.editor.opened", JSON.stringify([`${ROOT}/one`, `${ROOT}/two`]));
    await open();
    expect(JSON.parse(store.get("gizai.editor.opened")!)).toEqual([`${ROOT}/one`, `${ROOT}/two`, FOLDER]);
  });

  it("saves what was typed: the file on disk has the text, the tab loses its dot, the tree shows M", async () => {
    await open();
    await clickRow("src");
    await clickRow("src/app.ts");
    expect(tabs().map((t) => t.props["data-path"])).toEqual(["src/app.ts"]);
    expect(view().state.doc.toString()).toBe("export const one = 1;\n");
    expect(statusBar()).toContain("Saved");

    await typeAtEnd("export const two = 2;\n");
    expect(String(tab("src/app.ts").props.className)).toContain("unsaved");
    expect(tab("src/app.ts").props["aria-label"]).toBe("app.ts, not saved");
    expect(statusBar()).toContain("Not saved");
    expect(disk.get("src/app.ts")!.text).toBe("export const one = 1;\n");

    await key("s");
    expect(writes).toEqual([{ path: "src/app.ts", text: "export const one = 1;\nexport const two = 2;\n", version: "v2" }]);
    expect(disk.get("src/app.ts")!.text).toBe("export const one = 1;\nexport const two = 2;\n");
    expect(String(tab("src/app.ts").props.className)).not.toContain("unsaved");
    expect(tab("src/app.ts").props["aria-label"]).toBe("app.ts");
    expect(statusBar()).toContain("Saved");
    expect(statusBar()).toContain("Modified");
    expect(rowMark("src/app.ts")).toBe("M");
    expect(tabMark("src/app.ts")).toBe("M");
    expect(rowDot("src")).toBe(true);
  });

  it("saves again with the version it saved, so a second Ctrl+S isn't taken for an outside change", async () => {
    await open();
    await clickRow("README.md");
    await typeAtEnd("one\n");
    await key("s");
    await typeAtEnd("two\n");
    await key("s");
    expect(writes.map((w) => w.version)).toEqual(["v1", "v6"]);
    expect(disk.get("README.md")!.text).toBe("# Kade portal\none\ntwo\n");
    expect(fileBar()).toBe("");
    await wait(3000);
    expect(fileBar()).toBe("");
  });

  it("Ctrl+S on a file with no changes leaves it as it was on disk, unmarked", async () => {
    await open();
    await clickRow("README.md");
    await key("s");
    expect(disk.get("README.md")!.text).toBe("# Kade portal\n");
    expect(rowMark("README.md")).toBe("");
  });

  it("puts the typed text back to how it was: the dot goes without a save", async () => {
    await open();
    await clickRow("README.md");
    await typeAtEnd("x");
    expect(String(tab("README.md").props.className)).toContain("unsaved");
    const v = view();
    v.dispatch({ changes: { from: v.state.doc.length - 1, to: v.state.doc.length } });
    await settle();
    expect(String(tab("README.md").props.className)).not.toContain("unsaved");
  });

  it("keeps the open tabs for the folder, and opens them again with the page", async () => {
    await open();
    await clickRow("README.md");
    await clickRow("src");
    await clickRow("src/app.ts");
    root = () => null; render();
    await open();
    expect(tabs().map((t) => t.props["data-path"])).toEqual(["README.md", "src/app.ts"]);
    expect(tab("src/app.ts").props["aria-selected"]).toBe(true);
  });
});

describe("a file changed outside shows within 3 seconds (GA-113)", () => {
  it("an untouched tab reloads by itself and says so", async () => {
    await open();
    await clickRow("README.md");
    outside("README.md", "# Kade portal\n\nChanged by git pull.\n");
    await wait(2999);
    expect(view().state.doc.toString()).toBe("# Kade portal\n");
    await wait(1);
    expect(view().state.doc.toString()).toBe("# Kade portal\n\nChanged by git pull.\n");
    expect(String(tab("README.md").props.className)).not.toContain("unsaved");
    expect(statusBar()).toContain("Reloaded: changed on disk");
    expect(fileBar()).toBe("");
    // and a save after it goes through with the new version
    await typeAtEnd("More.\n");
    await key("s");
    expect(disk.get("README.md")!.text).toBe("# Kade portal\n\nChanged by git pull.\nMore.\n");
  });

  it("a tab with unsaved edits offers Reload and Keep mine, and keeps the edits until one is picked", async () => {
    await open();
    await clickRow("README.md");
    await typeAtEnd("Mine.\n");
    outside("README.md", "Theirs.\n");
    await wait(3000);
    expect(fileBar()).toContain("README.md changed on disk while you were editing it.");
    expect(barButtons()).toEqual(["Reload", "Keep mine"]);
    expect(view().state.doc.toString()).toBe("# Kade portal\nMine.\n");
    // Ctrl+S doesn't save over it while the bar asks
    await key("s");
    expect(writes).toEqual([]);
  });

  it("Reload shows the version on disk, and the edits here are gone", async () => {
    await open();
    await clickRow("README.md");
    await typeAtEnd("Mine.\n");
    outside("README.md", "Theirs.\n");
    await wait(3000);
    await click("Reload");
    expect(view().state.doc.toString()).toBe("Theirs.\n");
    expect(String(tab("README.md").props.className)).not.toContain("unsaved");
    expect(fileBar()).toBe("");
  });

  it("Keep mine keeps the edits; Ctrl+S asks once more, and the second saves over the newer file", async () => {
    await open();
    await clickRow("README.md");
    await typeAtEnd("Mine.\n");
    outside("README.md", "Theirs.\n");
    await wait(3000);
    await click("Keep mine");
    expect(fileBar()).toContain("Keeping your version of README.md.");
    await wait(3000);
    expect(fileBar()).toContain("Keeping your version of README.md.");
    await key("s");
    expect(writes).toEqual([]);
    expect(fileBar()).toContain("Save over the newer README.md on disk? Press Ctrl+S again.");
    await key("s");
    expect(disk.get("README.md")!.text).toBe("# Kade portal\nMine.\n");
    expect(fileBar()).toBe("");
    expect(String(tab("README.md").props.className)).not.toContain("unsaved");
  });

  it("a save the disk refuses (changed since the last poll) brings the same Reload and Keep mine bar", async () => {
    await open();
    await clickRow("README.md");
    await typeAtEnd("Mine.\n");
    outside("README.md", "Theirs.\n");
    await key("s");
    expect(writes).toHaveLength(1);
    expect(disk.get("README.md")!.text).toBe("Theirs.\n");
    expect(fileBar()).toContain("README.md changed on disk while you were editing it.");
    expect(String(tab("README.md").props.className)).toContain("unsaved");
  });

  it("an untouched file deleted outside says it isn't there any more", async () => {
    await open();
    await clickRow("README.md");
    disk.delete("README.md");
    await wait(3000);
    expect(fileBar()).toContain("README.md isn't there any more.");
  });

  it("checks only every 3 seconds, and not while the window is hidden", async () => {
    await open();
    await clickRow("README.md");
    outside("README.md", "Theirs.\n");
    vi.stubGlobal("document", { hidden: true, querySelector: () => null, body: { style: {} }, documentElement: { style: {} } });
    try {
      await wait(9000);
      expect(view().state.doc.toString()).toBe("# Kade portal\n");
    } finally { vi.stubGlobal("document", { hidden: false, querySelector: () => null, body: { style: {} }, documentElement: { style: {} } }); }
    await wait(3000);
    expect(view().state.doc.toString()).toBe("Theirs.\n");
  });
});

describe("search in the folder and go to file (GA-113)", () => {
  it("Ctrl+Shift+F shows search in the folder; a hit opens its file at its line and column", async () => {
    await open();
    await key("F", { ctrl: true, shift: true });
    expect(h.search?.folder).toBe(FOLDER);
    const hit: SearchHit = { path: "src/lib/util.ts", line: 5, column: 8, text: "// the needle is here" };
    h.search!.onOpen(hit);
    await settle();
    await wait(16); // the page moves to the line on the next frame, once the file is read
    expect(tab("src/lib/util.ts").props["aria-selected"]).toBe(true);
    expect(view().state.doc.lineAt(view().state.selection.main.head).number).toBe(5);
    expect(statusBar()).toContain("Ln 5, Col 8");
    // back on Files, its folders are open in the tree, so its row shows, marked as the open file
    await click("Files");
    expect(row("src/lib/util.ts").props["aria-selected"]).toBe(true);
  });

  it("a second hit in a file already open moves to its line without opening it again", async () => {
    await open();
    await key("F", { ctrl: true, shift: true });
    h.search!.onOpen({ path: "src/lib/util.ts", line: 5, column: 8, text: "// the needle is here" });
    await settle();
    h.search!.onOpen({ path: "src/lib/util.ts", line: 6, column: 14, text: "export const needle = add(1, 2);" });
    await settle();
    expect(tabs()).toHaveLength(1);
    expect(h.calls.filter((c) => c === "read src/lib/util.ts")).toHaveLength(1);
    expect(statusBar()).toContain("Ln 6, Col 14");
  });

  it("Ctrl+P opens Go to file for this folder, and the file it finds opens in a tab, shown in the tree", async () => {
    await open();
    expect(h.goto).toBe(null);
    await key("p");
    expect(h.goto?.folder).toBe(FOLDER);
    h.goto!.onOpen("src/lib/util.ts");
    await settle();
    expect(tab("src/lib/util.ts").props["aria-selected"]).toBe(true);
    expect(view().state.doc.toString()).toContain("needle");
    expect(row("src/lib/util.ts").props["aria-selected"]).toBe(true);
  });
});

describe("read-only files and folders can't be edited and say why (GA-113)", () => {
  it("a file over 1 MB opens read-only: the code takes no typing, the bar and the status bar say why, Ctrl+S writes nothing", async () => {
    await open();
    await clickRow("storage");
    await clickRow("storage/big.log");
    expect(view().state.readOnly).toBe(true);
    expect(fileBar()).toBe("Read-only: this file is over 1 MB (1.2 MB). Change it in your own editor.");
    expect(statusBar()).toContain("Read-only: over 1 MB");
    expect(tab("storage/big.log").props.children).toBeTruthy();
    expect(findAll(tab("storage/big.log").props.children as ReactNode, (f) => f.props["aria-label"] === "Read-only")).toHaveLength(1);
    await typeAtEnd("y");
    await key("s");
    expect(writes).toEqual([]);
  });

  it("a file in a folder git ignores is read-only and says which folder", async () => {
    await open();
    await clickRow("vendor");
    await clickRow("vendor/laravel");
    await clickRow("vendor/laravel/framework.php");
    expect(view().state.readOnly).toBe(true);
    expect(fileBar()).toBe("Read-only: it's in vendor/, which git ignores.");
    expect(statusBar()).toContain("Read-only: ignored by git");
    expect(String(row("vendor/laravel/framework.php").props.className)).toContain("ignored");
    expect(row("vendor/laravel/framework.php").props.title).toBe("Ignored by git: read-only");
    await key("s");
    expect(writes).toEqual([]);
  });

  it("a card's worktree an agent works on is read-only with the folder's bar saying who, and no New file or folder", async () => {
    folderInfo = folder({ name: "GA-12", kind: "card", taskId: "t12", readOnly: true, readOnlyWhy: "Backend Agent is working on GA-12" });
    await open();
    expect(folderBar()).toContain("Backend Agent is working on GA-12: its worktree is read-only until the run ends.");
    expect(button("Open card")).toBeTruthy();
    const newFile = findAll(tree, (f) => f.type === "button" && f.props["aria-label"] === "New file")[0]!.props;
    expect([newFile.disabled, newFile.title]).toEqual([true, "Backend Agent is working on GA-12"]);
    await clickRow("README.md");
    expect(view().state.readOnly).toBe(true);
    expect(statusBar()).toContain("Read-only: Backend Agent is working on GA-12");
    await typeAtEnd("x");
    await key("s");
    expect(writes).toEqual([]);
  });

  it("another person's card is read-only until Edit anyway, which lets it be typed in and saved", async () => {
    folderInfo = folder({ name: "GA-20", kind: "card", taskId: "t20" });
    task = { id: "t20", identifier: "GA-20", title: "Fix it", byHand: true, assigneeId: "u2", assigneeName: "Sam" } as unknown as Task;
    await open();
    expect(folderBar()).toContain("GA-20 is Sam's card: its worktree is read-only here.");
    await clickRow("README.md");
    expect(view().state.readOnly).toBe(true);
    await key("s");
    expect(writes).toEqual([]);
    await click("Edit anyway");
    expect(folderBar()).toContain("You're editing GA-20's worktree.");
    expect(view().state.readOnly).toBe(false);
    await typeAtEnd("By hand.\n");
    await key("s");
    expect(disk.get("README.md")!.text).toBe("# Kade portal\nBy hand.\n");
  });

  it("your own card done by hand is yours to edit, with its card bar (GA-115) instead of a read-only bar", async () => {
    folderInfo = folder({ name: "GA-21", kind: "card", taskId: "t21" });
    task = { id: "t21", identifier: "GA-21", title: "Mine", byHand: true, assigneeId: "u1", assigneeName: "You" } as unknown as Task;
    await open();
    expect(folderBar()).toBe("");
    expect(findAll(tree, (f) => f.props.className === "ed-bar hand" && f.props["aria-label"] === "GA-21 by hand")).toHaveLength(1);
    await clickRow("README.md");
    expect(view().state.readOnly).toBe(false);
  });
});

describe("a folder the Editor can't open (GA-113)", () => {
  it("says why, and offers Remove from the list", async () => {
    await open("/etc");
    const page = textOf(tree);
    expect(page).toContain("Gizai can't open /etc: The Editor can't open /etc.");
    await click("Remove from the list");
    expect(h.calls).toContain(`go ${JSON.stringify({ page: "editor" })}`);
  });
});

describe("closing a tab with text that isn't saved asks first (GA-128 QA)", () => {
  // Ctrl+W finds the tab by its path in a selector
  beforeEach(() => { Object.assign(globalThis, { CSS: { escape: (s: string) => s } }); });
  afterEach(() => { delete (globalThis as { CSS?: unknown }).CSS; });
  const at = { ...ev, currentTarget: { parentElement: { getBoundingClientRect: () => ({ left: 10, bottom: 20 }) } } };
  async function closeTab(name: string) {
    (one((f) => f.type === "button" && f.props["aria-label"] === `Close ${name}`, `close buttons of ${name}`).props.onClick as (e: unknown) => void)(at);
    await settle();
  }
  const asks = () => findAll(tree, (f) => f.props.role === "alertdialog");
  const askButtons = () => findAll(asks()[0]!.props.children as ReactNode, (f) => f.type === "button").map((f) => textOf(f.props.children as ReactNode).trim());
  async function answer(name: string) {
    const b = findAll(asks()[0]!.props.children as ReactNode, (f) => f.type === "button" && textOf(f.props.children as ReactNode).trim() === name);
    expect(b).toHaveLength(1);
    (b[0]!.props.onClick as () => void)();
    await settle();
  }
  async function typeInApp() {
    await open();
    await clickRow("src");
    await clickRow("src/app.ts");
    await typeAtEnd("export const two = 2;\n");
    expect(tab("src/app.ts").props["aria-label"]).toBe("app.ts, not saved");
  }

  it("asks with Save and close, Close without saving and Cancel; Cancel keeps the tab and its text and saves nothing", async () => {
    await typeInApp();
    await closeTab("app.ts");
    expect(asks()).toHaveLength(1);
    expect(asks()[0]!.props["aria-label"]).toBe("Close app.ts");
    expect(textOf(asks()[0]!.props.children as ReactNode)).toContain("app.ts has changes that aren't saved.");
    expect(askButtons()).toEqual(["Save and close", "Close without saving", "Cancel"]);
    await answer("Cancel");
    expect(asks()).toHaveLength(0);
    expect(tabs().map((t) => t.props["data-path"])).toEqual(["src/app.ts"]);
    expect(view().state.doc.toString()).toBe("export const one = 1;\nexport const two = 2;\n");
    expect(String(tab("src/app.ts").props.className)).toContain("unsaved");
    expect(writes).toEqual([]);
  });

  it("Close without saving closes the tab and leaves the file on disk as it was", async () => {
    await typeInApp();
    await closeTab("app.ts");
    await answer("Close without saving");
    expect(asks()).toHaveLength(0);
    expect(tabs()).toEqual([]);
    expect(writes).toEqual([]);
    expect(disk.get("src/app.ts")!.text).toBe("export const one = 1;\n");
  });

  it("Save and close saves exactly the text, then closes the tab", async () => {
    await typeInApp();
    await closeTab("app.ts");
    await answer("Save and close");
    await wait(500);
    expect(writes.map((w) => [w.path, w.text])).toEqual([["src/app.ts", "export const one = 1;\nexport const two = 2;\n"]]);
    expect(disk.get("src/app.ts")!.text).toBe("export const one = 1;\nexport const two = 2;\n");
    expect(tabs()).toEqual([]);
    expect(asks()).toHaveLength(0);
  });

  it("a saved tab closes without asking, and Ctrl+W asks the same question for one that isn't saved", async () => {
    await open();
    await clickRow("README.md");
    await closeTab("README.md");
    expect(asks()).toHaveLength(0);
    expect(tabs()).toEqual([]);
    await clickRow("src");
    await clickRow("src/app.ts");
    await typeAtEnd("export const two = 2;\n");
    await key("w");
    expect(asks().map((a) => a.props["aria-label"])).toEqual(["Close app.ts"]);
    expect(tabs().map((t) => t.props["data-path"])).toEqual(["src/app.ts"]);
    expect(writes).toEqual([]);
  });
});

// GA-137 QA: the Editor's terminal panel (TF2) on this page: the Terminal button's states and tooltips (design §4 and §8),
// the panel with the folder's terminals, Ctrl+` and Ctrl+Shift+`, and the Editor's own keys staying out of the terminal's
// way. TF0's store and the terminal's view are stand-ins (above); the cage probe (scripts/ui-test.sh, run 16) drives the
// real panel, its shell and its heights.
describe("the terminal panel: the Terminal button, its states and the keys (GA-137 QA)", () => {
  type ButtonProps = { onClick: (e?: unknown) => unknown; title: string; disabled?: boolean; "aria-pressed"?: boolean; "aria-disabled"?: boolean };
  const termButton = () => one((f) => f.type === "button" && String(f.props.className ?? "").includes("term-btn") && textOf(f.props.children as ReactNode).trim() === "Terminal", "Terminal buttons").props as ButtonProps;
  const plus = () => one((f) => f.type === "button" && String(f.props["aria-label"] ?? "").startsWith("New terminal in "), "+ buttons").props as ButtonProps & { "aria-label": string };
  const panelShown = () => findAll(tree, (f) => f.props.className === "term-panel").length === 1;
  const termTabs = () => findAll(tree, (f) => f.props.role === "tab" && typeof f.props["data-term"] === "string").map((f) => f.props["data-term"]);
  const opens = () => h.calls.filter((c) => c.startsWith("terminalOpen"));
  const session = (more: Partial<TerminalSession> = {}): TerminalSession => ({
    id: "term-1", name: "kade-portal", folder: FOLDER, folderPath: FOLDER, taskId: null, shell: "sh", busy: null, ended: null, cols: 80, rows: 24, openedAt: 1, ...more,
  });
  /** Ctrl+` (Ctrl+Shift+` with shift), from the page or from inside the terminal (its text field is in .term-place). */
  async function backquote(shift = false, inTerminal = false) {
    const e = Object.assign(new Event("keydown", { cancelable: true }), { key: shift ? "~" : "`", code: "Backquote", ctrlKey: true, metaKey: false, altKey: false, shiftKey: shift });
    if (inTerminal) Object.defineProperty(e, "target", { get: () => ({ closest: (sel: string) => (sel === ".term-place" ? {} : null) }) });
    win.dispatchEvent(e);
    await settle();
  }
  /** A Ctrl key from inside the terminal. */
  async function keyInTerminal(k: string, shift = false) {
    const e = Object.assign(new Event("keydown", { cancelable: true }), { key: k, code: `Key${k.toUpperCase()}`, ctrlKey: true, metaKey: false, altKey: false, shiftKey: shift });
    Object.defineProperty(e, "target", { get: () => ({ closest: (sel: string) => (sel === ".term-place" ? {} : null) }) });
    win.dispatchEvent(e);
    await settle();
    return e.defaultPrevented;
  }
  const treeShown = () => one((f) => f.type === "aside", "asides").props.hidden === false;

  beforeEach(() => { h.terms = []; h.termView = null; h.placed = ""; });
  afterEach(() => { h.terms = []; h.termView = null; h.placed = ""; });

  it("in a folder with no terminal, the button starts one in its root and shows the panel at 30%; pressed again it hides, and the shell keeps running", async () => {
    await open();
    let b = termButton();
    expect([b.title, b["aria-pressed"], b["aria-disabled"], b.disabled]).toEqual(["Show the terminal (Ctrl+`)", false, undefined, undefined]);
    expect(panelShown()).toBe(false);
    await click("Terminal");
    expect(opens(), "one terminal, in the folder's root").toEqual([`terminalOpen ${FOLDER} 0 0 null`]);
    expect(panelShown()).toBe(true);
    expect(h.termView, "the new terminal shows in the folder's panel, with the focus").toEqual({ id: "term-1", place: { kind: "panel", folder: FOLDER }, autoFocus: true });
    const panel = one((f) => f.props.className === "term-panel", "panels").props as { style: Record<string, string> };
    expect(panel.style["--term-share"]).toBe("30%");
    b = termButton();
    expect([b.title, b["aria-pressed"]]).toEqual(["Hide the terminal (Ctrl+`)", true]);
    expect(JSON.parse(store.get("gizai.editor.terminal")!), "kept as shown in this folder").toEqual([FOLDER]);
    await click("Terminal");
    expect(panelShown()).toBe(false);
    expect(termButton().title).toBe("Show the terminal (Ctrl+`)");
    expect(h.calls.filter((c) => c.startsWith("terminalClose")), "hiding closes no shell").toEqual([]);
    expect(JSON.parse(store.get("gizai.editor.terminal")!)).toEqual([]);
    // Ctrl+` shows it again on the same terminal, without a new one; Ctrl+` hides it
    await backquote();
    expect([panelShown(), h.termView?.id, opens().length]).toEqual([true, "term-1", 1]);
    await backquote();
    expect(panelShown()).toBe(false);
  });

  it("opens at the height kept (a share of the column, the same for every folder)", async () => {
    store.set("gizai.editor.panel", "42.5");
    h.terms = [session()];
    await open();
    await backquote();
    const panel = one((f) => f.props.className === "term-panel", "panels").props as { style: Record<string, string> };
    expect(panel.style["--term-share"]).toBe("42.5%");
  });

  it("its tabs are the folder's terminals, with +, Own page and Hide; Ctrl+Shift+` starts another in a new tab", async () => {
    h.terms = [session(), session({ id: "elsewhere", name: "gizai", folder: "/srv/test-root/Herd/gizai" }), session({ id: "term-2", name: "kade-portal 2", busy: "npm" })];
    await open();
    await backquote();
    expect(termTabs(), "this folder's terminals only").toEqual(["term-1", "term-2"]);
    const tabTitles = findAll(tree, (f) => f.props.role === "tab" && typeof f.props["data-term"] === "string").map((f) => f.props.title);
    expect(tabTitles).toEqual(["kade-portal · waiting for a command", "kade-portal 2 · running npm"]);
    expect([plus()["aria-label"], plus().title, plus()["aria-disabled"]]).toEqual(["New terminal in kade-portal", "New terminal in kade-portal (Ctrl+Shift+`)", undefined]);
    expect(button("Own page").title).toBe("Open kade-portal on its own page");
    expect(button("Hide").title).toBe("Hide the terminal (Ctrl+`): its shells keep running");
    await backquote(true);
    expect(opens()).toEqual([`terminalOpen ${FOLDER} 0 0 null`]);
    expect(termTabs()).toEqual(["term-1", "term-2", "term-4"]);
    expect(h.termView?.id, "the new tab shows").toBe("term-4");
    await click("Hide");
    expect(panelShown()).toBe(false);
    expect(h.calls.filter((c) => c.startsWith("terminalClose"))).toEqual([]);
  });

  it("Own page moves the view to the terminal's page, #/terminal/<id>", async () => {
    h.terms = [session()];
    await open();
    await backquote();
    await click("Own page");
    expect(h.calls).toContain(`showIn term-1 ${JSON.stringify({ kind: "page" })}`);
    expect(h.calls).toContain(`go ${JSON.stringify({ page: "terminal", id: "term-1" })}`);
  });

  it("Show in Editor (TF1: showIn the folder's panel, then the Editor) opens the panel on that terminal's tab, and it is kept as shown in the folder", async () => {
    h.terms = [session(), session({ id: "term-2", name: "kade-portal 2" })];
    h.placed = "term-2";
    await open();
    expect([panelShown(), h.termView?.id], "the panel opens on the terminal put there").toEqual([true, "term-2"]);
    expect(termButton()["aria-pressed"]).toBe(true);
    // whether the panel shows is kept per folder (the card's What to build, 2): it shows, so the folder keeps it as shown
    expect(JSON.parse(store.get("gizai.editor.terminal") ?? "[]"), "the panel shows, so the folder keeps it as shown").toEqual([FOLDER]);
  });

  it("after Show in Editor opened it, the panel shows again when the folder opens again; hidden, it stays hidden", async () => {
    h.terms = [session(), session({ id: "term-2", name: "kade-portal 2" })];
    h.placed = "term-2";
    await open();
    expect([panelShown(), h.termView?.id]).toEqual([true, "term-2"]);
    // leave the Editor (#/tasks): the panel's view goes, and leaves its place (TerminalView's leaveIn)
    async function leaveAndComeBack() {
      root = () => null;
      render();
      h.placed = "";
      h.termView = null;
      await open();
    }
    await leaveAndComeBack();
    expect([panelShown(), h.termView?.id], "kept as shown: it opens again, on the same terminal").toEqual([true, "term-2"]);
    expect(opens(), "no new shell").toEqual([]);
    await click("Hide");
    expect(panelShown()).toBe(false);
    expect(JSON.parse(store.get("gizai.editor.terminal")!), "hiding clears what the folder keeps").toEqual([]);
    await leaveAndComeBack();
    expect(panelShown(), "hidden, it stays hidden when the folder opens again").toBe(false);
    expect(h.calls.filter((c) => c.startsWith("terminalClose")), "no shell closed").toEqual([]);
  });

  it("with the focus in the terminal Ctrl+W, Ctrl+P, Ctrl+B and Ctrl+Shift+F don't reach the Editor, and Ctrl+` still hides the panel", async () => {
    await open();
    await clickRow("src");
    await clickRow("src/app.ts");
    await click("Terminal");
    expect(panelShown()).toBe(true);
    for (const k of ["w", "p", "b"]) expect(await keyInTerminal(k), `Ctrl+${k.toUpperCase()} is left to the shell`).toBe(false);
    expect(await keyInTerminal("f", true), "Ctrl+Shift+F is left to the terminal's find").toBe(false);
    expect(tabs().map((t) => t.props["data-path"]), "Ctrl+W closed no file").toEqual(["src/app.ts"]);
    expect([h.goto, h.search], "Ctrl+P opened no Go to file, Ctrl+Shift+F no search in the folder").toEqual([null, null]);
    expect(treeShown(), "Ctrl+B left the file tree").toBe(true);
    await backquote(false, true);
    expect(panelShown(), "Ctrl+` from inside the terminal hides the panel").toBe(false);
    // the same keys outside the terminal are the Editor's
    await key("b");
    expect(treeShown()).toBe(false);
    await key("p");
    expect(h.goto).not.toBe(null);
  });

  it("is off while an agent works on the card, its tooltip saying why; the button, + and both keys start nothing", async () => {
    folderInfo = folder({ name: "GA-118", kind: "card", taskId: "t118", readOnly: true, readOnlyWhy: "QA Agent is working on GA-118" });
    await open();
    const b = termButton();
    expect([b["aria-disabled"], b.disabled, b.title, b["aria-pressed"]]).toEqual([true, undefined, "QA Agent is working on GA-118: a terminal there waits until its run ends.", false]);
    await click("Terminal");
    await backquote();
    await backquote(true);
    expect([opens(), panelShown()]).toEqual([[], false]);
  });

  it("a terminal opened before the agent started stays: the button still shows and hides the panel, and + is off saying why", async () => {
    folderInfo = folder({ name: "GA-118", kind: "card", taskId: "t118", readOnly: true, readOnlyWhy: "QA Agent is working on GA-118" });
    h.terms = [session({ name: "GA-118", taskId: "t118" })];
    await open();
    expect([termButton()["aria-disabled"], termButton().title]).toEqual([undefined, "Show the terminal (Ctrl+`)"]);
    await click("Terminal");
    expect(panelShown()).toBe(true);
    expect([plus()["aria-disabled"], plus().title]).toEqual([true, "QA Agent is working on GA-118: a terminal there waits until its run ends."]);
    await plus().onClick();
    await settle();
    await backquote(true);
    expect(opens(), "+ and Ctrl+Shift+` start nothing").toEqual([]);
    expect(panelShown()).toBe(true);
    await click("Terminal");
    expect(panelShown()).toBe(false);
  });

  it("on a card that isn't pinned to you the button is off until Edit anyway, then on", async () => {
    folderInfo = folder({ name: "GA-112", kind: "card", taskId: "t112" });
    task = { id: "t112", identifier: "GA-112", title: "Theirs", byHand: false, assigneeId: "a2", assigneeName: "Frontend Agent" } as unknown as Task;
    await open();
    expect([termButton()["aria-disabled"], termButton().title]).toEqual([true, "GA-112 is Frontend Agent's card: press Edit anyway first, since a shell can change its files."]);
    await click("Terminal");
    await backquote();
    expect(opens()).toEqual([]);
    await click("Edit anyway");
    expect([termButton()["aria-disabled"], termButton().title]).toEqual([undefined, "Show the terminal (Ctrl+`)"]);
    await click("Terminal");
    expect([opens(), panelShown()]).toEqual([[`terminalOpen ${FOLDER} 0 0 null`], true]);
  });

  it("on your own card, pinned to you by hand, the button is on at once", async () => {
    folderInfo = folder({ name: "GA-21", kind: "card", taskId: "t21" });
    task = { id: "t21", identifier: "GA-21", title: "Mine", byHand: true, assigneeId: "u1", assigneeName: "You" } as unknown as Task;
    await open();
    expect([termButton()["aria-disabled"], termButton().title]).toEqual([undefined, "Show the terminal (Ctrl+`)"]);
  });
});
