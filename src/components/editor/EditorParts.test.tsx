// GA-113 QA: the Editor's smaller parts. The file tree: its rows from the loaded levels and the open folders, git's letter
// (M, A, R, U, C) on a changed file and a dot on a folder with changes inside, an ignored entry greyed, what is in an
// ignored folder read-only (a lock, its menu's New, Rename and Delete off and saying why), the whole folder read-only
// while an agent works (New file and New folder off with the reason), a click or Enter opening a file. Go to file over
// editorFiles (letters in order, Enter or a click opens). Search in the folder over editorSearch with Match case, Words and
// Regular expression, the hits by file, a hit opening its file at its line. The folder picker's groups, its filter box,
// and a pick going to #/editor/<id>. Open in (only the editors found, openInApp with the folder, file and line), and the
// find panel's count and its Match case, Words and Regular expression. There is no DOM here, so a small hook runtime
// stands in for React (as in src/pages/DocPageSave.test.tsx): components are called as functions and keep their hooks
// between renders by their place in the tree, effects run after each render; the api is a fake that records its calls,
// portals render in place, `window` is an EventTarget, and fake timers stand in for the clock.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useState, type ReactElement, type ReactNode } from "react";
import { EditorState, type TransactionSpec } from "@codemirror/state";
import { search } from "@codemirror/search";
import type { EditorView } from "@codemirror/view";
import type { DirEntry, EditorApp, EditorFolder, FolderGit, Project, SearchHit, SearchResult, Settings, Task } from "../../types";
import type { Route } from "../../router";
import type { Level } from "../../lib/editor";
import type { TreeActions } from "./FileTree";
import type { SearchState } from "./FolderSearch";
import type { MenuItem } from "./Menus";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
const h = vi.hoisted(() => ({
  cur: null as Inst | null,
  dirty: false,
  effects: [] as (() => void)[],
  /** What the fake api answers, by function name; a function without an answer never settles. */
  api: {} as Record<string, (...a: unknown[]) => Promise<unknown>>,
  calls: [] as [string, unknown[]][],
  went: [] as Route[],
  /** What the find panel's root was last asked to render. */
  panel: null as ReactNode,
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
// Portals render where they are; the find panel's React root keeps what it is asked to render, drawn by the runtime.
vi.mock("react-dom", async (orig) => ({ ...(await orig<object>()), createPortal: (n: ReactNode) => n, flushSync: (f: () => void) => f() }));
vi.mock("react-dom/client", () => ({ createRoot: () => ({ render: (el: ReactNode) => { h.panel = el; }, unmount: () => {} }) }));
vi.mock("../../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) {
    if (typeof real[k] === "function") out[k] = (...a: unknown[]) => { h.calls.push([k, a]); const f = h.api[k]; return f ? f(...a) : new Promise(() => {}); };
  }
  return { ...real, ...out };
});
vi.mock("../../router", async (orig) => ({ ...(await orig<typeof import("../../router")>()), go: (r: Route) => { h.went.push(r); } }));

const store = new Map<string, string>();
vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, String(v)); }, removeItem: (k: string) => { store.delete(k); } });
const win = Object.assign(new EventTarget(), { innerWidth: 1280, innerHeight: 800, location: { hash: "" } });
vi.stubGlobal("window", win);
vi.stubGlobal("document", { body: {}, activeElement: null, documentElement: { style: {} }, createElement: () => ({ className: "", setAttribute: () => {}, querySelector: () => null }) });

const { FileTree } = await import("./FileTree");
const { GoToFile } = await import("./GoToFile");
const { FolderSearch } = await import("./FolderSearch");
const { FolderPicker } = await import("./FolderPicker");
const { openInItems, useEditorApps, OpenInMenu, NO_APP } = await import("./Menus");
const { findPanel } = await import("./FindPanel");
const { href } = await import("../../router");
const { OPENED_KEY } = await import("../../lib/editor");

// ---- the hook runtime ----
let root: () => ReactNode = () => null;
const insts = new Map<string, Inst>();
const unmount = (inst: Inst) => { for (const s of inst.slots) if ((s as EffectSlot)?.effect) (s as EffectSlot).cleanup?.(); };
const keyOf = (n: unknown) => (n && typeof n === "object" && "key" in n ? (n as { key: string | null }).key : null);
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
  return { ...el, props: { ...el.props, children: expand(el.props.children as ReactNode, `${path}>${String(el.type)}`, seen) } } as ReactNode;
}
let tree: ReactNode = null;
function render() {
  for (let n = 0; n < 30; n++) {
    h.dirty = false;
    const seen = new Set<string>();
    tree = expand(root(), "", seen);
    for (const [k, inst] of insts) if (!seen.has(k)) { unmount(inst); insts.delete(k); }
    for (const f of h.effects.splice(0)) f();
    if (!h.dirty) return;
  }
  throw new Error("the render never settled");
}
/** Lets the api's answers arrive (a few turns of promises), then draws again. */
async function settle() { await vi.advanceTimersByTimeAsync(0); await vi.advanceTimersByTimeAsync(0); await vi.advanceTimersByTimeAsync(0); render(); }
async function wait(ms: number) { await vi.advanceTimersByTimeAsync(ms); await settle(); }
function show(node: () => ReactNode) { root = node; render(); }

// ---- finding things on screen ----
type Found = { type: unknown; props: Record<string, unknown>; el: ReactNode };
function findAll(node: ReactNode, pred: (f: Found) => boolean, out: Found[] = []): Found[] {
  if (Array.isArray(node)) node.forEach((n) => findAll(n, pred, out));
  else if (node && typeof node === "object" && "props" in node) {
    const el = node as ReactElement<Record<string, unknown>>;
    const f = { type: el.type, props: el.props, el };
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
const text = (f: Found) => textOf(f.el);
const one = (found: Found[], what: string) => { if (found.length !== 1) throw new Error(`${found.length} of ${what}`); return found[0]!; };
const byLabel = (label: string, within: ReactNode = tree) => one(findAll(within, (f) => f.props["aria-label"] === label && typeof f.type === "string"), `"${label}"`);
const byClass = (cls: string, within: ReactNode = tree) => findAll(within, (f) => typeof f.props.className === "string" && f.props.className.split(" ").includes(cls));
const buttonsNamed = (name: string, within: ReactNode = tree) => findAll(within, (f) => f.type === "button" && text(f).trim() === name);
const button = (name: string, within: ReactNode = tree) => one(buttonsNamed(name, within), `button "${name}"`);
const ev = (more: Record<string, unknown> = {}) => ({ preventDefault: () => {}, stopPropagation: () => {}, ...more });
/** Calls an element's handler (onClick, onChange…), then lets what it started finish and draws again. */
async function fire(f: Found, handler: string, e: unknown = ev()) {
  const fn = f.props[handler];
  if (typeof fn !== "function") throw new Error(`no ${handler}`);
  await fn(e);
  await settle();
}
const typeIn = (f: Found, value: string) => fire(f, "onChange", ev({ target: { value }, currentTarget: { value } }));
const key = (f: Found, k: string, more: Record<string, unknown> = {}) =>
  fire(f, "onKeyDown", ev({ key: k, shiftKey: false, ctrlKey: false, metaKey: false, target: { tagName: "DIV", getBoundingClientRect: () => ({ left: 10, bottom: 40 }) }, ...more }));
const calls = (name: string) => h.calls.filter(([k]) => k === name).map(([, a]) => a);

beforeEach(() => {
  vi.useFakeTimers();
  root = () => null;
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0;
  h.api = {};
  h.calls.length = 0;
  h.went.length = 0;
  h.panel = null;
  store.clear();
});
afterEach(() => { show(() => null); vi.useRealTimers(); });

// ---- the file tree ----

const file = (path: string, more: Partial<DirEntry> = {}): DirEntry =>
  ({ name: path.slice(path.lastIndexOf("/") + 1), path, dir: false, size: 120, ignored: false, git: null, ...more });
const folder = (path: string, more: Partial<DirEntry> = {}): DirEntry => file(path, { dir: true, size: 0, ...more });

/** A Laravel project: app (changes inside, open), vendor (ignored, open), storage (open, empty), logs (open, unreadable),
 *  the ignored .env, a README and a dump over 1 MB at the top. */
const LEVELS = new Map<string, Level>([
  ["", [folder("app"), folder("vendor", { ignored: true }), folder("storage"), folder("logs"), file(".env", { ignored: true }), file("README.md"), file("dump.sql", { size: 2 * 1024 * 1024 })]],
  ["app", [folder("app/Models"), file("app/Invoice.php"), file("app/Order.php"), file("app/Moved.php"), file("app/New.php"), file("app/Merge.php"), file("app/Same.php")]],
  ["vendor", [folder("vendor/laravel", { ignored: true }), file("vendor/autoload.php", { ignored: true })]],
  ["storage", []],
  ["logs", { error: "Can't read this folder: permission denied." }],
]);
const OPEN = new Set(["app", "app/Models", "vendor", "storage", "logs"]);
const GIT: FolderGit = {
  branch: "main", uncommitted: 6,
  files: { "app/Invoice.php": "modified", "app/Order.php": "added", "app/Moved.php": "renamed", "app/New.php": "untracked", "app/Merge.php": "conflicted", "app/Models/User.php": "modified" },
};

const did: string[] = [];
const refuse = { create: null as string | null, rename: null as string | null, remove: null as string | null };
const act: TreeActions = {
  toggle: (dir) => { did.push(`toggle ${dir}`); },
  openFile: (path) => { did.push(`open ${path}`); },
  create: async (path, dir) => { did.push(`create ${path}${dir ? " (folder)" : ""}`); return refuse.create; },
  rename: async (from, to) => { did.push(`rename ${from} → ${to}`); return refuse.rename; },
  remove: async (path, dir) => { did.push(`remove ${path}${dir ? " (folder)" : ""}`); return refuse.remove; },
  foldAll: () => { did.push("fold all"); },
  menuExtra: () => [],
  copyPath: (path) => { did.push(`copy ${path}`); },
};

function showTree(o: { git?: FolderGit | null; levels?: Map<string, Level>; open?: Set<string>; readOnly?: boolean; readOnlyWhy?: string | null } = {}) {
  show(() => (
    <FileTree title="kade-portal" levels={o.levels ?? LEVELS} open={o.open ?? OPEN} git={o.git === undefined ? GIT : o.git} active="app/Invoice.php"
      readOnly={o.readOnly ?? false} readOnlyWhy={o.readOnlyWhy ?? null} act={act} />
  ));
}
const rows = () => findAll(tree, (f) => f.props.role === "treeitem");
const row = (path: string) => one(rows().filter((f) => f.props["data-path"] === path), `row ${path}`);
const markOn = (path: string) => byClass("ed-mark", row(path).el).map((f) => ({ letter: text(f), tip: f.props.title, label: f.props["aria-label"] }));
const hasDot = (path: string) => byClass("ed-dot", row(path).el).length > 0;
const hasLock = (path: string) => findAll(row(path).el, (f) => f.props.className === "ro" && f.props["aria-label"] === "Read-only").length > 0;
const treeBox = () => one(findAll(tree, (f) => f.props.role === "tree"), "the tree");
const menu = () => findAll(tree, (f) => f.props.role === "menu")[0];
const menuItem = (label: string) => one(findAll(menu()!.el, (f) => f.props.role === "menuitem" && text(f).startsWith(label)), `menu item ${label}`);
const rightClick = (path: string) => fire(row(path), "onContextMenu", ev({ clientX: 100, clientY: 200 }));

describe("The file tree", () => {
  beforeEach(() => { did.length = 0; refuse.create = refuse.rename = refuse.remove = null; });

  it("shows the open folders' entries under them, one level deeper, with a note for a loading, an empty and an unreadable folder", () => {
    showTree();
    expect(rows().map((f) => [f.props["data-path"], f.props["aria-level"]])).toEqual([
      ["app", 1], ["app/Models", 2], ["app/Invoice.php", 2], ["app/Order.php", 2], ["app/Moved.php", 2], ["app/New.php", 2], ["app/Merge.php", 2], ["app/Same.php", 2],
      ["vendor", 1], ["vendor/laravel", 2], ["vendor/autoload.php", 2],
      ["storage", 1], ["logs", 1], [".env", 1], ["README.md", 1], ["dump.sql", 1],
    ]);
    expect(row("app").props["aria-expanded"]).toBe(true);
    expect(row("vendor/laravel").props["aria-expanded"]).toBe(false);
    expect(row("README.md").props["aria-expanded"]).toBeUndefined();
    expect(row("app/Invoice.php").props["aria-selected"]).toBe(true);
    expect(text(row("app/Invoice.php")).startsWith("Invoice.php")).toBe(true);
    const notes = byClass("ed-row-note").map((f) => [text(f), f.props.className]);
    expect(notes).toEqual([["Loading…", "ed-row-note"], ["Empty folder", "ed-row-note"], ["Can't read this folder: permission denied.", "ed-row-note error"]]);
    // a folded folder shows none of its entries
    showTree({ open: new Set() });
    expect(rows().map((f) => f.props["data-path"])).toEqual(["app", "vendor", "storage", "logs", ".env", "README.md", "dump.sql"]);
  });

  it("puts git's letter on each changed file, M, A, R, U and C, its name as the tooltip, and none on an unchanged file", () => {
    showTree();
    expect(markOn("app/Invoice.php")).toEqual([{ letter: "M", tip: "Modified", label: "Modified" }]);
    expect(markOn("app/Order.php")).toEqual([{ letter: "A", tip: "Added", label: "Added" }]);
    expect(markOn("app/Moved.php")).toEqual([{ letter: "R", tip: "Renamed", label: "Renamed" }]);
    expect(markOn("app/New.php")).toEqual([{ letter: "U", tip: "Untracked: new, not in git yet", label: "Untracked: new, not in git yet" }]);
    expect(markOn("app/Merge.php")).toEqual([{ letter: "C", tip: "Conflicted: finish the merge in a terminal", label: "Conflicted: finish the merge in a terminal" }]);
    expect(markOn("app/Same.php")).toEqual([]);
    expect(markOn("README.md")).toEqual([]);
    expect(byClass("ed-mark", row("app/Invoice.php").el)[0]!.props.className).toBe("ed-mark modified");
  });

  it("puts a dot on a folder with changes inside (also deeper down), none on a folder without, and no letter on a folder", () => {
    showTree();
    expect(hasDot("app")).toBe(true);
    expect(hasDot("app/Models")).toBe(true);
    expect(row("app").props.title).toBe("Changes inside");
    expect(hasDot("storage")).toBe(false);
    expect(hasDot("vendor")).toBe(false);
    expect(markOn("app")).toEqual([]);
  });

  it("takes the marks from the listing until the folder's git status is there", () => {
    const levels = new Map<string, Level>([["", [folder("src", { git: "modified" }), folder("docs"), file("main.ts", { git: "modified" }), file("new.ts", { git: "untracked" }), file("old.ts")]]]);
    showTree({ git: null, levels, open: new Set() });
    expect(markOn("main.ts").map((m) => m.letter)).toEqual(["M"]);
    expect(markOn("new.ts").map((m) => m.letter)).toEqual(["U"]);
    expect(markOn("old.ts")).toEqual([]);
    expect(hasDot("src")).toBe(true);
    expect(hasDot("docs")).toBe(false);
    // the git status wins once it is there: main.ts was committed meanwhile
    showTree({ git: { branch: "main", uncommitted: 1, files: { "new.ts": "added" } }, levels, open: new Set() });
    expect(markOn("main.ts")).toEqual([]);
    expect(markOn("new.ts").map((m) => m.letter)).toEqual(["A"]);
    expect(hasDot("src")).toBe(false);
  });

  it("greys an ignored entry; an ignored file at the top opens and saves as usual, without a lock", () => {
    showTree();
    expect(row("vendor").props.className).toBe("ed-row ignored");
    expect(row("vendor").props.title).toBe("Ignored by git: read-only, and search skips it");
    expect(row(".env").props.className).toBe("ed-row ignored");
    expect(row(".env").props.title).toBe("Ignored by git: it opens and saves as usual, and isn't committed");
    expect(hasLock(".env")).toBe(false);
    expect(row("README.md").props.className).toBe("ed-row");
    expect(row("app/Invoice.php").props.className).toBe("ed-row");
  });

  it("makes what is inside an ignored folder read-only: greyed, a lock that says Read-only, and why in its tooltip", () => {
    showTree();
    expect(row("vendor/autoload.php").props.className).toBe("ed-row ignored");
    expect(hasLock("vendor/autoload.php")).toBe(true);
    expect(row("vendor/autoload.php").props.title).toBe("Ignored by git: read-only");
    expect(row("vendor/laravel").props.className).toBe("ed-row ignored");
    expect(hasLock("README.md")).toBe(false);
    expect(hasLock("app/Invoice.php")).toBe(false);
  });

  it("doesn't let a file inside an ignored folder be renamed, deleted or added to: its menu's items are off and say why", async () => {
    showTree();
    await rightClick("vendor/autoload.php");
    expect(menu()!.props["aria-label"]).toBe("autoload.php");
    for (const label of ["New file", "New folder", "Rename", "Delete"]) {
      expect(menuItem(label).props.disabled, label).toBe(true);
      expect(menuItem(label).props.title, label).toBe("Ignored by git: read-only");
    }
    expect(menuItem("Copy path").props.disabled).toBeFalsy();
    await fire(menuItem("Copy path"), "onClick");
    expect(did).toEqual(["copy vendor/autoload.php"]);
    expect(menu()).toBeUndefined();
    // F2 and Delete do nothing there either
    await fire(row("vendor/autoload.php"), "onFocus");
    await key(treeBox(), "F2");
    await key(treeBox(), "Delete");
    expect(findAll(tree, (f) => f.type === "input")).toEqual([]);
    expect(findAll(tree, (f) => f.props.role === "alertdialog")).toEqual([]);
    expect(did).toEqual(["copy vendor/autoload.php"]);
  });

  // Design §3/§9: "ignored folders greyed and folded, their files read-only"; in an ignored folder "New, Rename and
  // Delete are off: Ignored by git: read-only". The ignored folder's own menu makes files in it, so New is off there too.
  it("doesn't make new files or folders inside an ignored folder from that folder's own menu", async () => {
    showTree();
    await rightClick("vendor");
    for (const label of ["New file", "New folder"]) {
      expect(menuItem(label).props.disabled, label).toBe(true);
      expect(menuItem(label).props.title, label).toBe("Ignored by git: read-only");
    }
  });

  // The backend refuses changes inside an ignored folder, not to the folder itself (in_ignored_folder looks at the
  // folders above a path), so the folder's own Rename and Delete stay on; an ignored file at the top makes new files
  // next to it, at the top, which isn't ignored.
  it("keeps Rename and Delete on for the ignored folder itself, and New on for an ignored file at the top", async () => {
    showTree();
    await rightClick("vendor");
    for (const label of ["Rename", "Delete"]) {
      expect(menuItem(label).props.disabled, label).toBeFalsy();
      expect(menuItem(label).props.title, label).toBeUndefined();
    }
    await fire(menuItem("Copy path"), "onClick");
    await rightClick(".env");
    for (const label of ["New file", "New folder", "Rename", "Delete"]) {
      expect(menuItem(label).props.disabled, label).toBeFalsy();
      expect(menuItem(label).props.title, label).toBeUndefined();
    }
  });

  it("marks a file over 1 MB read-only with a lock, saying why", () => {
    showTree();
    expect(hasLock("dump.sql")).toBe(true);
    expect(row("dump.sql").props.title).toBe("Read-only: over 1 MB");
  });

  it("turns New file, New folder, Rename and Delete off while the folder is read-only, each saying why", async () => {
    const why = "QA Agent is working on GA-118";
    showTree({ readOnly: true, readOnlyWhy: why });
    for (const label of ["New file", "New folder"]) {
      expect(byLabel(label).props.disabled, label).toBe(true);
      expect(byLabel(label).props.title, label).toBe(why);
    }
    expect(byLabel("Fold every folder").props.disabled).toBeUndefined();
    await rightClick("README.md");
    for (const label of ["New file", "New folder", "Rename", "Delete"]) {
      expect(menuItem(label).props.disabled, label).toBe(true);
      expect(menuItem(label).props.title, label).toBe(why);
    }
    // Copy path still works, and closes the menu
    await fire(menuItem("Copy path"), "onClick");
    await fire(row("README.md"), "onFocus");
    await key(treeBox(), "F2");
    await key(treeBox(), "Delete");
    expect(findAll(tree, (f) => f.type === "input")).toEqual([]);
    expect(findAll(tree, (f) => f.props.role === "alertdialog")).toEqual([]);
    // opening still works: read-only files open to read
    await fire(row("README.md"), "onClick");
    expect(did).toEqual(["copy README.md", "open README.md"]);
    // without a reason it still says Read-only
    showTree({ readOnly: true, readOnlyWhy: null });
    expect(byLabel("New file").props.title).toBe("Read-only");
  });

  it("opens a file on a click with its path, opens and folds a folder, and opens a read-only file too", async () => {
    showTree();
    await fire(row("app/Invoice.php"), "onClick");
    await fire(row("README.md"), "onClick");
    await fire(row("vendor/autoload.php"), "onClick");
    await fire(row("storage"), "onClick");
    expect(did).toEqual(["open app/Invoice.php", "open README.md", "open vendor/autoload.php", "toggle storage"]);
  });

  it("moves with ↑ and ↓ and opens the chosen file with Enter, a folder with →", async () => {
    showTree({ open: new Set() });
    expect(rows().filter((f) => f.props.tabIndex === 0).map((f) => f.props["data-path"])).toEqual(["app"]);
    await key(treeBox(), "ArrowDown");
    await key(treeBox(), "End");
    await key(treeBox(), "ArrowUp");
    expect(rows().filter((f) => f.props.tabIndex === 0).map((f) => f.props["data-path"])).toEqual(["README.md"]);
    await key(treeBox(), "Enter");
    await key(treeBox(), "Home");
    await key(treeBox(), "ArrowRight");
    expect(did).toEqual(["open README.md", "toggle app"]);
  });

  it("makes a new file typed in place, a path making its folders, and says under it why the backend refused", async () => {
    showTree();
    await fire(byLabel("New file"), "onClick");
    const input = () => one(findAll(tree, (f) => f.type === "input"), "the name box");
    expect(input().props.placeholder).toBe("Name, or a path like app/Exports/InvoicesCsv.php");
    refuse.create = "A file app/Exports/Csv.php is already there.";
    await typeIn(input(), "app/Exports/Csv.php");
    await key(input(), "Enter");
    expect(did).toEqual(["create app/Exports/Csv.php"]);
    expect(findAll(tree, (f) => f.props.role === "alert").map(text)).toEqual(["A file app/Exports/Csv.php is already there."]);
    refuse.create = null;
    await typeIn(input(), "app/Exports/Csv2.php");
    expect(findAll(tree, (f) => f.props.role === "alert")).toEqual([]);
    await key(input(), "Enter");
    expect(did).toEqual(["create app/Exports/Csv.php", "create app/Exports/Csv2.php"]);
    expect(findAll(tree, (f) => f.type === "input")).toEqual([]);
  });

  it("renames with F2 in place and deletes after asking under the row", async () => {
    showTree();
    await fire(row("README.md"), "onFocus");
    await key(treeBox(), "F2");
    const input = one(findAll(tree, (f) => f.type === "input"), "the name box");
    expect(input.props.value).toBe("README.md");
    await typeIn(input, "READ.md");
    await key(one(findAll(tree, (f) => f.type === "input"), "the name box"), "Enter");
    expect(did).toEqual(["rename README.md → READ.md"]);
    await key(treeBox(), "Delete");
    const ask = one(findAll(tree, (f) => f.props.role === "alertdialog"), "the question");
    expect(text(ask)).toContain("Delete README.md from disk?");
    expect(did).toEqual(["rename README.md → READ.md"]);
    await fire(button("Delete", ask.el), "onClick");
    expect(did).toEqual(["rename README.md → READ.md", "remove README.md"]);
    expect(findAll(tree, (f) => f.props.role === "alertdialog")).toEqual([]);
  });

  it("closes its menu on Escape", async () => {
    showTree();
    await rightClick("README.md");
    expect(menu()).toBeDefined();
    window.dispatchEvent(Object.assign(new Event("keydown"), { key: "Escape" }));
    render();
    expect(menu()).toBeUndefined();
  });
});

// ---- go to file ----

const FILES = ["README.md", "app/Http/Controllers/InvoiceController.php", "app/Models/Invoice.php", "resources/views/invoices/index.blade.php", "routes/web.php"];
const opened: string[] = [];
let closed = 0;
function showGoTo() {
  show(() => <GoToFile folder="/home/me/Herd/kade-portal" name="kade-portal" onOpen={(p) => { opened.push(p); }} onClose={() => { closed += 1; }} />);
}
const gotoBox = () => byLabel("Go to file in kade-portal");
const results = () => byClass("pal-item");

describe("Go to file", () => {
  beforeEach(() => { opened.length = 0; closed = 0; });

  it("loads the folder's files with editorFiles, saying Loading… until they come", async () => {
    let answer: (f: string[]) => void = () => {};
    h.api.editorFiles = () => new Promise((r) => { answer = r; });
    showGoTo();
    expect(calls("editorFiles")).toEqual([["/home/me/Herd/kade-portal"]]);
    expect(text(byClass("pal-list")[0]!)).toBe("Loading…");
    answer(FILES);
    await settle();
    expect(results().map((f) => f.props.className)).toHaveLength(FILES.length);
  });

  it("finds a file from letters typed in order, the name's match first, the letters in bold and its folder after it", async () => {
    h.api.editorFiles = async () => FILES;
    showGoTo();
    await settle();
    await typeIn(gotoBox(), "invoice");
    expect(results().map((f) => byClass("where", f.el).map(text)[0])).toEqual(["app/Models", "app/Http/Controllers", "resources/views/invoices"]);
    expect(findAll(results()[0]!.el, (f) => f.type === "b").map(text)).toEqual(["Invoice"]);
    expect(results()[0]!.props.className).toBe("pal-item active");
    await typeIn(gotoBox(), "zzz");
    expect(results()).toEqual([]);
    expect(text(byClass("pal-list")[0]!)).toBe("No file found");
    await key(gotoBox(), "Enter");
    expect(opened).toEqual([]);
  });

  it("opens the first file found with Enter, and the next with ↓ then Enter, closing itself", async () => {
    h.api.editorFiles = async () => FILES;
    showGoTo();
    await settle();
    await typeIn(gotoBox(), "invoice");
    await key(gotoBox(), "Enter");
    expect(opened).toEqual(["app/Models/Invoice.php"]);
    expect(closed).toBe(1);
    await key(gotoBox(), "ArrowDown");
    expect(results()[1]!.props.className).toBe("pal-item active");
    await key(gotoBox(), "Enter");
    expect(opened).toEqual(["app/Models/Invoice.php", "app/Http/Controllers/InvoiceController.php"]);
  });

  it("opens a file on a click on it", async () => {
    h.api.editorFiles = async () => FILES;
    showGoTo();
    await settle();
    await typeIn(gotoBox(), "web");
    await fire(results()[0]!, "onClick");
    expect(opened).toEqual(["routes/web.php"]);
    expect(closed).toBe(1);
  });

  it("closes on Escape and on a click outside it, opening nothing", async () => {
    h.api.editorFiles = async () => FILES;
    showGoTo();
    await settle();
    await key(gotoBox(), "Escape");
    await fire(byClass("pal-backdrop")[0]!, "onClick");
    expect(closed).toBe(2);
    expect(opened).toEqual([]);
  });

  it("says why when the files can't be listed", async () => {
    h.api.editorFiles = async () => { throw new Error("not a git repository"); };
    showGoTo();
    await settle();
    expect(text(byClass("pal-list")[0]!)).toBe("Error: not a git repository");
  });
});

// ---- search in the folder ----

const hit = (path: string, line: number, column: number, txt: string): SearchHit => ({ path, line, column, text: txt });
const HITS: SearchHit[] = [
  hit("app/Models/Invoice.php", 7, 7, "class Invoice extends Model"),
  hit("app/Models/Invoice.php", 21, 17, "        return $invoice->total;"),
  hit("app/Http/Controllers/InvoiceController.php", 5, 16, "use App\\Models\\Invoice;"),
];
const hits: SearchHit[] = [];
let searchState: SearchState = { query: "", caseSensitive: false, wholeWord: false, regex: false };
/** The Editor page's part: it keeps the search's state and hands it in. */
function SearchHost() {
  const [state, setState] = useState<SearchState>({ query: "", caseSensitive: false, wholeWord: false, regex: false });
  searchState = state;
  return <FolderSearch folder="/home/me/Herd/kade-portal" name="kade-portal" state={state} setState={setState} onOpen={(x) => { hits.push(x); }} focusKey={1} />;
}
const searchBox = () => byLabel("Search in kade-portal");
const said = () => text(one(findAll(tree, (f) => f.props.role === "status"), "the summary"));
const groups = () => findAll(byLabel("Results").el, (f) => f.props.className === "ed-row");
const hitRows = () => byClass("ed-hit");

describe("Search in the folder", () => {
  beforeEach(() => { hits.length = 0; });

  it("says what it searches, and searches nothing, until something is typed", async () => {
    show(() => <SearchHost />);
    await wait(1000);
    expect(said()).toContain("Searches the files git tracks and new files it doesn't ignore.");
    expect(calls("editorSearch")).toEqual([]);
  });

  it("searches the folder 300 ms after the typing stops, with the query and the toggles off", async () => {
    h.api.editorSearch = async () => ({ hits: HITS, truncated: false }) satisfies SearchResult;
    show(() => <SearchHost />);
    await typeIn(searchBox(), "inv");
    await wait(200);
    await typeIn(searchBox(), "invoice");
    expect(said()).toBe("Searching…");
    await wait(299);
    expect(calls("editorSearch")).toEqual([]);
    await wait(1);
    expect(calls("editorSearch")).toEqual([["/home/me/Herd/kade-portal", "invoice", false, false, false]]);
    expect(said()).toBe("3 results in 2 files");
  });

  it("shows the hits by file, each file's name, folder and count, and each hit's line number and line with the match marked", async () => {
    h.api.editorSearch = async () => ({ hits: HITS, truncated: false });
    show(() => <SearchHost />);
    await typeIn(searchBox(), "invoice");
    await wait(300);
    expect(groups().map((g) => [g.props.title, text(byClass("name", g.el)[0]!), text(byClass("where", g.el)[0]!), text(byClass("count", g.el)[0]!)])).toEqual([
      ["app/Models/Invoice.php", "Invoice.php", "app/Models", "2"],
      ["app/Http/Controllers/InvoiceController.php", "InvoiceController.php", "app/Http/Controllers", "1"],
    ]);
    expect(hitRows().map((f) => [text(byClass("no", f.el)[0]!), text(byClass("tx", f.el)[0]!), findAll(f.el, (x) => x.type === "mark").map(text)])).toEqual([
      ["7", "class Invoice extends Model", ["Invoice"]],
      ["21", "        return $invoice->total;", ["invoice"]],
      ["5", "use App\\Models\\Invoice;", ["Invoice"]],
    ]);
    // a click on a file's row folds its hits
    await fire(groups()[0]!, "onClick");
    expect(groups()[0]!.props["aria-expanded"]).toBe(false);
    expect(hitRows().map((f) => text(byClass("no", f.el)[0]!))).toEqual(["5"]);
  });

  it("marks the match itself on a line indented with tabs, git's column counting a tab as one", async () => {
    // "\treturn $invoice;": git grep --column puts "return" at column 2 (the tab is one character).
    h.api.editorSearch = async () => ({ hits: [hit("app/Jobs/Send.php", 9, 2, "\treturn $invoice;")], truncated: false });
    show(() => <SearchHost />);
    await typeIn(searchBox(), "return");
    await wait(300);
    expect(findAll(hitRows()[0]!.el, (x) => x.type === "mark").map(text)).toEqual(["return"]);
  });

  it("marks what a regular expression finds on lines indented with tabs, and a long line from a little before it, with …", async () => {
    const long = `\t\t${"x".repeat(50)}\treturn $invoice;`;
    h.api.editorSearch = async () => ({ hits: [hit("app/Jobs/Send.php", 9, 3, "\t\treturn $invoice;"), hit("app/Jobs/Send.php", 12, 54, long)], truncated: false });
    show(() => <SearchHost />);
    await fire(byLabel("Regular expression"), "onClick");
    await typeIn(searchBox(), "ret\\w+");
    await wait(300);
    expect(calls("editorSearch").at(-1)).toEqual(["/home/me/Herd/kade-portal", "ret\\w+", false, false, true]);
    expect(hitRows().map((f) => findAll(f.el, (x) => x.type === "mark").map(text))).toEqual([["return"], ["return"]]);
    expect(text(byClass("tx", hitRows()[1]!.el)[0]!)).toBe(`…${"x".repeat(23)}  return $invoice;`);
  });

  it("opens the file at the hit's line and column on a click, or with Enter on the hit, and the first hit with Enter in the box", async () => {
    h.api.editorSearch = async () => ({ hits: HITS, truncated: false });
    show(() => <SearchHost />);
    await typeIn(searchBox(), "invoice");
    await wait(300);
    await fire(hitRows()[1]!, "onClick");
    expect(hits).toEqual([HITS[1]]);
    expect(hits[0]).toMatchObject({ path: "app/Models/Invoice.php", line: 21, column: 17 });
    expect(hitRows().map((f) => f.props["aria-selected"])).toEqual([false, true, false]);
    await key(hitRows()[2]!, "Enter");
    await key(searchBox(), "Enter");
    expect(hits).toEqual([HITS[1], HITS[2], HITS[0]]);
  });

  it("searches again with Match case, Words and Regular expression as they are switched on, showing them pressed", async () => {
    h.api.editorSearch = async () => ({ hits: HITS, truncated: false });
    show(() => <SearchHost />);
    await typeIn(searchBox(), "Invoice");
    await wait(300);
    for (const label of ["Match case", "Words", "Regular expression"]) {
      expect(byLabel(label).props["aria-pressed"], label).toBe(false);
      await fire(byLabel(label), "onClick");
      expect(byLabel(label).props["aria-pressed"], label).toBe(true);
      await wait(300);
    }
    expect(calls("editorSearch").map((a) => a.slice(1))).toEqual([
      ["Invoice", false, false, false], ["Invoice", true, false, false], ["Invoice", true, true, false], ["Invoice", true, true, true],
    ]);
    expect(searchState).toEqual({ query: "Invoice", caseSensitive: true, wholeWord: true, regex: true });
  });

  it("says when nothing is found, and a bad regular expression in words", async () => {
    h.api.editorSearch = async () => ({ hits: [], truncated: false });
    show(() => <SearchHost />);
    await typeIn(searchBox(), "nothing");
    await wait(300);
    expect(said()).toBe("No results. Ignored files (node_modules/, vendor/, .env) aren't searched.");
    h.api.editorSearch = async () => { throw new Error("regex parse error: unclosed group"); };
    await fire(byLabel("Regular expression"), "onClick");
    await typeIn(searchBox(), "(inv");
    await wait(300);
    expect(said()).toBe("Not a valid regular expression: unclosed group.");
    expect(hitRows()).toEqual([]);
  });
});

// ---- the folder picker ----

const ef = (id: string, name: string, kind: EditorFolder["kind"], more: Partial<EditorFolder> = {}): EditorFolder =>
  ({ id, path: id.replace("/home/me", "~"), name, kind, projectId: null, taskId: null, readOnly: false, readOnlyWhy: null, ...more });
const FOLDERS: EditorFolder[] = [
  ef("/home/me/Herd/kade-portal", "kade-portal", "project", { projectId: "p1" }),
  ef("/home/me/Herd/blog", "blog", "root"),
  ef("/home/me/Herd/shop", "shop", "root"),
  ef("/home/me/.local/share/gizai/worktrees/GA/GA-118", "GA-118", "card", { taskId: "t1", readOnly: true, readOnlyWhy: "QA Agent is working on GA-118" }),
  ef("/home/me/.local/share/gizai/worktrees/GA/GA-120", "GA-120", "card", { taskId: "t2" }),
];
const task = (id: string, title: string, byHand: boolean) => ({ id, identifier: id, title, stateCategory: "in_progress", byHand }) as Task;
let pickerClosed = 0;
function showPicker() {
  h.api.getSettings = async () => ({ editorRoots: ["~/Herd"] }) as Settings;
  h.api.listTasks = async () => [task("t1", "The Editor page", false), task("t2", "Fix the login", true)];
  h.api.listProjects = async () => [{ id: "p1", name: "Kade portal", color: "var(--c-yellow)" } as Project];
  show(() => <FolderPicker at={{ x: 40, y: 120 }} onClose={() => { pickerClosed += 1; }} />);
}
const pickerGroupsShown = () => findAll(tree, (f) => f.props.role === "group").map((g) => [g.props["aria-label"], findAll(g.el, (f) => f.props.role === "option").map((o) => o.props.title)]);
// a card row's tooltip starts with the card's title, on its own line (GA-146): the path is its last line
const option = (path: string) => one(findAll(tree, (f) => f.props.role === "option" && String(f.props.title).split("\n").at(-1) === path), `option ${path}`);
const findBox = () => byLabel("Find a folder");

describe("The folder picker", () => {
  beforeEach(() => { pickerClosed = 0; });

  it("lists Projects, In ~/Herd and Cards, the cards worked on by hand first", async () => {
    h.api.editorFolders = async () => FOLDERS;
    showPicker();
    expect(text(byClass("pop-empty")[0]!)).toBe("Loading…");
    await settle();
    expect(pickerGroupsShown()).toEqual([
      ["Projects", ["~/Herd/kade-portal"]],
      ["In ~/Herd", ["~/Herd/blog", "~/Herd/shop"]],
      ["Cards", ["Fix the login\n~/.local/share/gizai/worktrees/GA/GA-120", "The Editor page\n~/.local/share/gizai/worktrees/GA/GA-118"]],
    ]);
    expect(text(byClass("label", option("~/Herd/kade-portal").el)[0]!)).toBe("Kade portal");
    const says = (path: string) => ["card-id", "label", "stand", "where"].map((c) => byClass(c, option(path).el).map(text).join(""));
    expect(says("~/.local/share/gizai/worktrees/GA/GA-118")).toEqual(["GA-118", "The Editor page", "QA Agent is working", ""]);
    expect(says("~/.local/share/gizai/worktrees/GA/GA-120")).toEqual(["GA-120", "Fix the login", "by hand", ""]);
    expect(says("~/Herd/blog")).toEqual(["", "blog", "", "~/Herd/blog"]);
    expect(option("~/Herd/kade-portal").props["aria-selected"]).toBe(true);
    expect(text(byClass("ed-picker-foot")[0]!)).toContain("the folders in ~/Herd and card worktrees");
    expect(findAll(tree, (f) => f.type === "a")[0]!.props.href).toBe("#/settings/general");
  });

  it("filters every group with the Find a folder box, by name and path", async () => {
    h.api.editorFolders = async () => FOLDERS;
    showPicker();
    await settle();
    await typeIn(findBox(), "sho");
    expect(pickerGroupsShown()).toEqual([["In ~/Herd", ["~/Herd/shop"]]]);
    await typeIn(findBox(), "worktrees");
    expect(pickerGroupsShown().map(([g]) => g)).toEqual(["Cards"]);
    await typeIn(findBox(), "nothing like it");
    expect(pickerGroupsShown()).toEqual([]);
    expect(text(byClass("pop-empty")[0]!)).toBe("No folder found");
  });

  it("opens a picked folder's page at #/editor/<its id, encoded>, closing, and adds it at the bottom of the sidebar's Editor list (GA-157)", async () => {
    h.api.editorFolders = async () => FOLDERS;
    store.set(OPENED_KEY, JSON.stringify(["/home/me/Herd/kade-portal"]));
    showPicker();
    await settle();
    await fire(option("~/Herd/blog"), "onClick");
    expect(pickerClosed).toBe(1);
    expect(h.went).toEqual([{ page: "editor", id: "/home/me/Herd/blog" }]);
    expect(href(h.went[0]!)).toBe("#/editor/%2Fhome%2Fme%2FHerd%2Fblog");
    expect(JSON.parse(store.get(OPENED_KEY)!)).toEqual(["/home/me/Herd/kade-portal", "/home/me/Herd/blog"]);
  });

  // GA-157 QA: a folder already in the sidebar's Editor list stays where it is when the picker opens it
  it("leaves a picked folder that is already in the sidebar's Editor list where it is, every other row too", async () => {
    h.api.editorFolders = async () => FOLDERS;
    const list = ["/home/me/Herd/kade-portal", "/home/me/Herd/blog", "/home/me/Herd/shop"];
    store.set(OPENED_KEY, JSON.stringify(list));
    showPicker();
    await settle();
    await fire(option("~/Herd/shop"), "onClick");
    expect(h.went).toEqual([{ page: "editor", id: "/home/me/Herd/shop" }]);
    expect(JSON.parse(store.get(OPENED_KEY)!)).toEqual(list);
    showPicker();
    await settle();
    await fire(option("~/Herd/kade-portal"), "onClick");
    expect(JSON.parse(store.get(OPENED_KEY)!)).toEqual(list);
  });

  it("opens the chosen folder with Enter, ↓ choosing the next, and the first of what the filter leaves", async () => {
    h.api.editorFolders = async () => FOLDERS;
    showPicker();
    await settle();
    await key(findBox(), "ArrowDown");
    expect(option("~/Herd/blog").props["aria-selected"]).toBe(true);
    await key(findBox(), "Enter");
    await typeIn(findBox(), "GA-118");
    await key(findBox(), "Enter");
    expect(h.went.map((r) => href(r))).toEqual(["#/editor/%2Fhome%2Fme%2FHerd%2Fblog", `#/editor/${encodeURIComponent("/home/me/.local/share/gizai/worktrees/GA/GA-118")}`]);
  });

  it("closes on Escape, opening nothing", async () => {
    h.api.editorFolders = async () => FOLDERS;
    showPicker();
    await settle();
    await key(byLabel("Open a folder"), "Escape");
    expect(pickerClosed).toBe(1);
    expect(h.went).toEqual([]);
  });

  it("says how to add folders when there are none, and why when they can't be listed", async () => {
    h.api.editorFolders = async () => [];
    showPicker();
    await settle();
    expect(text(byClass("pop-empty")[0]!)).toContain("No folders yet.");
    show(() => null);
    h.api.editorFolders = async () => { throw new Error("no database"); };
    showPicker();
    await settle();
    expect(text(byClass("pop-empty")[0]!)).toBe("Error: no database");
  });

  // GA-135: the Terminal page's New terminal uses the picker with its own title and onPick
  it("titled New terminal in, hands the picked folder to onPick, and keeps a card an agent works on off, saying why", async () => {
    h.api.editorFolders = async () => FOLDERS;
    h.api.getSettings = async () => ({ editorRoots: ["~/Herd"] }) as Settings;
    h.api.listTasks = async () => [task("t1", "The Editor page", false), task("t2", "Fix the login", true)];
    h.api.listProjects = async () => [{ id: "p1", name: "Kade portal", color: "var(--c-yellow)" } as Project];
    const picked: string[] = [];
    show(() => <FolderPicker at={{ x: 40, y: 120 }} title="New terminal in" onPick={(f) => picked.push(f.id)} onClose={() => { pickerClosed += 1; }} />);
    await settle();
    expect(text(byClass("ed-picker-title")[0]!)).toBe("New terminal in");
    expect(byLabel("New terminal in").props.role).toBe("dialog");
    const busy = one(findAll(tree, (f) => f.props.role === "option" && String(f.props.title).includes("\nQA Agent")), "GA-118");
    expect(busy.props.title).toBe("The Editor page\nQA Agent is working on GA-118: a terminal there waits until its run ends.");
    expect(busy.props["aria-disabled"]).toBe(true);
    await fire(busy, "onClick");
    await typeIn(findBox(), "GA-118");
    await key(findBox(), "Enter");
    expect([picked, pickerClosed]).toEqual([[], 0]);
    await typeIn(findBox(), "");
    expect(option("~/.local/share/gizai/worktrees/GA/GA-120").props["aria-disabled"]).toBeUndefined();
    await fire(option("~/.local/share/gizai/worktrees/GA/GA-120"), "onClick");
    expect([picked, pickerClosed]).toEqual([["/home/me/.local/share/gizai/worktrees/GA/GA-120"], 1]);
    expect(h.went).toEqual([]); // it doesn't open the folder in the Editor
  });

  // GA-146 QA: the full title leads a card row's tooltip; project and folder rows keep the path alone
  it("starts a card row's tooltip with the card's full title, a long one too; project and folder rows keep the path alone", async () => {
    const long = "Figma 3: overview, newsletters, recruitment, campaigns and exports, with every page of the brandbook redrawn";
    h.api.editorFolders = async () => FOLDERS;
    showPicker();
    h.api.listTasks = async () => [task("t1", long, false), task("t2", "Fix the login", true)];
    show(() => null);
    show(() => <FolderPicker at={{ x: 40, y: 120 }} onClose={() => { pickerClosed += 1; }} />);
    await settle();
    expect(option("~/.local/share/gizai/worktrees/GA/GA-118").props.title).toBe(`${long}\n~/.local/share/gizai/worktrees/GA/GA-118`);
    expect(text(byClass("label", option("~/.local/share/gizai/worktrees/GA/GA-118").el)[0]!)).toBe(long);
    expect(option("~/Herd/kade-portal").props.title).toBe("~/Herd/kade-portal");
    expect(option("~/Herd/blog").props.title).toBe("~/Herd/blog");
    expect(findAll(tree, (f) => f.props.role === "option").map((o) => o.props["data-kind"])).toEqual(["project", "root", "root", "card", "card"]);
  });

  it("gives a card it doesn't know the title of (its task not listed) the path alone as its tooltip, without an empty first line", async () => {
    h.api.editorFolders = async () => FOLDERS;
    showPicker();
    h.api.listTasks = async () => { throw new Error("no database"); };
    show(() => null);
    show(() => <FolderPicker at={{ x: 40, y: 120 }} onClose={() => { pickerClosed += 1; }} />);
    await settle();
    expect(pickerGroupsShown().at(-1)).toEqual(["Cards", ["~/.local/share/gizai/worktrees/GA/GA-118", "~/.local/share/gizai/worktrees/GA/GA-120"]]);
    expect(text(byClass("label", option("~/.local/share/gizai/worktrees/GA/GA-120").el)[0]!)).toBe("");
  });
});

// ---- Open in ----

const APPS: EditorApp[] = [{ id: "vscode", name: "VS Code" }, { id: "zed", name: "Zed" }];
let apps: EditorApp[] | null = null;
function AppsHost() { apps = useEditorApps(); return null; }
const run = (item: MenuItem) => { if (!("run" in item)) throw new Error("not an item"); item.run(); };

describe("Open in", () => {
  it("lists one item per editor found, which opens the folder at the file and line with openInApp, or says why there is none", async () => {
    const failed: string[] = [];
    expect(openInItems(null, "/f", null, null, () => {})).toEqual([{ note: "Looking for editors…" }]);
    expect(openInItems([], "/f", null, null, () => {})).toEqual([{ note: NO_APP }]);
    const items = openInItems(APPS, "/home/me/Herd/kade-portal", "app/Models/Invoice.php", 12, (e) => { failed.push(e); });
    expect(items.map((i) => ("label" in i ? i.label : null))).toEqual(["Open in VS Code", "Open in Zed"]);
    h.api.openInApp = async () => { throw new Error("zed: not found"); };
    run(items[1]!);
    await settle();
    expect(calls("openInApp")).toEqual([["zed", "/home/me/Herd/kade-portal", "app/Models/Invoice.php", 12]]);
    expect(failed).toEqual(["Error: zed: not found"]);
  });

  it("asks the computer for its editors once, and again only after the asking failed", async () => {
    h.api.editorApps = async () => { throw new Error("no"); };
    show(() => <AppsHost />);
    expect(apps).toBeNull();
    await settle();
    expect(apps).toEqual([]);
    show(() => null);
    h.api.editorApps = async () => APPS;
    show(() => <AppsHost />);
    await settle();
    expect(apps).toEqual(APPS);
    show(() => null);
    show(() => <AppsHost />);
    await settle();
    expect(apps).toEqual(APPS);
    expect(calls("editorApps")).toHaveLength(2);
  });

  it("on the top bar, lists only the editors found and opens the folder at the open file's line in the one chosen", async () => {
    h.api.editorApps = async () => APPS;
    h.api.openInApp = async () => undefined;
    show(() => <OpenInMenu folder="/home/me/Herd/kade-portal" name="kade-portal" path="app/Models/Invoice.php" line={12} fail={() => {}} />);
    await settle();
    await fire(one(findAll(tree, (f) => f.props["aria-haspopup"] === "menu"), "the Open in button"), "onClick");
    const list = menu()!;
    expect(findAll(list.el, (f) => f.props.role === "menuitem").map(text)).toEqual(["VS Code", "Zed"]);
    expect(byClass("pop-note", list.el).map(text)).toEqual(["At Invoice.php, line 12"]);
    await fire(button("Zed", list.el), "onClick");
    expect(calls("openInApp")).toEqual([["zed", "/home/me/Herd/kade-portal", "app/Models/Invoice.php", 12]]);
    expect(menu()).toBeUndefined();
  });
});

// ---- find and replace in the file ----

type FakeView = { state: EditorState; dispatch: (spec: TransactionSpec) => void; plugin: () => null; root: { activeElement: null }; focus: () => void };
/** A stand-in EditorView: a real EditorState with the search extension, no DOM. */
function fakeView(doc: string, readOnly = false): FakeView {
  const v: FakeView = {
    state: EditorState.create({ doc, extensions: [search({ top: true }), EditorState.readOnly.of(readOnly)] }),
    dispatch: (spec) => { v.state = v.state.update(spec).state; },
    plugin: () => null, root: { activeElement: null }, focus: () => {},
  };
  return v;
}
function showFind(v: FakeView) {
  const panel = findPanel(v as unknown as EditorView);
  expect(panel.top).toBe(true);
  const el = h.panel;
  show(() => el);
}
const findBoxIn = () => byLabel("Find");
const count = () => text(one(findAll(tree, (f) => f.props.role === "status"), "the count"));
const DOC = "Invoice one\ninvoice two\nan invoices list\nINVOICE";

describe("Find and replace in the file", () => {
  it("counts the matches as you type, and where the selection is among them", async () => {
    const v = fakeView(DOC);
    showFind(v);
    expect(count()).toBe("");
    await typeIn(findBoxIn(), "invoice");
    expect(count()).toBe("4 results");
    await key(findBoxIn(), "Enter");
    expect(v.state.selection.main).toMatchObject({ from: 0, to: 7 });
    expect(count()).toBe("1 of 4");
    await key(findBoxIn(), "Enter");
    expect(count()).toBe("2 of 4");
    await key(findBoxIn(), "Enter", { shiftKey: true });
    expect(count()).toBe("1 of 4");
    await typeIn(findBoxIn(), "nowhere");
    expect(count()).toBe("No results");
  });

  it("narrows with Match case, Words and Regular expression, shown pressed, and says when the expression isn't valid", async () => {
    const v = fakeView(DOC);
    showFind(v);
    await typeIn(findBoxIn(), "invoice");
    await fire(byLabel("Match case"), "onClick");
    expect(byLabel("Match case").props["aria-pressed"]).toBe(true);
    expect(count()).toBe("2 results");
    await fire(byLabel("Words"), "onClick");
    expect(count()).toBe("1 result");
    await fire(byLabel("Match case"), "onClick");
    expect(count()).toBe("3 results");
    await fire(byLabel("Regular expression"), "onClick");
    await typeIn(findBoxIn(), "invoice(");
    expect(count()).toBe("Not a valid regular expression");
    await typeIn(findBoxIn(), "inv[a-z]+");
    expect(count()).toBe("4 results");
  });

  it("replaces the next match and all of them, and has no replace box in a read-only file", async () => {
    const v = fakeView("a cat, a cat, a cat");
    showFind(v);
    await typeIn(findBoxIn(), "cat");
    await typeIn(byLabel("Replace"), "dog");
    await fire(button("Replace"), "onClick");
    await fire(button("Replace"), "onClick");
    expect(v.state.doc.toString()).toBe("a dog, a cat, a cat");
    await key(byLabel("Replace"), "Enter");
    expect(v.state.doc.toString()).toBe("a dog, a dog, a cat");
    await fire(button("Replace all"), "onClick");
    expect(v.state.doc.toString()).toBe("a dog, a dog, a dog");
    // Ctrl+Enter in the replace box replaces them all
    show(() => null);
    const w = fakeView("one cat, two cats");
    showFind(w);
    await typeIn(findBoxIn(), "cat");
    await typeIn(byLabel("Replace"), "dog");
    await key(byLabel("Replace"), "Enter", { ctrlKey: true });
    expect(w.state.doc.toString()).toBe("one dog, two dogs");
    show(() => null);
    showFind(fakeView("read me", true));
    expect(findAll(tree, (f) => f.props["aria-label"] === "Replace")).toEqual([]);
    expect(buttonsNamed("Replace all")).toEqual([]);
  });
});
