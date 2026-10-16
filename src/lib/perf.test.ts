// GA-188: the benchmark harness's counter. Outside a perf run (no window.__GIZAI_PERF__, which only the backend's gizai-perf
// plugin sets) invoke and listen are Tauri's own functions, so the app runs as without the counter. In a perf run they
// count every call (its command, ms, answer bytes as JSON, errors) and every event heard, and still answer as Tauri's do.
import { afterEach, describe, expect, it, vi } from "vitest";

const tauriInvoke = vi.fn(async (cmd: string, _args?: unknown, _options?: unknown): Promise<unknown> => {
  if (cmd === "fails") throw new Error("no such command");
  return { cmd, cards: [1, 2, 3] };
});
const handlers: ((e: { event: string; id: number; payload: unknown }) => void)[] = [];
const unlisten = () => {};
const tauriListen = vi.fn(async (_event: string, handler: (e: { event: string; id: number; payload: unknown }) => void) => {
  handlers.push(handler);
  return unlisten;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: tauriInvoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauriListen }));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
  tauriInvoke.mockClear();
  tauriListen.mockClear();
  handlers.length = 0;
});

describe("outside a perf run", () => {
  it("hands back Tauri's own invoke and listen, and counts nothing", async () => {
    const perf = await import("./perf");
    expect(perf.perfOn).toBe(false);
    expect(perf.invoke).toBe(tauriInvoke);
    expect(perf.listen).toBe(tauriListen);
    await perf.invoke("list_tasks", { project: "KADE" });
    await perf.listen("rows-changed", () => {});
    expect(perf.perfCalls).toEqual([]);
    expect(perf.perfHeard).toEqual([]);
    expect(perf.perfFirst).toEqual({});
  });

  it("stays off with a window that has no flag, or a flag that isn't true", async () => {
    for (const win of [{}, { __GIZAI_PERF__: "true" }, { __GIZAI_PERF__: 1 }]) {
      vi.resetModules();
      vi.stubGlobal("window", win);
      const perf = await import("./perf");
      expect(perf.perfOn, JSON.stringify(win)).toBe(false);
      expect(perf.invoke).toBe(tauriInvoke);
      expect(perf.listen).toBe(tauriListen);
    }
  });

  it("is what api.ts calls: Tauri's invoke gets the command and its arguments as they are", async () => {
    const api = await import("../api");
    await api.listProjects();
    expect(tauriInvoke).toHaveBeenCalledTimes(1);
    expect(tauriInvoke.mock.calls[0][0]).toBe("list_projects");
  });
});

describe("in a perf run (window.__GIZAI_PERF__ set before the page's scripts)", () => {
  async function perfRun() {
    const observed: unknown[] = [];
    vi.stubGlobal("window", { __GIZAI_PERF__: true });
    vi.stubGlobal("document", { documentElement: {}, querySelector: () => null, querySelectorAll: () => [] });
    vi.stubGlobal("MutationObserver", class {
      observe(target: unknown) { observed.push(target); }
      disconnect() {}
    });
    return { perf: await import("./perf"), observed };
  }

  it("counts each call with its command, time and answer size, and passes the answer and the arguments on", async () => {
    const { perf, observed } = await perfRun();
    expect(perf.perfOn).toBe(true);
    expect(perf.invoke).not.toBe(tauriInvoke);
    expect(observed).toHaveLength(1); // watches for the first drawn sidebar, card and board from the start
    const out = await perf.invoke("list_tasks", { project: "KADE" });
    expect(out).toEqual({ cmd: "list_tasks", cards: [1, 2, 3] });
    expect(tauriInvoke).toHaveBeenCalledWith("list_tasks", { project: "KADE" }, undefined);
    await expect(perf.invoke("fails")).rejects.toThrow("no such command");
    expect(perf.perfCalls.map((c) => [c.cmd, c.bytes, c.error])).toEqual([
      ["list_tasks", JSON.stringify(out).length, false],
      ["fails", 0, true],
    ]);
    for (const c of perf.perfCalls) expect(c.ms).toBeGreaterThanOrEqual(0);
  });

  it("counts a listener as a call to listen, and each event heard with its payload's size", async () => {
    const { perf } = await perfRun();
    const got: unknown[] = [];
    const un = await perf.listen<{ id: string }>("rows-changed", (e) => { got.push(e.payload); });
    expect(un).toBe(unlisten);
    handlers[0]({ event: "rows-changed", id: 1, payload: { id: "KADE-1" } });
    handlers[0]({ event: "rows-changed", id: 1, payload: null });
    expect(got).toEqual([{ id: "KADE-1" }, null]);
    expect(perf.perfCalls.map((c) => c.cmd)).toEqual(["listen"]);
    expect(perf.perfHeard.map((h) => [h.event, h.bytes])).toEqual([["rows-changed", 15], ["rows-changed", 0]]);
  });

  it("adds them up per command and per event, most bytes first, within a window of time", async () => {
    const { perf } = await perfRun();
    await perf.invoke("list_tasks");
    await perf.invoke("list_tasks");
    await perf.invoke("app_info");
    await perf.listen("run-event", () => {});
    handlers[0]({ event: "run-event", id: 1, payload: "x" });
    const all = perf.perfCount();
    expect(all.calls).toBe(4);
    expect(all.heard).toBe(1);
    expect(all.commands.map((c) => [c.cmd, c.calls])).toEqual([["list_tasks", 2], ["app_info", 1], ["listen", 1]]);
    expect(all.bytes).toBe(all.commands.reduce((n, c) => n + c.bytes, 0));
    expect(all.events).toEqual({ "run-event": { heard: 1, bytes: 3 } });
    const none = perf.perfCount(performance.now() + 1000);
    expect([none.calls, none.heard, none.commands]).toEqual([0, 0, []]);
  });
});
