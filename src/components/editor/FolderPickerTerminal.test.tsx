// GA-136 QA: the Editor's folder picker as New terminal uses it (TF1; design/terminal §2 and §8): titled "New terminal in",
// the note from 20 open terminals on top of the list, the folder the page shows on top and chosen, a card an agent works
// on off (with the pulse and why), a card that isn't pinned to you asking under its row with Edit anyway, a card pinned to
// you starting at once, and a terminal that couldn't start saying why under its row with the picker still open. The
// Editor's own use (no onPick) is checked to be as it was. There is no DOM here, so a small hook runtime stands in for React
// (as in SidebarBehind.test.tsx), and the portal renders in place.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import type { EditorFolder } from "../../types";

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
  const hooks = { useState, useRef, useMemo, useCallback, useEffect, useLayoutEffect: useEffect };
  return { ...R, default: { ...R, ...hooks }, ...hooks };
});
vi.mock("react-dom", async (orig) => ({ ...(await orig<object>()), createPortal: (children: ReactNode) => children }));

const HERD = "/home/you/Herd";
const WT = "/home/you/.local/share/gizai/worktrees";
const folder = (id: string, more: Partial<EditorFolder>): EditorFolder =>
  ({ id, path: id.replace("/home/you", "~"), name: id.split("/").pop()!, kind: "root", projectId: null, taskId: null, readOnly: false, readOnlyWhy: null, ...more });
const FOLDERS: EditorFolder[] = [
  folder(`${HERD}/gizai`, { kind: "project", projectId: "p1" }),
  folder(`${HERD}/invoice-api`, {}),
  folder(`${HERD}/kade-portal`, {}),
  folder(`${WT}/KADE/KADE-41`, { kind: "card", taskId: "k41" }),
  folder(`${WT}/GA/GA-118`, { kind: "card", taskId: "g118", readOnly: true, readOnlyWhy: "QA Agent is working on GA-118" }),
  folder(`${WT}/GA/GA-112`, { kind: "card", taskId: "g112" }),
];
const TASKS = [
  { id: "k41", identifier: "KADE-41", title: "Fix the invoice total", byHand: true, assigneeId: "u1", assigneeName: "Jeffrey", stateCategory: "in_progress" },
  { id: "g118", identifier: "GA-118", title: "Show a run's changes", byHand: false, assigneeId: "qa", assigneeName: "QA Agent", stateCategory: "in_progress" },
  { id: "g112", identifier: "GA-112", title: "Remember the open tabs per folder", byHand: false, assigneeId: "fe", assigneeName: "Frontend Agent", stateCategory: "todo" },
];
const s = vi.hoisted(() => ({ went: [] as unknown[] }));
vi.mock("../../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) if (typeof real[k] === "function") out[k] = () => new Promise(() => {});
  return {
    ...out,
    editorFolders: () => Promise.resolve(FOLDERS),
    getSettings: () => Promise.resolve({ editorRoots: ["~/Herd"] }),
    listTasks: () => Promise.resolve(TASKS),
    listProjects: () => Promise.resolve([{ id: "p1", name: "Gizai", color: "var(--c-blue)" }]),
  };
});
vi.mock("../../router", async (orig) => ({ ...(await orig<object>()), go: (r: unknown) => { s.went.push(r); } }));

const store = new Map<string, string>();
vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, String(v)); }, removeItem: (k: string) => { store.delete(k); } });
vi.stubGlobal("window", Object.assign(new EventTarget(), { innerWidth: 1280, innerHeight: 720 }));
vi.stubGlobal("document", { body: {} });

const { FolderPicker } = await import("./FolderPicker");

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
const fire = (f: Found, handler: string, e: Record<string, unknown> = {}) => {
  (f.props[handler] as (e: unknown) => void)({ preventDefault: () => {}, stopPropagation: () => {}, ...e });
  render();
};
const groups = () => findAll(tree, (f) => f.props.role === "group").map((g) => [g.props["aria-label"], findAll(kids(g), (f) => f.props.role === "option").map((o) => textOf(kids(findAll(kids(o), (x) => x.props.className === "card-id" || x.props.className === "label")[0]!)))]);
const option = (name: string) => findAll(tree, (f) => f.props.role === "option").find((o) => findAll(kids(o), (x) => (x.props.className === "card-id" || x.props.className === "label") && textOf(kids(x)) === name).length > 0)!;
const confirmBox = () => one((f) => f.props.role === "alertdialog");
const failedNote = () => one((f) => f.props.role === "alert");

let picked: string[] = [];
let closed = 0;
async function show(props: Record<string, unknown> = {}) {
  root = () => <FolderPicker at={{ x: 240, y: 100 }} onClose={() => { closed += 1; }} {...props} />;
  render();
  await settle();
}
const terminal = (more: Record<string, unknown> = {}) => show({ title: "New terminal in", you: "u1", onPick: (f: EditorFolder) => { picked.push(f.name); }, ...more });

beforeEach(() => {
  vi.useFakeTimers();
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0;
  picked = []; closed = 0; s.went = []; store.clear();
});
afterEach(() => { root = () => null; render(); vi.useRealTimers(); });

describe("New terminal's folder picker (design §2)", () => {
  it("is titled 'New terminal in' above 'Find a folder', with the Editor's groups: Projects, In ~/Herd, Cards", async () => {
    await terminal();
    expect(one((f) => f.props.role === "dialog")!.props["aria-label"]).toBe("New terminal in");
    expect(textOf(kids(one((f) => f.props.className === "ed-picker-title")!))).toBe("New terminal in");
    expect(one((f) => f.type === "input")!.props.placeholder).toBe("Find a folder");
    expect(groups()).toEqual([["Projects", ["Gizai"]], ["In ~/Herd", ["invoice-api", "kade-portal"]], ["Cards", ["KADE-41", "GA-118", "GA-112"]]]);
  });
  it("shows the note on top of the list when it has one (from 20 open terminals), and none without", async () => {
    await terminal({ note: "20 terminals are open: each keeps its shell running until you close it." });
    const note = one((f) => f.props.className === "ed-picker-note")!;
    expect(textOf(kids(note))).toBe("20 terminals are open: each keeps its shell running until you close it.");
    expect(note.props.role).toBe("status");
    root = () => null; render();
    await terminal({ note: null });
    expect(one((f) => f.props.className === "ed-picker-note")).toBeUndefined();
  });
  it("puts the folder the page shows on top, chosen, so Enter starts another terminal there", async () => {
    await terminal({ current: `${HERD}/kade-portal` });
    expect(groups()[0]).toEqual(["In ~/Herd", ["kade-portal", "invoice-api"]]);
    expect(option("kade-portal").props["aria-selected"]).toBe(true);
    fire(one((f) => f.type === "input")!, "onKeyDown", { key: "Enter" });
    expect(picked).toEqual(["kade-portal"]);
    expect(closed).toBe(1);
  });
  it("a card an agent works on is off, with the pulse and who works; its tooltip says why; a click does nothing", async () => {
    await terminal();
    const off = option("GA-118");
    expect(off.props["aria-disabled"]).toBe(true);
    expect(off.props.title).toBe("QA Agent is working on GA-118: a terminal there waits until its run ends.");
    const stand = findAll(kids(off), (f) => f.props.className === "stand")[0]!;
    expect(textOf(kids(stand))).toBe("QA Agent is working");
    expect(findAll(kids(stand), (f) => f.props.className === "pulse")).toHaveLength(1);
    fire(off, "onClick");
    expect(picked).toEqual([]);
    expect(closed).toBe(0);
  });
  it("a card that isn't pinned to you asks under its row: 'GA-112 is Frontend Agent's card: …', and Edit anyway starts it", async () => {
    await terminal();
    fire(option("GA-112"), "onClick");
    expect(picked).toEqual([]);
    const ask = confirmBox()!;
    expect(textOf(kids(findAll(kids(ask), (f) => f.props.className === "grow")[0]!))).toBe("GA-112 is Frontend Agent's card: a shell there can change its files, as an edit can.");
    expect(findAll(kids(ask), (f) => f.type === "button").map((b) => textOf(kids(b)))).toEqual(["Edit anyway", "Cancel"]);
    // under its row
    const cards = findAll(tree, (f) => f.props.role === "group" && f.props["aria-label"] === "Cards")[0]!;
    const order = findAll(kids(cards), (f) => f.props.role === "option" || f.props.role === "alertdialog").map((f) => (f.props.role === "option" ? textOf(kids(findAll(kids(f), (x) => x.props.className === "card-id")[0]!)) : "ask"));
    expect(order).toEqual(["KADE-41", "GA-118", "GA-112", "ask"]);
    expect(option("GA-112").props.title).toBe("GA-112 is Frontend Agent's card");
    fire(findAll(kids(ask), (f) => f.type === "button" && textOf(kids(f)) === "Edit anyway")[0]!, "onClick");
    expect(picked).toEqual(["GA-112"]);
    expect(closed).toBe(1);
  });
  it("Cancel keeps the picker open, without starting anything", async () => {
    await terminal();
    fire(option("GA-112"), "onClick");
    fire(findAll(kids(confirmBox()!), (f) => f.type === "button" && textOf(kids(f)) === "Cancel")[0]!, "onClick");
    expect(confirmBox()).toBeUndefined();
    expect(picked).toEqual([]);
    expect(closed).toBe(0);
  });
  it("a card pinned to you (by hand) starts at once", async () => {
    await terminal();
    expect(textOf(kids(findAll(kids(option("KADE-41")), (f) => f.props.className === "stand")[0]!))).toBe("by hand");
    fire(option("KADE-41"), "onClick");
    expect(confirmBox()).toBeUndefined();
    expect(picked).toEqual(["KADE-41"]);
  });
  it("a terminal that couldn't start says why under its row, with the picker still open; one that started closes it", async () => {
    let answer: string | null = "Couldn't start zsh in ~/Herd/invoice-api: /usr/bin/zsh wasn't found.";
    await terminal({ onPick: (f: EditorFolder) => { picked.push(f.name); return Promise.resolve(answer); } });
    fire(option("invoice-api"), "onClick");
    await settle();
    expect(closed).toBe(0);
    expect(textOf(kids(failedNote()!))).toBe("Couldn't start zsh in ~/Herd/invoice-api: /usr/bin/zsh wasn't found.");
    const list = findAll(tree, (f) => f.props.role === "group" && f.props["aria-label"] === "In ~/Herd")[0]!;
    const order = findAll(kids(list), (f) => f.props.role === "option" || f.props.role === "alert").map((f) => (f.props.role === "option" ? textOf(kids(findAll(kids(f), (x) => x.props.className === "label")[0]!)) : "why"));
    expect(order).toEqual(["invoice-api", "why", "kade-portal"]);
    answer = null;
    fire(option("invoice-api"), "onClick");
    await settle();
    expect(failedNote()).toBeUndefined();
    expect(closed).toBe(1);
    expect(picked).toEqual(["invoice-api", "invoice-api"]);
  });
});

describe("the Editor's own picker (no onPick) is as it was", () => {
  it("opens any card's worktree at once, an agent's too (read-only there), without asking", async () => {
    await show();
    expect(one((f) => f.props.className === "ed-picker-title")).toBeUndefined();
    expect(option("GA-118").props["aria-disabled"]).toBeUndefined();
    fire(option("GA-112"), "onClick");
    expect(confirmBox()).toBeUndefined();
    expect(s.went).toEqual([{ page: "editor", id: `${WT}/GA/GA-112` }]);
    expect(closed).toBe(1);
  });
});
