import { describe, expect, it } from "vitest";
import { badgeOf, dayRate, canContinue, commitCount, elapsed, formatCost, formatTokens, lastAgentText, mergeEvents, noteIsGood, resumeCommand, runReason, toolCalls } from "./runs";
import type { Run, SeqEvent } from "../types";

const ev = (seq: number, text = `t${seq}`) => ({ seq, event: { kind: "text" as const, text } });

describe("mergeEvents", () => {
  it("keeps one copy of each event, in order, whichever arrived first", () => {
    const merged = mergeEvents([ev(0), ev(1), ev(2)], [ev(2), ev(3), ev(1)]);
    expect(merged.map((e) => e.seq)).toEqual([0, 1, 2, 3]);
  });
  it("caps the list at the newest N", () => {
    const many = Array.from({ length: 10 }, (_, i) => ev(i));
    expect(mergeEvents(many, [], 4).map((e) => e.seq)).toEqual([6, 7, 8, 9]);
  });
});

describe("formats", () => {
  it("shows cost, tokens and elapsed time compactly", () => {
    expect(formatCost(420_000)).toBe("$0.42");
    expect(formatCost(0)).toBe("$0.00");
    expect(formatCost(12_345_678)).toBe("$12.35");
    expect(formatTokens(950)).toBe("950");
    expect(formatTokens(38_000)).toBe("38k");
    expect(formatTokens(1_250_000)).toBe("1.3M");
    expect(elapsed(0, 45_000)).toBe("45s");
    expect(elapsed(0, 372_000)).toBe("6m 12s");
    expect(elapsed(0, 3_900_000)).toBe("1h 5m");
  });
  it("builds a resume command that survives spaces in paths", () => {
    expect(resumeCommand("/home/j/My Data/wt/KADE-1", "0192-ab")).toBe("cd '/home/j/My Data/wt/KADE-1' && claude --resume 0192-ab");
    expect(resumeCommand("/x/it's", "s")).toBe("cd '/x/it'\\''s' && claude --resume s");
  });
});

const run = (over: Partial<Run>): Run => ({ id: "r1", agentId: "a", agentName: "Backend Agent", trigger: "manual", status: "succeeded",
  createdAt: 0, costUsdMicros: 0, inputTokens: 0, outputTokens: 0, logPath: "/x.jsonl", ...over });

describe("runReason", () => {
  it("gives the full error of a failed or stopped run", () => {
    const long = "Claude Code: There's an issue with the selected model (opus 5.5). It may not exist or you may not have access to it.";
    expect(runReason(run({ status: "failed", outcome: "error", error: long }))).toBe(long);
    expect(runReason(run({ status: "timed_out", error: "stopped at the limit (45 min or 80 tool turns)" }))).toBe("stopped at the limit (45 min or 80 tool turns)");
  });
  it("explains the runs that end without an error message", () => {
    expect(runReason(run({ status: "succeeded", outcome: "no_result" }))).toMatch(/without reporting a result/);
    expect(runReason(run({ status: "cancelled" }))).toMatch(/Stopped/);
    expect(runReason(run({ status: "failed" }))).toMatch(/no error message/);
  });
  it("has nothing to explain for a run that reported its result", () => {
    expect(runReason(run({ status: "succeeded", outcome: "ready_for_testing" }))).toBeNull();
  });
});

describe("what the agent did", () => {
  const events: SeqEvent[] = [
    { seq: 0, event: { kind: "text", text: "Looking at the wireframe module." } },
    { seq: 1, event: { kind: "tool_use", name: "Read", summary: "src/a.ts" } },
    { seq: 2, event: { kind: "tool_use", name: "Bash", summary: "npm test" } },
    { seq: 3, event: { kind: "text", text: "Tests fail on the import; fixing the path.\nGIZAI_RESULT: {\"outcome\":\"no\"}" } },
    { seq: 4, event: { kind: "text", text: "   " } },
  ];
  it("finds the agent's last words, without the result line", () => {
    expect(lastAgentText(events)).toBe("Tests fail on the import; fixing the path.");
    expect(lastAgentText([])).toBeNull();
  });
  it("counts its tool calls", () => {
    expect(toolCalls(events)).toBe(2);
  });
});

describe("canContinue", () => {
  const worked = { sessionId: "S", worktreePath: "/wt", costUsdMicros: 3_920_000 };
  it("offers Continue for a run that stopped after doing some work", () => {
    expect(canContinue(run({ status: "timed_out", ...worked }))).toBe(true);
    expect(canContinue(run({ status: "failed", ...worked }))).toBe(true);
    expect(canContinue(run({ status: "cancelled", ...worked }))).toBe(true);
    expect(canContinue(run({ status: "succeeded", outcome: "no_result", ...worked }))).toBe(true);
  });
  it("not for a finished run, one that never got going, or one without a session or worktree", () => {
    expect(canContinue(run({ status: "succeeded", outcome: "ready_for_testing", ...worked }))).toBe(false);
    expect(canContinue(run({ status: "failed", ...worked, costUsdMicros: 0 }))).toBe(false);
    expect(canContinue(run({ status: "timed_out", ...worked, sessionId: null }))).toBe(false);
    expect(canContinue(run({ status: "timed_out", ...worked, worktreePath: null }))).toBe(false);
    expect(canContinue(run({ status: "running", ...worked }))).toBe(false);
  });
});

describe("badgeOf", () => {
  it("shows a run that is working now as live, never as failed", () => {
    expect(badgeOf(run({ status: "running" }))).toEqual({ cls: "live", text: "Running" });
    expect(badgeOf(run({ status: "queued" }))).toEqual({ cls: "live", text: "Starting" });
  });
  it("keeps the finished states", () => {
    expect(badgeOf(run({ status: "succeeded", outcome: "ready_for_testing" })).cls).toBe("ok");
    expect(badgeOf(run({ status: "timed_out" }))).toEqual({ cls: "warn", text: "Hit a limit" });
    expect(badgeOf(run({ status: "failed", outcome: "error" }))).toEqual({ cls: "fail", text: "Failed" });
    expect(badgeOf(run({ status: "cancelled" })).text).toBe("stopped");
  });
});

describe("dayRate", () => {
  it("has no rate for a day without finished runs", () => {
    expect(dayRate({ dayStart: 0, succeeded: 0, failed: 0, other: 1 })).toBeNull();
    expect(dayRate({ dayStart: 0, succeeded: 3, failed: 1, other: 1 })).toBe(0.75);
  });
});

describe("commitCount", () => {
  it("says how many commits a run made, in words", () => {
    expect(commitCount(0)).toBe("No commits");
    expect(commitCount(1)).toBe("1 commit");
    expect(commitCount(2)).toBe("2 commits");
    expect(commitCount(12)).toBe("12 commits");
  });
});

// GA-56: what Gizai pushed after a run is good news; its other notes (a failed push among them) are warnings.
describe("noteIsGood", () => {
  it("is true only for Gizai's push after the run", () => {
    expect(noteIsGood("Gizai pushed gizai/kade-1-export-invoices (2 commits).")).toBe(true);
    expect(noteIsGood("Gizai pushed gizai/kade-1-export-invoices (1 commit).")).toBe(true);
    expect(noteIsGood("Couldn't push gizai/kade-1-export-invoices to https://github.com/acme/shop: The branch on GitHub has commits this one doesn't. Gizai never forces a push: merge GitHub's copy into the branch first.")).toBe(false);
    expect(noteIsGood("Its worktree has 3 uncommitted changes, which Gizai doesn't push.")).toBe(false);
    expect(noteIsGood("Left out the folder /home/u/notes: it isn't there")).toBe(false);
    expect(noteIsGood("")).toBe(false);
  });
});

import { triggerName } from "./runs";

describe("triggerName (GA-31)", () => {
  it("tells Gizai's nudge from a Continue, also for a nudge recorded before GA-31", () => {
    expect(triggerName({ trigger: "result_nudge", nudged: true })).toBe("Nudge");
    expect(triggerName({ trigger: "result_nudge" })).toBe("Nudge");
    expect(triggerName({ trigger: "nudge", nudged: true })).toBe("Nudge");
    expect(triggerName({ trigger: "nudge", nudged: false })).toBe("Continue");
    expect(triggerName({ trigger: "nudge" })).toBe("Continue");
  });
  it("names the other triggers, and shows an unknown one as it is", () => {
    expect(["manual", "routed", "assigned", "chat", "board_check"].map((trigger) => triggerName({ trigger })))
      .toEqual(["Manual", "Heartbeat", "Assigned", "Chat", "Board check"]);
    expect(triggerName({ trigger: "webhook" })).toBe("webhook");
  });
});
