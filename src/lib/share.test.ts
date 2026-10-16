// GA-190 QA: reads that data hooks share (lib/share.ts), and the table map they go by (lib/tables.ts). Two hooks' fetches
// that ask the same read with the same arguments while one is on its way make one backend call; other arguments, a call
// outside a hook's fetch, a read the map doesn't know and a read after a notice about its table each make their own. The
// map's table names are those the backend sends in `rows-changed` (src-tauri), and its commands are commands src/api.ts
// calls.
import { describe, expect, it } from "vitest";
import { forgetReads, runFetch, shared } from "./share";
import { COUNTS, READS, tablesOf } from "./tables";

type Deferred = { promise: Promise<unknown>; answer: (v: unknown) => void; fail: (e: unknown) => void };
/** A backend that holds each call until the test answers it, and counts them. */
function backend() {
  const calls: { cmd: string; args: unknown; d: Deferred }[] = [];
  const call = (cmd: string, args?: unknown) => () => {
    let answer!: (v: unknown) => void;
    let fail!: (e: unknown) => void;
    const promise = new Promise((a, f) => { answer = a; fail = f; });
    calls.push({ cmd, args, d: { promise, answer, fail } });
    return promise;
  };
  /** As src/api.ts's invoke: every call goes through shared(). */
  const invoke = (cmd: string, args?: Record<string, unknown>) => shared(cmd, args, call(cmd, args));
  return { calls, invoke };
}
const flush = async () => { for (let i = 0; i < 5; i++) await Promise.resolve(); };

describe("runFetch and shared", () => {
  it("two hooks' fetches of the same read with the same arguments: one backend call, both get its answer", async () => {
    const b = backend();
    const one = runFetch(() => b.invoke("list_tasks", { filter: {} }));
    const two = runFetch(() => b.invoke("list_tasks", { filter: {} }));
    expect(b.calls).toHaveLength(1);
    expect(one.commands).toEqual(["list_tasks"]);
    expect(two.commands).toEqual(["list_tasks"]);
    b.calls[0].d.answer(["card"]);
    expect(await one.answer).toEqual(["card"]);
    expect(await two.answer).toEqual(["card"]);
  });

  it("arguments are compared by value: equal objects share, other arguments don't", async () => {
    const b = backend();
    runFetch(() => b.invoke("list_tasks", { filter: { projectId: "p1" } }));
    runFetch(() => b.invoke("list_tasks", { filter: { projectId: "p1" } }));
    runFetch(() => b.invoke("list_tasks", { filter: { projectId: "p2" } }));
    runFetch(() => b.invoke("list_tasks", { filter: {} }));
    runFetch(() => b.invoke("get_team", { id: null }));
    runFetch(() => b.invoke("get_team", { id: "t1" }));
    expect(b.calls.map((c) => [c.cmd, JSON.stringify(c.args)])).toEqual([
      ["list_tasks", '{"filter":{"projectId":"p1"}}'],
      ["list_tasks", '{"filter":{"projectId":"p2"}}'],
      ["list_tasks", '{"filter":{}}'],
      ["get_team", '{"id":null}'],
      ["get_team", '{"id":"t1"}'],
    ]);
    b.calls.forEach((c) => c.d.answer(null));
    await flush();
  });

  it("once the answer is in, the next ask starts a new call (nothing is cached)", async () => {
    const b = backend();
    const one = runFetch(() => b.invoke("list_projects"));
    b.calls[0].d.answer([1]);
    await one.answer;
    await flush();
    runFetch(() => b.invoke("list_projects"));
    expect(b.calls).toHaveLength(2);
    b.calls[1].d.answer([]);
    await flush();
  });

  it("a failed read isn't shared after it failed, and both askers of it get the failure", async () => {
    const b = backend();
    const one = runFetch(() => b.invoke("list_users"));
    const two = runFetch(() => b.invoke("list_users"));
    b.calls[0].d.fail("locked");
    await expect(one.answer).rejects.toBe("locked");
    await expect(two.answer).rejects.toBe("locked");
    await flush();
    runFetch(() => b.invoke("list_users"));
    expect(b.calls).toHaveLength(2);
    b.calls[1].d.answer([]);
    await flush();
  });

  it("forgetReads(table): the next ask of a read of that table starts a new call; reads of other tables still share", async () => {
    const b = backend();
    runFetch(() => b.invoke("list_tasks", { filter: {} }));
    runFetch(() => b.invoke("list_docs"));
    forgetReads("tasks");
    const after = runFetch(() => b.invoke("list_tasks", { filter: {} }));
    runFetch(() => b.invoke("list_docs"));
    expect(b.calls.map((c) => c.cmd)).toEqual(["list_tasks", "list_docs", "list_tasks"]);
    // a hook after it joins the new call, not the old one
    const third = runFetch(() => b.invoke("list_tasks", { filter: {} }));
    expect(b.calls).toHaveLength(3);
    b.calls[2].d.answer(["new"]);
    b.calls[0].d.answer(["old"]);
    expect(await after.answer).toEqual(["new"]);
    expect(await third.answer).toEqual(["new"]);
    b.calls[1].d.answer([]);
    await flush();
  });

  it("forgetReads drops every read that shows the table (a card's write: the cards, the chats, the projects' counts)", async () => {
    const b = backend();
    runFetch(() => b.invoke("list_tasks", { filter: {} }));
    runFetch(() => b.invoke("list_chat_threads", { limit: null }));
    runFetch(() => b.invoke("list_projects"));
    runFetch(() => b.invoke("get_team", { id: null }));
    forgetReads("tasks");
    runFetch(() => b.invoke("list_tasks", { filter: {} }));
    runFetch(() => b.invoke("list_chat_threads", { limit: null }));
    runFetch(() => b.invoke("list_projects"));
    runFetch(() => b.invoke("get_team", { id: null }));
    expect(b.calls.map((c) => c.cmd)).toEqual(["list_tasks", "list_chat_threads", "list_projects", "get_team",
      "list_tasks", "list_chat_threads", "list_projects"]);
    b.calls.forEach((c) => c.d.answer(null));
    await flush();
  });

  it("a call outside a hook's fetch is never shared, not with a hook's read on its way nor with another call", async () => {
    const b = backend();
    runFetch(() => b.invoke("list_tasks", { filter: {} }));
    const outside = b.invoke("list_tasks", { filter: {} });
    b.invoke("list_tasks", { filter: {} });
    expect(b.calls).toHaveLength(3);
    // and a hook's read after it doesn't join the outside call
    runFetch(() => b.invoke("list_tasks", { filter: {} }));
    expect(b.calls).toHaveLength(3);
    b.calls[1].d.answer(["outside"]);
    b.calls[0].d.answer(["hook"]);
    expect(await outside).toEqual(["outside"]);
    b.calls[2].d.answer(null);
    await flush();
  });

  it("a command the map doesn't know (a write, app_info) is noted but never shared", async () => {
    const b = backend();
    const one = runFetch(() => b.invoke("app_info"));
    const two = runFetch(() => b.invoke("app_info"));
    runFetch(() => b.invoke("move_task", { id: "t1", stateId: "s", sortKey: "k" }));
    runFetch(() => b.invoke("move_task", { id: "t1", stateId: "s", sortKey: "k" }));
    expect(b.calls).toHaveLength(4);
    expect(one.commands).toEqual(["app_info"]);
    expect(two.commands).toEqual(["app_info"]);
    b.calls.forEach((c) => c.d.answer(null));
    await flush();
  });

  it("runFetch notes the commands called before the fetch returned, not those after an await", async () => {
    const b = backend();
    const r = runFetch(async () => {
      const [t] = await Promise.all([b.invoke("list_tasks", { filter: {} }), b.invoke("get_team", { id: null })]);
      await b.invoke("list_comments", { taskId: "x" });
      return t;
    });
    expect(r.commands).toEqual(["list_tasks", "get_team"]);
    b.calls[0].d.answer("t");
    b.calls[1].d.answer("team");
    await flush();
    expect(b.calls.map((c) => c.cmd)).toEqual(["list_tasks", "get_team", "list_comments"]);
    b.calls[2].d.answer(null);
    expect(await r.answer).toBe("t");
    expect(r.commands).toEqual(["list_tasks", "get_team"]);
  });

  it("a fetch that throws at once gives a rejected answer; the noting stops with it", async () => {
    const b = backend();
    const r = runFetch(() => { b.invoke("list_docs"); throw new Error("boom"); });
    await expect(r.answer).rejects.toThrow("boom");
    expect(r.commands).toEqual(["list_docs"]);
    // outside a fetch again: not shared
    b.invoke("list_docs");
    b.invoke("list_docs");
    expect(b.calls).toHaveLength(3);
    b.calls.forEach((c) => c.d.answer(null));
    await flush();
  });

  it("a fetch inside a fetch: each notes its own commands", async () => {
    const b = backend();
    let inner: { commands: string[] } | null = null;
    const outer = runFetch(() => {
      inner = runFetch(() => b.invoke("list_docs"));
      return b.invoke("list_tasks", { filter: {} });
    });
    expect(inner!.commands).toEqual(["list_docs"]);
    expect(outer.commands).toEqual(["list_tasks"]);
    b.calls.forEach((c) => c.d.answer(null));
    await flush();
  });
});

describe("tablesOf", () => {
  it("the tables of the reads a fetch called; null when it called none or one the map doesn't know", () => {
    expect(tablesOf(["list_docs"])).toEqual(new Set(["docs"]));
    expect(tablesOf(["list_docs", "list_contacts"])).toEqual(new Set(["docs", "contacts"]));
    expect(tablesOf([])).toBeNull();
    expect(tablesOf(["list_docs", "app_info"])).toBeNull();
  });

  it("counts: false leaves out the counts' tables (the sidebar's and the board's projects don't reload on a card's write)", () => {
    expect(tablesOf(["list_projects"], false)).toEqual(new Set(["projects", "clients"]));
    expect(tablesOf(["list_projects"], true)!.has("tasks")).toBe(true);
    expect(tablesOf(["list_users"], false)).toEqual(new Set(["actors"]));
    // a read without counts keeps all its tables
    expect(tablesOf(["list_tasks"], false)).toEqual(new Set(READS.list_tasks));
  });

  it("COUNTS names only reads in READS, and only tables those reads list", () => {
    for (const [cmd, tables] of Object.entries(COUNTS)) {
      expect(READS[cmd], cmd).toBeDefined();
      for (const t of tables) expect(READS[cmd], `${cmd}: ${t}`).toContain(t);
    }
  });
});

// node:fs comes in through a computed specifier: the build type-checks tests without Node's types (as startChunks.test.ts).
const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const read = (path: string): string => fs.readFileSync(new URL(path, import.meta.url), "utf8");
/** Every .rs file under a folder of src-tauri, read whole. */
function rustSources(dir: URL): string[] {
  const out: string[] = [];
  for (const e of fs.readdirSync(dir, { withFileTypes: true }) as { name: string; isDirectory: () => boolean }[]) {
    const at = new URL(e.isDirectory() ? `${e.name}/` : e.name, dir);
    if (e.isDirectory()) out.push(...rustSources(at));
    else if (e.name.endsWith(".rs")) out.push(fs.readFileSync(at, "utf8"));
  }
  return out;
}

describe("the map against the backend and the api", () => {
  // The backend's notices: commands.rs's changed(&app, "table"), the tools' cx.changed("table") and every
  // Note::RowsChanged("table") (runs, chat, the Team Lead, …), which lib.rs sends as rows-changed.
  const rust = rustSources(new URL("../../src-tauri/src/", import.meta.url)).join("\n");
  const sent = new Set<string>();
  for (const m of rust.matchAll(/\bchanged\(\s*&\w+\s*,\s*"([a-z_]+)"\s*\)/g)) sent.add(m[1]);
  for (const m of rust.matchAll(/\.changed\(\s*"([a-z_]+)"\s*\)/g)) sent.add(m[1]);
  for (const m of rust.matchAll(/RowsChanged\(\s*"([a-z_]+)"/g)) sent.add(m[1]);

  it("finds the backend's notices (a sanity check of the search itself)", () => {
    expect(rust).toContain('"rows-changed"');
    for (const t of ["tasks", "comments", "runs", "chat_threads", "chat_messages", "docs", "projects"]) expect(sent).toContain(t);
  });

  it("each table a read lists is one the backend sends in rows-changed", () => {
    const listed = new Set(Object.values(READS).flat());
    const unknown = [...listed].filter((t) => !sent.has(t));
    expect(unknown).toEqual([]);
  });

  it("each read in the map is a command src/api.ts calls", () => {
    const apiSource = read("../api.ts");
    const commands = new Set([...apiSource.matchAll(/invoke(?:<[^(]*?>)?\(\s*"([a-z_]+)"/g)].map((m) => m[1]));
    const missing = Object.keys(READS).filter((c) => !commands.has(c));
    expect(missing).toEqual([]);
  });
});
