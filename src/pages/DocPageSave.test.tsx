// GA-118: the doc page's Editor | Markdown switch and when it saves a version, and the same save rules on the Memory
// page's editor. The View switch (Editor first, kept in localStorage under gizai.doc.mode for every doc) gives the editor
// marks "hide" or "show", without a remount, a version or a change of status. Save and Ctrl+S make one version each,
// leaving the editor (the title, the History list, another window) makes none, 5 minutes without typing make one, and
// leaving the page saves what is unsaved. There is no DOM here, so a small hook runtime stands in for React: components
// are called as functions and keep their hooks between renders by their place in the tree, effects run after each
// render and clean up when a component goes. The editor is a stand-in that keeps the props it is given; the api is a
// fake that records the saves, and fake timers stand in for the clock.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import type { Doc, DocVersion, Project } from "../types";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
type EditorProps = { value: string; marks?: string; onChange?: (md: string) => void; onSave?: (md: string) => void; onBlur?: unknown; wiki?: unknown };
const h = vi.hoisted(() => ({
  cur: null as Inst | null,
  dirty: false,
  effects: [] as (() => void)[],
  doc: null as unknown,
  versions: [] as unknown[],
  saves: [] as { id: string; md: string; base: number }[],
  conflicts: 0, // the next saves that find an agent's newer version
  calls: [] as string[],
  editor: null as EditorProps | null,
  editorMounts: 0,
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
// The editor keeps the props it was given last, and counts its mounts.
vi.mock("../components/MarkdownEditor", async () => {
  const { useEffect } = await import("react");
  return { MarkdownEditor: (p: EditorProps) => { h.editor = p; useEffect(() => { h.editorMounts++; }, []); return null; } };
});
vi.mock("../components/memory/NoteView", () => ({ NoteView: () => null }));
// An old version opens in a drawer: it only notes that it opened.
vi.mock("../components/Drawer", () => ({ Drawer: (p: { title: string }) => { h.calls.push(`drawer ${p.title}`); return null; } }));
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => unknown) => {
    let data: unknown = null;
    try { const v = fetch(); if (!(v instanceof Promise)) data = v; } catch { /* no data */ }
    return { data, error: null, reload: () => {}, setData: () => {} };
  },
}));
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) if (typeof real[k] === "function") out[k] = () => new Promise(() => {});
  return {
    ...out,
    getDoc: () => h.doc,
    docVersions: () => h.versions,
    getProject: () => ({ id: "p1", name: "Kade" }) as Project,
    docVersionBody: (_id: string, v: number) => { h.calls.push(`docVersionBody ${v}`); return Promise.resolve(`body of ${v}`); },
    renameDoc: (_id: string, t: string) => { h.calls.push(`renameDoc ${t}`); return Promise.resolve(); },
    // A save makes the next version (and the doc reloads with it), or finds an agent's newer version.
    saveDoc: (id: string, md: string, base: number) => {
      h.saves.push({ id, md, base });
      if (h.conflicts > 0) { h.conflicts--; return Promise.reject(new Error(`the doc changed since version ${base}`)); }
      const d = h.doc as Doc;
      const v = d.currentVersion + 1;
      h.doc = { ...d, bodyMd: md, currentVersion: v };
      return Promise.resolve(v);
    },
  };
});
const store = new Map<string, string>();
const storage = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, v); }, removeItem: (k: string) => { store.delete(k); } };
vi.stubGlobal("localStorage", storage);
vi.stubGlobal("navigator", { userAgent: "Linux" });
vi.stubGlobal("requestAnimationFrame", (f: () => void) => setTimeout(f, 0));

const { DocPage } = await import("./DocPage");
const { NotePane } = await import("../components/memory/NotePane");

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
async function settle() { await vi.advanceTimersByTimeAsync(0); render(); }
async function wait(ms: number) { await vi.advanceTimersByTimeAsync(ms); render(); }

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
type Btn = { onClick: (e?: unknown) => unknown; onMouseDown?: (e: unknown) => void; disabled?: boolean; title?: string; className?: string; "aria-pressed"?: boolean };
const buttons = (name: string) => findAll(tree, (f) => f.type === "button" && textOf(f.props.children as ReactNode).trim() === name).map((f) => f.props as Btn);
const button = (name: string) => {
  const all = buttons(name);
  if (all.length !== 1) throw new Error(`${all.length} buttons called ${name}`);
  return all[0];
};
const ev = { preventDefault: () => {}, stopPropagation: () => {} };
async function click(name: string) { await button(name).onClick(ev); await settle(); }
const status = () => textOf(findAll(tree, (f) => f.props.role === "status")[0].props.children as ReactNode);
const banners = () => findAll(tree, (f) => f.props.className === "error-banner").map((f) => textOf(f.props.children as ReactNode));
const editor = () => { if (!h.editor) throw new Error("no editor"); return h.editor; };
async function typeText(md: string) { editor().onChange!(md); await settle(); }
const MIN = 60 * 1000;

const doc = (more: Partial<Doc> = {}): Doc => ({ id: "d1", title: "Notes", bodyMd: "# Notes\n", currentVersion: 2, updatedAt: 0, kind: "doc", projectId: "p1", ...more });
async function open(d: Doc = doc()) {
  h.doc = d;
  root = () => <DocPage key={(h.doc as Doc).id} id={(h.doc as Doc).id} />;
  render();
  await settle();
}
function close() { root = () => null; render(); }

beforeEach(() => {
  vi.useFakeTimers();
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  store.clear();
  h.effects.length = 0;
  h.saves.length = 0;
  h.calls.length = 0;
  h.conflicts = 0;
  h.editor = null;
  h.editorMounts = 0;
  h.versions = [
    { version: 2, authorName: "Jefsev", createdAt: 0 },
    { version: 1, authorName: "Backend Agent", createdAt: 0 },
  ] satisfies DocVersion[];
  vi.stubGlobal("navigator", { userAgent: "Linux" });
});
afterEach(() => { close(); vi.useRealTimers(); });

describe("Doc page: the Editor | Markdown switch", () => {
  it("is a View group in the top bar's actions, before the status text, with Editor pressed the first time", async () => {
    await open();
    const actions = findAll(tree, (f) => f.props.className === "actions")[0];
    const kids = [actions.props.children].flat(Infinity).filter((k) => k && typeof k === "object") as ReactElement<Record<string, unknown>>[];
    expect(kids.map((k) => [k.type, k.props.className, k.props.role ?? null])).toEqual([
      ["div", "seg", "group"], ["span", "faint", "status"], ["button", "btn primary", null],
    ]);
    expect(kids[0].props["aria-label"]).toBe("View");
    expect(findAll(kids[0], (f) => f.type === "button").map((f) => textOf(f.props.children as ReactNode))).toEqual(["Editor", "Markdown"]);
    expect(button("Editor").title).toBe("Write with the formatting shown, without Markdown's marks");
    expect(button("Markdown").title).toBe("Write the Markdown itself, every mark shown");
    expect(button("Editor")["aria-pressed"]).toBe(true);
    expect(button("Editor").className).toBe("on");
    expect(button("Markdown")["aria-pressed"]).toBe(false);
    expect(editor().marks).toBe("hide");
  });

  it("gives the editor every mark in Markdown, and keeps the choice in gizai.doc.mode, for every doc", async () => {
    await open();
    await click("Markdown");
    expect(store.get("gizai.doc.mode")).toBe("markdown");
    expect(button("Markdown")["aria-pressed"]).toBe(true);
    expect(button("Editor")["aria-pressed"]).toBe(false);
    expect(editor().marks).toBe("show");
    // Reopening the doc, and opening another one: still Markdown.
    close();
    await open();
    expect(button("Markdown")["aria-pressed"]).toBe(true);
    expect(editor().marks).toBe("show");
    close();
    await open(doc({ id: "d2", title: "Other" }));
    expect(button("Markdown")["aria-pressed"]).toBe(true);
    expect(editor().marks).toBe("show");
    await click("Editor");
    expect(store.get("gizai.doc.mode")).toBe("editor");
    expect(editor().marks).toBe("hide");
  });

  it("opens in Editor when localStorage holds something else or can't be read, and still switches", async () => {
    store.set("gizai.doc.mode", "weird");
    await open();
    expect(editor().marks).toBe("hide");
    close();
    vi.stubGlobal("localStorage", { getItem: () => { throw new Error("no storage"); }, setItem: () => { throw new Error("no storage"); } });
    try {
      await open();
      expect(editor().marks).toBe("hide");
      await click("Markdown");
      expect(editor().marks).toBe("show");
    } finally { vi.stubGlobal("localStorage", storage); }
  });

  it("keeps the text, the editor and the unsaved status when switching, and makes no version", async () => {
    await open();
    expect(h.editorMounts).toBe(1);
    await typeText("# Notes\nNew line");
    expect(status()).toBe("Unsaved changes · Ctrl+S saves");
    // A mouse press on the switch keeps the focus (and the cursor) in the editor.
    const pressed = vi.fn();
    button("Markdown").onMouseDown!({ preventDefault: pressed });
    button("Editor").onMouseDown!({ preventDefault: pressed });
    expect(pressed).toHaveBeenCalledTimes(2);
    await click("Markdown");
    await click("Editor");
    await click("Markdown");
    expect(editor().value).toBe("# Notes\nNew line");
    expect(status()).toBe("Unsaved changes · Ctrl+S saves");
    expect(h.saves).toEqual([]);
    expect(h.editorMounts).toBe(1); // the same editor, so its cursor and undo stay
  });
});

describe("Doc page: a version when you save", () => {
  it("passes the editor no blur, so leaving it makes no version", async () => {
    await open();
    expect(editor().onBlur).toBeUndefined();
  });

  it("makes no version on a click on the title or the History list, and stays Unsaved changes", async () => {
    await open();
    await typeText("# Notes\nTyped");
    const title = findAll(tree, (f) => f.props["aria-label"] === "Doc title")[0].props as { onBlur: () => void; onChange: (e: unknown) => void };
    title.onBlur();
    await settle();
    const history = findAll(tree, (f) => f.type === "button" && textOf(f.props.children as ReactNode).startsWith("v1Backend Agent"));
    expect(history).toHaveLength(1);
    await (history[0].props as Btn).onClick(ev);
    await settle();
    expect(h.calls).toContain("docVersionBody 1");
    expect(h.calls).toContain("drawer Notes, version 1");
    await wait(4 * MIN);
    expect(h.saves).toEqual([]);
    expect(status()).toBe("Unsaved changes · Ctrl+S saves");
  });

  it("disables Save with nothing to save, and says what it does in its tooltip", async () => {
    await open();
    expect(button("Save").disabled).toBe(true);
    expect(button("Save").title).toBe("Save as a new version (Ctrl+S)");
    await typeText("# Notes\nTyped");
    expect(button("Save").disabled).toBe(false);
  });

  it("says Cmd in the tooltip and the status on macOS", async () => {
    vi.stubGlobal("navigator", { userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)" });
    await open();
    expect(button("Save").title).toBe("Save as a new version (Cmd+S)");
    await typeText("# Notes\nTyped");
    expect(status()).toBe("Unsaved changes · Cmd+S saves");
  });

  it("makes exactly one version on Save, then shows Saved · version N+1 and disables Save", async () => {
    await open();
    await typeText("# Notes\nSaved by the button");
    await click("Save");
    expect(h.saves).toEqual([{ id: "d1", md: "# Notes\nSaved by the button", base: 2 }]);
    expect(status()).toBe("Saved · version 3");
    expect(button("Save").disabled).toBe(true);
    await wait(10 * MIN);
    expect(h.saves).toHaveLength(1);
  });

  it("makes exactly one version on Ctrl+S, then shows Saved · version N+1", async () => {
    await open();
    await typeText("# Notes\nSaved by Ctrl+S");
    editor().onSave!("# Notes\nSaved by Ctrl+S");
    await settle();
    expect(h.saves).toEqual([{ id: "d1", md: "# Notes\nSaved by Ctrl+S", base: 2 }]);
    expect(status()).toBe("Saved · version 3");
    await wait(10 * MIN);
    expect(h.saves).toHaveLength(1);
  });

  it("saves one version after 5 minutes without typing, each change restarting the wait", async () => {
    await open();
    await typeText("# Notes\nA");
    await wait(3 * MIN);
    await typeText("# Notes\nAB");
    await wait(5 * MIN - 1);
    expect(h.saves).toEqual([]);
    await wait(1);
    expect(h.saves).toEqual([{ id: "d1", md: "# Notes\nAB", base: 2 }]);
    expect(status()).toBe("Saved · version 3");
  });

  it("saves unsaved changes as one version on leaving the page", async () => {
    await open();
    await typeText("# Notes\nLeaving");
    close();
    await settle();
    expect(h.saves).toEqual([{ id: "d1", md: "# Notes\nLeaving", base: 2 }]);
    await wait(10 * MIN);
    expect(h.saves).toHaveLength(1);
  });

  it("shows the conflict bar on Save after an agent's save, and the automatic save doesn't save over it", async () => {
    await open();
    await typeText("# Notes\nMine");
    h.conflicts = 1;
    await click("Save");
    expect(banners().some((b) => b.includes("saved by someone else"))).toBe(true);
    expect(status()).toBe("Not saved");
    await typeText("# Notes\nMine, more");
    await wait(20 * MIN);
    expect(h.saves).toHaveLength(1);
    expect(buttons("Save mine as a new version")).toHaveLength(1);
    expect(buttons("Discard mine, load theirs")).toHaveLength(1);
  });
});

describe("Memory page note: the same save rules", () => {
  const note = () => doc({ id: "n1", title: "Notes", kind: "memory", path: "Team Lead/Notes", projectId: undefined });
  async function openNote() {
    h.doc = note();
    const links = { notes: [], cards: new Set<string>(), open: () => {}, create: () => {}, hover: () => {} };
    root = () => <NotePane key="n1" id="n1" notes={[]} scopeNotes={[]} links={links} tagFilter={null} onTag={() => {}} editing panel={false} />;
    render();
    await settle();
  }

  it("passes its editor no blur and no marks (dimmed, as before)", async () => {
    await openNote();
    expect(editor().wiki).toBeTruthy();
    expect(editor().onBlur).toBeUndefined();
    expect(editor().marks).toBeUndefined();
  });

  it("makes no version without the 5 minutes passing, then one", async () => {
    await openNote();
    await typeText("# Notes\nIn memory");
    await wait(5 * MIN - 1);
    expect(h.saves).toEqual([]);
    expect(status()).toContain("Unsaved changes · Ctrl+S saves");
    await wait(1);
    expect(h.saves).toEqual([{ id: "n1", md: "# Notes\nIn memory", base: 2 }]);
    expect(status()).toContain("Saved · version 3");
  });

  it("makes one version on Save and one on Ctrl+S", async () => {
    await openNote();
    await typeText("# Notes\nOne");
    await click("Save");
    await typeText("# Notes\nOne two");
    editor().onSave!("# Notes\nOne two");
    await settle();
    expect(h.saves.map((s) => [s.md, s.base])).toEqual([["# Notes\nOne", 2], ["# Notes\nOne two", 3]]);
    await wait(10 * MIN);
    expect(h.saves).toHaveLength(2);
  });

  it("still saves on switching to Read", async () => {
    await openNote();
    await typeText("# Notes\nRead me");
    await click("Read");
    expect(h.saves).toEqual([{ id: "n1", md: "# Notes\nRead me", base: 2 }]);
  });
});
