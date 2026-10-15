// GA-114 QA: behind main on the Editor page (#/editor/<folder>, F3) against the card's acceptance criteria and D2's design
// §5. A folder under a test root is opened against a fake of contract C3 that behaves as the backend does: editorMain says
// how the folder stands as last fetched, editorFetch fetches first (it can be held, to see Fetching…), editorUpdate checks
// and starts Update or rejects with why (nothing changed), and an update's end comes as editor-updated through
// onEditorUpdated. The 10-second look and the 10-minute fetch are run with fake timers. As in EditorPage.test.tsx there is
// no DOM: a small hook runtime stands in for React (components are called as functions and keep their hooks by their
// place in the tree, effects run after each render) and the CodeMirror view is a stand-in.
// GA-177 QA: a window focus refreshes git, the folder, the tree's open folders and main's standing at most once every 5
// seconds (on Omarchy the focus follows the mouse); the 10-second refresh and look keep their timing and skip while hidden.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import type { EditorState, Transaction, TransactionSpec } from "@codemirror/state";
import type { DirEntry, EditorFolder, EditorUpdated, FolderGit, FolderMain } from "../types";

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
  // a store read through useSyncExternalStore (the terminal panel reads TF0's, GA-137): its snapshot as it is now
  const useSyncExternalStore = ((sub: (f: () => void) => () => void, get: () => unknown, server?: () => unknown) =>
    (h.cur ? get() : R.useSyncExternalStore(sub, get, server))) as typeof R.useSyncExternalStore;
  const hooks = { useState, useRef, useMemo, useCallback, useEffect, useLayoutEffect: useEffect, useSyncExternalStore };
  return { ...R, default: { ...R, ...hooks }, ...hooks };
});

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
vi.mock("../components/editor/FolderPicker", () => ({ FolderPicker: () => null }));
vi.mock("../components/editor/Menus", async (orig) => ({ ...(await orig<object>()), OpenInMenu: () => null }));
vi.mock("../router", async (orig) => ({ ...(await orig<object>()), go: () => {} }));

// ---- a fake of contract C3 for one folder, kade-portal: a clone 3 commits behind its origin's main ----
const NOW = Date.UTC(2026, 9, 12, 12, 0, 0);
const MIN = 60_000;
const FOLDER = "/srv/test-root/Herd/kade-portal";
let folderInfo: EditorFolder;
/** The folder as the backend sees it. */
type Repo = { branch: string | null; behind: number; ahead: number; uncommitted: number; fetchedAt: number | null; fetchError: string | null;
  updating: boolean; mainLabel: string; refusal: string | null; head: string };
let repo: Repo;
const calls: string[] = [];
const listeners: ((e: EditorUpdated) => void)[] = [];
/** A held fetch: resolves when `release` runs. */
let hold: { release: () => void } | null = null;
let holdFetches = false;
let failFetch: string | null = null;
/** Update rejects at its click with this, though the folder's last answer let it run (it changed in between). */
let refuseAtClick: string | null = null;
const PLAN = "move main 3 commits to 319bf0b and run npm ci";

function answer(): FolderMain {
  const r = repo;
  const why = !r.mainLabel ? "kade-portal has no origin remote: Gizai can't tell whether it's behind."
    : r.refusal ?? (r.behind === 0 ? "kade-portal is up to date with origin/main." : null);
  return {
    mainLabel: r.mainLabel, fetchedAt: r.fetchedAt, fetchError: r.fetchError, branch: r.branch, onDefault: r.branch === "main",
    behind: r.behind, ahead: r.ahead, uncommitted: r.uncommitted, canUpdate: !r.updating && !why, updateSays: r.updating ? PLAN : why ?? PLAN, updating: r.updating,
  };
}
const snapshot = () => JSON.stringify(repo);

vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) if (typeof real[k] === "function") out[k] = () => new Promise(() => {});
  const mine = (folder: string) => { if (folder !== FOLDER) throw new Error(`The Editor can't open ${folder}`); };
  return {
    ...out,
    editorFolders: () => { calls.push("folders"); return Promise.resolve([folderInfo]); },
    editorList: async (folder: string): Promise<DirEntry[]> => { mine(folder); calls.push("list"); return [{ name: "README.md", path: "README.md", dir: false, size: 14, ignored: false, git: null }]; },
    editorVersions: async (folder: string, paths: string[]) => { mine(folder); return paths.map(() => "v1"); },
    editorGit: async (folder: string): Promise<FolderGit> => { mine(folder); calls.push("git"); return { branch: repo.branch, files: {}, uncommitted: repo.uncommitted }; },
    editorApps: () => Promise.resolve([]),
    editorMain: async (folder: string) => { mine(folder); calls.push("main"); return answer(); },
    editorFetch: async (folder: string) => {
      mine(folder);
      calls.push("fetch");
      if (holdFetches) await new Promise<void>((res) => { hold = { release: () => { hold = null; res(); } }; });
      if (repo.mainLabel) {
        if (failFetch) repo.fetchError = failFetch;
        else { repo.fetchedAt = Date.now(); repo.fetchError = null; }
      }
      return answer();
    },
    editorUpdate: async (folder: string) => {
      mine(folder);
      calls.push("update");
      if (repo.updating) throw "An update of kade-portal is already running.";
      if (refuseAtClick) throw refuseAtClick;
      const a = answer();
      if (!a.canUpdate) throw a.updateSays;
      repo.updating = true;
      return PLAN;
    },
    onEditorUpdated: (cb: (e: EditorUpdated) => void) => { listeners.push(cb); return Promise.resolve(() => { listeners.splice(listeners.indexOf(cb), 1); }); },
  };
});

// ---- the browser bits the page uses ----
const store = new Map<string, string>();
vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, v); }, removeItem: (k: string) => { store.delete(k); } });
vi.stubGlobal("navigator", { userAgent: "Linux", platform: "Linux x86_64" });
const win = new EventTarget();
vi.stubGlobal("window", win);
const doc = { hidden: false, querySelector: () => null, body: { style: {} }, documentElement: { style: {} } };
vi.stubGlobal("document", doc);
vi.stubGlobal("requestAnimationFrame", (f: () => void) => setTimeout(f, 0));
/** What the page told the sidebar (src/lib/behind.ts tellMain). */
const told: { id: string; main: FolderMain }[] = [];
win.addEventListener("gizai-editor-main", (e) => told.push((e as CustomEvent<{ id: string; main: FolderMain }>).detail));

const { EditorPage } = await import("./EditorPage");

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
  if (node && typeof node === "object" && "props" in node) return textOf((node as ReactElement<{ children?: ReactNode }>).props.children);
  return "";
}
const cls = (f: Found) => String(f.props.className ?? "");
/** The top bar's standing: "main · 3 behind origin/main". */
const standing = () => {
  const s = findAll(tree, (f) => cls(f) === "ed-standing");
  if (s.length !== 1) throw new Error(`${s.length} standings`);
  return textOf(s[0]!.props.children as ReactNode).split("·").join(" · ");
};
const standingTip = () => findAll(tree, (f) => cls(f) === "ed-standing")[0]!.props.title;
/** The bar about main under the top bar (not a file's bar), or null. */
const bars = () => findAll(tree, (f) => /^ed-bar( (behind|refused|done|failed))?$/.test(cls(f)));
function mainBar() {
  const all = bars();
  if (all.length > 1) throw new Error(`${all.length} bars about main`);
  const b = all[0];
  if (!b) return null;
  const say = findAll(b.props.children as ReactNode, (f) => cls(f) === "say")[0];
  const buttons = findAll(b.props.children as ReactNode, (f) => f.type === "button").map((f) => ({
    name: textOf(f.props.children as ReactNode).trim() || String(f.props["aria-label"] ?? ""), disabled: !!f.props.disabled, title: f.props.title as string | undefined,
    cls: cls(f), onClick: f.props.onClick as () => void,
  }));
  const pre = findAll(b.props.children as ReactNode, (f) => f.type === "pre")[0];
  return { cls: cls(b), role: b.props.role, say: say ? textOf(say.props.children as ReactNode) : "", bold: say ? findAll(say.props.children as ReactNode, (f) => f.type === "b").map((f) => textOf(f.props.children as ReactNode)) : [],
    buttons, output: pre ? textOf(pre.props.children as ReactNode) : null };
}
function topButton(name: string) {
  const all = findAll(tree, (f) => f.type === "button" && cls(f) === "btn ghost sm" && /^(Fetch|Fetching…)$/.test(textOf(f.props.children as ReactNode).trim()));
  if (all.length > 1) throw new Error(`${all.length} Fetch buttons`);
  const b = all[0];
  if (!b) return null;
  return { name: textOf(b.props.children as ReactNode).trim(), disabled: !!b.props.disabled, title: b.props.title as string, onClick: b.props.onClick as () => void, busy: b.props["aria-busy"], want: name };
}
async function press(name: string) {
  const b = mainBar()?.buttons.find((x) => x.name === name);
  if (!b) throw new Error(`no ${name} in the bar`);
  if (b.disabled) throw new Error(`${name} is off`);
  b.onClick();
  await settle();
}
/** The update ends: the backend's state after it, then editor-updated. */
async function ends(e: Omit<EditorUpdated, "folder">, after: Partial<Repo>) {
  Object.assign(repo, after, { updating: false });
  for (const l of [...listeners]) l({ folder: FOLDER, ...e });
  await settle();
}
const count = (what: string) => calls.filter((c) => c === what).length;

const folder = (more: Partial<EditorFolder> = {}): EditorFolder => ({
  id: FOLDER, path: FOLDER, name: "kade-portal", kind: "root", projectId: null, taskId: null, readOnly: false, readOnlyWhy: null, ...more,
});
async function open() {
  root = () => <EditorPage key={FOLDER} id={FOLDER} youId="u1" />;
  render();
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0;
  store.clear(); calls.length = 0; listeners.length = 0; told.length = 0;
  hold = null; holdFetches = false; failFetch = null; refuseAtClick = null; doc.hidden = false;
  folderInfo = folder();
  repo = { branch: "main", behind: 3, ahead: 0, uncommitted: 0, fetchedAt: NOW - 2 * MIN, fetchError: null, updating: false, mainLabel: "origin/main", refusal: null, head: "a1b2c3d" };
});
afterEach(() => { root = () => null; render(); vi.useRealTimers(); });

describe("a folder 3 commits behind origin/main (criterion 1)", () => {
  it("fetches main when the folder opens, showing Fetching… in the top bar until it is back", async () => {
    holdFetches = true;
    await open();
    expect(count("fetch")).toBe(1);
    expect(topButton("Fetching…")).toMatchObject({ name: "Fetching…", disabled: true, busy: true });
    // meanwhile the folder's standing as last fetched shows
    expect(standing()).toBe("main · 3 behind origin/main");
    expect(mainBar()?.say).toBe("3 commits behind origin/main, fetched 2m ago. Update will move main 3 commits to 319bf0b and run npm ci.");
    hold!.release();
    await settle();
    expect(topButton("Fetch")).toMatchObject({ name: "Fetch", disabled: false, title: "Fetch origin/main now (last fetched just now)" });
  });

  it("says '3 commits behind origin/main' in bold with when it was fetched, and offers Update with what it will do", async () => {
    await open();
    expect(standing()).toBe("main · 3 behind origin/main");
    const bar = mainBar()!;
    expect(bar.cls).toBe("ed-bar behind");
    expect(bar.role).toBe("status");
    expect(bar.bold).toEqual(["3 commits behind origin/main"]);
    expect(bar.say).toBe("3 commits behind origin/main, fetched just now. Update will move main 3 commits to 319bf0b and run npm ci.");
    expect(bar.buttons).toEqual([expect.objectContaining({ name: "Update", disabled: false, cls: "btn sm primary", title: "Move main 3 commits to 319bf0b and run npm ci" })]);
    expect(standingTip()).toBe("main is 3 commits behind origin/main, fetched just now");
  });

  it("Fetch in the top bar fetches again, and the bar's 'fetched' follows the clock", async () => {
    await open();
    await wait(2 * MIN);
    expect(mainBar()?.say).toContain(", fetched 2m ago.");
    expect(topButton("Fetch")?.title).toBe("Fetch origin/main now (last fetched 2m ago)");
    const before = count("fetch");
    topButton("Fetch")!.onClick();
    await settle();
    expect(count("fetch")).toBe(before + 1);
    expect(mainBar()?.say).toContain(", fetched just now.");
  });

  it("fetches again every 10 minutes while the page shows the folder, and looks (locally) every 10 seconds", async () => {
    await open();
    expect(count("fetch")).toBe(1);
    const looks = count("main");
    await wait(10_000);
    expect(count("main")).toBe(looks + 1);
    await wait(10 * MIN - 20_000);
    expect(count("fetch")).toBe(1);
    await wait(10_000);
    expect(count("fetch")).toBe(2);
    // not while the window is hidden
    doc.hidden = true;
    await wait(20 * MIN);
    expect(count("fetch")).toBe(2);
    doc.hidden = false;
    await wait(10_000);
    expect(count("fetch")).toBe(3);
  });

  it("shows a new commit on origin at the next fetch: 4 behind, on the bar and in the top bar", async () => {
    await open();
    repo.behind = 4;
    await wait(10 * MIN);
    expect(standing()).toBe("main · 4 behind origin/main");
    expect(mainBar()?.bold).toEqual(["4 commits behind origin/main"]);
  });

  it("up to date: the top bar says so, and no bar", async () => {
    repo.behind = 0;
    await open();
    expect(standing()).toBe("main · up to date with origin/main");
    expect(mainBar()).toBeNull();
  });

  it("no origin remote: 'no remote', no bar and no Fetch", async () => {
    repo.mainLabel = ""; repo.fetchedAt = null; repo.behind = 0;
    await open();
    expect(standing()).toBe("main · no remote");
    expect(mainBar()).toBeNull();
    expect(topButton("Fetch")).toBeNull();
  });

  it("a card's worktree has no behind bar", async () => {
    folderInfo = folder({ kind: "card", name: "GA-118", taskId: "t1" });
    repo.branch = "gizai/ga-118-show-a-run-s-changes";
    await open();
    // the bar under the top bar is the card's own (F2), not the behind bar
    const bar = mainBar()!;
    expect(bar.say).toContain("GA-118 isn't assigned to anyone");
    expect(bar.say).not.toContain("behind origin/main");
    expect(bar.buttons.map((b) => b.name)).not.toContain("Update");
  });
});

describe("a refused Update says why in the bar and changes nothing (criterion 2)", () => {
  const cases: [string, Partial<Repo>, string][] = [
    ["an uncommitted change in a tracked file", { uncommitted: 2, refusal: "kade-portal has uncommitted changes in app/Models/Invoice.php and routes/web.php. Commit or stash them first." }, "main · 3 behind origin/main · 2 changed"],
    ["a merge in progress", { uncommitted: 4, refusal: "a merge is in progress in kade-portal. Finish or abort it first." }, "main · 3 behind origin/main · 4 changed"],
    ["another branch", { branch: "feature/pdf-export", refusal: "kade-portal is on branch feature/pdf-export, not main. Switch to main first, in a terminal or your own editor." }, "feature/pdf-export · 3 behind origin/main"],
    ["no branch", { branch: null, refusal: "kade-portal has no branch checked out. Switch to main first, in a terminal or your own editor." }, "no branch · 3 behind origin/main"],
    ["main with its own commits", { ahead: 1, refusal: "main in kade-portal has 1 commit that origin/main doesn't, so a fast-forward can't bring it up to date. Push or merge it first, in a terminal or your own editor." }, "main · 3 behind origin/main · 1 ahead"],
  ];
  it.each(cases)("%s: Update off, the same sentence in the bar and as its tooltip", async (_, state, top) => {
    Object.assign(repo, state);
    await open();
    const was = snapshot();
    expect(standing()).toBe(top);
    const bar = mainBar()!;
    expect(bar.cls).toBe("ed-bar refused");
    expect(bar.role).toBe("alert");
    expect(bar.say).toBe(`3 commits behind origin/main, fetched just now. Update can't run: ${state.refusal}`);
    expect(bar.buttons).toEqual([expect.objectContaining({ name: "Update", disabled: true, cls: "btn sm", title: `Update can't run: ${state.refusal}` })]);
    await wait(30_000);
    expect(count("update")).toBe(0);
    expect(snapshot()).toBe(was);
  });

  it("refused at the click (the folder changed since the last look): the bar says why at once, nothing changes, no update ends", async () => {
    await open();
    refuseAtClick = "kade-portal has uncommitted changes in routes/web.php. Commit or stash them first.";
    const was = snapshot();
    await press("Update");
    expect(count("update")).toBe(1);
    const bar = mainBar()!;
    expect(bar.cls).toBe("ed-bar refused");
    expect(bar.say).toBe("3 commits behind origin/main, fetched just now. Update can't run: kade-portal has uncommitted changes in routes/web.php. Commit or stash them first.");
    expect(snapshot()).toBe(was);
    // the next look sees the change: the folder's own answer says why, Update off
    Object.assign(repo, { uncommitted: 1, refusal: refuseAtClick });
    await wait(10_000);
    expect(mainBar()).toMatchObject({ cls: "ed-bar refused", say: `3 commits behind origin/main, fetched just now. Update can't run: ${refuseAtClick}` });
    expect(mainBar()!.buttons[0]).toMatchObject({ name: "Update", disabled: true });
  });

  it("another update of the folder already runs (the Team Lead's): says so", async () => {
    await open();
    refuseAtClick = "An update of kade-portal is already running.";
    await press("Update");
    // the backend's answer says an update runs: the bar shows it running
    repo.updating = true;
    await wait(10_000);
    expect(mainBar()).toMatchObject({ cls: "ed-bar", say: "Updating kade-portal: move main 3 commits to 319bf0b and run npm ci…" });
  });
});

describe("Update, and after it the bar and the sidebar show the new state without a reload (criterion 3)", () => {
  it("click Update: 'Updating kade-portal: …' with Updating… and Fetch off, and the sidebar is told it updates", async () => {
    await open();
    await press("Update");
    expect(count("update")).toBe(1);
    const bar = mainBar()!;
    expect(bar.cls).toBe("ed-bar");
    expect(bar.say).toBe("Updating kade-portal: move main 3 commits to 319bf0b and run npm ci…");
    expect(bar.buttons).toEqual([expect.objectContaining({ name: "Updating…", disabled: true })]);
    expect(topButton("Fetch")).toMatchObject({ disabled: true });
    expect(standing()).toBe("main · 3 behind origin/main");
    expect(told.at(-1)).toMatchObject({ id: FOLDER, main: { updating: true } });
    // the 10-second look while it runs keeps it so
    await wait(10_000);
    expect(mainBar()?.say).toBe("Updating kade-portal: move main 3 commits to 319bf0b and run npm ci…");
  });

  it("done: the bar says what it did, the top bar is up to date, the tree and git's marks are looked at again, Show output shows what it printed", async () => {
    await open();
    await press("Update");
    const looks = { list: count("list"), git: count("git") };
    await ends({ ok: true, said: "Moved main 3 commits to 319bf0b and ran npm ci.", output: "Updating a1b2c3d..319bf0b\nFast-forward\n\nadded 812 packages in 9s\n" }, { behind: 0, head: "319bf0b" });
    expect(standing()).toBe("main · up to date with origin/main");
    expect(standingTip()).toBe("main, up to date with origin/main");
    const bar = mainBar()!;
    expect(bar.cls).toBe("ed-bar done");
    expect(bar.say).toBe("Updated kade-portal: moved main 3 commits to 319bf0b and ran npm ci.");
    expect(bar.buttons.map((b) => b.name)).toEqual(["Show output", "Dismiss"]);
    expect(bar.output).toBeNull();
    expect(count("list")).toBeGreaterThan(looks.list);
    expect(count("git")).toBeGreaterThan(looks.git);
    expect(told.at(-1)).toMatchObject({ id: FOLDER, main: { updating: false, behind: 0 } });

    await press("Show output");
    expect(mainBar()!.output).toBe("Updating a1b2c3d..319bf0b\nFast-forward\n\nadded 812 packages in 9s\n");
    expect(mainBar()!.buttons.map((b) => b.name)).toEqual(["Hide output", "Dismiss"]);
    await press("Hide output");
    expect(mainBar()!.output).toBeNull();
    // the bar stays until Dismiss, through the next looks
    await wait(30_000);
    expect(mainBar()?.cls).toBe("ed-bar done");
    await press("Dismiss");
    expect(mainBar()).toBeNull();
    expect(standing()).toBe("main · up to date with origin/main");
  });

  it("an update that printed nothing has no Show output", async () => {
    await open();
    await press("Update");
    await ends({ ok: true, said: "Moved main 3 commits to 319bf0b.", output: "" }, { behind: 0 });
    expect(mainBar()).toMatchObject({ cls: "ed-bar done", say: "Updated kade-portal: moved main 3 commits to 319bf0b." });
    expect(mainBar()!.buttons.map((b) => b.name)).toEqual(["Dismiss"]);
  });

  it("stopped: what failed and what it did before, on danger, with its output to show", async () => {
    await open();
    await press("Update");
    const said = "The update of kade-portal stopped: npm ci failed (exit code 1). Before that it moved main 3 commits to 319bf0b.";
    const output = "npm error code EBADENGINE\nnpm error engine Unsupported engine\n";
    await ends({ ok: false, said, output }, { behind: 0 });
    expect(mainBar()).toMatchObject({ cls: "ed-bar failed", role: "alert", say: said });
    await press("Show output");
    expect(mainBar()!.output).toBe(output);
  });

  it("didn't start when its turn came: why, with Dismiss and no output", async () => {
    await open();
    await press("Update");
    const said = "The update of kade-portal didn't start: kade-portal has uncommitted changes in routes/web.php. Commit or stash them first.";
    await ends({ ok: false, said, output: "" }, { uncommitted: 1, refusal: "kade-portal has uncommitted changes in routes/web.php. Commit or stash them first." });
    expect(mainBar()).toMatchObject({ cls: "ed-bar refused", say: said });
    expect(mainBar()!.buttons.map((b) => b.name)).toEqual(["Dismiss"]);
    await press("Dismiss");
    // then the folder's own refusal shows
    expect(mainBar()).toMatchObject({ cls: "ed-bar refused", say: "3 commits behind origin/main, fetched just now. Update can't run: kade-portal has uncommitted changes in routes/web.php. Commit or stash them first." });
  });

  it("an update of another folder doesn't touch this page", async () => {
    await open();
    for (const l of [...listeners]) l({ folder: "/srv/test-root/Herd/invoice-api", ok: true, said: "Moved main 1 commit to 7c2e0aa.", output: "" });
    await settle();
    expect(mainBar()?.cls).toBe("ed-bar behind");
  });

  it("an update that ends before editorUpdate's answer arrives still shows how it ended", async () => {
    await open();
    const b = mainBar()!.buttons[0]!;
    b.onClick();
    // the event comes before the promise's answer is handled
    Object.assign(repo, { behind: 0, updating: false });
    for (const l of [...listeners]) l({ folder: FOLDER, ok: true, said: "Moved main 3 commits to 319bf0b.", output: "" });
    await settle();
    expect(mainBar()).toMatchObject({ cls: "ed-bar done", say: "Updated kade-portal: moved main 3 commits to 319bf0b." });
    expect(standing()).toBe("main · up to date with origin/main");
  });
});

describe("a fetch that failed, with Try again", () => {
  it("the counts are from the last fetch; Try again fetches again and the bar goes when it works", async () => {
    failFetch = "Couldn't fetch main from origin: could not resolve host github.com";
    repo.fetchedAt = NOW - 3 * 60 * MIN;
    await open();
    const bar = mainBar()!;
    expect(bar.cls).toBe("ed-bar refused");
    expect(bar.say).toBe("Couldn't fetch main from origin: could not resolve host github.com. The counts are from the last fetch, 3h ago.");
    expect(bar.buttons.map((b) => b.name)).toEqual(["Try again"]);
    expect(standing()).toBe("main · 3 behind origin/main");
    failFetch = null;
    await press("Try again");
    expect(count("fetch")).toBe(2);
    expect(mainBar()).toMatchObject({ cls: "ed-bar behind", say: "3 commits behind origin/main, fetched just now. Update will move main 3 commits to 319bf0b and run npm ci." });
  });

  it("never fetched: Gizai can't tell yet whether the folder is behind", async () => {
    failFetch = "Couldn't fetch main from origin: could not resolve host github.com";
    Object.assign(repo, { fetchedAt: null, behind: 0 });
    await open();
    expect(standing()).toBe("main · not fetched yet");
    expect(mainBar()?.say).toBe("Couldn't fetch main from origin: could not resolve host github.com. Gizai can't tell yet whether kade-portal is behind.");
  });
});

describe("GA-177: a window focus refreshes at most once every 5 seconds", () => {
  const focus = () => win.dispatchEvent(new Event("focus"));
  const KINDS = ["git", "folders", "list", "main", "fetch"] as const;
  const counts = () => Object.fromEntries(KINDS.map((k) => [k, count(k)])) as Record<(typeof KINDS)[number], number>;
  /** What was asked since `before`. */
  const since = (before: ReturnType<typeof counts>) => Object.fromEntries(KINDS.map((k) => [k, count(k) - before[k]]));
  const none = { git: 0, folders: 0, list: 0, main: 0, fetch: 0 };
  const once = { git: 1, folders: 1, list: 1, main: 1, fetch: 0 };

  it("20 focus events within 3 seconds refresh git, the folder, the tree and main's standing once, not 20 times (criterion 1)", async () => {
    await open();
    await wait(1000);
    const before = counts();
    const at: number[] = [];
    for (let i = 0; i < 20; i++) { at.push(Date.now()); focus(); await wait(150); }
    expect(at.length).toBe(20);
    expect(at[19]! - at[0]!).toBeLessThan(3000);
    expect(since(before)).toEqual(once);
  });

  it("a focus more than 5 seconds after the last refresh refreshes at once; 5 seconds or less after it, nothing (criterion 2)", async () => {
    await open();
    await wait(1000);
    focus();
    await settle();
    let before = counts();
    await wait(5000);
    focus();
    await settle();
    expect(since(before)).toEqual(none);
    before = counts();
    await wait(1);
    focus();
    await settle();
    expect(since(before)).toEqual(once);
  });

  it("the first focus after a quiet while refreshes at once: hidden for 20 minutes, a focus refreshes git and fetches main", async () => {
    await open();
    const before = counts();
    doc.hidden = true;
    await wait(20 * MIN);
    expect(since(before), "nothing while the window is hidden").toEqual(none);
    doc.hidden = false;
    focus();
    await settle();
    // main's fetch was due, so the focus fetches (its answer is the standing) instead of looking
    expect(since(before)).toEqual({ git: 1, folders: 1, list: 1, main: 0, fetch: 1 });
  });

  it("git and the folder every 10 seconds and main's look keep their timing: a focus doesn't move them (criterion 4)", async () => {
    await open();
    let before = counts();
    await wait(8000);
    focus(); // 8 s: refreshes
    await settle();
    expect(since(before)).toEqual(once);
    before = counts();
    await wait(2000); // 10 s: the 10-second refresh and look, though the focus was 2 s ago
    expect(since(before)).toEqual({ git: 1, folders: 1, list: 0, main: 1, fetch: 0 });
    before = counts();
    await wait(9999);
    expect(since(before)).toEqual(none);
    await wait(1); // 20 s
    expect(since(before)).toEqual({ git: 1, folders: 1, list: 0, main: 1, fetch: 0 });
  });

  it("a focus 5 seconds or less after the 10-second refresh starts no git; the tree, refreshed only on a focus, keeps its own gap", async () => {
    await open();
    await wait(4000);
    focus(); // 4 s: refreshes everything
    await settle();
    await wait(6000); // 10 s: the 10-second refresh and look
    let before = counts();
    await wait(3000);
    focus(); // 13 s: 3 s after the 10-second refresh, 9 s after the tree's last
    await settle();
    expect(since(before)).toEqual({ git: 0, folders: 0, list: 1, main: 0, fetch: 0 });
    before = counts();
    await wait(2001);
    focus(); // 15.001 s: more than 5 s after the 10-second refresh, 2 s after the tree's
    await settle();
    expect(since(before)).toEqual({ git: 1, folders: 1, list: 0, main: 1, fetch: 0 });
  });

  it("the 10-second refresh and look skip while the window is hidden, and start again when it shows (criterion 4)", async () => {
    await open();
    let before = counts();
    doc.hidden = true;
    await wait(60_000);
    expect(since(before)).toEqual(none);
    doc.hidden = false;
    before = counts();
    await wait(10_000);
    expect(since(before)).toEqual({ git: 1, folders: 1, list: 0, main: 1, fetch: 0 });
  });

  it("leaving the page stops the 10-second refresh and the focus", async () => {
    await open();
    root = () => null;
    render();
    const before = counts();
    focus();
    await wait(60_000);
    focus();
    await settle();
    expect(since(before)).toEqual(none);
  });
});
