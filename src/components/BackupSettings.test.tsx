// GA-217 QA: Settings → General → Backups, on GA-216's contract BK. The section's text and fields (Folder, When, Keep, the
// last and next backup with Back up now, the list); Browse… and Turn off save at once and show the new status; Keep 0 and
// 101 show the backend's reason under Keep and aren't saved; Back up now is off without a folder and while a backup runs,
// shows its step, and the last backup and the list follow `backups-changed` without a reload; a folder problem and the last
// error show with their text; each row's time, reason, size, computer and why it can't be restored. There is no DOM here,
// so a small hook runtime stands in for React (as in EditorFoldersSetting.test.tsx), with a fake backend that keeps the
// status, refuses as gizai-core's backups::save_settings does and sends `backups-changed` as src-tauri's backups.rs does.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import type { BackupEntry, BackupSettings as Settings, BackupStatus } from "../types";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
const h = vi.hoisted(() => ({
  cur: null as Inst | null,
  dirty: false,
  effects: [] as (() => void)[],
  status: null as unknown,
  list: [] as unknown[],
  saves: [] as unknown[],
  calls: { status: 0, list: [] as (string | null)[], now: 0 },
  listeners: new Set<(s: unknown) => void>(),
  picked: null as string | null,
  pickerOpts: [] as unknown[],
  nowRefusal: null as string | null,
  statusRefusal: null as string | null,
  listRefusal: null as string | null,
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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async (o: unknown) => { h.pickerOpts.push(o); return h.picked; } }));

// The backend. Tauri rejects a command with the error's text, so the refusals are strings.
const st = () => h.status as BackupStatus;
const send = () => { const x = structuredClone(st()); for (const cb of [...h.listeners]) cb(x); };
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(real)) if (typeof real[k] === "function") out[k] = () => new Promise(() => {});
  return {
    ...out,
    backupStatus: async () => { h.calls.status++; if (h.statusRefusal) throw h.statusRefusal; return structuredClone(st()); },
    saveBackupSettings: async (s: Settings) => {
      h.saves.push(structuredClone(s));
      if (!(Number.isInteger(s.keep) && s.keep >= 1 && s.keep <= 100)) throw `Keep must be 1 to 100 snapshots, not ${s.keep}`;
      const folder = s.folder?.trim() || null;
      if (folder && folder !== st().settings.folder && folder.startsWith("/readonly")) throw `Gizai can't write in ${folder}: permission denied`;
      const moved = folder !== st().settings.folder;
      Object.assign(st(), { settings: { ...s, folder }, folder: folder && `${folder}/Gizai backups`, folderProblem: null });
      if (moved) Object.assign(st(), { last: null, lastError: null, next: folder && s.every !== "manual" ? NOW : null });
      if (s.every === "manual") st().next = null;
      send();
      return structuredClone(st());
    },
    listBackups: async (folder: string | null) => { h.calls.list.push(folder); if (h.listRefusal) throw h.listRefusal; return st().settings.folder ? structuredClone(h.list) : []; },
    backUpNow: async () => {
      h.calls.now++;
      if (h.nowRefusal) throw h.nowRefusal;
      if (!st().folder) throw "Choose a folder for backups first";
      if (st().running) throw "A backup is running already";
      st().running = { reason: "manual", startedAt: Date.now(), step: "database" };
      send();
      return structuredClone(st());
    },
    onBackupsChanged: (cb: (s: unknown) => void) => { h.listeners.add(cb); return Promise.resolve(() => { h.listeners.delete(cb); }); },
  };
});

const { BackupSettings } = await import("./BackupSettings");

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
const kids = (f: Found) => f.props.children as ReactNode;
const isField = (f: Found) => f.type === "div" && /^field( |$)/.test(String(f.props.className));
/** The fields' labels, in order. */
const fields = () => findAll(tree, isField).map((f) => textOf((findAll(kids(f), (n) => n.type === "label")[0]?.props.children ?? "") as ReactNode));
function field(label: string): Found {
  const all = findAll(tree, (f) => isField(f) && textOf((findAll(kids(f), (n) => n.type === "label")[0]?.props.children ?? "") as ReactNode) === label);
  if (all.length !== 1) throw new Error(`${all.length} fields called ${label}`);
  return all[0]!;
}
/** What shows under a field (Field's hint, warning or error, its last child), as "kind: text", or "". */
function under(label: string): string {
  const c = kids(field(label));
  const last = Array.isArray(c) ? c[c.length - 1] : null;
  if (!last || typeof last !== "object" || !("props" in last)) return "";
  const el = last as ReactElement<{ className?: string; children?: ReactNode }>;
  return el.type === "span" && ["hint", "warn", "error"].includes(String(el.props.className)) ? `${el.props.className}: ${textOf(el.props.children)}` : "";
}
/** A field's text without its label. */
const fieldText = (label: string) => { const c = kids(field(label)); return textOf((Array.isArray(c) ? c.slice(1) : c) as ReactNode); };
function button(name: string) {
  const all = findAll(tree, (f) => f.type === "button" && textOf(kids(f)).trim() === name);
  if (all.length !== 1) throw new Error(`${all.length} buttons called ${name}`);
  return all[0]!.props as { onClick: () => unknown; disabled?: boolean };
}
async function click(name: string) { await button(name).onClick(); await settle(); }
type Box = { checked: boolean; onChange: (e: unknown) => void; type: string };
const radios = () => findAll(tree, (f) => f.type === "label" && findAll(kids(f), (n) => n.type === "input" && n.props.type === "radio").length === 1)
  .map((l) => ({ label: textOf(kids(l)), input: findAll(kids(l), (n) => n.type === "input")[0]!.props as unknown as Box }));
const onQuit = () => findAll(tree, (f) => f.type === "input" && f.props.type === "checkbox")[0]!.props as unknown as Box;
type KeepInput = { value: string; min: number; max: number; onChange: (e: unknown) => void; onBlur: () => void; onKeyDown: (e: unknown) => void };
const keep = () => findAll(tree, (f) => f.type === "input" && f.props.id === "bk-keep")[0]!.props as unknown as KeepInput;
async function typeKeep(v: string) { keep().onChange({ target: { value: v } }); render(); }
const step = () => findAll(tree, (f) => f.props.role === "status").map((f) => textOf(kids(f)));
/** The list's rows, each as its time | what | size, and why it can't be restored when it says so. */
const rows = () => findAll(tree, (f) => f.type === "ul" && f.props.className === "backup-list").flatMap((ul) => findAll(kids(ul), (f) => f.type === "li"))
  .map((li) => {
    const span = (c: string) => findAll(kids(li), (f) => String(f.props.className).split(" ").includes(c)).map((f) => textOf(kids(f))).join("");
    const why = findAll(kids(li), (f) => f.props.className === "warn").map((f) => textOf(kids(f)));
    return [span("bk-when"), span("bk-what"), span("bk-size"), ...why].join(" | ");
  });
const all = () => textOf(tree);

// local times, so "today" and "yesterday" don't depend on the time zone the tests run in
const NOW = new Date(2026, 9, 27, 14, 0).getTime();
const at = (day: number, hh: number, mm = 0) => new Date(2026, 9, day, hh, mm).getTime();
const HOUR = 3_600_000;
const entry = (over: Partial<BackupEntry>): BackupEntry => ({
  name: "gizai-daily-20261027-110000-000-ce82a749", reason: "daily", madeAt: at(27, 11), deviceId: "me", deviceName: "this machine",
  thisComputer: true, appVersion: "0.14.0", schemaVersion: 40, dbBytes: 4_456_448, filesCount: 14, filesBytes: 3_276_800, filesMissing: 0,
  restorable: true, problem: null, ...over,
});
const LIST: BackupEntry[] = [
  entry({}),
  entry({ name: "gizai-manual-20261026-090000-000-ce82a749", reason: "manual", madeAt: at(26, 9), dbBytes: 4_390_912 }),
  entry({ name: "gizai-quit-20261025-180000-000-ce82a749", reason: "quit", madeAt: at(25, 18), dbBytes: 1_000_000, filesBytes: 48_576 }),
  entry({ name: "gizai-daily-20261025-080000-000-0199f3a2", madeAt: at(25, 8), deviceId: "laptop", deviceName: "work-laptop", thisComputer: false,
    appVersion: "0.15.0", schemaVersion: 41, dbBytes: 4_128_768, restorable: false, problem: "A newer Gizai made it (0.15.0): update Gizai to restore it" }),
  entry({ name: "gizai-weekly-20261021-080000-000-77aa0011", reason: "weekly", madeAt: at(21, 8), deviceId: "nas", deviceName: "this machine", thisComputer: false }),
  entry({ name: "gizai-before-restore-20261020-120000-000-ce82a749", reason: "before-restore", madeAt: at(20, 12) }),
];
const STATUS: BackupStatus = {
  settings: { folder: "/home/you/Dropbox", every: "daily", onQuit: false, keep: 10 },
  folder: "/home/you/Dropbox/Gizai backups", folderProblem: null, running: null,
  last: { name: LIST[0]!.name, madeAt: at(27, 11), bytes: 1_153_434 }, lastError: null, next: at(28, 11),
};
const OFF: BackupStatus = { ...STATUS, settings: { ...STATUS.settings, folder: null }, folder: null, last: null, next: null };

async function show(status: BackupStatus = STATUS, list: BackupEntry[] = LIST) {
  h.status = structuredClone(status);
  h.list = structuredClone(list);
  root = () => <BackupSettings />;
  render();
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
  for (const [k, inst] of insts) { unmount(inst); insts.delete(k); }
  h.effects.length = 0; h.saves.length = 0; h.pickerOpts.length = 0; h.listeners.clear();
  h.calls = { status: 0, list: [], now: 0 };
  h.picked = null; h.nowRefusal = null; h.statusRefusal = null; h.listRefusal = null;
});
afterEach(() => { root = () => null; render(); vi.useRealTimers(); });

describe("the section (GA-217)", () => {
  it("is Backups, with the card's text word for word", async () => {
    await show();
    const section = findAll(tree, (f) => f.type === "section")[0]!;
    expect(textOf(kids(findAll(kids(section), (f) => f.type === "h3")[0] as never))).toBe("Backups");
    expect(textOf(kids(findAll(kids(section), (f) => f.type === "p")[0] as never))).toBe(
      "Copies of your data in a folder you choose: a USB disk, or a folder that Dropbox or Syncthing keeps. A backup holds the database and your files, never keys or passwords: those stay in your system's keychain.");
  });

  it("has Folder, When, Keep, the last and next backup with Back up now, and the list, in that order", async () => {
    await show();
    expect(fields()).toEqual(["Folder", "When", "Keep", "Last and next", "Backups"]);
    expect(button("Back up now").disabled).toBe(false);
  });

  it("reads the status once and the list of the folder in Settings (list_backups(null))", async () => {
    await show();
    expect(h.calls.status).toBe(1);
    expect(h.calls.list).toEqual([null]);
  });

  it("shows nothing until the status is read, and says so when it can't be", async () => {
    h.status = structuredClone(STATUS);
    root = () => <BackupSettings />;
    render();
    expect(all()).toBe("");
    root = () => null;
    render();
    h.statusRefusal = "database is locked";
    await show();
    expect(fields()).toEqual(["Backups"]);
    expect(under("Backups")).toBe("error: database is locked");
    expect(all()).toContain("The backups can't be shown now.");
  });
});

describe("Folder", () => {
  it("shows the backups folder in use (…/Gizai backups), with Browse… and Turn off", async () => {
    await show();
    expect(fieldText("Folder")).toContain("/home/you/Dropbox/Gizai backups");
    expect(button("Browse…").disabled).toBeFalsy();
    expect(button("Turn off").disabled).toBe(false);
    expect(under("Folder")).toBe("");
  });

  it("without one: Not set: Gizai makes no backups., Turn off off, and the list asks for a folder", async () => {
    await show(OFF, []);
    expect(fieldText("Folder")).toContain("Not set: Gizai makes no backups.");
    expect(button("Turn off").disabled).toBe(true);
    expect(fieldText("Backups")).toBe("Choose a folder to see its backups.");
  });

  it("Browse… opens the system's folder picker and saves the folder at once; the section shows the new status", async () => {
    await show(OFF, LIST);
    h.picked = "/media/you/USB";
    await click("Browse…");
    expect(h.pickerOpts).toEqual([expect.objectContaining({ directory: true, multiple: false })]);
    expect(h.saves).toEqual([{ folder: "/media/you/USB", every: "daily", onQuit: false, keep: 10 }]);
    expect(fieldText("Folder")).toContain("/media/you/USB/Gizai backups");
    expect(button("Turn off").disabled).toBe(false);
    expect(button("Back up now").disabled).toBe(false);
    // the list is read again for the new folder, without a reload
    expect(h.calls.status).toBe(1);
    expect(h.calls.list.length).toBeGreaterThanOrEqual(2);
    expect(rows()).toHaveLength(LIST.length);
  });

  it("Browse… closed without a folder (Cancel) saves nothing", async () => {
    await show();
    h.picked = null;
    await click("Browse…");
    expect(h.pickerOpts).toHaveLength(1);
    expect(h.saves).toEqual([]);
    expect(fieldText("Folder")).toContain("/home/you/Dropbox/Gizai backups");
  });

  it("a folder the backend refuses shows why under Folder, and nothing is saved", async () => {
    await show();
    h.picked = "/readonly/disk";
    await click("Browse…");
    expect(under("Folder")).toBe("error: Gizai can't write in /readonly/disk: permission denied");
    expect(st().settings.folder).toBe("/home/you/Dropbox");
    expect(fieldText("Folder")).toContain("/home/you/Dropbox/Gizai backups");
    // a good folder after it clears the refusal
    h.picked = "/media/you/USB";
    await click("Browse…");
    expect(under("Folder")).toBe("");
    expect(fieldText("Folder")).toContain("/media/you/USB/Gizai backups");
  });

  it("Turn off clears the folder at once (folder null): Not set, Back up now off, no next, no list", async () => {
    await show();
    await click("Turn off");
    expect(h.saves).toEqual([{ folder: null, every: "daily", onQuit: false, keep: 10 }]);
    expect(fieldText("Folder")).toContain("Not set: Gizai makes no backups.");
    expect(button("Turn off").disabled).toBe(true);
    expect(button("Back up now").disabled).toBe(true);
    expect(fieldText("Last and next")).not.toContain("Next:");
    expect(rows()).toEqual([]);
    expect(fieldText("Backups")).toBe("Choose a folder to see its backups.");
  });

  it("a folder problem shows as a warning under Folder, with its text", async () => {
    await show({ ...STATUS, folderProblem: "The folder isn't there: is the disk plugged in?", last: null, next: null });
    expect(under("Folder")).toBe("warn: The folder isn't there: is the disk plugged in?");
    expect(fieldText("Last and next")).toContain("Next: once the folder can be used again");
  });

  it("a folder problem that comes in backups-changed shows without a reload", async () => {
    await show();
    Object.assign(st(), { folderProblem: "Gizai can't write in /home/you/Dropbox/Gizai backups" });
    send();
    await settle();
    expect(under("Folder")).toBe("warn: Gizai can't write in /home/you/Dropbox/Gizai backups");
    expect(h.calls.status).toBe(1);
  });
});

describe("When", () => {
  it("Daily, Weekly and Only when I ask, the saved one picked, and Also when I quit Gizai completely with its hint", async () => {
    await show();
    expect(radios().map((r) => [r.label, r.input.checked])).toEqual([["Daily", true], ["Weekly", false], ["Only when I ask", false]]);
    expect(fieldText("When")).toContain("Also when I quit Gizai completely");
    expect(onQuit().checked).toBe(false);
    expect(under("When")).toBe("hint: Quitting then waits for the backup, usually a few seconds.");
  });

  it("a click on Weekly saves at once, the rest as saved", async () => {
    await show();
    radios()[1]!.input.onChange({});
    await settle();
    expect(h.saves).toEqual([{ folder: "/home/you/Dropbox", every: "weekly", onQuit: false, keep: 10 }]);
    expect(radios().map((r) => r.input.checked)).toEqual([false, true, false]);
  });

  it("Only when I ask: Next: when you ask", async () => {
    await show();
    radios()[2]!.input.onChange({});
    await settle();
    expect(h.saves).toEqual([{ folder: "/home/you/Dropbox", every: "manual", onQuit: false, keep: 10 }]);
    expect(fieldText("Last and next")).toContain("Next: when you ask");
  });

  it("the checkbox saves onQuit at once", async () => {
    await show();
    onQuit().onChange({ target: { checked: true } });
    await settle();
    expect(h.saves).toEqual([{ folder: "/home/you/Dropbox", every: "daily", onQuit: true, keep: 10 }]);
    expect(onQuit().checked).toBe(true);
  });

  it("two changes in a row both land: the second builds on the first's answer", async () => {
    await show();
    radios()[1]!.input.onChange({});
    onQuit().onChange({ target: { checked: true } });
    await settle();
    expect(h.saves).toEqual([
      { folder: "/home/you/Dropbox", every: "weekly", onQuit: false, keep: 10 },
      { folder: "/home/you/Dropbox", every: "weekly", onQuit: true, keep: 10 },
    ]);
    expect(st().settings).toEqual({ folder: "/home/you/Dropbox", every: "weekly", onQuit: true, keep: 10 });
  });
});

describe("Keep", () => {
  it("a number from 1 to 100, the saved one, with the card's hint", async () => {
    await show();
    expect(keep().value).toBe("10");
    expect([keep().min, keep().max]).toEqual([1, 100]);
    expect(under("Keep")).toBe("hint: Your newest backups from this computer. Older ones are removed. A backup made before a restore is never removed.");
  });

  it("typing saves once you pause, the rest as saved", async () => {
    await show();
    await typeKeep("20");
    await wait(300);
    expect(h.saves).toEqual([]);
    await wait(400);
    expect(h.saves).toEqual([{ folder: "/home/you/Dropbox", every: "daily", onQuit: false, keep: 20 }]);
    expect(keep().value).toBe("20");
    expect(under("Keep")).toMatch(/^hint: /);
  });

  it("Keep 0 shows the backend's reason under Keep and isn't saved", async () => {
    await show();
    await typeKeep("0");
    await wait(700);
    expect(h.saves).toEqual([{ folder: "/home/you/Dropbox", every: "daily", onQuit: false, keep: 0 }]);
    expect(under("Keep")).toBe("error: Keep must be 1 to 100 snapshots, not 0");
    expect(st().settings.keep).toBe(10);
    // the other fields show no error, and the field keeps what was typed
    expect(under("Folder")).toBe("");
    expect(under("When")).toMatch(/^hint: /);
    expect(keep().value).toBe("0");
  });

  it("Keep 101 on Enter: the backend's reason under Keep, not saved", async () => {
    await show();
    await typeKeep("101");
    keep().onKeyDown({ key: "Enter" });
    await settle();
    expect(h.saves).toEqual([{ folder: "/home/you/Dropbox", every: "daily", onQuit: false, keep: 101 }]);
    expect(under("Keep")).toBe("error: Keep must be 1 to 100 snapshots, not 101");
    expect(st().settings.keep).toBe(10);
    // the pause after typing doesn't send it a second time
    await wait(700);
    expect(h.saves).toHaveLength(1);
  });

  it("a refused Keep isn't sent with the next change of another field: that one saves Keep as saved", async () => {
    await show();
    await typeKeep("0");
    await wait(700);
    radios()[1]!.input.onChange({});
    await settle();
    expect(h.saves.at(-1)).toEqual({ folder: "/home/you/Dropbox", every: "weekly", onQuit: false, keep: 10 });
    expect(st().settings).toEqual({ folder: "/home/you/Dropbox", every: "weekly", onQuit: false, keep: 10 });
  });

  it("a good number after a refusal saves and clears the reason", async () => {
    await show();
    await typeKeep("0");
    await wait(700);
    await typeKeep("5");
    await wait(700);
    expect(h.saves.at(-1)).toEqual({ folder: "/home/you/Dropbox", every: "daily", onQuit: false, keep: 5 });
    expect(st().settings.keep).toBe(5);
    expect(under("Keep")).toMatch(/^hint: /);
  });

  it("an empty field saves nothing, and leaving it shows the saved value again", async () => {
    await show();
    await typeKeep("");
    await wait(700);
    keep().onBlur();
    await settle();
    expect(h.saves).toEqual([]);
    expect(keep().value).toBe("10");
  });

  it("a number that isn't whole says so under Keep and isn't sent", async () => {
    await show();
    await typeKeep("2.5");
    await wait(700);
    expect(h.saves).toEqual([]);
    expect(under("Keep")).toBe("error: Keep must be a whole number, 1 to 100");
  });
});

describe("Back up now", () => {
  it("is off without a folder", async () => {
    await show(OFF, []);
    expect(button("Back up now").disabled).toBe(true);
  });

  it("is off while a scheduled backup runs, and its step shows next to it", async () => {
    await show({ ...STATUS, running: { reason: "daily", startedAt: NOW - 2000, step: "compress" } });
    expect(button("Back up now").disabled).toBe(true);
    expect(step()).toEqual(["Compressing the database…"]);
  });

  it("starts one; while it runs it is off and its step follows backups-changed; after it the last backup and the list update without a reload", async () => {
    await show();
    expect(step()).toEqual([]);
    await click("Back up now");
    expect(h.calls.now).toBe(1);
    expect(button("Back up now").disabled).toBe(true);
    expect(step()).toEqual(["Copying the database…"]);
    for (const [s, words] of [["files", "Copying your files…"], ["compress", "Compressing the database…"], ["manifest", "Writing its manifest…"],
      ["keep", "Removing older backups…"]] as const) {
      st().running = { ...st().running!, step: s };
      send();
      await settle();
      expect(step()).toEqual([words]);
      expect(button("Back up now").disabled).toBe(true);
    }
    // it ends: the backend adds it to the folder and says so
    const made = entry({ name: "gizai-manual-20261027-140005-000-ce82a749", reason: "manual", madeAt: NOW + 5000, dbBytes: 4_500_000 });
    h.list = [made, ...LIST];
    Object.assign(st(), { running: null, last: { name: made.name, madeAt: made.madeAt, bytes: 2_097_152 }, next: made.madeAt + 24 * HOUR });
    vi.setSystemTime(NOW + 6000);
    send();
    await settle();
    expect(step()).toEqual([]);
    expect(button("Back up now").disabled).toBe(false);
    expect(fieldText("Last and next")).toContain("Last backup: today 14:00 (2.0 MB)");
    expect(rows()[0]).toBe("today 14:00 | By hand | 7.4 MB");
    expect(rows()).toHaveLength(LIST.length + 1);
    // no reload: the status was read once, and the list again because a backup ended
    expect(h.calls.status).toBe(1);
    expect(h.calls.list.every((f) => f === null)).toBe(true);
  });

  it("a refusal shows why next to it, and the button is on again", async () => {
    await show();
    h.nowRefusal = "Gizai can't write in /home/you/Dropbox/Gizai backups";
    await click("Back up now");
    expect(under("Last and next")).toBe("error: Gizai can't write in /home/you/Dropbox/Gizai backups");
    expect(button("Back up now").disabled).toBe(false);
  });

  it("a backup that fails shows the last error in red with its time and text, from backups-changed", async () => {
    await show();
    await click("Back up now");
    Object.assign(st(), { running: null, lastError: { at: NOW + 3000, message: "No space left on device (os error 28)" } });
    send();
    await settle();
    const err = findAll(tree, (f) => f.props.className === "bk-error");
    expect(err.map((f) => textOf(kids(f)))).toEqual(["The last backup failed (today 14:00): No space left on device (os error 28)"]);
    const line = findAll(tree, (f) => f.props.role === "alert");
    expect(line.map((f) => f.props.className)).toEqual(["gh-line failed"]);
    expect(button("Back up now").disabled).toBe(false);
  });
});

describe("the last and next backup", () => {
  it("Last backup: <time> (<size>) and Next: <time>", async () => {
    await show();
    expect(fieldText("Last and next")).toContain("Last backup: today 11:00 (1.1 MB)");
    expect(fieldText("Last and next")).toContain("Next: tomorrow 11:00");
  });

  it("No backup yet, and Next: when you ask for Only when I ask", async () => {
    await show({ ...STATUS, settings: { ...STATUS.settings, every: "manual" }, last: null, next: null }, []);
    expect(fieldText("Last and next")).toContain("No backup yet");
    expect(fieldText("Last and next")).toContain("Next: when you ask");
    expect(fieldText("Backups")).toBe("No backups in this folder yet.");
  });

  it("an earlier failure shows in red with its time", async () => {
    await show({ ...STATUS, lastError: { at: at(26, 9), message: "The folder isn't there: is the disk plugged in?" } });
    expect(findAll(tree, (f) => f.props.className === "bk-error").map((f) => textOf(kids(f))))
      .toEqual(["The last backup failed (yesterday 09:00): The folder isn't there: is the disk plugged in?"]);
  });

  it("no error, no red line", async () => {
    await show();
    expect(findAll(tree, (f) => f.props.className === "bk-error")).toEqual([]);
  });
});

describe("the list", () => {
  it("newest first: each row's time, reason in words and size (database and files); another computer's from its name", async () => {
    await show();
    const r = rows();
    expect(r[0]).toBe("today 11:00 | Daily | 7.4 MB");
    expect(r[1]).toBe("yesterday 09:00 | By hand | 7.3 MB");
    expect(r[2]).toMatch(/^\w{3} 25 Oct, 18:00 \| On quit \| 1\.0 MB$/);
    expect(r[3]).toMatch(/^\w{3} 25 Oct, 08:00 \| Daily · from work-laptop \| 7\.1 MB \| A newer Gizai made it \(0\.15\.0\): update Gizai to restore it$/);
    expect(r[4]).toBe("21 Oct, 08:00 | Weekly · from another computer | 7.4 MB");
    expect(r[5]).toBe("20 Oct, 12:00 | Before a restore | 7.4 MB");
    expect(fieldText("Backups")).not.toContain("Restore");
  });

  it("this computer's rows don't name a computer", async () => {
    await show();
    expect(rows().filter((r) => r.includes("from "))).toHaveLength(2);
  });

  it("a row's full time is in its tooltip", async () => {
    await show();
    const when = findAll(tree, (f) => f.props.className === "bk-when")[0]!;
    expect(String(when.props.title)).toMatch(/27 October 2026 at 11:00$/);
  });

  it("a list that can't be read says why while the folder is fine, and waits for the folder while it has a problem", async () => {
    h.listRefusal = "/home/you/Dropbox/Gizai backups/gizai-backups.json: not JSON";
    await show();
    expect(fieldText("Backups")).toBe("/home/you/Dropbox/Gizai backups/gizai-backups.json: not JSON");
    root = () => null;
    render();
    h.listRefusal = "No such file or directory (os error 2)";
    await show({ ...STATUS, folderProblem: "The folder isn't there: is the disk plugged in?", last: null, next: null });
    expect(fieldText("Backups")).toBe("They show once the folder can be used again.");
  });
});

describe("backups-changed", () => {
  it("settings changed elsewhere show without a reload", async () => {
    await show();
    Object.assign(st(), { settings: { ...st().settings, every: "weekly", keep: 25, onQuit: true } });
    send();
    await settle();
    expect(radios().map((r) => r.input.checked)).toEqual([false, true, false]);
    expect(keep().value).toBe("25");
    expect(onQuit().checked).toBe(true);
    expect(h.calls.status).toBe(1);
  });

  it("a scheduled backup that ends adds its row and moves the last backup", async () => {
    await show();
    Object.assign(st(), { running: { reason: "daily", startedAt: NOW, step: "database" } });
    send();
    await settle();
    expect(step()).toEqual(["Copying the database…"]);
    const made = entry({ name: "gizai-daily-20261027-140001-000-ce82a749", madeAt: NOW + 1000 });
    h.list = [made, ...LIST];
    Object.assign(st(), { running: null, last: { name: made.name, madeAt: made.madeAt, bytes: 1_200_000 } });
    send();
    await settle();
    expect(rows()).toHaveLength(LIST.length + 1);
    expect(fieldText("Last and next")).toContain("Last backup: today 14:00 (1.1 MB)");
  });

  it("stops listening when the section goes", async () => {
    await show();
    expect(h.listeners.size).toBe(1);
    root = () => null;
    render();
    expect(h.listeners.size).toBe(0);
  });
});
