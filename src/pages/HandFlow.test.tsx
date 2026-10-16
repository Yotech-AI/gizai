// GA-115 QA: a person's card, clicked through. On the card's page, Open in editor (spinner while the worktree is
// prepared, then the Editor, the folder at the top of the sidebar's list; a refusal says why). In the Editor, the card
// bar of your card by hand in the folder bar's place: how far it is, Commit… with its box (the message filled in, the
// files it commits, Ctrl+Enter), Push and Hand over, what each did for a few seconds, a red bar for what went wrong
// (Try again for a push), where Hand over sent the card, and QA sending it back (needs) until you open the card. The
// backend is contract C4 faked in memory. There is no DOM here, so a small hook runtime stands in for React (as in
// EditorPage.test): components are called as functions and keep their hooks by their place in the tree, effects run
// after each render; the code view, search in the folder, Go to file and the folder picker are stand-ins.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import type { EditorState, Transaction, TransactionSpec } from "@codemirror/state";
import type { EditorFolder, FolderGit, GitMark, HandCard, Project, Run, Task, Team } from "../types";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
type Fn = (...a: unknown[]) => unknown;
const h = vi.hoisted(() => ({
  cur: null as Inst | null,
  dirty: false,
  effects: [] as (() => void)[],
  /** What the fake backend answers, by api name; an api without one never answers. */
  answers: {} as Record<string, Fn>,
  /** Every api call, in order. */
  api: [] as [string, unknown[]][],
  /** go() calls. */
  went: [] as unknown[],
  /** The listeners of "rows changed" and "runs changed". */
  rows: [] as ((table?: string) => void)[],
  openIn: null as null | { folder: string },
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

// The code view: a stand-in that keeps the state the page gives it.
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
vi.mock("@codemirror/search", async (orig) => ({ ...(await orig<object>()), openSearchPanel: () => true, gotoLine: () => true }));
vi.mock("../components/editor/FolderSearch", () => ({ FolderSearch: () => null }));
vi.mock("../components/editor/GoToFile", () => ({ GoToFile: () => null }));
vi.mock("../components/editor/FolderPicker", () => ({ FolderPicker: () => null }));
vi.mock("../components/editor/Menus", async (orig) => ({ ...(await orig<object>()), OpenInMenu: (p: { folder: string }) => { h.openIn = p; return null; } }));
vi.mock("../router", async (orig) => ({ ...(await orig<object>()), go: (r: unknown) => { h.went.push(r); } }));
// The card page's parts this file doesn't test.
vi.mock("../components/MarkdownEditor", () => ({ MarkdownEditor: () => null }));
vi.mock("../components/MarkdownView", () => ({ MarkdownView: () => null }));
vi.mock("../components/RunHistory", () => ({ RunHistory: () => null }));
vi.mock("../components/Properties", () => ({ Properties: () => null }));
vi.mock("../components/FileDrop", () => ({ FileDrop: () => null }));
vi.mock("../components/RunPanel", () => ({ RunPanel: () => null }));
vi.mock("../components/PullPanel", () => ({ PullPanel: () => null }));
vi.mock("../components/changes/ChangesView", () => ({ ChangesView: () => null }));

vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) {
    out[k] = typeof v !== "function" ? v : (...a: unknown[]) => {
      h.api.push([k, a]);
      const f = h.answers[k];
      return f ? Promise.resolve().then(() => f(...a)) : new Promise(() => {});
    };
  }
  return out;
});

// ---- the browser bits ----
const store = new Map<string, string>();
vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, v); }, removeItem: (k: string) => { store.delete(k); } });
vi.stubGlobal("navigator", { userAgent: "Linux", platform: "Linux x86_64" });
const win = new EventTarget();
vi.stubGlobal("window", win);
vi.stubGlobal("document", { hidden: false, querySelector: () => null, getElementById: () => null, body: { style: {} }, documentElement: { style: {} }, addEventListener: () => {}, removeEventListener: () => {} });
vi.stubGlobal("requestAnimationFrame", (f: () => void) => setTimeout(f, 0));

const { TaskPage } = await import("./TaskPage");
const { EditorPage } = await import("./EditorPage");
const { CardBar } = await import("../components/editor/CardBar");
const { OPENED_KEY } = await import("../lib/editor");
const { SEEN_KEY } = await import("../lib/hand");

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
async function settle() { for (let i = 0; i < 6; i++) { await vi.advanceTimersByTimeAsync(0); render(); } }
async function wait(ms: number) { await vi.advanceTimersByTimeAsync(ms); render(); await settle(); }
async function show(r: () => ReactNode) { root = r; render(); await settle(); }

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
type ButtonProps = { onClick: (e?: unknown) => unknown; disabled?: boolean; title?: string; className?: string; "aria-busy"?: boolean };
function button(name: string | RegExp, within: ReactNode = tree): ButtonProps {
  const is = (t: string) => (typeof name === "string" ? t === name : name.test(t));
  const all = findAll(within, (f) => f.type === "button" && is(textOf(f.props.children as ReactNode).trim()));
  if (all.length !== 1) throw new Error(`${all.length} buttons called ${name}`);
  return all[0]!.props as ButtonProps;
}
const hasButton = (name: string) => findAll(tree, (f) => f.type === "button" && textOf(f.props.children as ReactNode).trim() === name).length > 0;
const ev = { preventDefault: () => {}, stopPropagation: () => {}, currentTarget: { getBoundingClientRect: () => ({ left: 0, bottom: 0 }), parentElement: null } };
// the handler is not awaited: Open in editor waits for the worktree, which a test hands over when it likes
async function click(name: string | RegExp) { const b = button(name); expect(b.disabled, `${name} is off: ${b.title}`).toBeFalsy(); void b.onClick(ev); await settle(); }
const cls = (f: Found) => String(f.props.className ?? "");
/** The bars about the folder (under the top bar), with their class and text. */
const bars = () => findAll(tree, (f) => f.type === "div" && /^ed-bar( |$)/.test(cls(f)) && !cls(f).startsWith("ed-bar file"))
  .map((f) => ({ cls: cls(f), role: f.props.role, text: textOf(f.props.children as ReactNode).replace(/\s+/g, " ").trim(), node: f.props.children as ReactNode }));
const bar = () => { const b = bars(); if (b.length !== 1) throw new Error(`${b.length} folder bars: ${JSON.stringify(b.map((x) => x.text))}`); return b[0]!; };
const commitBox = () => findAll(tree, (f) => f.props.role === "dialog" && f.props["aria-label"] === "Commit");
const messageBox = () => findAll(tree, (f) => f.type === "textarea" && f.props.id === "ed-commit-message")[0]!.props as {
  value: string; onChange: (e: unknown) => void; onKeyDown: (e: unknown) => void;
};
const called = (name: string) => h.api.filter(([k]) => k === name).map(([, a]) => a);

// ---- the fake backend: a card, its project and team, its worktree ----
const YOU = "u-you";
const FOLDER = "/home/you/.local/share/gizai/worktrees/KADE/KADE-41";
const BRANCH = "gizai/kade-41-fix-the-invoice-total";
const team: Team = {
  id: "team", name: "Software", labels: [],
  members: [
    { actorId: "a-backend", name: "Backend Agent", kind: "agent", roleKey: "backend", handle: "backend", status: "idle", isLead: false, allowedTools: [], chatEnabled: false },
    { actorId: "a-qa", name: "QA Agent", kind: "agent", roleKey: "qa", handle: "qa", status: "idle", isLead: false, allowedTools: [], chatEnabled: false },
  ],
  states: [
    { id: "s-todo", name: "To do", category: "ready", sortKey: "a1", auto: true, agentIds: ["a-backend"] },
    { id: "s-prog", name: "In progress", category: "started", sortKey: "a2", auto: true, agentIds: ["a-backend"] },
    { id: "s-test", name: "Testing", category: "testing", sortKey: "a3", auto: true, agentIds: ["a-qa"] },
    { id: "s-rev", name: "Review", category: "review", sortKey: "a4" },
  ],
} as unknown as Team;
const project = { id: "p1", key: "KADE", name: "Kade portal", repoUrl: "https://github.com/acme/kade", teamId: "team" } as unknown as Project;
const TODO = {
  id: "t1", identifier: "KADE-41", projectId: "p1", projectName: "Kade portal", title: "Fix the invoice total", descriptionMd: "", acceptanceMd: "",
  stateId: "s-todo", stateName: "To do", stateCategory: "ready", priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: "a0", testing: true,
  createdAt: 0, updatedAt: 0, byHand: false, assigneeId: YOU, assigneeName: "Jeffrey", assigneeKind: "user", branch: null,
} as unknown as Task;
const BY_HAND = { ...TODO, stateId: "s-prog", stateName: "In progress", stateCategory: "started", byHand: true, branch: BRANCH } as Task;

let task: Task;
let hand: HandCard;
let files: Record<string, GitMark>;
let own: number;
let runs: Run[];
let live: { runId: string; taskId: string; agentId: string }[];
const folderInfo = (more: Partial<EditorFolder> = {}): EditorFolder => ({
  id: FOLDER, path: "~/.local/share/gizai/worktrees/KADE/KADE-41", name: "KADE-41", kind: "card", projectId: "p1", taskId: "t1", readOnly: false, readOnlyWhy: null, ...more,
});
let folder: EditorFolder;
const git = (): FolderGit => ({ branch: BRANCH, files, uncommitted: Object.keys(files).length });
/** A deferred answer: the test says when it comes. */
function later<T>() { let done!: (v: T) => void, fail!: (e: unknown) => void; const p = new Promise<T>((a, b) => { done = a; fail = b; }); return { p, done, fail }; }
/** The backend reports a write (Hand over, QA sending the card back). */
// GA-190: a write's notice names its table, and a screen reloads only for the tables it shows (a run's end, QA sending a
// card back, sends "tasks"); the 100 ms fallback lets a coalesced reload run when no frame comes
async function rowsChanged(table = "tasks") { for (const f of [...h.rows]) f(table); await vi.advanceTimersByTimeAsync(100); await settle(); }

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-10-12T12:00:00Z"));
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0; h.api.length = 0; h.went.length = 0; h.rows.length = 0; h.openIn = null;
  for (const k of Object.keys(h.answers)) delete h.answers[k];
  store.clear();
  task = { ...TODO };
  files = {};
  own = 0;
  runs = [];
  live = [];
  hand = { taskId: "t1", folder: FOLDER, branch: BRANCH, changed: 0, unpushed: 0 };
  folder = folderInfo();
  const listen = (cb: (table?: string) => void) => { h.rows.push(cb); return () => { h.rows = h.rows.filter((f) => f !== cb); }; };
  Object.assign(h.answers, {
    getTask: () => task, getTeam: () => team, getProject: () => project, listUsers: () => [], listComments: () => [], taskActivity: () => [],
    listRuns: () => runs, liveRuns: () => live, onRowsChanged: listen, onRunsChanged: listen,
    handCard: () => { if (!task.byHand) throw new Error(`${task.identifier} isn't yours by hand`); return { ...hand, changed: Object.keys(files).length }; },
    editorMain: () => ({ mainLabel: "origin/main", fetchedAt: null, fetchError: null, behind: 0, ahead: own }),
    editorFolders: () => [folder], editorList: () => [], editorGit: () => git(), editorVersions: () => [], editorFiles: () => [], editorApps: () => [],
  });
});
afterEach(() => { root = () => null; render(); vi.useRealTimers(); });

describe("Open in editor on the card's page (GA-115)", () => {
  it("shows Preparing the worktree… with a spinner while it is made, then opens it in the Editor, at the bottom of the sidebar's list (GA-157)", async () => {
    store.set(OPENED_KEY, JSON.stringify(["/srv/Herd/other"]));
    const start = later<HandCard>();
    h.answers.startByHand = () => start.p;
    await show(() => <TaskPage id="t1" youId={YOU} />);
    await click("Open in editor");
    expect(called("startByHand")).toEqual([["t1"]]);
    const busy = button("Preparing the worktree…");
    expect([busy.disabled, busy["aria-busy"]]).toEqual([true, true]);
    expect(findAll(tree, (f) => cls(f).includes("spin")).length).toBe(1);
    expect(h.went).toEqual([]);

    // the backend made it: the card is in In progress, by hand, pinned to you
    task = { ...BY_HAND };
    start.done({ ...hand });
    await settle();
    expect(h.went).toEqual([{ page: "editor", id: FOLDER }]);
    expect(JSON.parse(store.get(OPENED_KEY)!)).toEqual(["/srv/Herd/other", FOLDER]);
  });

  it("a refusal says why on the card, and Open in editor is on again", async () => {
    h.answers.startByHand = () => { throw "Error: npm ci failed in its new worktree (exit code 1)"; };
    await show(() => <TaskPage id="t1" youId={YOU} />);
    await click("Open in editor");
    const banner = findAll(tree, (f) => cls(f) === "error-banner" && f.props.role === "alert");
    expect(textOf(banner[0]?.props.children as ReactNode)).toContain("Couldn't open KADE-41 in the editor: npm ci failed in its new worktree (exit code 1).");
    expect(button("Open in editor").disabled).toBeFalsy();
    expect(h.went).toEqual([]);
  });

  it("a card already started by hand just opens its worktree, without making it again, and offers Open in for its folder", async () => {
    task = { ...BY_HAND };
    await show(() => <TaskPage id="t1" youId={YOU} />);
    expect(h.openIn?.folder).toBe(FOLDER);
    await click("Open in editor");
    expect(called("startByHand")).toEqual([]);
    expect(h.went).toEqual([{ page: "editor", id: FOLDER }]);
  });

  // GA-157 QA: a card's worktree already in the sidebar's Editor list stays where it is when the card opens it again
  it("Open in editor leaves a card already in the sidebar's list where it is, every other row too", async () => {
    task = { ...BY_HAND };
    const list = ["/srv/Herd/other", FOLDER, "/srv/Herd/third"];
    store.set(OPENED_KEY, JSON.stringify(list));
    await show(() => <TaskPage id="t1" youId={YOU} />);
    await click("Open in editor");
    expect(h.went).toEqual([{ page: "editor", id: FOLDER }]);
    expect(JSON.parse(store.get(OPENED_KEY)!)).toEqual(list);
  });

  it("is off while an agent works on the card", async () => {
    task = { ...TODO, stateId: "s-prog", stateName: "In progress", stateCategory: "started" };
    live = [{ runId: "r1", taskId: "t1", agentId: "a-backend" }];
    await show(() => <TaskPage id="t1" youId={YOU} />);
    const b = button("Open in editor");
    expect(b.disabled).toBe(true);
    expect(b.title).toBe("Backend Agent is working on KADE-41: wait for its run to end, or stop it.");
  });

  it("opening the card marks it seen, for the card bar's back from QA", async () => {
    await show(() => <TaskPage id="t1" youId={YOU} />);
    expect(JSON.parse(store.get(SEEN_KEY)!)).toEqual({ t1: Date.parse("2026-10-12T12:00:00Z") });
  });
});

describe("the card bar on its own (GA-115)", () => {
  let changedCalls: number;
  let handed: string[];
  const showBar = async (t: Task = BY_HAND) => {
    changedCalls = 0;
    handed = [];
    await show(() => <CardBar task={t} folder={FOLDER} git={git()} onChanged={() => { changedCalls++; }} onHanded={(s) => { handed.push(s); }} />);
  };
  beforeEach(() => { task = { ...BY_HAND }; });

  it("with changes: the column's glyph, the card, its branch and how far it is; Commit… primary, Push and Hand over off with why", async () => {
    files = { "src/Invoice.php": "modified", "tests/InvoiceTest.php": "untracked" };
    await showBar();
    const b = bar();
    expect(b.cls).toBe("ed-bar hand");
    expect(b.text).toBe(`In progressKADE-41Fix the invoice total${BRANCH}2 files changedCommit…PushHand over to Testing`);
    expect(findAll(b.node, (f) => f.type === "svg" && cls(f).startsWith("st ")).length).toBe(1);
    expect(button("Commit…")).toMatchObject({ className: "btn sm primary", disabled: false, title: "Commit every change in KADE-41's worktree: 2 files" });
    expect(button("Push")).toMatchObject({ className: "btn sm", disabled: true, title: "Nothing to push" });
    expect(button("Hand over to Testing")).toMatchObject({ disabled: true, title: "Commit your changes first: 2 files aren't committed." });
  });

  it("every button is off until Gizai knows where the card stands, saying why when it can't", async () => {
    files = { "a.txt": "modified" };
    h.answers.handCard = () => { throw "Error: KADE-41's worktree isn't there any more: open it in the editor again to make it"; };
    await showBar();
    expect(button("Commit…")).toMatchObject({ disabled: true, title: "KADE-41's worktree isn't there any more: open it in the editor again to make it." });
    expect(bar().text).toContain("KADE-41's worktree isn't there any more");
  });

  it("Commit…: the box has the message filled in and the files it commits; Commit says what it did for about 6 seconds", async () => {
    files = { "src/Invoice.php": "modified", "tests/InvoiceTest.php": "untracked" };
    h.answers.handCommit = () => { files = {}; own = 1; hand = { ...hand, unpushed: 1 }; return { sha: "3f5c1a29b8e7", subject: "KADE-41: Fix the invoice total" }; };
    await showBar();
    await click("Commit…");
    expect(commitBox()).toHaveLength(1);
    expect(messageBox().value).toBe("KADE-41: Fix the invoice total");
    const box = commitBox()[0]!.props.children as ReactNode;
    const listed = findAll(box, (f) => f.type === "li").map((f) => textOf(f.props.children as ReactNode));
    expect(listed).toEqual(["Msrc/Invoice.php", "Utests/InvoiceTest.php"]);
    expect(textOf(box)).toContain("Commits every change, as git sees it: 2 files.");

    messageBox().onChange({ target: { value: "KADE-41: Round the total per line" } });
    await settle();
    await click(/^Commit Ctrl/);
    expect(called("handCommit")).toEqual([["t1", "KADE-41: Round the total per line"]]);
    expect(commitBox()).toHaveLength(0);
    expect(bar().text).toContain("Committed 2 files as 3f5c1a2.");
    expect(changedCalls).toBe(1);
    await wait(6000);
    expect(bar().text).not.toContain("Committed");
  });

  it("Commit…: Ctrl+Enter commits; an empty message can't; Cancel and Escape close the box", async () => {
    files = { "a.txt": "modified" };
    h.answers.handCommit = () => ({ sha: "abcdef123456", subject: "x" });
    await showBar();
    await click("Commit…");
    await click("Cancel");
    expect(commitBox()).toHaveLength(0);
    await click("Commit…");
    (commitBox()[0]!.props.onKeyDown as (e: unknown) => void)({ key: "Escape", ...ev });
    await settle();
    expect(commitBox()).toHaveLength(0);

    await click("Commit…");
    messageBox().onChange({ target: { value: "   " } });
    await settle();
    expect(button(/^Commit Ctrl/)).toMatchObject({ disabled: true, title: "Write a commit message first." });
    messageBox().onKeyDown({ key: "Enter", ctrlKey: true, ...ev });
    await settle();
    expect(called("handCommit")).toEqual([]);
    messageBox().onChange({ target: { value: "KADE-41: done" } });
    await settle();
    messageBox().onKeyDown({ key: "Enter", ctrlKey: true, ...ev });
    await settle();
    expect(called("handCommit")).toEqual([["t1", "KADE-41: done"]]);
  });

  it("Commit…'s box lists at most 8 files, then how many more", async () => {
    files = Object.fromEntries(Array.from({ length: 11 }, (_, i) => [`f${String(i).padStart(2, "0")}.txt`, "untracked" as GitMark]));
    await showBar();
    await click("Commit…");
    const listed = findAll(commitBox()[0]!.props.children as ReactNode, (f) => f.type === "li").map((f) => textOf(f.props.children as ReactNode));
    expect(listed).toHaveLength(9);
    expect(listed.at(-1)).toBe("and 3 more");
  });

  it("Push: on with commits the remote doesn't have, says what it did", async () => {
    own = 2;
    hand = { ...hand, unpushed: 2 };
    h.answers.handPush = () => { hand = { ...hand, unpushed: 0 }; return `Pushed ${BRANCH} (2 commits).`; };
    await showBar();
    expect(bar().text).toContain("2 commits not pushed");
    expect(button("Push")).toMatchObject({ disabled: false, title: `Push ${BRANCH}: 2 commits the remote doesn't have` });
    expect(button("Hand over to Testing")).toMatchObject({ className: "btn sm primary", disabled: false,
      title: "Pushes, moves KADE-41 to Testing for QA Agent and takes the pin off" });
    await click("Push");
    expect(called("handPush")).toEqual([["t1"]]);
    expect(bar().text).toContain(`Pushed ${BRANCH} (2 commits).`);
    expect(changedCalls).toBe(1);
  });

  it("a failed push: a red bar with what went wrong, Try again and Dismiss", async () => {
    own = 1;
    hand = { ...hand, unpushed: 1 };
    let tries = 0;
    h.answers.handPush = () => { tries++; if (tries === 1) throw "Error: GitHub refused the push: permission denied"; return `Pushed ${BRANCH} (1 commit).`; };
    await showBar();
    await click("Push");
    expect(bar()).toMatchObject({ cls: "ed-bar failed", role: "alert", text: "GitHub refused the push: permission denied.Try again" });
    await click("Try again");
    expect(tries).toBe(2);
    expect(bar().cls).toBe("ed-bar hand");
    expect(bar().text).toContain(`Pushed ${BRANCH} (1 commit).`);
  });

  it("a refused Hand over: a red bar without Try again, and Dismiss brings the card bar back", async () => {
    own = 1;
    h.answers.handOver = () => { throw "Error: KADE-41 has 1 uncommitted change: commit it before you hand it over"; };
    await showBar();
    await click("Hand over to Testing");
    expect(bar()).toMatchObject({ cls: "ed-bar failed", text: "KADE-41 has 1 uncommitted change: commit it before you hand it over." });
    expect(hasButton("Try again")).toBe(false);
    const dismiss = findAll(tree, (f) => f.type === "button" && f.props["aria-label"] === "Dismiss")[0]!.props as ButtonProps;
    await dismiss.onClick(ev);
    await settle();
    expect(bar().cls).toBe("ed-bar hand");
    expect(handed).toEqual([]);
  });

  it("Hand over to Testing: says where it went and who takes it", async () => {
    own = 1;
    h.answers.handOver = () => "Testing";
    await showBar();
    await click("Hand over to Testing");
    expect(called("handOver")).toEqual([["t1"]]);
    expect(handed).toEqual(["Handed over to Testing: QA Agent takes it from here. Its worktree is read-only while an agent works on it."]);
  });

  it("with the card's Testing switch off: Hand over to Review, for your review", async () => {
    own = 1;
    h.answers.handOver = () => "Review";
    const t = { ...BY_HAND, testing: false } as Task;
    await showBar(t);
    expect(button("Hand over to Review").title).toBe("Pushes, moves KADE-41 to Review for you and takes the pin off");
    await click("Hand over to Review");
    expect(handed).toEqual(["Handed over to Review: it waits for your review."]);
  });

  it("a project without a link to push to: Push off saying so, Hand over only moves it", async () => {
    own = 1;
    h.answers.getProject = () => ({ ...project, repoUrl: null });
    await showBar();
    expect(bar().text).toContain("Everything committed");
    expect(bar().text).not.toContain("pushed");
    expect(button("Push")).toMatchObject({ disabled: true, title: "Kade portal has no GitHub, Bitbucket or git link to push to" });
    expect(button("Hand over to Testing").title).toBe("Moves KADE-41 to Testing for QA Agent and takes the pin off");
  });

  it("nothing done yet: No changes yet, every button off", async () => {
    await showBar();
    expect(bar().text).toContain("No changes yet");
    expect([button("Commit…").disabled, button("Push").disabled, button("Hand over to Testing").disabled]).toEqual([true, true, true]);
    expect(button("Hand over to Testing").title).toBe("Commit a change first");
  });

  it("back from QA: the bar says so in needs with Open card until you open the card", async () => {
    files = { "a.txt": "modified" };
    store.set(SEEN_KEY, JSON.stringify({ t1: Date.parse("2026-10-12T11:00:00Z") }));
    runs = [{ id: "r9", agentId: "a-qa", agentName: "QA Agent", taskId: "t1", trigger: "task", status: "succeeded", outcome: "qa_fail",
      createdAt: Date.parse("2026-10-12T11:30:00Z"), endedAt: Date.parse("2026-10-12T11:40:00Z"), costUsdMicros: 0, inputTokens: 0, outputTokens: 0, logPath: "" } as Run];
    await showBar();
    const b = bar();
    expect(b.cls).toBe("ed-bar needs");
    expect(b.text).toContain("QA Agent sent KADE-41 back to you: what it found is on the card.");
    expect(button("Commit…").className).toBe("btn sm");
    await click("Open card");
    expect(h.went).toEqual([{ page: "task", id: "t1" }]);

    // you opened the card since: the usual bar
    store.set(SEEN_KEY, JSON.stringify({ t1: Date.parse("2026-10-12T11:50:00Z") }));
    await showBar({ ...BY_HAND, updatedAt: 1 } as Task);
    expect(bar().cls).toBe("ed-bar hand");
  });
});

describe("the card bar in the Editor (GA-115)", () => {
  beforeEach(() => { task = { ...BY_HAND }; });
  const open = () => show(() => <EditorPage key={FOLDER} id={FOLDER} youId={YOU} />);

  it("your card by hand: the card bar takes the folder bar's place, counting git's changes", async () => {
    files = { "src/Invoice.php": "modified" };
    await open();
    expect(bar()).toMatchObject({ cls: "ed-bar hand" });
    expect(bar().text).toContain("1 file changed");
    // a save in the Editor shows at the next read of git's status (every 10 seconds)
    files = { "src/Invoice.php": "modified", "src/Total.php": "untracked" };
    await wait(10_000);
    expect(bar().text).toContain("2 files changed");
  });

  it("someone else's card by hand has no card bar, but says whose it is", async () => {
    task = { ...BY_HAND, assigneeId: "u-other", assigneeName: "Sam" } as Task;
    await open();
    expect(bar().cls).toBe("ed-bar");
    expect(bar().text).toContain("KADE-41 is Sam's card");
    expect(hasButton("Commit…")).toBe(false);
  });

  it("while an agent works on it, the read-only bar shows instead of the card bar", async () => {
    folder = folderInfo({ readOnly: true, readOnlyWhy: "QA Agent is working on KADE-41" });
    await open();
    expect(bar().text).toContain("QA Agent is working on KADE-41: its worktree is read-only until the run ends.");
    expect(hasButton("Commit…")).toBe(false);
  });

  it("commit then hand over: git's status is read again after the commit, then the bar says where the card went", async () => {
    files = { "src/Invoice.php": "modified" };
    h.answers.handCommit = () => { files = {}; own = 1; return { sha: "3f5c1a29b8e7", subject: "s" }; };
    h.answers.handOver = () => { task = { ...BY_HAND, byHand: false, assigneeId: "a-qa", stateId: "s-test", stateName: "Testing", stateCategory: "testing" } as Task; return "Testing"; };
    await open();
    await click("Commit…");
    const gitReads = called("editorGit").length;
    await click(/^Commit Ctrl/);
    expect(called("editorGit").length).toBeGreaterThan(gitReads);
    expect(bar().text).toContain("Committed 1 file as 3f5c1a2.");
    expect(button("Hand over to Testing")).toMatchObject({ className: "btn sm primary", disabled: false });

    await click("Hand over to Testing");
    await rowsChanged();
    expect(bar()).toMatchObject({ cls: "ed-bar done", text: "Handed over to Testing: QA Agent takes it from here. Its worktree is read-only while an agent works on it.Open card" });
    await click("Open card");
    expect(h.went).toEqual([{ page: "task", id: "t1" }]);
  });

  it("back from QA, pinned to you again: the card bar comes back in place of where Hand over sent it", async () => {
    own = 1;
    h.answers.handOver = () => { task = { ...BY_HAND, byHand: false, assigneeId: "a-qa", stateId: "s-test", stateName: "Testing", stateCategory: "testing" } as Task; return "Testing"; };
    await open();
    await click("Hand over to Testing");
    await rowsChanged();
    expect(bar().cls).toBe("ed-bar done");

    // QA failed it: the card is yours by hand again, and its latest run is QA's fail
    runs = [{ id: "r9", agentId: "a-qa", agentName: "QA Agent", taskId: "t1", trigger: "task", status: "succeeded", outcome: "qa_fail",
      createdAt: Date.now() + 1000, endedAt: Date.now() + 2000, costUsdMicros: 0, inputTokens: 0, outputTokens: 0, logPath: "" } as Run];
    task = { ...BY_HAND, updatedAt: 2 } as Task;
    await rowsChanged();
    expect(bar().cls).toBe("ed-bar needs");
    expect(bar().text).toContain("QA Agent sent KADE-41 back to you");
  });
});
