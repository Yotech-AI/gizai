// GA-136 QA: the sidebar's Terminal section (TF1; design/terminal §1, §2 and §8) against the card's acceptance criteria.
// The sidebar runs in a small hook runtime (as in SidebarBehind.test.tsx): components are called as functions, keep
// their hooks by their place in the tree, and effects run after each render; its data (useData, the team, the live runs,
// the drawers) is handed in. The open terminals come from a fake store (useTerminals), which terminalRename's answer
// updates as noteChanged does; contract CT's calls are recorded. The folder picker and the row menu are stand-ins that
// keep their props, so what the section hands them is checked here (FolderPicker's own question is in terminalWords.test.ts).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createElement, type ReactElement, type ReactNode } from "react";
import type { TerminalSession } from "../types";

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
  const useSyncExternalStore = ((subscribe: never, snapshot: () => unknown) => (h.cur ? snapshot() : R.useSyncExternalStore(subscribe, snapshot))) as typeof R.useSyncExternalStore;
  const hooks = { useState, useRef, useMemo, useCallback, useEffect, useLayoutEffect: useEffect, useSyncExternalStore };
  return { ...R, default: { ...R, ...hooks }, ...hooks };
});

// ---- the terminals: a fake store, and contract CT recorded ----
const s = vi.hoisted(() => ({
  terms: [] as TerminalSession[],
  data: {} as Record<string, unknown>,
  calls: [] as string[],
  shown: [] as [string, unknown][],
  went: [] as unknown[],
  copied: [] as string[],
}));
vi.mock("../lib/terminals", async (orig) => ({
  ...(await orig<object>()),
  useTerminals: () => s.terms,
  noteChanged: (x: TerminalSession) => { s.terms = s.terms.map((t) => (t.id === x.id ? x : t)); },
  noteOpened: (x: TerminalSession) => { s.terms = [...s.terms, x]; },
  showIn: (id: string, place: unknown) => { s.calls.push(`showIn ${id}`); s.shown.push([id, place]); },
}));
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) if (typeof real[k] === "function") out[k] = () => new Promise(() => {});
  const as = (key: string) => () => { const p = { key, catch: () => p }; return p; };
  return {
    ...out,
    listProjects: as("projects"), listTasks: as("tasks"), getTeam: as("team"), memoryNotes: as("notes"), listChatThreads: as("threads"),
    editorFolders: () => Promise.resolve([]),
    editorApps: () => Promise.resolve([]),
    onEditorUpdated: () => Promise.resolve(() => {}),
    terminalClose: (id: string) => { s.calls.push(`close ${id}`); s.terms = s.terms.filter((t) => t.id !== id); return Promise.resolve(); },
    // the backend's rule: the name without spaces around it; an empty one gives the default back (here: the folder's name)
    terminalRename: (id: string, name: string) => {
      s.calls.push(`rename ${id} "${name}"`);
      const t = s.terms.find((x) => x.id === id)!;
      return Promise.resolve({ ...t, name: name.trim() || t.folderPath.split("/").pop()! });
    },
  };
});
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => { key: string }) => ({ data: s.data[fetch().key] ?? null, error: null, reload: () => {}, setData: () => {} }),
}));
vi.mock("../lib/team", async (orig) => ({ ...(await orig<object>()), useCurrentTeam: () => ["t1", () => {}] }));
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));
vi.mock("../lib/drawers", async (orig) => ({ ...(await orig<object>()), useDrawer: () => () => {} }));
vi.mock("../router", async (orig) => ({ ...(await orig<object>()), go: (r: unknown) => { s.calls.push("go"); s.went.push(r); } }));
vi.mock("./chat/useChat", () => ({ useChatLive: () => [] }));
vi.mock("./UpdateNotice", () => ({ UpdateNotice: () => null }));
// stand-ins that keep their props, so the tree shows what they were handed
vi.mock("./editor/FolderPicker", async (orig) => ({ ...(await orig<object>()), FolderPicker: (p: object) => createElement("folder-picker", p) }));
vi.mock("./editor/Menus", async (orig) => ({ ...(await orig<object>()), ContextMenu: (p: object) => createElement("context-menu", p) }));

const store = new Map<string, string>();
vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, String(v)); }, removeItem: (k: string) => { store.delete(k); } });
vi.stubGlobal("navigator", { userAgent: "Linux", platform: "Linux x86_64", clipboard: { writeText: (t: string) => { s.copied.push(t); return Promise.resolve(); } } });
vi.stubGlobal("window", new EventTarget());
vi.stubGlobal("document", { hidden: false, querySelector: () => null, body: { style: {} }, documentElement: { style: {} } });
vi.stubGlobal("CSS", { escape: (x: string) => x });
vi.stubGlobal("HTMLButtonElement", class {});
vi.stubGlobal("requestAnimationFrame", (f: () => void) => { f(); return 0; });

const { Sidebar } = await import("./Sidebar");

// ---- the hook runtime ----
let root: () => ReactNode = () => null;
const insts = new Map<string, Inst>();
const unmount = (inst: Inst) => { for (const x of inst.slots) if ((x as EffectSlot)?.effect) (x as EffectSlot).cleanup?.(); };
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
/** A reload: every component starts again (state from localStorage). */
async function reload() { for (const [k, inst] of insts) { unmount(inst); insts.delete(k); } render(); await settle(); }

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
const kids = (f: Found) => f.props.children as ReactNode;
const sectionOf = (label: string) => findAll(tree, (f) => f.props.className === "nav-section" && f.props["aria-label"] === label)[0];
const terminal = () => sectionOf("Terminal")!;
/** The section's heading: its name, the folded tag, + and the caret. */
const headingOf = () => {
  const label = findAll(kids(terminal()), (f) => f.props.className === "nav-label")[0]!;
  const tag = findAll(kids(label), (f) => f.props.className === "fold-tag")[0];
  const buttons = findAll(kids(label), (f) => f.type === "button");
  return {
    tag: tag ? { word: textOf(kids(tag)), tip: tag.props.title } : null,
    add: buttons.find((b) => b.props["aria-label"] === "New terminal"),
    caret: buttons.find((b) => "aria-expanded" in b.props)!,
  };
};
/** Each terminal's row: its link, name, the words on its right, its tooltip. */
const rows = () => findAll(kids(terminal()), (f) => f.type === "a").map((a) => {
  const meta = findAll(kids(a), (f) => f.props.className === "meta")[0];
  const spans = findAll(kids(a), (f) => f.type === "span" && ["card-id", "ellipsis", "faint ellipsis"].includes(String(f.props.className)));
  return {
    el: a, id: a.props["data-term"] as string, href: a.props.href as string, tip: a.props.title as string, aria: a.props["aria-label"] as string,
    on: String(a.props.className).includes(" on"),
    name: spans.filter((x) => x.props.className !== "faint ellipsis").map((x) => textOf(kids(x))).join(""),
    nameClass: spans[0]?.props.className as string,
    faint: spans.filter((x) => x.props.className === "faint ellipsis").map((x) => textOf(kids(x))).join(""),
    says: meta ? textOf(kids(meta)) : "",
    mono: meta ? findAll(kids(meta), (f) => f.props.className === "prog").map((x) => textOf(kids(x))) : [],
  };
});
const row = (id: string) => rows().find((r) => r.id === id)!;
const picker = () => findAll(tree, (f) => f.type === "folder-picker")[0]?.props;
const menu = () => findAll(tree, (f) => f.type === "context-menu")[0]?.props as { label: string; items: ({ label: string; kbd?: string; run: () => void } | { sep: true })[]; onClose: () => void } | undefined;
const asked = () => findAll(kids(terminal()), (f) => f.props.role === "alertdialog")[0];
const nameBox = () => findAll(kids(terminal()), (f) => f.type === "input" && f.props["aria-label"] === "Terminal's name")[0];
/** An element's handler, called with a stand-in event. */
const fire = (f: Found, handler: string, e: Record<string, unknown> = {}) => {
  const target = { getBoundingClientRect: () => ({ left: 10, right: 230, top: 300, bottom: 320 }), closest: () => ({ getBoundingClientRect: () => ({ right: 240 }) }), focus: () => {} };
  (f.props[handler] as (e: unknown) => void)({ preventDefault: () => {}, stopPropagation: () => {}, button: 0, shiftKey: false, clientX: 50, clientY: 300, currentTarget: target, target, ...e });
  render();
};
const item = (label: string) => menu()!.items.find((x) => "label" in x && x.label === label) as { label: string; kbd?: string; run: () => void };

// ---- the data ----
const GIZAI = "/home/you/Herd/gizai";
const KADE41 = "/home/you/.local/share/gizai/worktrees/KADE/KADE-41";
const term = (id: string, more: Partial<TerminalSession> = {}): TerminalSession => ({
  id, name: "gizai", folder: GIZAI, folderPath: "~/Herd/gizai", taskId: null, shell: "zsh", busy: null, ended: null, cols: 80, rows: 24, openedAt: 1, ...more,
});
const three = () => [
  term("t1"),
  term("t2", { name: "gizai 2", busy: "npm" }),
  term("t3", { name: "KADE-41", folder: KADE41, folderPath: "~/.local/share/gizai/worktrees/KADE/KADE-41", taskId: "k41", ended: { code: 2, at: 2, why: "zsh ended with exit code 2" } }),
];

async function show(route: object = { page: "tasks" }) {
  root = () => <Sidebar route={route as never} youId="u1" onSearch={() => {}} onNewTask={() => {}} />;
  render();
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0;
  store.clear();
  s.terms = []; s.calls = []; s.shown = []; s.went = []; s.copied = [];
  s.data = {
    projects: [{ id: "p1", name: "Kade portal", color: "var(--c-yellow)", status: "active" }],
    tasks: [{ id: "k41", identifier: "KADE-41", title: "Fix the invoice total" }],
    threads: [], notes: [], team: { id: "t1", name: "Software", members: [], states: [], labels: [] },
  };
});
afterEach(() => { root = () => null; render(); vi.useRealTimers(); });

describe("the rows (acceptance: ten terminals, ten rows between Projects and Editor; closing them all leaves New terminal)", () => {
  it("ten open terminals are ten rows under Terminal, oldest first, and Terminal sits between Projects and Editor", async () => {
    s.terms = Array.from({ length: 10 }, (_, i) => term(`t${i + 1}`, { name: i ? `gizai ${i + 1}` : "gizai" }));
    await show();
    const sections = findAll(tree, (f) => f.props.className === "nav-section").map((f) => f.props["aria-label"] ?? textOf(kids(findAll(kids(f), (x) => x.props.className === "nav-label")[0]!)).trim());
    const at = (name: string) => sections.findIndex((x) => String(x).startsWith(name));
    expect(at("Projects"), sections.join(" | ")).toBeGreaterThan(-1);
    expect(at("Terminal")).toBe(at("Projects") + 1);
    expect(at("Editor")).toBe(at("Terminal") + 1);
    expect(rows().map((r) => r.name)).toEqual(["gizai", "gizai 2", "gizai 3", "gizai 4", "gizai 5", "gizai 6", "gizai 7", "gizai 8", "gizai 9", "gizai 10"]);
  });
  it("a row is a link to the terminal's page (#/terminal/<id>), with the terminal icon; it is selected while that page shows", async () => {
    s.terms = three();
    await show({ page: "terminal", id: "t2" });
    expect(rows().map((r) => r.href)).toEqual(["#/terminal/t1", "#/terminal/t2", "#/terminal/t3"]);
    expect(rows().map((r) => r.on)).toEqual([false, true, false]);
    const icons = findAll(kids(row("t1").el), (f) => typeof f.type === "object" || typeof f.type === "function");
    expect(icons.length, "the terminal icon").toBeGreaterThan(0);
  });
  it("with none open there is one row, 'New terminal', which opens the picker as + does", async () => {
    await show();
    expect(rows()).toEqual([]);
    const only = findAll(kids(terminal()), (f) => String(f.props.className).startsWith("nav-item"));
    expect(only).toHaveLength(1);
    expect(textOf(kids(only[0]!))).toBe("New terminal");
    expect(picker()).toBeUndefined();
    fire(only[0]!, "onClick");
    expect(picker()?.title).toBe("New terminal in");
  });
  it("closing them all leaves 'New terminal'", async () => {
    s.terms = three();
    await show();
    expect(rows()).toHaveLength(3);
    for (const id of ["t1", "t3"]) { fire(row(id).el, "onContextMenu"); item("Close").run(); render(); }
    fire(row("t2").el, "onContextMenu"); item("Close").run(); render();
    findAll(kids(asked()!), (f) => f.type === "button" && textOf(kids(f)) === "Close").forEach((b) => fire(b, "onClick"));
    expect(s.calls.filter((c) => c.startsWith("close"))).toEqual(["close t1", "close t3", "close t2"]);
    expect(rows()).toEqual([]);
    expect(textOf(kids(terminal()))).toContain("New terminal");
  });
});

describe("what a row says (acceptance: the program running or 'ended'; a card's identifier with the title faint)", () => {
  it("the program in mono on the right while it runs, nothing while the shell waits, 'ended' when the shell ended", async () => {
    s.terms = three();
    await show();
    expect(row("t1").says).toBe("");
    expect(row("t2").says).toBe("npm");
    expect(row("t2").mono).toEqual(["npm"]);
    expect(row("t3").says).toBe("ended");
    expect(row("t3").mono).toEqual([]);
  });
  it("never teal or a pulse, which mean an agent at work", async () => {
    s.terms = three();
    await show();
    const all = JSON.stringify(rows().map((r) => findAll(kids(r.el), (f) => /pulse|live/.test(String(f.props.className))).length));
    expect(all).toBe("[0,0,0]");
  });
  it("a card's worktree: its identifier in mono (card-id) and the card's title faint", async () => {
    s.terms = three();
    await show();
    expect(row("t3").name).toBe("KADE-41");
    expect(row("t3").nameClass).toBe("card-id");
    expect(row("t3").faint).toBe("Fix the invoice total");
    expect(row("t1").nameClass).toBe("ellipsis");
    expect(row("t1").faint).toBe("");
  });
  it("a card's terminal given another name shows that name (not in mono), with the title still faint", async () => {
    s.terms = [term("t3", { name: "the server", folder: KADE41, folderPath: "~/.local/share/gizai/worktrees/KADE/KADE-41", taskId: "k41" })];
    await show();
    expect(row("t3").name).toBe("the server");
    expect(row("t3").nameClass).toBe("ellipsis");
    expect(row("t3").faint).toBe("Fix the invoice total");
  });
  it("the tooltips of design §8, and a name for screen readers", async () => {
    s.terms = three();
    await show();
    expect(row("t1").tip).toBe("~/Herd/gizai · zsh · waiting for a command");
    expect(row("t2").tip).toBe("~/Herd/gizai · zsh · running npm");
    expect(row("t3").tip).toBe("KADE-41's worktree · zsh ended with exit code 2");
    expect(row("t2").aria).toBe("gizai 2, running npm");
    expect(row("t3").aria).toBe("KADE-41 Fix the invoice total, ended");
  });
});

describe("the heading (acceptance: folds like Editor and remembers it; folded, it says how many are open)", () => {
  it("is Editor's: 'Terminal', + (New terminal) and the caret (Fold Terminal)", async () => {
    s.terms = three();
    await show();
    const head = headingOf();
    expect(head.add?.props.title).toBe("New terminal");
    expect(head.caret.props["aria-label"]).toBe("Fold Terminal");
    expect(head.caret.props["aria-controls"]).toBe("side-terminal");
    expect(head.tag).toBeNull();
  });
  it("folded, only the heading is left, saying '3 open' (tooltip: 3 terminals are open); it is kept and a reload shows it folded", async () => {
    s.terms = three();
    await show();
    fire(headingOf().caret, "onClick");
    expect(rows()).toEqual([]);
    expect(headingOf().caret.props["aria-label"]).toBe("Show Terminal");
    expect(headingOf().tag).toEqual({ word: "3 open", tip: "3 terminals are open" });
    expect(store.get("gizai.sidebar.folded")?.split(",")).toContain("terminal");
    await reload();
    expect(rows()).toEqual([]);
    expect(headingOf().tag).toEqual({ word: "3 open", tip: "3 terminals are open" });
    // and opens again, kept too
    fire(headingOf().caret, "onClick");
    expect(rows()).toHaveLength(3);
    await reload();
    expect(rows()).toHaveLength(3);
    expect(store.get("gizai.sidebar.folded")?.split(",") ?? []).not.toContain("terminal");
  });
  it("folding Terminal leaves Editor as it was, and folding Editor leaves Terminal", async () => {
    s.terms = three();
    await show();
    fire(headingOf().caret, "onClick");
    const editorCaret = findAll(kids(sectionOf("Editor")!), (f) => f.type === "button" && "aria-expanded" in f.props)[0]!;
    expect(editorCaret.props["aria-expanded"]).toBe(true);
    fire(editorCaret, "onClick");
    expect(headingOf().caret.props["aria-expanded"]).toBe(false);
    expect(store.get("gizai.sidebar.folded")?.split(",").sort()).toEqual(["editor", "terminal"]);
  });
  it("folded with none open, it says nothing", async () => {
    await show();
    fire(headingOf().caret, "onClick");
    expect(headingOf().tag).toBeNull();
  });
});

describe("New terminal (acceptance: from 20 open terminals, New terminal shows the note)", () => {
  it("+ opens the Editor's folder picker titled 'New terminal in', for you, with no note under 20", async () => {
    s.terms = three();
    await show();
    fire(headingOf().add!, "onClick");
    expect(picker()?.title).toBe("New terminal in");
    expect(picker()?.you).toBe("u1");
    expect(picker()?.note).toBeNull();
    expect(typeof picker()?.onPick).toBe("function");
  });
  it("+ works while the section is folded", async () => {
    s.terms = three();
    await show();
    fire(headingOf().caret, "onClick");
    fire(headingOf().add!, "onClick");
    expect(picker()?.title).toBe("New terminal in");
  });
  it("from 20 open: '20 terminals are open: each keeps its shell running until you close it.' (21 for 21)", async () => {
    s.terms = Array.from({ length: 19 }, (_, i) => term(`t${i}`));
    await show();
    fire(headingOf().add!, "onClick");
    expect(picker()?.note).toBeNull();
    s.terms = [...s.terms, term("t19")];
    render();
    expect(picker()?.note).toBe("20 terminals are open: each keeps its shell running until you close it.");
    s.terms = [...s.terms, term("t20")];
    render();
    expect(picker()?.note).toBe("21 terminals are open: each keeps its shell running until you close it.");
  });
  it("the folder the page shows is the picker's current one: a terminal's page, or the Editor's folder", async () => {
    s.terms = three();
    await show({ page: "terminal", id: "t3" });
    fire(headingOf().add!, "onClick");
    expect(picker()?.current).toBe(KADE41);
    await show({ page: "editor", id: GIZAI });
    fire(headingOf().add!, "onClick");
    expect(picker()?.current).toBe(GIZAI);
  });
});

describe("the row menu (right click): Rename (F2), Show in Editor, Copy folder path and Close", () => {
  it("has the four, in that order, with F2 on Rename", async () => {
    s.terms = three();
    await show();
    fire(row("t2").el, "onContextMenu");
    expect(menu()?.label).toBe("Terminal");
    expect(menu()!.items.map((x) => ("sep" in x ? "—" : x.label))).toEqual(["Rename", "Show in Editor", "Copy folder path", "—", "Close"]);
    expect(item("Rename").kbd).toBe("F2");
  });
  it("opens with the context menu key and Shift+F10 too", async () => {
    s.terms = three();
    await show();
    fire(row("t1").el, "onKeyDown", { key: "ContextMenu" });
    expect(menu()).toBeDefined();
    menu()!.onClose(); render();
    expect(menu()).toBeUndefined();
    fire(row("t1").el, "onKeyDown", { key: "F10", shiftKey: true });
    expect(menu()).toBeDefined();
  });
  it("Show in Editor calls showIn with the folder's panel, then opens the Editor on that folder", async () => {
    s.terms = three();
    await show();
    fire(row("t3").el, "onContextMenu");
    item("Show in Editor").run();
    expect(s.shown).toEqual([["t3", { kind: "panel", folder: KADE41 }]]);
    expect(s.went).toEqual([{ page: "editor", id: KADE41 }]);
    expect(s.calls).toEqual(["showIn t3", "go"]);
  });
  it("Copy folder path copies the folder's path", async () => {
    s.terms = three();
    await show();
    fire(row("t1").el, "onContextMenu");
    item("Copy folder path").run();
    expect(s.copied).toEqual([GIZAI]);
  });
});

describe("closing (acceptance: asks under the row while a program runs; a terminal waiting for a command closes at once)", () => {
  it("Close on a terminal running npm asks under its row first: 'npm is still running in gizai 2. Close it and end npm?'", async () => {
    s.terms = three();
    await show();
    fire(row("t2").el, "onContextMenu");
    item("Close").run(); render();
    expect(s.calls).toEqual([]);
    const ask = asked()!;
    expect(ask.props["aria-label"]).toBe("Close gizai 2");
    expect(textOf(kids(ask))).toContain("npm is still running in gizai 2. Close it and end npm?");
    expect(findAll(kids(ask), (f) => f.type === "button").map((b) => textOf(kids(b)))).toEqual(["Close", "Cancel"]);
    // under the row: right after gizai 2's row in the list
    const list = findAll(kids(terminal()), (f) => f.props.className === "nav-list")[0]!;
    const order = findAll(kids(list), (f) => f.type === "a" || f.props.role === "alertdialog").map((f) => (f.type === "a" ? f.props["data-term"] : "ask"));
    expect(order).toEqual(["t1", "t2", "ask", "t3"]);
  });
  it("Close then ends it, and the row goes", async () => {
    s.terms = three();
    await show();
    fire(row("t2").el, "onContextMenu");
    item("Close").run(); render();
    fire(findAll(kids(asked()!), (f) => f.type === "button" && textOf(kids(f)) === "Close")[0]!, "onClick");
    expect(s.calls).toEqual(["close t2"]);
    expect(rows().map((r) => r.id)).toEqual(["t1", "t3"]);
    expect(asked()).toBeUndefined();
  });
  it("Cancel (or Escape) keeps it running", async () => {
    s.terms = three();
    await show();
    fire(row("t2").el, "onContextMenu");
    item("Close").run(); render();
    fire(findAll(kids(asked()!), (f) => f.type === "button" && textOf(kids(f)) === "Cancel")[0]!, "onClick");
    expect(asked()).toBeUndefined();
    fire(row("t2").el, "onAuxClick", { button: 1 });
    fire(asked()!, "onKeyDown", { key: "Escape" });
    expect(asked()).toBeUndefined();
    expect(s.calls).toEqual([]);
    expect(rows()).toHaveLength(3);
  });
  it("Enter in the question closes", async () => {
    s.terms = three();
    await show();
    fire(row("t2").el, "onAuxClick", { button: 1 });
    fire(asked()!, "onKeyDown", { key: "Enter" });
    expect(s.calls).toEqual(["close t2"]);
  });
  it("a middle click closes: at once for a terminal that waits for a command, or one that ended; it asks for one running npm", async () => {
    s.terms = three();
    await show();
    fire(row("t1").el, "onAuxClick", { button: 1 });
    expect(s.calls).toEqual(["close t1"]);
    fire(row("t3").el, "onAuxClick", { button: 1 });
    expect(s.calls).toEqual(["close t1", "close t3"]);
    fire(row("t2").el, "onAuxClick", { button: 1 });
    expect(s.calls).toEqual(["close t1", "close t3"]);
    expect(asked()).toBeDefined();
  });
  it("a right click (button 2) or a left click doesn't close", async () => {
    s.terms = three();
    await show();
    fire(row("t1").el, "onAuxClick", { button: 2 });
    expect(s.calls).toEqual([]);
  });
});

describe("Rename (acceptance: F2 on the row; an empty name gives the default back)", () => {
  it("F2 puts the name, in a box, in its place; Enter keeps the new name, which the row shows", async () => {
    s.terms = three();
    await show();
    fire(row("t2").el, "onKeyDown", { key: "F2" });
    const box = nameBox()!;
    expect(box.props.value).toBe("gizai 2");
    expect(box.props.maxLength).toBe(40);
    expect(rows().map((r) => r.id)).toEqual(["t1", "t3"]);
    fire(box, "onChange", { target: { value: "dev server" } });
    fire(nameBox()!, "onKeyDown", { key: "Enter" });
    await settle();
    expect(s.calls).toEqual(['rename t2 "dev server"']);
    expect(nameBox()).toBeUndefined();
    expect(row("t2").name).toBe("dev server");
  });
  it("an empty name gives the default back", async () => {
    s.terms = [term("t1", { name: "dev server" })];
    await show();
    fire(row("t1").el, "onKeyDown", { key: "F2" });
    fire(nameBox()!, "onChange", { target: { value: "" } });
    fire(nameBox()!, "onKeyDown", { key: "Enter" });
    await settle();
    expect(s.calls).toEqual(['rename t1 ""']);
    expect(row("t1").name).toBe("gizai");
  });
  it("Escape cancels; the same name again asks nothing of the backend", async () => {
    s.terms = three();
    await show();
    fire(row("t1").el, "onKeyDown", { key: "F2" });
    fire(nameBox()!, "onChange", { target: { value: "other" } });
    fire(nameBox()!, "onKeyDown", { key: "Escape" });
    await settle();
    expect(nameBox()).toBeUndefined();
    expect(row("t1").name).toBe("gizai");
    fire(row("t1").el, "onKeyDown", { key: "F2" });
    fire(nameBox()!, "onKeyDown", { key: "Enter" });
    await settle();
    expect(s.calls).toEqual([]);
  });
  it("leaving the box keeps the name", async () => {
    s.terms = three();
    await show();
    fire(row("t1").el, "onKeyDown", { key: "F2" });
    fire(nameBox()!, "onChange", { target: { value: "tests" } });
    fire(nameBox()!, "onBlur");
    await settle();
    expect(row("t1").name).toBe("tests");
  });
  it("Rename in the row menu does what F2 does", async () => {
    s.terms = three();
    await show();
    fire(row("t1").el, "onContextMenu");
    item("Rename").run(); render();
    expect(nameBox()?.props.value).toBe("gizai");
  });
});
