// The Runs tab of a card (design system: RunHistory): one row per run, with how many commits it made. A row opens to
// show why the run ended, in full, the agent's last message, the commits, the facts (model, time, tool calls, tokens,
// branch, worktree) and its whole output.
import { useEffect, useState } from "react";
import { ChevronRight } from "lucide-react";
import { runCommits, runEvents } from "../api";
import { commitCount, elapsed, formatCost, formatTokens, lastAgentText, resumeCommand, runReason, toolCalls, triggerName } from "../lib/runs";
import { relTime } from "../lib/format";
import type { Commit, Run, SeqEvent } from "../types";
import { MarkdownView } from "./MarkdownView";
import { outcomeBadge, Stream } from "./RunPanel";

const when = (ms: number) => new Date(ms).toLocaleString("en-GB", { dateStyle: "medium", timeStyle: "short" });

/** The commits a run made, oldest first, under a heading that counts them. */
export function RunCommits({ commits }: { commits: Commit[] }) {
  return (
    <>
      <h4>{commitCount(commits.length)}</h4>
      {commits.length > 0 && (
        <ul className="run-commits">
          {commits.map((c) => <li key={c.sha}><span className="mono" title={c.sha}>{c.sha.slice(0, 9)}</span><span>{c.subject}</span></li>)}
        </ul>
      )}
    </>
  );
}

function RunRow({ r }: { r: Run }) {
  const [open, setOpen] = useState(false);
  const [events, setEvents] = useState<SeqEvent[] | null>(null);
  const [all, setAll] = useState(false);
  // Once it has ended: the commits from where it started to where it ended never change, so they load once.
  const [commits, setCommits] = useState<Commit[] | null>(null);
  const [commitsError, setCommitsError] = useState<string | null>(null);
  useEffect(() => {
    if (!r.baseSha || !r.headSha) return;
    let alive = true;
    runCommits(r.id).then((c) => alive && setCommits(c)).catch((e) => alive && setCommitsError(String(e)));
    return () => { alive = false; };
  }, [r.id, r.baseSha, r.headSha]);
  const reason = runReason(r);
  const toggle = () => {
    setOpen((o) => !o);
    if (!events) runEvents(r.id).then(setEvents).catch(() => setEvents([]));
  };
  const last = events ? lastAgentText(events) : null;
  const init = events?.find((e) => e.event.kind === "init")?.event;
  const model = init && init.kind === "init" ? init.model : null;
  return (
    <div className={`run-row${open ? " open" : ""}`}>
      <button className="panel-row" aria-expanded={open} onClick={toggle}>
        <ChevronRight className="icon sm chev" />{outcomeBadge(r)}<span className="chip-id">{r.id.slice(-8)}</span>
        <span className="grow">{r.agentName} <span className="faint">· {r.roleKey} · {triggerName(r)}</span>
          {commits && commits.length > 0 && <span className="faint"> · {commitCount(commits.length)}</span>}{reason && <span className="faint"> · {reason}</span>}</span>
        <span className="faint">{formatCost(r.costUsdMicros)}</span><span className="faint" title={when(r.createdAt)}>{relTime(r.createdAt)}</span>
      </button>
      {open && (
        <div className="run-detail">
          {reason && <><h4>Why it ended</h4><p className="run-reason">{reason}</p></>}
          {r.summaryMd ? <><h4>Summary</h4><MarkdownView md={r.summaryMd} /></>
            : last && <><h4>Last message from {r.agentName}</h4><div className="run-last"><MarkdownView md={last} /></div></>}
          {commits && <RunCommits commits={commits} />}
          <dl className="run-facts">
            {r.startedAt && <><dt>Started</dt><dd>{when(r.startedAt)}</dd></>}
            {r.startedAt && r.endedAt && <><dt>Ran for</dt><dd>{elapsed(r.startedAt, r.endedAt)}</dd></>}
            {model && <><dt>Model</dt><dd className="mono">{model}</dd></>}
            <dt>Tool calls</dt><dd>{events ? toolCalls(events) : "…"}</dd>
            <dt>Tokens</dt><dd>{formatTokens(r.inputTokens)} in, {formatTokens(r.outputTokens)} out</dd>
            <dt>Cost</dt><dd>{formatCost(r.costUsdMicros)}</dd>
            {r.branch && <><dt>Branch</dt><dd className="mono">{r.branch}</dd></>}
            {r.baseSha && <><dt>Started at commit</dt><dd className="mono">{r.baseSha.slice(0, 9)}</dd></>}
            {r.headSha && <><dt>Ended at commit</dt><dd className="mono">{r.headSha.slice(0, 9)}</dd></>}
            {commitsError && <><dt>Commits</dt><dd>{commitsError}</dd></>}
            {r.worktreePath && <><dt>Worktree</dt><dd className="mono">{r.worktreePath}</dd></>}
            {r.worktreePath && r.sessionId && (!r.adapter || r.adapter === "claude_code") && r.trigger !== "board_check" && <><dt>Continue by hand</dt><dd className="mono">{resumeCommand(r.worktreePath, r.sessionId)}</dd></>}
          </dl>
          {events && events.length > 0 && (
            <button className="link" onClick={() => setAll((a) => !a)}>{all ? "Hide output" : `Show the whole output (${events.length} steps)`}</button>
          )}
          {all && events && <Stream events={events} />}
        </div>
      )}
    </div>
  );
}

export function RunHistory({ runs }: { runs: Run[] | null | undefined }) {
  return (
    <div className="panel">
      {(runs ?? []).map((r) => <RunRow key={r.id} r={r} />)}
      {runs && runs.length === 0 && <div className="panel-row faint">No runs yet.</div>}
    </div>
  );
}
