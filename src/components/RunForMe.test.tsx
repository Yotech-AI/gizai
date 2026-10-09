// GA-31 on the card's Run panel: the note box above Continue (sent with Continue, also with Enter), "Run this for me"
// on a held card (each command exactly, a Copy button each, Done, continue) and the trigger's name (Continue or Nudge).
// Rendered to HTML on the server; the clicks call the components as functions with plain stand-ins for React's hooks,
// and the Tauri calls they make are recorded.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { DependencyList, EffectCallback } from "react";
import type { Member, Run, Task, Team } from "../types";

const { invoke, data, fake } = vi.hoisted(() => ({
  invoke: vi.fn(async (_cmd: string, _args?: unknown) => "run-2"),
  data: { runs: [] as unknown[] },
  // Calling a component as a function: useState gives its initial value (an empty string from `strings` when there is
  // one), effects don't run, refs are plain objects.
  fake: { on: false, strings: [] as string[] },
}));
vi.mock("@tauri-apps/api/core", async (orig) => ({ ...(await orig<typeof import("@tauri-apps/api/core")>()), invoke }));
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => {
    if (!fake.on) return R.useState(init);
    const v = typeof init === "function" ? (init as () => unknown)() : init;
    return [v === "" && fake.strings.length ? fake.strings.shift() : v, () => {}];
  }) as typeof R.useState;
  const useEffect = ((effect: EffectCallback, deps?: DependencyList) => (fake.on ? undefined : R.useEffect(effect, deps))) as typeof R.useEffect;
  const useRef = ((init: unknown) => (fake.on ? { current: init } : R.useRef(init))) as typeof R.useRef;
  const hooks = { useState, useEffect, useRef };
  return { ...R, ...hooks, default: { ...R, ...hooks } };
});
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));
vi.mock("../lib/useData", () => ({ useData: () => ({ data: data.runs, error: null, reload: () => {}, setData: () => {} }) }));

const { RunPanel } = await import("./RunPanel");
const { RunForMe } = await import("./RunForMe");
const { continueAfterRunForMe, continueRun } = await import("../api");

const CMDS = ["sudo pacman -S libayatana-appindicator",
  'echo "fs.inotify.max_user_watches=524288" | sudo tee /etc/sysctl.d/40-watches.conf && sudo sysctl --system'];

const agent: Member = { actorId: "be2", name: "Backend Agent 2", kind: "agent", roleKey: "backend", handle: "be2", status: "active", isLead: false,
  allowedTools: [], chatEnabled: false };
const team = { id: "team", name: "Software", members: [agent], states: [], labels: [] } as unknown as Team;
const run = (over: Partial<Run>): Run => ({ id: "run-000000001", agentId: "be2", agentName: "Backend Agent 2", taskId: "t1", trigger: "manual",
  status: "succeeded", createdAt: 0, endedAt: 1, costUsdMicros: 310_000, inputTokens: 0, outputTokens: 0, logPath: "/x.jsonl", sessionId: "S1",
  worktreePath: "/wt", ...over });
const task = (over: Partial<Task>) => ({ id: "t1", identifier: "GA-31", title: "Tray icon", stateId: "ip", stateCategory: "in_progress",
  assigneeId: "be2", hold: null, ...over }) as unknown as Task;
const asking = run({ outcome: "needs_decision", summaryMd: "The tray needs libayatana-appindicator, which needs sudo.", runForMe: CMDS });
const heldAsking = task({ hold: "needs_decision", holdReason: "The tray needs libayatana-appindicator, which needs sudo.", runForMe: CMDS });
const stopped = run({ outcome: "no_result" });

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, "<")
  .replace(/&gt;/g, ">").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
const panel = (t: Task, runs: Run[]) => { data.runs = runs; return renderToStaticMarkup(<RunPanel task={t} team={team} />); };

// A rendered tree, as elements: what a component returned, with the elements in its props (children, action) too.
type El = { type: unknown; props: Record<string, unknown> };
const isEl = (x: unknown): x is El => !!x && typeof x === "object" && "type" in x && "props" in x;
function elements(node: unknown, out: El[] = []): El[] {
  if (Array.isArray(node)) node.forEach((n) => elements(n, out));
  else if (isEl(node)) { out.push(node); Object.values(node.props).forEach((v) => elements(v, out)); }
  return out;
}
const flush = () => new Promise((r) => setTimeout(r, 0));
/** Calls a component as a function with the stand-in hooks; `strings` are the values of its useState("") calls, in order. */
function call<P>(component: (p: P) => unknown, props: P, strings: string[] = []): El[] {
  fake.on = true;
  fake.strings = [...strings];
  try { return elements(component(props)); } finally { fake.on = false; fake.strings = []; }
}

beforeEach(() => invoke.mockClear());
afterEach(() => vi.unstubAllGlobals());

describe("Run this for me on the Run panel", () => {
  it("shows a held card's commands exactly, each with a Copy button, and Done, continue", () => {
    const html = panel(heldAsking, [asking]);
    expect(html).toContain('aria-label="Run this for me"');
    const codes = [...html.matchAll(/<code class="mono">([^<]*)<\/code>/g)].map((m) => text(m[1] ?? ""));
    expect(codes).toEqual(CMDS);
    expect(html.match(/aria-label="Copy [^"]*"/g)).toHaveLength(2);
    expect(html).toContain('aria-label="Copy sudo pacman -S libayatana-appindicator"');
    const t = text(html);
    expect(t).toContain("Backend Agent 2 asks you to run these commands:");
    expect(t).toContain("Run them in a terminal, then press Done, continue: Backend Agent 2 checks that they worked and carries on.");
    expect(t).toContain("Done, continue");
    expect(t).toContain("Run what the agent asks for below, then press Done, continue.");
    expect(t).not.toContain("Clear the hold to run an agent.");
  });

  it("isn't there when the card isn't held, or holds for something else", () => {
    for (const t of [task({ runForMe: CMDS }), task({ hold: "needs_decision", holdReason: "CSV or JSON?" })]) {
      const html = panel(t, [run({ outcome: "needs_decision" })]);
      expect(html).not.toContain('aria-label="Run this for me"');
      expect(text(html)).not.toContain("Done, continue");
    }
    expect(text(panel(task({ hold: "needs_decision", holdReason: "CSV or JSON?" }), [run({ outcome: "needs_decision" })])))
      .toContain("Clear the hold to run an agent.");
  });

  it("Done, continue continues the card's run", async () => {
    data.runs = [asking];
    const button = call(RunPanel, { task: heldAsking, team }).find((e) => e.props.name === "ranForMe");
    expect(button).toBeDefined();
    (button!.props.onClick as () => void)();
    await flush();
    expect(invoke).toHaveBeenCalledWith("continue_after_run_for_me", { taskId: "t1" });
  });

  it("Copy puts that one command on the clipboard, exactly", async () => {
    const writeText = vi.fn(async (_t: string) => {});
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    vi.useFakeTimers();
    try {
      const copies = call(RunForMe, { commands: CMDS, agent: "Backend Agent 2" }).filter((e) => e.type === "button");
      expect(copies.map((b) => b.props["aria-label"])).toEqual(CMDS.map((c) => `Copy ${c}`));
      await (copies[1]!.props.onClick as () => Promise<void>)();
      await (copies[0]!.props.onClick as () => Promise<void>)();
    } finally { vi.useRealTimers(); }
    expect(writeText.mock.calls.map((c) => c[0])).toEqual([CMDS[1], CMDS[0]]);
  });

  it("says this command for one, and the agent when it has no name", () => {
    const t = text(renderToStaticMarkup(<RunForMe commands={["sudo make install"]} />));
    expect(t).toContain("The agent asks you to run this command:");
    expect(t).toContain("Run it in a terminal, then press Done, continue: the agent checks that it worked and carries on.");
  });

  it("shows a command as text, never as markup", () => {
    const html = renderToStaticMarkup(<RunForMe commands={["echo <b>x</b> > /tmp/a"]} />);
    expect(html).toContain("echo &lt;b&gt;x&lt;/b&gt; &gt; /tmp/a");
    expect(html).not.toContain("<b>x</b>");
  });
});

describe("Continue with a note", () => {
  it("has a note box above Continue for a run that stopped, and none for a question or a finished run", () => {
    const html = panel(task({}), [stopped]);
    expect(html).toContain('aria-label="Note for Backend Agent 2"');
    expect(html).toContain("A note for Backend Agent 2 with Continue (optional), like: use the existing CSV writer");
    expect(html.indexOf('class="run-note"')).toBeLessThan(html.indexOf("Continue</button>"));
    for (const r of [asking, run({ outcome: "ready_for_testing" })]) {
      expect(panel(task({}), [r])).not.toContain('class="run-note"');
    }
  });

  it("Continue sends the note with the run, and Enter in the box does too", async () => {
    data.runs = [stopped];
    const els = call(RunPanel, { task: task({}), team }, ["", "  Use the existing CSV writer. "]);
    const resume = els.find((e) => e.props.name === "resume");
    (resume!.props.onClick as () => void)();
    await flush();
    expect(invoke).toHaveBeenLastCalledWith("continue_run", { runId: "run-000000001", note: "Use the existing CSV writer." });

    invoke.mockClear();
    const box = call(RunPanel, { task: task({}), team }, ["", "Keep the column order."]).find((e) => e.props["aria-label"] === "Note for Backend Agent 2");
    const preventDefault = vi.fn();
    (box!.props.onKeyDown as (e: unknown) => void)({ key: "Enter", nativeEvent: { isComposing: false }, preventDefault });
    await flush();
    expect(preventDefault).toHaveBeenCalled();
    expect(invoke).toHaveBeenLastCalledWith("continue_run", { runId: "run-000000001", note: "Keep the column order." });
  });

  it("an empty note is a plain Continue", async () => {
    await continueRun("r1", "   ");
    await continueRun("r1");
    await continueRun("r1", null);
    expect(invoke.mock.calls).toEqual([["continue_run", { runId: "r1", note: null }], ["continue_run", { runId: "r1", note: null }],
      ["continue_run", { runId: "r1", note: null }]]);
    await continueAfterRunForMe("t9");
    expect(invoke).toHaveBeenLastCalledWith("continue_after_run_for_me", { taskId: "t9" });
  });
});

describe("what started a run", () => {
  it("says Nudge for Gizai's nudge and Continue for a Continue on the Run panel", () => {
    const badge = (r: Run) => /<span class="badge info">([^<]*)<\/span>/.exec(panel(task({}), [r]))?.[1];
    expect(badge(run({ trigger: "result_nudge", nudged: true, outcome: "ready_for_testing" }))).toBe("Nudge");
    expect(badge(run({ trigger: "nudge", nudged: true, outcome: "ready_for_testing" }))).toBe("Nudge");
    expect(badge(run({ trigger: "nudge", outcome: "ready_for_testing" }))).toBe("Continue");
    expect(badge(run({ trigger: "manual", outcome: "ready_for_testing" }))).toBe("Manual");
  });
});
