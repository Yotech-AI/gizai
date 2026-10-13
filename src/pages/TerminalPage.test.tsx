// GA-136 QA: the Terminal page (#/terminal/<id>, TF1; design/terminal §3 and §8) against the card's acceptance criteria:
// the top bar, the folder's tabs (+ without a picker, rename, close, Ctrl+PgDn and Ctrl+PgUp), the ended and couldn't-start
// bars with Start again and Try again, a terminal that leaves the list (exit), Show in Editor, and the empty and closed
// pages. There is no DOM here, so a small hook runtime stands in for React (as in SidebarBehind.test.tsx): components are
// called as functions and keep their hooks by their place in the tree, effects run after each render. The open terminals
// come from a fake store that contract CT's fake updates (open, rename, close); the terminal's view, the folder picker and
// the ⋯ menu are stand-ins that keep their props.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createElement, type ReactElement, type ReactNode } from "react";
import type { EditorFolder, TerminalSession } from "../types";

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

// ---- the terminals: a fake store, and contract CT as a fake that keeps it ----
const s = vi.hoisted(() => ({
  terms: [] as TerminalSession[],
  loaded: true,
  size: null as { cols: number; rows: number } | null,
  folders: [] as EditorFolder[],
  calls: [] as string[],
  shown: [] as [string, unknown][],
  went: [] as unknown[],
  replaced: [] as string[],
  copied: [] as string[],
  cleared: [] as string[],
  next: 10,
  /** Why the next terminalOpen fails, or null. */
  fails: null as string | null,
}));
vi.mock("../lib/terminals", async (orig) => ({
  ...(await orig<object>()),
  useTerminals: () => s.terms,
  useTerminalsLoaded: () => s.loaded,
  useTerminalSize: () => s.size,
  clearTerminal: (id: string) => { s.cleared.push(id); },
  noteChanged: (x: TerminalSession) => { s.terms = s.terms.map((t) => (t.id === x.id ? x : t)); },
  noteOpened: (x: TerminalSession) => { if (!s.terms.some((t) => t.id === x.id)) s.terms = [...s.terms, x]; },
  showIn: (id: string, place: unknown) => { s.calls.push(`showIn ${id}`); s.shown.push([id, place]); },
}));
const folderName = (folder: string) => folder.split("/").pop()!;
/** The backend's default name: the folder's, then "gizai 2", "gizai 3" (the lowest free number). */
const defaultName = (folder: string, but?: string) => {
  const base = folderName(folder);
  const taken = s.terms.filter((t) => t.id !== but).map((t) => t.name);
  if (!taken.includes(base)) return base;
  for (let n = 2; ; n++) if (!taken.includes(`${base} ${n}`)) return `${base} ${n}`;
};
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) if (typeof real[k] === "function") out[k] = () => new Promise(() => {});
  return {
    ...out,
    editorFolders: () => Promise.resolve(s.folders),
    terminalOpen: (folder: string, cols: number, rows: number, name: string | null) => {
      s.calls.push(`open ${folder} ${cols}x${rows} ${name === null ? "null" : `"${name}"`}`);
      if (s.fails) return Promise.reject(s.fails);
      const t = term(`n${s.next++}`, { folder, folderPath: `~/Herd/${folderName(folder)}`, name: name?.trim() || defaultName(folder) });
      return Promise.resolve(t);
    },
    terminalClose: (id: string) => { s.calls.push(`close ${id}`); s.terms = s.terms.filter((t) => t.id !== id); return Promise.resolve(); },
    terminalRename: (id: string, name: string) => {
      s.calls.push(`rename ${id} "${name}"`);
      const t = s.terms.find((x) => x.id === id)!;
      return Promise.resolve({ ...t, name: name.trim() || defaultName(t.folder, id) });
    },
  };
});
vi.mock("../router", async (orig) => ({ ...(await orig<object>()), go: (r: unknown) => { s.calls.push(`go ${JSON.stringify(r)}`); s.went.push(r); } }));
vi.mock("../components/terminal/TerminalView", async (orig) => ({ ...(await orig<object>()), TerminalView: (p: object) => createElement("terminal-view", p) }));
vi.mock("../components/editor/FolderPicker", async (orig) => ({ ...(await orig<object>()), FolderPicker: (p: object) => createElement("folder-picker", p) }));
vi.mock("../components/editor/Menus", async (orig) => ({ ...(await orig<object>()), ContextMenu: (p: object) => createElement("context-menu", p) }));

/** The window: its key listeners are called with plain stand-in events (an Event's target can't be set in Node). */
const heard = new Map<string, Set<(e: unknown) => void>>();
const win = {
  addEventListener: (type: string, f: (e: unknown) => void) => { if (!heard.has(type)) heard.set(type, new Set()); heard.get(type)!.add(f); },
  removeEventListener: (type: string, f: (e: unknown) => void) => { heard.get(type)?.delete(f); },
  innerWidth: 1280, innerHeight: 720,
  location: { hash: "", replace: (u: string) => { s.replaced.push(u); } },
};
vi.stubGlobal("window", win);
vi.stubGlobal("navigator", { userAgent: "Linux", platform: "Linux x86_64", clipboard: { writeText: (t: string) => { s.copied.push(t); return Promise.resolve(); } } });
vi.stubGlobal("document", { hidden: false, querySelector: () => null, body: { style: {} }, documentElement: { style: {} } });
vi.stubGlobal("CSS", { escape: (x: string) => x });
vi.stubGlobal("HTMLButtonElement", class {});
vi.stubGlobal("requestAnimationFrame", (f: () => void) => { f(); return 0; });

const { TerminalPage } = await import("./TerminalPage");

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
const one = (pred: (f: Found) => boolean) => findAll(tree, pred)[0];
const byClass = (c: string) => one((f) => f.props.className === c);
/** An element's handler, called with a stand-in event. */
const fire = (f: Found, handler: string, e: Record<string, unknown> = {}) => {
  const target = { getBoundingClientRect: () => ({ left: 300, right: 420, top: 40, bottom: 72 }), parentElement: null, focus: () => {}, closest: () => null };
  (f.props[handler] as (e: unknown) => void)({ preventDefault: () => {}, stopPropagation: () => {}, button: 0, shiftKey: false, currentTarget: target, target, ...e });
  render();
};
/** A key on the page (the window hears it, as it does from the terminal). */
const key = (k: string, more: Record<string, unknown> = {}) => {
  const e = { key: k, code: k, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, target: null, defaultPrevented: false, ...more,
    preventDefault() { e.defaultPrevented = true; } };
  for (const f of [...(heard.get("keydown") ?? [])]) f(e);
  render();
};

// the top bar
const crumbs = () => byClass("crumbs ed-crumbs")!;
const where = () => byClass("term-where")!;
const showInEditorBtn = () => one((f) => f.type === "button" && textOf(kids(f)).includes("Show in Editor"))!;
const moreBtn = () => one((f) => f.type === "button" && f.props["aria-label"] === "More")!;
const menu = () => one((f) => f.type === "context-menu")?.props as { label: string; items: ({ label: string; kbd?: string; title?: string; run: () => void } | { sep: true })[]; onClose: () => void } | undefined;
const item = (label: string) => menu()!.items.find((x) => "label" in x && x.label === label) as { label: string; kbd?: string; title?: string; run: () => void };
/** Chooses a menu item as ContextMenu does: the menu closes, then the item runs. */
const choose = (label: string) => { const run = item(label).run; menu()!.onClose(); run(); render(); };
const view = () => one((f) => f.type === "terminal-view")?.props as { id: string; place: unknown; findAsk: number; autoFocus: boolean } | undefined;
const picker = () => one((f) => f.type === "folder-picker")?.props;
// the tabs
const strip = () => one((f) => f.props.role === "tablist")!;
const tabs = () => findAll(kids(strip()), (f) => f.props.role === "tab").map((t) => {
  const name = findAll(kids(t), (f) => f.type === "span" && String(f.props.className).startsWith("name"))[0];
  const right = findAll(kids(t), (f) => f.props.className === "prog" || f.props.className === "state")[0];
  return {
    el: t, id: t.props["data-term"] as string, name: name ? textOf(kids(name)) : "", selected: t.props["aria-selected"] as boolean, tip: t.props.title as string,
    says: right ? textOf(kids(right)) : "", x: findAll(kids(t), (f) => f.type === "button" && f.props.className === "x")[0]!,
  };
});
const tab = (id: string) => tabs().find((t) => t.id === id)!;
const plus = () => findAll(kids(strip()), (f) => f.type === "button" && String(f.props["aria-label"]).startsWith("New terminal in"))[0]!;
const nameBox = () => one((f) => f.type === "input" && f.props["aria-label"] === "Terminal's name");
const ask = () => one((f) => f.props.role === "alertdialog" && String(f.props.className).includes("ed-ask"));
const askButton = (label: string) => findAll(kids(ask()!), (f) => f.type === "button" && textOf(kids(f)) === label)[0]!;
const bar = () => one((f) => String(f.props.className).startsWith("ed-bar file"));
const barButton = (label: string) => findAll(kids(bar()!), (f) => f.type === "button" && textOf(kids(f)).includes(label))[0]!;

// ---- the data: two terminals in gizai, one in kade-portal ----
const GIZAI = "/home/you/Herd/gizai";
const KADE = "/home/you/Herd/kade-portal";
const term = (id: string, more: Partial<TerminalSession> = {}): TerminalSession => ({
  id, name: "gizai", folder: GIZAI, folderPath: "~/Herd/gizai", taskId: null, shell: "zsh", busy: null, ended: null, cols: 80, rows: 24, openedAt: 1, ...more,
});
const three = () => [term("t1"), term("t2", { name: "gizai 2", busy: "npm" }), term("k1", { name: "kade-portal", folder: KADE, folderPath: "~/Herd/kade-portal" })];
const folder = (id: string, more: Partial<EditorFolder> = {}): EditorFolder =>
  ({ id, path: id, name: folderName(id), kind: "root", projectId: null, taskId: null, readOnly: false, readOnlyWhy: null, ...more });

async function show(id?: string) {
  root = () => <TerminalPage id={id} youId="u1" />;
  render();
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0;
  s.terms = three(); s.loaded = true; s.size = { cols: 132, rows: 38 }; s.folders = [folder(GIZAI), folder(KADE)];
  s.calls = []; s.shown = []; s.went = []; s.replaced = []; s.copied = []; s.cleared = []; s.next = 10; s.fails = null;
});
afterEach(() => { root = () => null; render(); vi.useRealTimers(); });

describe("the top bar (design §3 and §8)", () => {
  it("Terminal / gizai 2, the folder in mono, the shell and its size: '~/Herd/gizai · zsh · 132 × 38'", async () => {
    await show("t2");
    expect(textOf(kids(crumbs()))).toBe("Terminal/gizai 2");
    expect(findAll(kids(crumbs()), (f) => f.type === "a")[0]!.props.href).toBe("#/terminal");
    expect(findAll(kids(crumbs()), (f) => f.type === "b")[0]!.props.title).toBe("Double-click to rename (F2)");
    expect(textOf(kids(where()))).toBe("~/Herd/gizai·zsh·132 × 38");
    expect(findAll(kids(where()), (f) => f.props.className === "path")[0]!.props.title).toBe("~/Herd/gizai");
    expect(findAll(kids(where()), (f) => f.props.className === "size")[0]!.props.title).toBe("132 columns, 38 rows");
  });
  it("the size follows the place it shows in, and is the terminal's own before its view is made", async () => {
    s.size = null;
    await show("t2");
    expect(textOf(kids(where()))).toBe("~/Herd/gizai·zsh·80 × 24");
    s.size = { cols: 120, rows: 12 };
    render();
    expect(textOf(kids(where()))).toBe("~/Herd/gizai·zsh·120 × 12");
  });
  it("Show in Editor (tooltip: Show gizai 2 in gizai's Editor, in its terminal panel) calls showIn with the folder's panel, then opens the Editor on the folder", async () => {
    await show("t2");
    expect(showInEditorBtn().props.title).toBe("Show gizai 2 in gizai's Editor, in its terminal panel");
    fire(showInEditorBtn(), "onClick");
    expect(s.shown).toEqual([["t2", { kind: "panel", folder: GIZAI }]]);
    expect(s.went).toEqual([{ page: "editor", id: GIZAI }]);
    expect(s.calls).toEqual(["showIn t2", `go {"page":"editor","id":"${GIZAI}"}`]);
  });
  it("⋯ (tooltip: More) has Rename (F2), Find (Ctrl Shift F), Clear (empties the scrollback), Copy folder path and Close", async () => {
    await show("t2");
    expect(moreBtn().props.title).toBe("More");
    fire(moreBtn(), "onClick");
    expect(menu()!.items.map((x) => ("sep" in x ? "—" : x.label))).toEqual(["Rename", "Find", "Clear", "Copy folder path", "—", "Close"]);
    expect(item("Rename").kbd).toBe("F2");
    expect(item("Find").kbd).toBe("Ctrl Shift F");
    expect(item("Clear").title).toBe("Empties the scrollback");
  });
  it("⋯ → Find opens find in the view, Clear clears it, Copy folder path copies the folder", async () => {
    await show("t2");
    expect(view()!.findAsk).toBe(0);
    fire(moreBtn(), "onClick"); choose("Find");
    expect(view()!.findAsk).toBe(1);
    fire(moreBtn(), "onClick"); choose("Clear");
    expect(s.cleared).toEqual(["t2"]);
    fire(moreBtn(), "onClick"); choose("Copy folder path");
    expect(s.copied).toEqual([GIZAI]);
  });
  it("the terminal shows in the page's place, with the focus", async () => {
    await show("t2");
    expect(view()).toMatchObject({ id: "t2", place: { kind: "page" }, autoFocus: true });
  });
});

describe("the tabs (acceptance: the folder's terminals; + starts another there; Ctrl+PgDn and Ctrl+PgUp switch)", () => {
  it("are the terminals of the page's folder only, the shown one selected, each with ×, the program or 'ended', and its tooltip", async () => {
    s.terms = [...three(), term("t3", { name: "gizai 3", ended: { code: 2, at: 2, why: "zsh ended with exit code 2" } })];
    await show("t2");
    expect(strip().props["aria-label"]).toBe("Terminals in gizai");
    expect(tabs().map((t) => [t.id, t.name, t.selected, t.says])).toEqual([["t1", "gizai", false, ""], ["t2", "gizai 2", true, "npm"], ["t3", "gizai 3", false, "ended"]]);
    expect(tabs().map((t) => t.tip)).toEqual(["gizai · waiting for a command", "gizai 2 · running npm", "gizai 3 · ended"]);
    expect(tabs().map((t) => t.x.props["aria-label"])).toEqual(["Close gizai", "Close gizai 2", "Close gizai 3"]);
  });
  it("a click on another tab opens it", async () => {
    await show("t2");
    fire(tab("t1").el, "onClick");
    expect(s.went).toEqual([{ page: "terminal", id: "t1" }]);
  });
  it("+ (tooltip: New terminal in gizai (Ctrl+Shift+`)) starts another terminal in the folder, without a picker, and shows it", async () => {
    await show("t2");
    expect(plus().props.title).toBe("New terminal in gizai (Ctrl+Shift+`)");
    fire(plus(), "onClick");
    await settle();
    expect(picker()).toBeUndefined();
    expect(s.calls).toEqual([`open ${GIZAI} 0x0 null`, 'go {"page":"terminal","id":"n10"}']);
    expect(s.terms.find((t) => t.id === "n10")?.name).toBe("gizai 3");
  });
  it("Ctrl+Shift+` does what + does", async () => {
    await show("t1");
    key("~", { code: "Backquote", ctrlKey: true, shiftKey: true });
    await settle();
    expect(s.calls[0]).toBe(`open ${GIZAI} 0x0 null`);
  });
  it("+ is off while an agent works on the card, with why as its tooltip", async () => {
    const CARD = "/home/you/.local/share/gizai/worktrees/GA/GA-118";
    s.terms = [term("c1", { name: "GA-118", folder: CARD, folderPath: "~/.local/share/gizai/worktrees/GA/GA-118", taskId: "g118" })];
    s.folders = [folder(CARD, { kind: "card", taskId: "g118", readOnly: true, readOnlyWhy: "QA Agent is working on GA-118" })];
    await show("c1");
    expect(plus().props["aria-disabled"]).toBe(true);
    expect(plus().props.title).toBe("QA Agent is working on GA-118: a terminal there waits until its run ends.");
    fire(plus(), "onClick");
    key("~", { code: "Backquote", ctrlKey: true, shiftKey: true });
    await settle();
    expect(s.calls).toEqual([]);
  });
  it("Ctrl+PgDn and Ctrl+PgUp step through the folder's tabs, round from the last to the first", async () => {
    s.terms = [...three(), term("t3", { name: "gizai 3" })];
    await show("t2");
    key("PageDown", { ctrlKey: true });
    expect(s.went.pop()).toEqual({ page: "terminal", id: "t3" });
    key("PageUp", { ctrlKey: true });
    expect(s.went.pop()).toEqual({ page: "terminal", id: "t1" });
    await show("t3");
    key("PageDown", { ctrlKey: true });
    expect(s.went.pop()).toEqual({ page: "terminal", id: "t1" });
    // not without Ctrl, and never to another folder's terminal
    key("PageDown");
    expect(s.went).toEqual([]);
  });
  it("← and → on a tab with the focus move to the next tab", async () => {
    await show("t1");
    fire(tab("t1").el, "onKeyDown", { key: "ArrowRight", target: undefined });
    expect(s.went).toEqual([]); // the event's target is the tab itself only when the tab has the focus
    const t1 = tab("t1").el;
    const self = { getBoundingClientRect: () => ({}) };
    (t1.props.onKeyDown as (e: unknown) => void)({ key: "ArrowRight", preventDefault: () => {}, target: self, currentTarget: self });
    render();
    expect(s.went).toEqual([{ page: "terminal", id: "t2" }]);
  });
});

describe("Rename (acceptance: a double-click on a tab or on the page's name; the new name on the tab and the page; an empty name gives the default back)", () => {
  it("a double-click on a tab renames it in place; the tab and the page's name show the new name", async () => {
    await show("t2");
    fire(tab("t2").el, "onDoubleClick");
    expect(nameBox()!.props.value).toBe("gizai 2");
    fire(nameBox()!, "onChange", { target: { value: "dev server" } });
    fire(nameBox()!, "onKeyDown", { key: "Enter" });
    await settle();
    expect(s.calls).toEqual(['rename t2 "dev server"']);
    expect(tab("t2").name).toBe("dev server");
    expect(textOf(kids(crumbs()))).toBe("Terminal/dev server");
  });
  it("a double-click on the page's name renames it there; the tab shows it too", async () => {
    await show("t1");
    fire(findAll(kids(crumbs()), (f) => f.type === "b")[0]!, "onDoubleClick");
    expect(findAll(kids(crumbs()), (f) => f.type === "input")[0]!.props.value).toBe("gizai");
    fire(nameBox()!, "onChange", { target: { value: "tests" } });
    fire(nameBox()!, "onKeyDown", { key: "Enter" });
    await settle();
    expect(textOf(kids(crumbs()))).toBe("Terminal/tests");
    expect(tab("t1").name).toBe("tests");
  });
  it("F2 on the page renames it too (not while typing in the terminal)", async () => {
    await show("t1");
    key("F2", { target: { closest: (sel: string) => (sel.includes(".xterm") ? {} : null) } });
    expect(nameBox()).toBeUndefined();
    key("F2", { target: { closest: () => null } });
    expect(nameBox()!.props.value).toBe("gizai");
  });
  it("⋯ → Rename renames it in place", async () => {
    await show("t1");
    fire(moreBtn(), "onClick"); choose("Rename");
    expect(nameBox()!.props.value).toBe("gizai");
  });
  it("an empty name gives the default back, on the tab and the page", async () => {
    s.terms = [term("t1", { name: "dev server" }), term("t2", { name: "gizai 2" })];
    await show("t1");
    fire(tab("t1").el, "onDoubleClick");
    fire(nameBox()!, "onChange", { target: { value: "   " } });
    fire(nameBox()!, "onKeyDown", { key: "Enter" });
    await settle();
    expect(s.calls).toEqual(['rename t1 "   "']);
    expect(tab("t1").name).toBe("gizai");
    expect(textOf(kids(crumbs()))).toBe("Terminal/gizai");
  });
});

describe("closing a tab (acceptance: asks under the tab while a program runs; one waiting for a command closes at once)", () => {
  it("× on a terminal running npm asks under the tab: 'npm is still running in gizai 2. Close it and end npm?', Cancel and Close", async () => {
    await show("t2");
    fire(tab("t2").x, "onClick", { currentTarget: { parentElement: { getBoundingClientRect: () => ({ left: 300, bottom: 72 }) } } });
    expect(s.calls).toEqual([]);
    expect(ask()!.props["aria-label"]).toBe("Close gizai 2");
    expect(textOf(kids(ask()!))).toBe("npm is still running in gizai 2. Close it and end npm?CancelClose");
    // under the tab
    expect((ask()!.props.style as { left: number; top: number })).toMatchObject({ left: 300, top: 76 });
    fire(askButton("Close"), "onClick");
    expect(s.calls).toEqual(["close t2"]);
    expect(ask()).toBeUndefined();
  });
  it("Cancel keeps it", async () => {
    await show("t2");
    fire(tab("t2").el, "onAuxClick", { button: 1 });
    fire(askButton("Cancel"), "onClick");
    expect(ask()).toBeUndefined();
    expect(s.calls).toEqual([]);
  });
  it("a middle click on a tab waiting for a command closes it at once", async () => {
    await show("t2");
    fire(tab("t1").el, "onAuxClick", { button: 1 });
    expect(s.calls).toEqual(["close t1"]);
    expect(tabs().map((t) => t.id)).toEqual(["t2"]);
  });
  it("⋯ → Close asks the same under the tab while npm runs, and closes at once otherwise", async () => {
    await show("t2");
    fire(moreBtn(), "onClick"); choose("Close");
    expect(textOf(kids(ask()!))).toContain("npm is still running in gizai 2. Close it and end npm?");
    fire(askButton("Cancel"), "onClick");
    await show("t1");
    fire(moreBtn(), "onClick"); choose("Close");
    expect(s.calls).toEqual(["close t1"]);
  });
  it("the shown terminal closing gives way to the tab next to it, else the one before it, else #/terminal", async () => {
    s.terms = [...three(), term("t3", { name: "gizai 3" })];
    await show("t2");
    s.terms = s.terms.filter((t) => t.id !== "t2"); // closed (×, the sidebar) or `exit`: it leaves the list
    render();
    expect(s.replaced).toEqual(["#/terminal/t3"]);
    await show("t3");
    s.terms = s.terms.filter((t) => t.id !== "t3");
    render();
    expect(s.replaced.pop()).toBe("#/terminal/t1");
    await show("t1");
    s.terms = s.terms.filter((t) => t.id !== "t1");
    render();
    expect(s.replaced.pop()).toBe("#/terminal");
  });
});

describe("the end states (acceptance: exit closes; another code shows why, with Start again in the same folder)", () => {
  const ended2 = () => [term("t1"), term("t2", { name: "gizai 2", ended: { code: 2, at: 2, why: "zsh ended with exit code 2" } })];
  it("`exit` (code 0): the terminal leaves the list, so its page gives way to the next tab", async () => {
    await show("t1");
    s.terms = s.terms.filter((t) => t.id !== "t1");
    render();
    expect(s.replaced).toEqual(["#/terminal/t2"]);
  });
  it("another code: 'The shell ended with exit code 2.' under the tabs, with Start again and Close; the tab says 'ended'", async () => {
    s.terms = ended2();
    await show("t2");
    expect(textOf(kids(bar()!))).toBe("The shell ended with exit code 2.Start againClose");
    expect(tab("t2").says).toBe("ended");
    expect(view()?.id).toBe("t2");
  });
  it("a signal: 'The shell was ended by a signal.'", async () => {
    s.terms = [term("t1", { ended: { code: null, at: 2, why: "zsh was ended by SIGKILL" } })];
    await show("t1");
    expect(textOf(kids(bar()!))).toBe("The shell was ended by a signal.Start againClose");
  });
  it("Start again opens a new terminal in the same folder with the same name, shows it, then closes the ended one", async () => {
    s.terms = ended2();
    await show("t2");
    fire(barButton("Start again"), "onClick");
    await settle();
    expect(s.calls).toEqual([`open ${GIZAI} 0x0 "gizai 2"`, 'go {"page":"terminal","id":"n10"}', "close t2"]);
    expect(s.terms.map((t) => [t.id, t.name, t.folder])).toEqual([["t1", "gizai", GIZAI], ["n10", "gizai 2", GIZAI]]);
  });
  it("Close on the bar closes the ended terminal", async () => {
    s.terms = ended2();
    await show("t2");
    fire(barButton("Close"), "onClick");
    expect(s.calls).toEqual(["close t2"]);
  });
  it("a shell that couldn't start: 'Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found.', with Try again and Close", async () => {
    await show("t1");
    s.fails = "Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found";
    fire(plus(), "onClick");
    await settle();
    expect(bar()!.props.role).toBe("alert");
    expect(textOf(kids(bar()!))).toBe("Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found.Try againClose");
    // Try again asks again, and the bar goes when it works
    s.fails = null;
    fire(barButton("Try again"), "onClick");
    await settle();
    expect(s.calls.filter((c) => c.startsWith("open"))).toHaveLength(2);
    expect(s.went.pop()).toEqual({ page: "terminal", id: "n10" });
  });
  it("Close on the couldn't-start bar puts it away", async () => {
    await show("t1");
    s.fails = "Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found";
    fire(plus(), "onClick");
    await settle();
    fire(barButton("Close"), "onClick");
    expect(bar()).toBeUndefined();
    expect(s.calls.filter((c) => c.startsWith("close"))).toEqual([]);
  });
  it("Start again that can't start says why, and Close there closes the ended terminal", async () => {
    s.terms = ended2();
    await show("t2");
    s.fails = "Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found";
    fire(barButton("Start again"), "onClick");
    await settle();
    expect(textOf(kids(bar()!))).toBe("Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found.Try againClose");
    fire(barButton("Close"), "onClick");
    expect(s.calls.filter((c) => c.startsWith("close"))).toEqual(["close t2"]);
  });
});

describe("the pages without a terminal (design §3)", () => {
  it("#/terminal with none open: 'No terminal open', what it is for, and New terminal, which opens the picker titled 'New terminal in'", async () => {
    s.terms = [];
    await show();
    const empty = byClass("empty")!;
    expect(textOf(kids(empty))).toBe("No terminal openStart a shell in a project's folder, a folder in ~/Herd or a card's worktree.New terminal");
    fire(findAll(kids(empty), (f) => f.type === "button")[0]!, "onClick");
    expect(picker()?.title).toBe("New terminal in");
    expect(picker()?.you).toBe("u1");
  });
  it("#/terminal/<id> of a terminal that was closed: 'This terminal isn't open any more.' with New terminal", async () => {
    await show("gone");
    const empty = byClass("empty")!;
    expect(textOf(kids(empty))).toBe("This terminal isn't open any more.New terminal");
    fire(findAll(kids(empty), (f) => f.type === "button")[0]!, "onClick");
    expect(picker()?.title).toBe("New terminal in");
  });
  it("says nothing before the list has come from the backend", async () => {
    s.loaded = false;
    await show("t9");
    expect(byClass("empty")).toBeUndefined();
  });
});
