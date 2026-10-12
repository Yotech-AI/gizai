// GA-138 QA: Settings → Data → the worktrees of finished cards. A worktree with uncommitted changes lists them under its
// row (each file's path and whether it is new, changed or deleted; at most 10, then "and N more"). Its Remove asks to
// Remove anyway, with Keep and Remove anyway; Remove anyway asks the backend to stash the changes first, and when that
// fails the row says why. A clean worktree's Remove, Remove all and a worktree an agent works in behave as before.
// There is no DOM here, so a small hook runtime stands in for React (as in EditorFoldersSetting.test.tsx), with a fake
// backend.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import type { OldWorktree, RemovedWorktree, UncommittedFile } from "../types";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
const h = vi.hoisted(() => ({
  cur: null as Inst | null,
  dirty: false,
  effects: [] as (() => void)[],
  list: [] as unknown[],
  removed: [] as string[][],
  anyway: [] as string[],
  /** What Remove anyway answers: by default it removes the worktree. */
  anywayAnswer: null as null | ((taskId: string) => unknown),
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
  const useEffect = ((f: () => void | (() => void), deps?: unknown[]) => {
    if (!h.cur) return R.useEffect(f, deps);
    const s = slot<EffectSlot>(() => ({ effect: true, first: true }));
    if (s.first || changed(deps, s.deps)) {
      s.first = false;
      s.deps = deps;
      h.effects.push(() => { s.cleanup?.(); const c = f(); s.cleanup = typeof c === "function" ? c : undefined; });
    }
  }) as typeof R.useEffect;
  const hooks = { useState, useEffect };
  return { ...R, default: { ...R, ...hooks }, ...hooks };
});

// The backend: Remove removes a worktree without uncommitted changes and keeps the others (never by force); Remove
// anyway stashes and removes, unless the test says otherwise.
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) if (typeof real[k] === "function") out[k] = () => new Promise(() => {});
  const list = () => h.list as OldWorktree[];
  return {
    ...out,
    listOldWorktrees: async () => structuredClone(list()),
    removeOldWorktrees: async (ids: string[]): Promise<RemovedWorktree[]> => {
      h.removed.push(ids);
      return ids.map((id) => {
        const w = list().find((x) => x.taskId === id)!;
        if (w.uncommitted > 0) return { taskId: id, identifier: w.identifier, removed: false, note: `kept its worktree ${w.path} (it has uncommitted changes)` };
        h.list = list().filter((x) => x.taskId !== id);
        return { taskId: id, identifier: w.identifier, removed: true, note: `removed its worktree and deleted branch ${w.branch}` };
      });
    },
    removeOldWorktreeAnyway: async (taskId: string): Promise<RemovedWorktree> => {
      h.anyway.push(taskId);
      if (h.anywayAnswer) return h.anywayAnswer(taskId) as RemovedWorktree;
      const w = list().find((x) => x.taskId === taskId)!;
      h.list = list().filter((x) => x.taskId !== taskId);
      return { taskId, identifier: w.identifier, removed: true,
               note: `put its ${w.uncommitted} uncommitted change in git's stash as "${w.identifier} leftover, removed in Settings", removed its worktree and deleted branch ${w.branch}` };
    },
  };
});

const { OldWorktrees } = await import("./OldWorktrees");

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
type Button = { onClick: () => unknown; disabled?: boolean; children: ReactNode };
function buttons(pred: (b: Found) => boolean) { return findAll(tree, (f) => f.type === "button" && pred(f)).map((f) => f.props as unknown as Button); }
/** A button of the question at the bottom (Keep, Remove, Remove anyway). */
function button(name: string) {
  const all = findAll(tree, (f) => f.props.role === "alertdialog")
    .flatMap((d) => findAll(d.props.children as ReactNode, (f) => f.type === "button" && textOf(f.props.children as ReactNode).trim() === name));
  if (all.length !== 1) throw new Error(`${all.length} buttons called ${name}`);
  return all[0]!.props as unknown as Button;
}
/** A row's own Remove, by the worktree's folder (its title). */
function removeOf(w: OldWorktree) {
  const all = buttons((f) => f.props.title === w.path);
  if (all.length !== 1) throw new Error(`${all.length} Remove buttons for ${w.identifier}`);
  return all[0]!;
}
async function press(b: Button) { await b.onClick(); await settle(); }
const dialog = () => findAll(tree, (f) => f.props.role === "alertdialog").map((f) => ({ label: f.props["aria-label"], text: textOf(findAll(f.props.children as ReactNode, (n) => n.type === "span")[0]?.props.children as ReactNode) }));
/** What shows under each row: its files ("deleted old.txt"), "and N more", and why it stayed. */
function under(identifier: string) {
  const row = findAll(tree, (f) => f.type === "li" && findAll(f.props.children as ReactNode, (n) => n.type === "b" && textOf(n.props.children as ReactNode) === identifier).length > 0)[0];
  if (!row) return null;
  const lists = findAll(row.props.children as ReactNode, (f) => f.type === "ul");
  const files = lists.flatMap((l) => findAll(l.props.children as ReactNode, (f) => f.type === "li").map((li) => {
    const kids = findAll(li.props.children as ReactNode, (n) => n.type === "span");
    return kids.length ? kids.map((k) => `${k.props.className}: ${textOf(k.props.children as ReactNode)}`).join(" | ") : `${li.props.className}: ${textOf(li.props.children as ReactNode)}`;
  }));
  const label = lists[0]?.props["aria-label"] ?? null;
  const warn = findAll(row.props.children as ReactNode, (f) => f.type === "span" && f.props.className === "warn").map((f) => textOf(f.props.children as ReactNode));
  return { label, files, warn, line: textOf(row.props.children as ReactNode) };
}
const message = () => findAll(tree, (f) => f.props.className === "hint").map((f) => textOf(f.props.children as ReactNode)).join("");

const file = (path: string, change: UncommittedFile["change"]): UncommittedFile => ({ path, change });
const worktree = (n: number, files: UncommittedFile[], more = 0, extra: Partial<OldWorktree> = {}): OldWorktree => ({
  taskId: `t-${n}`, identifier: `GA-${n}`, title: `Card ${n}`, category: "done", projectName: "Gizai", branch: `gizai/ga-${n}-card`,
  path: `/home/you/.local/share/gizai/worktrees/GA/GA-${n}`, bytes: 1024 * 1024, uncommitted: files.length + more, changes: files, live: false, ...extra,
});
const ONE = [file("src/lib.rs", "changed")];

async function show(list: OldWorktree[]) {
  h.list = structuredClone(list);
  root = () => <OldWorktrees />;
  render();
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0; h.removed.length = 0; h.anyway.length = 0; h.anywayAnswer = null;
});
afterEach(() => { root = () => null; render(); vi.useRealTimers(); });

describe("Settings → Data: a finished worktree's uncommitted files (GA-138)", () => {
  it("lists them under the row, each with new, changed or deleted, at most 10 and then and N more", async () => {
    const ten = [file("old.txt", "deleted"), file("readme.md", "changed"), file("drafts/", "new"),
      ...Array.from({ length: 7 }, (_, i) => file(`new-0${i + 1}.md`, "new"))];
    await show([worktree(122, ten, 4), worktree(120, [])]);
    const u = under("GA-122")!;
    expect(u.label).toBe("GA-122's uncommitted changes");
    expect(u.files).toEqual([
      "ow-change deleted: deleted | ow-path mono: old.txt", "ow-change changed: changed | ow-path mono: readme.md",
      "ow-change new: new | ow-path mono: drafts/",
      ...Array.from({ length: 7 }, (_, i) => `ow-change new: new | ow-path mono: new-0${i + 1}.md`),
      "faint: and 4 more",
    ]);
    expect(u.line).toContain("GA-122 Card 122 · Gizai · Done · 14 uncommitted changes");
    expect(under("GA-120")!.files).toEqual([]);
    expect(under("GA-120")!.line).not.toContain("uncommitted");
  });

  it("shows no more than 10 files even when it is given more, and no and N more when all are shown", async () => {
    const twelve = Array.from({ length: 12 }, (_, i) => file(`f${String(i + 1).padStart(2, "0")}.rs`, "changed"));
    await show([worktree(1, twelve), worktree(2, [file("a.md", "new"), file("b.md", "deleted")])]);
    expect(under("GA-1")!.files).toHaveLength(11);
    expect(under("GA-1")!.files.at(-2)).toBe("ow-change changed: changed | ow-path mono: f10.rs");
    expect(under("GA-1")!.files.at(-1)).toBe("faint: and 2 more");
    expect(under("GA-2")!.files).toEqual(["ow-change new: new | ow-path mono: a.md", "ow-change deleted: deleted | ow-path mono: b.md"]);
  });
});

describe("Settings → Data: Remove anyway (GA-138)", () => {
  it("asks first for a worktree with 1 uncommitted change, with Keep and Remove anyway, and Keep changes nothing", async () => {
    const w = worktree(122, ONE);
    await show([w, worktree(120, [])]);
    await press(removeOf(w));
    expect(dialog()).toEqual([{ label: "Remove a worktree with uncommitted changes",
      text: "GA-122 has 1 uncommitted change. Remove it anyway? Gizai puts the change in git's stash first, so it isn't lost." }]);
    expect(button("Keep")).toBeTruthy();
    expect(button("Remove anyway")).toBeTruthy();
    await press(button("Keep"));
    expect(dialog()).toEqual([]);
    expect(h.anyway).toEqual([]);
    expect(h.removed).toEqual([]);
    expect(under("GA-122")).not.toBeNull();
  });

  it("says changes, and they, for more than one", async () => {
    const w = worktree(121, [file("a.rs", "changed"), file("b.rs", "new"), file("c.rs", "deleted")]);
    await show([w]);
    await press(removeOf(w));
    expect(dialog()[0]!.text).toBe("GA-121 has 3 uncommitted changes. Remove it anyway? Gizai puts the changes in git's stash first, so they aren't lost.");
  });

  it("Remove anyway asks the backend to stash and remove, then the row is gone and the message says what happened", async () => {
    const w = worktree(122, ONE);
    await show([w, worktree(120, [])]);
    await press(removeOf(w));
    await press(button("Remove anyway"));
    expect(h.anyway).toEqual(["t-122"]);
    expect(h.removed, "not the plain Remove, which never stashes").toEqual([]);
    expect(dialog()).toEqual([]);
    expect(under("GA-122")).toBeNull();
    expect(under("GA-120")).not.toBeNull();
    expect(message()).toBe("GA-122: put its 1 uncommitted change in git's stash as \"GA-122 leftover, removed in Settings\", removed its worktree and deleted branch gizai/ga-122-card.");
  });

  it("when the stash fails the worktree stays and its row says why", async () => {
    const w = worktree(122, ONE);
    h.anywayAnswer = (taskId) => ({ taskId, identifier: "GA-122", removed: false,
      note: "kept its worktree: git couldn't put its uncommitted change in the stash, so nothing was removed (Cannot save the current status)" });
    await show([w]);
    await press(removeOf(w));
    await press(button("Remove anyway"));
    expect(h.anyway).toEqual(["t-122"]);
    expect(dialog()).toEqual([]);
    const u = under("GA-122")!;
    expect(u.warn).toEqual(["GA-122: kept its worktree: git couldn't put its uncommitted change in the stash, so nothing was removed (Cannot save the current status)."]);
    expect(u.files, "its change is still listed").toEqual(["ow-change changed: changed | ow-path mono: src/lib.rs"]);
    expect(message()).toBe("");

    // an error from the backend shows on the row too, and goes once a later Remove anyway works
    h.anywayAnswer = () => { throw new Error("GA-122 has no worktree of a Done or Cancelled card"); };
    await press(removeOf(w));
    await press(button("Remove anyway"));
    expect(under("GA-122")!.warn).toEqual(["GA-122: Error: GA-122 has no worktree of a Done or Cancelled card."]);
    h.anywayAnswer = null;
    await press(removeOf(w));
    await press(button("Remove anyway"));
    expect(under("GA-122")).toBeNull();
  });

  it("a worktree an agent is working in can't be removed, with or without changes", async () => {
    const live = worktree(130, ONE, 0, { live: true });
    await show([live, worktree(120, [])]);
    expect(removeOf(live).disabled).toBe(true);
    expect(under("GA-130")!.line).toContain("· 1 uncommitted change · an agent is working in it");
    expect(buttons((f) => textOf(f.props.children as ReactNode).startsWith("Remove all")), "one removable worktree: no Remove all").toEqual([]);
  });
});

describe("Settings → Data: Remove as before (GA-138)", () => {
  it("a worktree without uncommitted changes asks the old question and is removed by the plain Remove", async () => {
    const w = worktree(120, []);
    await show([w, worktree(122, ONE)]);
    await press(removeOf(w));
    expect(dialog()).toEqual([{ label: "Remove worktrees", text: "Remove GA-120's worktree (1.0 MB)? Its folder is deleted; committed work stays on its branch, and a branch with commits main doesn't have is kept. One with uncommitted changes stays." }]);
    await press(button("Remove"));
    expect(h.removed).toEqual([["t-120"]]);
    expect(h.anyway).toEqual([]);
    expect(under("GA-120")).toBeNull();
    expect(message()).toBe("Removed 1 worktree.");
  });

  it("Remove all still skips a worktree with uncommitted changes, never stashes, and says so", async () => {
    await show([worktree(120, []), worktree(122, ONE), worktree(123, []), worktree(130, [], 0, { live: true })]);
    const all = buttons((f) => textOf(f.props.children as ReactNode).startsWith("Remove all"));
    expect(all.map((b) => textOf(b.children))).toEqual(["Remove all 3 (3.0 MB)"]);
    await press(all[0]!);
    expect(dialog()[0]!.text).toBe("Remove 3 worktrees (3.0 MB)? Its folder is deleted; committed work stays on its branch, and a branch with commits main doesn't have is kept. One with uncommitted changes stays.");
    await press(button("Remove"));
    expect(h.removed).toEqual([["t-120", "t-122", "t-123"]]);
    expect(h.anyway, "Remove all never stashes").toEqual([]);
    expect(message()).toBe("Removed 2 worktrees. GA-122: kept its worktree /home/you/.local/share/gizai/worktrees/GA/GA-122 (it has uncommitted changes).");
    expect(under("GA-122")!.files).toEqual(["ow-change changed: changed | ow-path mono: src/lib.rs"]);
    expect(under("GA-130")).not.toBeNull();
  });

  it("a clean row's Remove while Remove anyway is asked asks the plain question instead, and the other way round", async () => {
    const [left, clean] = [worktree(122, ONE), worktree(120, [])];
    await show([left, clean]);
    await press(removeOf(left));
    expect(dialog().map((d) => d.label)).toEqual(["Remove a worktree with uncommitted changes"]);
    await press(removeOf(clean));
    expect(dialog().map((d) => d.text.split("?")[0])).toEqual(["Remove GA-120's worktree (1.0 MB)"]);
    await press(removeOf(left));
    expect(dialog().map((d) => d.text.split(".")[0])).toEqual(["GA-122 has 1 uncommitted change"]);
    expect(h.removed).toEqual([]);
    expect(h.anyway).toEqual([]);
  });
});
