// GA-113 QA: Settings → General → Editor folders. The folders your projects are in, a row each, saved as soon as one is
// added, changed (after a pause in typing) or removed, and shown as the backend kept them (~/Code for /home/you/Code/);
// under a saved folder how many folders the Editor opens in it; a folder the backend refuses says why under its row and
// isn't saved; a folder that isn't there gets a warning and is saved all the same. There is no DOM here, so a small hook
// runtime stands in for React (as in DocPageSave.test.tsx), with a fake backend that keeps the settings and fake timers.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import type { AgentFolder, EditorFolder, FolderCheck, Settings } from "../types";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
const h = vi.hoisted(() => ({
  cur: null as Inst | null,
  dirty: false,
  effects: [] as (() => void)[],
  saved: null as unknown,
  saves: [] as string[][],
  picked: null as string | null,
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
  const useEffect = ((f: () => void | (() => void), deps?: unknown[]) => {
    if (!h.cur) return R.useEffect(f, deps);
    const s = slot<EffectSlot>(() => ({ effect: true, first: true }));
    if (s.first || changed(deps, s.deps)) {
      s.first = false;
      s.deps = deps;
      h.effects.push(() => { s.cleanup?.(); const c = f(); s.cleanup = typeof c === "function" ? c : undefined; });
    }
  }) as typeof R.useEffect;
  const hooks = { useState, useRef, useEffect };
  return { ...R, default: { ...R, ...hooks }, ...hooks };
});
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => h.picked }));

// The backend: it keeps the settings, writes a folder the way Settings shows it (~ for the home folder, no slash at the
// end) and refuses the home folder, as gizai-core's editor::clean_roots does.
const HOME = "/home/you";
const shown = (p: string) => { const t = p.trim().replace(/\/+$/, ""); return t.startsWith(`${HOME}/`) ? `~${t.slice(HOME.length)}` : t; };
const FOLDERS: EditorFolder[] = ["kade-portal", "invoice-api", "gizai-site", "notes", "shop"].map((name) => ({
  id: `${HOME}/Herd/${name}`, path: `~/Herd/${name}`, name, kind: "root", projectId: null, taskId: null, readOnly: false, readOnlyWhy: null,
}));
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) if (typeof real[k] === "function") out[k] = () => new Promise(() => {});
  return {
    ...out,
    getSettings: async () => structuredClone(h.saved),
    saveSettings: async (s: Settings) => {
      h.saves.push(s.editorRoots);
      for (const r of s.editorRoots) if (shown(r) === "~") throw new Error(`Editor folders: ${r} can't be used: that's your home folder: pick the folder your projects are in`);
      h.saved = { ...s, editorRoots: s.editorRoots.map(shown).filter((r) => r !== "") };
    },
    editorFolders: async () => ((h.saved as Settings).editorRoots.includes("~/Herd") ? FOLDERS : []),
    checkAgentFolders: async (fs: AgentFolder[]): Promise<FolderCheck[]> => fs.map((f) => ({ path: f.path, warning: f.path.includes("Gone") ? "It isn't there" : null })),
  };
});

const { EditorFoldersSetting } = await import("./EditorFoldersSetting");

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
type Input = { value: string; onChange: (e: unknown) => void; onFocus: () => void; onBlur: () => void };
const inputs = () => findAll(tree, (f) => f.type === "input").map((f) => f.props as unknown as Input);
const values = () => inputs().map((i) => i.value);
/** What shows under each row (the count, a warning or why it is refused), in order. */
const under = () => findAll(tree, (f) => f.props.className === "folder-row").map((f) => {
  const notes = findAll(f.props.children as ReactNode, (n) => n.type === "span" && ["hint", "warn", "error"].includes(String(n.props.className)));
  return notes.map((n) => `${String(n.props.className)}: ${textOf(n.props.children as ReactNode)}`).join("");
});
function button(name: string) {
  const all = findAll(tree, (f) => f.type === "button" && (textOf(f.props.children as ReactNode).trim() === name || f.props["aria-label"] === name));
  if (all.length !== 1) throw new Error(`${all.length} buttons called ${name}`);
  return all[0]!.props as { onClick: () => unknown };
}
async function click(name: string) { await button(name).onClick(); await settle(); }
async function type(i: number, value: string) { inputs()[i]!.onFocus(); render(); inputs()[i]!.onChange({ target: { value } }); render(); }

const SETTINGS: Settings = {
  claudeBin: "/usr/bin/claude", dataDir: "/home/you/.local/share/gizai", maxConcurrentRuns: 4, agentsPaused: false, maxRunUsd: null,
  maxRunMinutes: 45, maxRunToolCalls: 400, maxChatMinutes: 15, maxChatToolCalls: 60, pushOver: "ssh",
  notifications: { hold: true, waiting: true, leadAsks: true, leadAnswered: true }, editorRoots: ["~/Herd"],
};
/** Settings' own state, which the section updates with what the backend kept. */
let page: Settings | null = null;
const setS = (x: Settings | null | ((p: Settings | null) => Settings | null)) => { page = typeof x === "function" ? x(page) : x; };
async function show(roots = ["~/Herd"]) {
  h.saved = { ...SETTINGS, editorRoots: roots };
  page = { ...SETTINGS, editorRoots: roots };
  root = () => <EditorFoldersSetting s={page!} setS={setS as never} />;
  render();
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0; h.saves.length = 0; h.picked = null;
});
afterEach(() => { root = () => null; render(); vi.useRealTimers(); });

describe("Settings → General → Editor folders (GA-113)", () => {
  it("is a section called Editor folders with a row per saved folder, ~/Herd by default", async () => {
    await show();
    expect(textOf(tree)).toContain("Editor folders");
    expect(values()).toEqual(["~/Herd"]);
    expect(inputs().map((i) => (i as unknown as { "aria-label": string })["aria-label"])).toEqual(["Folder 1"]);
  });

  it("says under a saved folder how many folders the Editor opens in it, with the first names", async () => {
    await show();
    expect(under()).toEqual(["hint: 5 folders: kade-portal, invoice-api, gizai-site and 2 more."]);
  });

  it("saves a typed folder once you pause, and shows it as the backend kept it", async () => {
    await show();
    await click("Add folder");
    expect(values()).toEqual(["~/Herd", ""]);
    await type(1, "/home/you/Code/");
    await wait(500);
    expect(h.saves).toEqual([]);
    await wait(200);
    expect(h.saves).toEqual([["~/Herd", "/home/you/Code/"]]);
    expect(page!.editorRoots).toEqual(["~/Herd", "~/Code"]);
    // the row you are typing in stays as typed; leaving it shows it as saved
    expect(values()).toEqual(["~/Herd", "/home/you/Code/"]);
    inputs()[1]!.onBlur();
    render();
    expect(values()).toEqual(["~/Herd", "~/Code"]);
  });

  it("saves at once when a folder is removed", async () => {
    await show(["~/Herd", "~/Code"]);
    await click("Remove folder 2");
    expect(h.saves).toEqual([["~/Herd"]]);
    expect(values()).toEqual(["~/Herd"]);
    expect(page!.editorRoots).toEqual(["~/Herd"]);
  });

  it("saves at once a folder picked with Browse…", async () => {
    await show();
    h.picked = "/home/you/Sites";
    await click("Browse…");
    expect(h.saves).toEqual([["~/Herd", "/home/you/Sites"]]);
    expect(values()).toEqual(["~/Herd", "~/Sites"]);
  });

  it("says under its row why a folder is refused, and keeps the last good list saved", async () => {
    await show();
    await click("Add folder");
    await type(1, "~");
    await wait(700);
    expect(h.saves).toEqual([["~/Herd", "~"]]);
    expect(under()[1]).toBe("error: That's your home folder: pick the folder your projects are in.");
    expect(page!.editorRoots).toEqual(["~/Herd"]);
    expect((h.saved as Settings).editorRoots).toEqual(["~/Herd"]);
  });

  it("warns under a folder that isn't there, and saves it all the same", async () => {
    await show();
    await click("Add folder");
    await type(1, "~/Gone");
    await wait(700);
    expect(under()[1]).toBe("warn: It isn't there now: the picker lists its folders once it is.");
    expect(page!.editorRoots).toEqual(["~/Herd", "~/Gone"]);
  });

  it("with no folders says the Editor opens projects' linked folders and card worktrees only", async () => {
    await show([]);
    expect(values()).toEqual([]);
    expect(textOf(tree)).toContain("No folders: the Editor opens projects' linked folders and card worktrees only.");
  });
});
