// GA-191: run and chat events come in batches. `run-events` is { runId, events: { seq, event }[] } and `chat-events` is
// { threadId, events: ChatEvent[] }; onRunEvent and onChatEvent keep their signatures and call back once per event of a
// batch, in order, so no page changes. runEvents asks run_events for the events after a number (all without one). A page
// opened mid-run (the Run panel listens, then loads the history) gets every event once.
import { describe, expect, it, vi } from "vitest";
import type { ChatEvent, RunEvent, SeqEvent } from "./types";
import { mergeEvents } from "./lib/runs";

const invoked: [string, unknown][] = [];
const listening: [string, (m: { payload: unknown }) => void][] = [];
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => { invoked.push([cmd, args]); return Promise.resolve([]); },
  Channel: class<T> { constructor(public onmessage: (m: T) => void) {} },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, cb: (m: { payload: unknown }) => void) => { listening.push([event, cb]); return Promise.resolve(() => {}); },
}));

const api = await import("./api");

const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const read = (path: string): string => fs.readFileSync(new URL(path, import.meta.url), "utf8");

/** Sends `payload` as the window's `event`, as Tauri would. */
function emit(event: string, payload: unknown) {
  const found = listening.filter(([e]) => e === event);
  expect(found.length, `someone listens to ${event}`).toBeGreaterThan(0);
  for (const [, cb] of found) cb({ payload });
}

const text = (t: string): RunEvent => ({ kind: "text", text: t });

describe("run-events", () => {
  it("onRunEvent calls back once per event of each batch, in seq order, and listens to run-events only", async () => {
    listening.length = 0;
    const got: { runId: string; seq: number; event: RunEvent }[] = [];
    await api.onRunEvent((e) => got.push(e));
    expect(listening.map(([e]) => e)).toEqual(["run-events"]);
    emit("run-events", { runId: "r1", events: [{ seq: 0, event: text("a") }, { seq: 1, event: text("b") }, { seq: 2, event: text("c") }] });
    emit("run-events", { runId: "r2", events: [{ seq: 0, event: text("other") }] });
    emit("run-events", { runId: "r1", events: [{ seq: 3, event: text("d") }] });
    expect(got).toEqual([
      { runId: "r1", seq: 0, event: text("a") }, { runId: "r1", seq: 1, event: text("b") }, { runId: "r1", seq: 2, event: text("c") },
      { runId: "r2", seq: 0, event: text("other") }, { runId: "r1", seq: 3, event: text("d") },
    ]);
  });

  it("runEvents asks run_events for the events after a number, or all of them", async () => {
    invoked.length = 0;
    await api.runEvents("r1");
    await api.runEvents("r1", 41);
    expect(invoked).toEqual([["run_events", { runId: "r1", afterSeq: null }], ["run_events", { runId: "r1", afterSeq: 41 }]]);
  });

  it("a page opened mid-run gets every event once, in order: its history merged with the batches it heard", async () => {
    // the panel listens first, then loads the history; batches before, during and after the load overlap it
    const all: SeqEvent[] = Array.from({ length: 30 }, (_, i) => ({ seq: i, event: text(`line ${i}`) }));
    const history = all.slice(0, 18);
    const heard = [...all.slice(12, 20), ...all.slice(20, 30)];
    let events: SeqEvent[] = [];
    for (const e of heard.slice(0, 8)) events = mergeEvents(events, [e]);
    events = mergeEvents(history, events);
    for (const e of heard.slice(8)) events = mergeEvents(events, [e]);
    expect(events).toEqual(all);
  });
});

describe("chat-events", () => {
  it("onChatEvent calls back once per event of each batch, in order, and listens to chat-events only", async () => {
    listening.length = 0;
    const got: ChatEvent[] = [];
    await api.onChatEvent((e) => got.push(e));
    expect(listening.map(([e]) => e)).toEqual(["chat-events"]);
    const batch1: ChatEvent[] = [
      { kind: "block", threadId: "t1", seq: 1 }, { kind: "delta", threadId: "t1", text: "Hello, ", seq: 3 }, { kind: "tool", threadId: "t1", name: "list_tasks" },
    ];
    const batch2: ChatEvent[] = [{ kind: "delta", threadId: "t1", text: "world", seq: 4 }];
    emit("chat-events", { threadId: "t1", events: batch1 });
    emit("chat-events", { threadId: "t1", events: batch2 });
    expect(got).toEqual([...batch1, ...batch2]);
  });
});

describe("the Rust side", () => {
  it("emits run-events and chat-events, batched every 50 ms, and nothing listens to the old events", () => {
    const lib = read("../src-tauri/src/lib.rs");
    expect(lib).toContain(`app.emit("run-events", serde_json::json!({ "runId": run_id, "events": events }))`);
    expect(lib).toContain(`app.emit("chat-events", serde_json::json!({ "threadId": thread_id, "events": events }))`);
    expect(lib).not.toMatch(/"run-event"|"chat-event"/);
    expect(read("../src-tauri/src/stream.rs")).toContain("pub const EVERY: Duration = Duration::from_millis(50);");
    expect(read("./api.ts")).not.toMatch(/"run-event"|"chat-event"/);
  });
});
