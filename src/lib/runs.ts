import type { DayStat, LeadAnswer, Run, RunOutcome, SeqEvent } from "../types";

/** History from run_events plus live run-event messages: one copy per seq, oldest first, newest `cap` kept. */
export function mergeEvents(a: SeqEvent[], b: SeqEvent[], cap = 500): SeqEvent[] {
  const bySeq = new Map<number, SeqEvent>();
  for (const e of [...a, ...b]) if (!bySeq.has(e.seq)) bySeq.set(e.seq, e);
  return [...bySeq.values()].sort((x, y) => x.seq - y.seq).slice(-cap);
}

export function formatCost(micros: number): string {
  return `$${(micros / 1_000_000).toFixed(2)}`;
}

export function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

export function elapsed(from: number, to: number): string {
  const s = Math.max(0, Math.round((to - from) / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}

/** Paste into a terminal to continue the agent's session by hand. */
export function resumeCommand(worktree: string, sessionId: string): string {
  return `cd '${worktree.replaceAll("'", "'\\''")}' && claude --resume ${sessionId}`;
}

const OUTCOME_TEXT: Record<RunOutcome, string> = {
  ready_for_testing: "Ready for testing", qa_pass: "QA passed", qa_fail: "QA failed", needs_decision: "Needs your decision",
  deployed: "Deployed", no_result: "Ended without a result", error: "Failed",
};
const STATUS_TEXT: Record<string, string> = { timed_out: "Hit a limit", failed: "Failed", running: "Running", queued: "Starting", succeeded: "Done" };

/** A run's badge: teal while it works (the design system keeps teal for that), then how it ended. */
export function badgeOf(r: Run): { cls: string; text: string } {
  if (r.status === "running" || r.status === "queued") return { cls: "live", text: STATUS_TEXT[r.status] };
  if (r.status === "cancelled") return { cls: "", text: "stopped" };
  if (r.status === "timed_out") return { cls: "warn", text: STATUS_TEXT.timed_out };
  const ok = r.status === "succeeded" && r.outcome !== "no_result";
  const cls = r.outcome === "needs_decision" || r.outcome === "qa_fail" ? "needs" : ok ? "ok" : "fail";
  const text = r.trigger === "chat" && r.status === "succeeded" ? "answered" : (r.outcome ? OUTCOME_TEXT[r.outcome] : undefined) ?? STATUS_TEXT[r.status] ?? r.status;
  return { cls, text };
}

/** The share of a day's finished runs that succeeded; null when none finished (running runs don't count). */
export function dayRate(d: DayStat): number | null {
  const finished = d.succeeded + d.failed;
  return finished === 0 ? null : d.succeeded / finished;
}

/** Why a run ended the way it did, in full; null for a run that reported its result. */
export function runReason(r: Run): string | null {
  if (r.error) return r.error;
  if (r.status === "cancelled") return "Stopped before it finished.";
  if (r.outcome === "no_result") return "It ended without reporting a result (no GIZAI_RESULT line), so the card went back for another try or on hold.";
  if (r.status === "failed" || r.status === "timed_out" || r.outcome === "error") return "It failed with no error message. Its output below shows how far it got.";
  return null;
}

/** Gizai's notes in a run's output are warnings (a folder or MCP server left out, a push that failed), except what
 * Gizai pushed after the run (GA-56), which is good news. */
export function noteIsGood(text: string): boolean {
  return text.startsWith("Gizai pushed ");
}

/** The agent's last words in a run, without the GIZAI_RESULT line. */
export function lastAgentText(events: SeqEvent[]): string | null {
  for (let i = events.length - 1; i >= 0; i--) {
    const e = events[i].event;
    if (e.kind !== "text") continue;
    const t = e.text.replace(/^GIZAI_RESULT:.*$/m, "").trim();
    if (t) return t;
  }
  return null;
}

const TRIGGER_TEXT: Record<string, string> = {
  manual: "Manual", routed: "Heartbeat", assigned: "Assigned", chat: "Chat", board_check: "Board check", nudge: "Continue", result_nudge: "Nudge",
  question: "Question",
};

/** What the Team Lead did with a run's question (GA-70), in a few words: "With the Team Lead", "Team Lead answered",
 *  "Team Lead escalated to you: <why>". */
export function leadText(l: LeadAnswer): string {
  switch (l.state) {
    case "asking": return "With the Team Lead";
    case "answering": return "Team Lead answered; the agent carries on";
    case "answered": return "Team Lead answered";
    case "escalated": return `Team Lead escalated to you${l.reason ? `: ${l.reason}` : ""}`;
    case "dropped": return "The Team Lead stopped: the card moved on first";
    default: return "Team Lead";
  }
}

/** What started a run, in a word: a Continue (yours, the Team Lead's or Done, continue) is "Continue", and Gizai's own
 *  nudge after a run ended without its result line is "Nudge" (trigger result_nudge; an older nudge was recorded as a
 *  Continue, and is told apart by `nudged`). */
export function triggerName(r: Pick<Run, "trigger" | "nudged">): string {
  if (r.nudged) return TRIGGER_TEXT.result_nudge;
  return TRIGGER_TEXT[r.trigger] ?? r.trigger;
}

/** A run that stopped part-way after doing some work: Continue resumes its session in its worktree. */
export function canContinue(r: Run): boolean {
  const stopped = ["timed_out", "failed", "cancelled"].includes(r.status) || (r.status === "succeeded" && r.outcome === "no_result");
  return stopped && !!r.sessionId && !!r.worktreePath && r.costUsdMicros > 0;
}

export function toolCalls(events: SeqEvent[]): number {
  return events.filter((e) => e.event.kind === "tool_use").length;
}

/** How many commits a run made, in words: "No commits", "1 commit", "2 commits". */
export function commitCount(n: number): string {
  return n === 0 ? "No commits" : n === 1 ? "1 commit" : `${n} commits`;
}
