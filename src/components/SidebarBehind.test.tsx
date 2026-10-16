// GA-114 QA: the sidebar's Editor section and behind main (F3). Each opened folder's row says how it stands against
// main as last fetched ("3 behind"), "updating" while Update runs, and loses its "3 behind" when the update ends, without
// a reload: it follows what the Editor page tells it (src/lib/behind.ts tellMain) and the editor-updated event. Folded,
// the heading says how many folders are behind. The sidebar runs in a small hook runtime (as in EditorPage.test.tsx):
// components are called as functions, keep their hooks by their place in the tree, and effects run after each render;
// its data (useData, the team, the live runs, the drawers) is handed in, and contract C3 is a fake.
// GA-151 QA: a card's row keeps its identifier whole and its title on one line (the ellipsis class), and its tooltip starts
// with the full title; a by-hand row's tooltip, a row whose card isn't known and the folder rows stay as they were.
// GA-177 QA: the rows are asked again every 30 seconds (not while the window is hidden) and on the window's focus, but a
// focus 5 seconds or less after the last ask asks nothing (on Omarchy the focus follows the mouse).
// GA-190: the 30-second timer asks only git's marks (editor_git); main's standing (editor_main) on a first open and a focus.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import type { EditorFolder, EditorUpdated, FolderGit, FolderMain, Task } from "../types";

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
  // the Terminal section (GA-136) reads the open terminals from their store: the snapshot
  const useSyncExternalStore = ((subscribe: never, snapshot: () => unknown) => (h.cur ? snapshot() : R.useSyncExternalStore(subscribe, snapshot))) as typeof R.useSyncExternalStore;
  const hooks = { useState, useRef, useMemo, useCallback, useEffect, useLayoutEffect: useEffect, useSyncExternalStore };
  return { ...R, default: { ...R, ...hooks }, ...hooks };
});

// ---- two folders under ~/Herd, and a card's worktree; contract C3 as a fake ----
const KADE = "/home/you/Herd/kade-portal";
const API = "/home/you/Herd/invoice-api";
const CARD = "/home/you/.local/share/gizai/worktrees/GA/GA-118";
const FOLDERS: EditorFolder[] = [
  { id: KADE, path: KADE, name: "kade-portal", kind: "root", projectId: null, taskId: null, readOnly: false, readOnlyWhy: null },
  { id: API, path: API, name: "invoice-api", kind: "root", projectId: null, taskId: null, readOnly: false, readOnlyWhy: null },
  { id: CARD, path: CARD, name: "GA-118", kind: "card", projectId: "p1", taskId: "t1", readOnly: false, readOnlyWhy: null },
];
const NOW = Date.UTC(2026, 9, 12, 12, 0, 0);
const mains = new Map<string, FolderMain>();
const gits = new Map<string, FolderGit>();
const listeners: ((e: EditorUpdated) => void)[] = [];
/** What the sidebar asked contract C3 for: "folders", "git <id>", "main <id>" (GA-177). */
const asked: string[] = [];
const main = (more: Partial<FolderMain> = {}): FolderMain => ({
  mainLabel: "origin/main", fetchedAt: NOW - 2 * 60_000, fetchError: null, branch: "main", onDefault: true, behind: 3, ahead: 0, uncommitted: 0,
  canUpdate: true, updateSays: "move main 3 commits to 319bf0b and run npm ci", updating: false, ...more,
});

const state = vi.hoisted(() => ({ data: {} as Record<string, unknown> }));
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) if (typeof real[k] === "function") out[k] = () => new Promise(() => {});
  const as = (key: string) => () => { const p = { key, catch: () => p }; return p; };
  return {
    ...out,
    listProjects: as("projects"), listTasks: as("tasks"), getTeam: as("team"), memoryNotes: as("notes"), listChatThreads: as("threads"),
    editorFolders: () => { asked.push("folders"); return Promise.resolve(FOLDERS); },
    editorApps: () => Promise.resolve([]),
    editorGit: (id: string) => { asked.push(`git ${id}`); return Promise.resolve(gits.get(id) ?? { branch: "main", files: {}, uncommitted: 0 }); },
    editorMain: (id: string) => { asked.push(`main ${id}`); return mains.has(id) ? Promise.resolve(mains.get(id)!) : Promise.reject("no such folder"); },
    onEditorUpdated: (cb: (e: EditorUpdated) => void) => { listeners.push(cb); return Promise.resolve(() => { listeners.splice(listeners.indexOf(cb), 1); }); },
  };
});
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => { key: string }) => ({ data: state.data[fetch().key] ?? null, error: null, reload: () => {}, setData: () => {} }),
}));
vi.mock("../lib/team", async (orig) => ({ ...(await orig<object>()), useCurrentTeam: () => ["t1", () => {}] }));
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));
vi.mock("../lib/drawers", async (orig) => ({ ...(await orig<object>()), useDrawer: () => () => {} }));
vi.mock("../router", async (orig) => ({ ...(await orig<object>()), go: () => {} }));
vi.mock("./chat/useChat", () => ({ useChatLive: () => [] }));
vi.mock("./UpdateNotice", () => ({ UpdateNotice: () => null }));
vi.mock("./editor/FolderPicker", async (orig) => ({ ...(await orig<object>()), FolderPicker: () => null }));

const store = new Map<string, string>();
vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, String(v)); }, removeItem: (k: string) => { store.delete(k); } });
vi.stubGlobal("navigator", { userAgent: "Linux", platform: "Linux x86_64" });
const win = new EventTarget();
vi.stubGlobal("window", win);
const doc = { hidden: false, querySelector: () => null, body: { style: {} }, documentElement: { style: {} } };
vi.stubGlobal("document", doc);

const { Sidebar } = await import("./Sidebar");
const { tellMain } = await import("../lib/behind");

// ---- the hook runtime ----
let root: () => ReactNode = () => null;
const insts = new Map<string, Inst>();
const unmount = (inst: Inst) => { for (const s of inst.slots) if ((s as EffectSlot)?.effect) (s as EffectSlot).cleanup?.(); };
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
const editorSection = () => findAll(tree, (f) => f.props.className === "nav-section" && f.props["aria-label"] === "Editor")[0]!;
/** Each opened folder's row: its name, what it says on the right and its tooltip. */
const rows = () => findAll(editorSection().props.children as ReactNode, (f) => f.type === "a").map((a) => {
  const meta = findAll(a.props.children as ReactNode, (f) => f.props.className === "meta")[0];
  const spin = meta ? findAll(meta.props.children as ReactNode, (f) => String(f.props.className ?? "").includes("spin")).length > 0 : false;
  const name = findAll(a.props.children as ReactNode, (f) => f.type === "span" && f.props.className !== "meta" && f.props.className !== "stand")[0];
  return { name: name ? textOf(name.props.children as ReactNode) : "", says: meta ? textOf(meta.props.children as ReactNode) : "", spin, tip: a.props.title as string };
});
const says = () => Object.fromEntries(rows().map((r) => [r.name, r.says]));
/** The folded heading's tag, or null. */
const tag = () => { const t = findAll(editorSection().props.children as ReactNode, (f) => f.props.className === "fold-tag")[0]; return t ? { word: textOf(t.props.children as ReactNode), tip: t.props.title } : null; };
const caret = () => findAll(editorSection().props.children as ReactNode, (f) => f.type === "button" && String(f.props["aria-label"] ?? "").endsWith("Editor"))[0]!.props.onClick as () => void;
async function ends(e: EditorUpdated) { for (const l of [...listeners]) l(e); await settle(); }

async function show() {
  root = () => <Sidebar route={{ page: "tasks" }} youId="u1" onSearch={() => {}} onNewTask={() => {}} />;
  render();
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0;
  store.clear(); mains.clear(); gits.clear(); listeners.length = 0; asked.length = 0; doc.hidden = false;
  state.data = { projects: [], tasks: [], threads: [], notes: [], team: { id: "t1", name: "Software", members: [], states: [], labels: [] } };
  store.set("gizai.editor.opened", JSON.stringify([KADE, API, CARD]));
  mains.set(KADE, main());
  mains.set(API, main({ behind: 0, canUpdate: false, updateSays: "invoice-api is up to date with origin/main." }));
  mains.set(CARD, main({ branch: "gizai/ga-118-show-a-run-s-changes", onDefault: false, behind: 5, ahead: 2, canUpdate: false, updateSays: "x" }));
  gits.set(API, { branch: "main", files: {}, uncommitted: 4 });
});
afterEach(() => { root = () => null; render(); vi.useRealTimers(); });

describe("the sidebar's Editor rows and behind main (GA-114)", () => {
  it("a folder behind main says '3 behind'; one that's up to date says how many files changed; a card's worktree never says behind", async () => {
    await show();
    expect(says()).toEqual({ "kade-portal": "3 behind", "invoice-api": "4 changed", "GA-118": "" });
    const kade = rows().find((r) => r.name === "kade-portal")!;
    expect(kade.tip).toBe("/home/you/Herd/kade-portal · main is 3 commits behind origin/main, fetched 2m ago");
  });

  it("while Update runs (told by the Editor page) the row says 'updating' with a turning loader, then loses '3 behind' at its end, without a reload", async () => {
    await show();
    tellMain(KADE, main({ updating: true, canUpdate: false }));
    await settle();
    const kade = rows().find((r) => r.name === "kade-portal")!;
    expect(kade.says).toBe("updating");
    expect(kade.spin).toBe(true);
    // the update ends: the backend now says up to date, and editor-updated comes
    mains.set(KADE, main({ behind: 0, canUpdate: false, updateSays: "kade-portal is up to date with origin/main." }));
    await ends({ folder: KADE, ok: true, said: "Moved main 3 commits to 319bf0b and ran npm ci.", output: "" });
    expect(says()["kade-portal"]).toBe("");
    expect(rows().find((r) => r.name === "kade-portal")!.tip).toBe("/home/you/Herd/kade-portal · main, up to date with origin/main");
  });

  it("an update from the Team Lead's chat (no page open) still clears the row at its end", async () => {
    await show();
    expect(says()["kade-portal"]).toBe("3 behind");
    mains.set(KADE, main({ behind: 0, canUpdate: false, updateSays: "kade-portal is up to date with origin/main." }));
    await ends({ folder: KADE, ok: true, said: "Moved main 3 commits to 319bf0b.", output: "" });
    expect(says()["kade-portal"]).toBe("");
  });

  it("a fetch on the Editor page that finds more commits shows on the row at once", async () => {
    await show();
    tellMain(KADE, main({ behind: 4 }));
    await settle();
    expect(says()["kade-portal"]).toBe("4 behind");
  });

  it("folded, the Editor heading says how many folders are behind, cards' worktrees not counted", async () => {
    await show();
    expect(tag()).toBeNull();
    caret()();
    await settle();
    expect(tag()).toEqual({ word: "1 behind", tip: "1 folder is behind origin/main" });
    mains.set(API, main({ behind: 2 }));
    tellMain(API, main({ behind: 2 }));
    await settle();
    expect(tag()).toEqual({ word: "2 behind", tip: "2 folders are behind origin/main" });
    tellMain(KADE, main({ behind: 0 }));
    tellMain(API, main({ behind: 0 }));
    await settle();
    expect(tag()).toBeNull();
  });
});

// ---- GA-151 QA: a card's row ----
const LONG = "Sidebar Editor list: long card titles run over the rows above and below them, wrapped onto two or three lines";
const cardTask = (more: Partial<Task> = {}) => ({ id: "t1", identifier: "GA-118", title: LONG, byHand: false, ...more }) as Task;
const SAYS = ["meta", "stand", "pulse"];
/** The card's row: the name's parts in order (class, inline style, text), what it says on the right and its tooltip. */
const cardRow = () => {
  const a = findAll(editorSection().props.children as ReactNode, (f) => f.type === "a" && findAll(f.props.children as ReactNode, (s) => s.props.className === "card-id").length > 0)[0]!;
  const parts = findAll(a.props.children as ReactNode, (f) => f.type === "span" && !SAYS.includes(String(f.props.className)))
    .map((s) => ({ cls: s.props.className, style: s.props.style, text: textOf(s.props.children as ReactNode) }));
  const meta = findAll(a.props.children as ReactNode, (f) => f.props.className === "meta")[0];
  return { parts, says: meta ? textOf(meta.props.children as ReactNode) : "", tip: a.props.title as string };
};
const READ_ONLY = `Frontend Agent is working on GA-118: read-only until its run ends`;
// Vitest hands a .css file back empty, even with ?raw, so the stylesheets are read from disk (as in
// FolderPickerTerminal.test.tsx); node:fs comes in through a computed specifier: the build type-checks tests without Node's types.
const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const sheet = (name: string): string => fs.readFileSync(new URL(`../styles/${name}`, import.meta.url), "utf8");
const componentsCss = sheet("components.css"), appCss = sheet("app.css"), terminalCss = sheet("terminal.css");
/** One CSS rule's declarations, by its exact selector. */
const rule = (css: string, sel: string) => {
  const at = css.split("\n").find((l) => l.startsWith(`${sel} {`));
  return at ? at.slice(at.indexOf("{") + 1, at.lastIndexOf("}")).split(";").map((d) => d.trim()).filter(Boolean) : null;
};

describe("the sidebar's Editor rows: a card's title on one line (GA-151)", () => {
  const card = { ...FOLDERS[2]! };
  afterEach(() => { FOLDERS[2] = { ...card }; });

  it("a card's row is its identifier, then its title with the ellipsis class and no inline style; how it stands comes after", async () => {
    state.data.tasks = [cardTask()];
    gits.set(CARD, { branch: "gizai/ga-118", files: {}, uncommitted: 12 });
    await show();
    const row = cardRow();
    expect(row.parts).toEqual([{ cls: "card-id", style: undefined, text: "GA-118" }, { cls: "faint ellipsis", style: undefined, text: LONG }]);
    expect(row.says).toBe("12 changed");
  });

  it("a short title shows the same way, whole", async () => {
    state.data.tasks = [cardTask({ title: "Fix" })];
    await show();
    expect(cardRow().parts).toEqual([{ cls: "card-id", style: undefined, text: "GA-118" }, { cls: "faint ellipsis", style: undefined, text: "Fix" }]);
    expect(cardRow().tip.split("\n")[0]).toBe("Fix");
  });

  it("the tooltip starts with the full title, then a line break, then what it said before (the path and how it stands)", async () => {
    // before: the card isn't known yet, so the row says what it always said
    await show();
    const before = cardRow().tip;
    expect(before).not.toContain("\n");
    expect(before.startsWith(CARD)).toBe(true);
    state.data.tasks = [cardTask()];
    await settle();
    expect(cardRow().tip).toBe(`${LONG}\n${before}`);
  });

  it("while it is updating, the tooltip still starts with the full title", async () => {
    state.data.tasks = [cardTask()];
    await show();
    tellMain(CARD, main({ branch: "gizai/ga-118", onDefault: false, updating: true, updateSays: "run npm ci" }));
    await settle();
    expect(cardRow().says).toBe("updating");
    expect(cardRow().tip).toBe(`${LONG}\n${CARD} · Updating: run npm ci`);
  });

  it("read-only while an agent works on it: the title, then the read-only sentence; the pulse and read-only on the right", async () => {
    FOLDERS[2] = { ...card, readOnly: true, readOnlyWhy: "Frontend Agent is working on GA-118" };
    state.data.tasks = [cardTask()];
    await show();
    expect(cardRow().says).toBe("read-only");
    expect(cardRow().tip).toBe(`${LONG}\n${READ_ONLY}`);
  });

  it("a by-hand row's tooltip is exactly as before: it already starts with the identifier and the title", async () => {
    state.data.tasks = [cardTask({ byHand: true })];
    await show();
    expect(cardRow().says).toBe("by hand");
    expect(cardRow().tip).toBe(`GA-118 ${LONG}. Pinned to you: agents leave it alone`);
    expect(cardRow().parts[1]).toEqual({ cls: "faint ellipsis", style: undefined, text: LONG });
  });

  it("read-only and by hand: read-only wins, so the tooltip starts with the title", async () => {
    FOLDERS[2] = { ...card, readOnly: true, readOnlyWhy: "Frontend Agent is working on GA-118" };
    state.data.tasks = [cardTask({ byHand: true })];
    await show();
    expect(cardRow().says).toBe("read-only");
    expect(cardRow().tip).toBe(`${LONG}\n${READ_ONLY}`);
  });

  it("a row whose card isn't known shows its identifier only, and its tooltip as before, with no title", async () => {
    await show();
    expect(cardRow().parts).toEqual([{ cls: "card-id", style: undefined, text: "GA-118" }]);
    expect(cardRow().tip).toBe(`${CARD} · gizai/ga-118-show-a-run-s-changes is 5 commits behind origin/main, fetched 2m ago · gizai/ga-118-show-a-run-s-changes has 2 commits that origin/main doesn't`);
    FOLDERS[2] = { ...card, readOnly: true, readOnlyWhy: "Frontend Agent is working on GA-118" };
    await show();
    expect(cardRow().tip).toBe(READ_ONLY);
  });

  it("folder rows stay as they were: the name alone, and the tooltip without a title", async () => {
    state.data.tasks = [cardTask()];
    await show();
    const kade = findAll(editorSection().props.children as ReactNode, (f) => f.type === "a" && f.props.title !== cardRow().tip);
    expect(kade.map((a) => findAll(a.props.children as ReactNode, (f) => f.type === "span" && !SAYS.includes(String(f.props.className))).map((s) => [s.props.className, textOf(s.props.children as ReactNode)])))
      .toEqual([[[undefined, "kade-portal"]], [[undefined, "invoice-api"]]]);
    expect(kade.map((a) => a.props.title)).toEqual([
      "/home/you/Herd/kade-portal · main is 3 commits behind origin/main, fetched 2m ago",
      "/home/you/Herd/invoice-api · main, up to date with origin/main · 4 files changed, not committed",
    ]);
  });

  it("the CSS: a row keeps its one-line height; the identifier and how it stands never wrap or shrink; the title gives way", () => {
    expect(rule(componentsCss, ".nav-item")).toEqual(expect.arrayContaining(["height: calc(32px + var(--ui-box, 0px))", "align-items: center"]));
    expect(rule(componentsCss, ".nav-item .card-id")).toEqual(expect.arrayContaining(["flex: none", "white-space: nowrap"]));
    expect(rule(componentsCss, ".nav-item .card-id ~ .meta")).toEqual(["flex: none"]);
    expect(rule(componentsCss, ".nav-item .meta")).toContain("margin-left: auto");
    expect(rule(componentsCss, ".nav-item .stand")).toContain("white-space: nowrap");
    expect(rule(appCss, ".ellipsis")).toEqual(["overflow: hidden", "text-overflow: ellipsis", "white-space: nowrap", "min-width: 0"]);
    // the Terminal section's rule is as it was
    expect(rule(terminalCss, ".nav-item[data-term] .card-id")).toEqual(["flex: none"]);
  });
});

describe("the sidebar's section headings: the + and the caret in the middle of their hover background (GA-158)", () => {
  it("the CSS: a 20 x 20 box with no padding of its own, its icon centred; no theme or other rule changes that", () => {
    expect(rule(componentsCss, ".nav-label button")).toEqual(expect.arrayContaining(["width: 20px", "height: 20px", "padding: 0", "display: grid", "place-items: center"]));
    // the browser's own button padding is what pushed the icon 3px right: no other rule pads these buttons again
    const others = [componentsCss, appCss, terminalCss].flatMap((css) => css.split("\n"))
      .filter((l) => /\.nav-label[^{]*button[^{]*\{/.test(l) && !l.startsWith(".nav-label button {") && !l.startsWith(".nav-label button:hover {"));
    expect(others).toEqual([]);
    expect(rule(componentsCss, ".nav-label button:hover")).toEqual(["background: var(--hover)", "color: var(--text)"]);
  });
});

describe("GA-177: the Editor rows are asked again every 30 seconds and on a focus, at most once every 5 seconds", () => {
  const focus = () => win.dispatchEvent(new Event("focus"));
  async function wait(ms: number) { await vi.advanceTimersByTimeAsync(ms); render(); await settle(); }
  // GA-190: the 30-second timer asks only the folders and git's marks (editor_git); how each folder stands against main
  // (editor_main, some 15 to 30 git processes) is asked when a folder opens and on the focus, after the marks.
  /** A focus's (and a first open's) ask of the rows: the folders, git's status for each opened folder, then main's standing for each. */
  const ONE = ["folders", `git ${KADE}`, `git ${API}`, `git ${CARD}`, `main ${KADE}`, `main ${API}`, `main ${CARD}`];
  /** The 30-second timer's ask: the folders and git's status for each opened folder, no editor_main (GA-190). */
  const TICK = ["folders", `git ${KADE}`, `git ${API}`, `git ${CARD}`];
  const since = (n: number) => asked.slice(n);

  it("20 focus events within 3 seconds ask for the rows once, not 20 times", async () => {
    await show();
    expect(asked).toEqual(ONE); // GA-190: same calls, now the marks first, then main's standing
    await wait(1000);
    const n = asked.length;
    for (let i = 0; i < 20; i++) { focus(); await wait(150); }
    expect(since(n)).toEqual(ONE); // GA-190: same calls, new order
  });

  it("a focus more than 5 seconds after the last ask asks at once; 5 seconds or less after it, nothing", async () => {
    await show();
    await wait(1000);
    focus();
    await settle();
    let n = asked.length;
    await wait(5000);
    focus();
    await settle();
    expect(since(n)).toEqual([]);
    n = asked.length;
    await wait(1);
    focus();
    await settle();
    expect(since(n)).toEqual(ONE); // GA-190: same calls, new order
  });

  it("asks git's marks every 30 seconds whatever the focus did, no main; a focus 5 seconds or less after the last focus does nothing", async () => {
    await show();
    await wait(28_000);
    focus(); // 28 s: asks
    await settle();
    let n = asked.length;
    await wait(2000); // 30 s: the 30-second ask, though the focus was 2 s ago
    // GA-190: the timer asks the folders and git only (was ONE, with main for each folder)
    expect(since(n)).toEqual(TICK);
    n = asked.length;
    await wait(2000);
    focus(); // 32 s: 4 s after the focus's ask
    await settle();
    expect(since(n)).toEqual([]);
    // GA-190: the timer's lighter ask doesn't count for the gap (onWindowFocus's tick), so a focus 4 s after it but
    // 6 s after the last focus asks (before: nothing, as the timer's ask counted)
    await wait(2000);
    focus(); // 34 s: 6 s after the focus at 28 s, 4 s after the timer's ask
    await settle();
    expect(since(n)).toEqual(ONE);
    n = asked.length;
    await wait(25_999);
    expect(since(n)).toEqual([]);
    await wait(1); // 60 s
    expect(since(n)).toEqual(TICK); // GA-190: was ONE
  });

  it("the 30-second ask skips while the window is hidden; the first focus after it asks at once", async () => {
    await show();
    const n = asked.length;
    doc.hidden = true;
    await wait(5 * 60_000);
    expect(since(n)).toEqual([]);
    doc.hidden = false;
    focus();
    await settle();
    expect(since(n)).toEqual(ONE); // GA-190: same calls, new order
  });

  it("GA-190: over 5 minutes in use the timer asks git's marks 10 times and editor_main not once", async () => {
    await show();
    const n = asked.length;
    await wait(5 * 60_000);
    const asks = since(n);
    expect(asks.filter((a) => a.startsWith("main "))).toEqual([]);
    expect(asks.filter((a) => a === `git ${KADE}`).length).toBe(10);
    expect(asks).toEqual(Array.from({ length: 10 }, () => TICK).flat());
  });
});
