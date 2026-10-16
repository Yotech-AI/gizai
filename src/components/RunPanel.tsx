// The agent run on a task (design system: RunCard): live transcript with Stop and Open in terminal while it runs;
// otherwise the last run's result and a Run button with an agent picker.
import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Check, CircleAlert, Play, Square, StepForward, Terminal, X } from "lucide-react";
import { continueAfterRunForMe, continueRun, listRuns, onRunEvent, runEvents, startRun, stopRun, suggestAgent } from "../api";
import { useData } from "../lib/useData";
import { useLiveRuns } from "../lib/useLiveRuns";
import { usePending } from "../lib/usePending";
import { badgeOf, canContinue, elapsed, formatCost, formatTokens, mergeEvents, noteIsGood, resumeCommand, runReason, triggerName } from "../lib/runs";
import { relTime, textEnd } from "../lib/format";
import type { Refusal, Run, RunEvent, SeqEvent, Task, Team } from "../types";
import { Avatar } from "./Avatar";
import { BusyButton } from "./BusyButton";
import { MarkdownView } from "./MarkdownView";
import { RunForMe } from "./RunForMe";

export function outcomeBadge(r: Run) {
  const b = badgeOf(r);
  return <span className={`badge ${b.cls}`}>{b.text}</span>;
}

/** The line an event makes in a run's log; null for one the log doesn't show. */
function lineOf(e: RunEvent) {
  switch (e.kind) {
    case "init": return <div className="faint">Started{e.model ? ` · ${e.model}` : ""}</div>;
    case "text": { const t = e.text.replace(/^GIZAI_RESULT:.*$/m, "").trim(); return t ? <div style={{ fontFamily: "var(--font-sans)", color: "var(--text)" }}><MarkdownView md={t} /></div> : null; }
    case "tool_use": return <div className="tool"><b>{e.name}</b> {e.summary}</div>;
    case "tool_result": return e.is_error ? <div className="err">{e.preview}</div> : null;
    case "result": return <div className={e.is_error ? "err" : "ok"}>{e.is_error ? `Ended with ${e.subtype}` : "Done"} · {e.num_turns} turns</div>;
    case "note": return noteIsGood(e.text) ? <div className="ok">{e.text}</div> : <div style={{ color: "var(--warning)" }}>{e.text}</div>;
    case "refused": return <div className="err">Refused: <b>{e.tool}</b> {e.input}{e.reason ? ` (${e.reason})` : ""}</div>;
    case "mcp_servers": return <div className="faint">MCP servers: {e.servers.map((s) => `${s.name} (${s.status === "connected" ? "connected" : s.status || "failed"})`).join(", ")}</div>;
    default: return e.raw_type?.startsWith("cap_exceeded") ? <div className="err">{e.raw_type.endsWith(":time") ? "Stopped at the time limit" : e.raw_type.endsWith(":tools") ? "Stopped at the tool-call limit" : "Stopped at the time or tool-call limit"}</div> : null;
  }
}

/** A run's log. Memoised (GA-176): it redraws when an event arrives, not when the panel around it does; an earlier event's
 *  Markdown isn't parsed again then, as MarkdownView is memoised on its text. It opens at its end and follows new lines
 *  while you are at its end; scrolled up to read, it stays put. Only its own box scrolls, never the page (GA-208). Only
 *  the lines in sight, and a margin around them, are in the page (GA-190): each line's height is measured as it shows,
 *  the lines out of sight are space of the height they had, or are thought to have. */
export const Stream = memo(function Stream({ events }: { events: SeqEvent[] }) {
  const box = useRef<HTMLDivElement>(null);
  // Whether the box is at its end, within 24px: it is at first, and again when the log empties for another run.
  const atEnd = useRef(true);
  // the events that make a line (a tool's result that went well doesn't)
  const shown = useMemo(() => events.filter((x) => lineOf(x.event) !== null), [events]);
  const virt = useVirtualizer({
    count: shown.length, getScrollElement: () => box.current, getItemKey: (i) => shown[i]!.seq, overscan: 20,
    estimateSize: (i) => (shown[i]!.event.kind === "text" ? 60 : 21),
    // it opens at its end (the virtualizer scrolls there as the box shows); before the box is measured, about two boxes'
    // worth of lines
    initialOffset: Number.MAX_SAFE_INTEGER, initialRect: { width: 0, height: 640 },
  });
  // Only the box's own scrollTop: scrollIntoView scrolled the card page too, back to the log, on each new line.
  useLayoutEffect(() => {
    const b = box.current;
    if (!events.length) atEnd.current = true;
    if (b && atEnd.current) b.scrollTop = b.scrollHeight;
  }, [events.length]);
  const rows = virt.getVirtualItems();
  const above = rows.length ? rows[0]!.start : 0;
  const below = rows.length ? virt.getTotalSize() - rows[rows.length - 1]!.end : 0;
  return (
    <div className="run-stream" aria-live="polite" ref={box}
      onScroll={(e) => { const b = e.currentTarget; atEnd.current = b.scrollHeight - b.scrollTop - b.clientHeight <= 24; }}>
      {above > 0 && <div aria-hidden style={{ height: above }} />}
      {rows.map((r) => (
        <div key={r.key} data-index={r.index} ref={virt.measureElement} style={{ display: "flow-root" }}>{lineOf(shown[r.index]!.event)}</div>
      ))}
      {below > 0 && <div aria-hidden style={{ height: below }} />}
      {/* An empty last line, as before: StreamParses.test.tsx counts the log's lines. */}
      <div />
    </div>
  );
});

/** How long the live run has run, ticking each second: on its own, so the clock redraws only itself (GA-176). */
function Elapsed({ since }: { since: number }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => { const t = setInterval(() => setNow(Date.now()), 1000); return () => clearInterval(t); }, []);
  return <>{elapsed(since, now)}</>;
}

/** The tool calls the run's CLI refused: nobody could approve them during the run. */
export function Refused({ list }: { list: Refusal[] }) {
  return (
    <div className="run-refused" aria-label="Refused in this run" style={{ marginTop: 8, fontSize: "var(--fs-sm)" }}>
      <div style={{ color: "var(--warning)" }}>Refused in this run ({list.length})</div>
      <ul style={{ margin: "4px 0 0", paddingLeft: 18 }}>
        {list.map((r, i) => <li key={i}><b>{r.tool}</b> <code className="mono" style={{ whiteSpace: "pre-wrap", wordBreak: "break-word" }}>{r.input}</code>
          {r.reason && <span className="muted"> {r.reason}</span>}</li>)}
      </ul>
    </div>
  );
}

function useEvents(runId: string | null) {
  const [events, setEvents] = useState<SeqEvent[]>([]);
  useEffect(() => {
    setEvents([]);
    if (!runId) return;
    let alive = true;
    let un: (() => void) | undefined;
    // Listen first, then load the history: merging by seq loses nothing and doubles nothing.
    onRunEvent((m) => { if (alive && m.runId === runId) setEvents((xs) => mergeEvents(xs, [{ seq: m.seq, event: m.event }])); })
      .then((f) => (alive ? (un = f) : f()))
      .then(() => runEvents(runId))
      .then((hist) => { if (alive && hist) setEvents((xs) => mergeEvents(hist, xs)); })
      .catch(() => {});
    return () => { alive = false; un?.(); };
  }, [runId]);
  return events;
}

export function RunPanel({ task, team }: { task: Task; team: Team }) {
  const live = useLiveRuns().find((r) => r.taskId === task.id) ?? null;
  const { data: runs } = useData(() => listRuns(task.id), [task.id]);
  const run: Run | undefined = live ? runs?.find((r) => r.id === live.runId) : runs?.[0];
  const [showLast, setShowLast] = useState(false);
  const events = useEvents(live ? live.runId : showLast && run ? run.id : null);
  const agents = team.members.filter((m) => m.kind === "agent" && m.status === "active");
  const [agentId, setAgentId] = useState<string>(""); // "" = let Gizai choose (assigned agent, else the column's first agent)
  const [suggested, setSuggested] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  // Continue with a message: your note goes to the agent with Continue, and on the card as your comment.
  const [note, setNote] = useState("");
  useEffect(() => setNote(""), [task.id]);
  useEffect(() => { setAgentId(""); suggestAgent(task.id).then(setSuggested).catch(() => setSuggested(null)); }, [task.id, task.stateId, task.assigneeId]);
  const suggestedName = agents.find((a) => a.actorId === suggested)?.name;
  const result = [...events].reverse().find((e) => e.event.kind === "result")?.event;
  const fail = (e: unknown) => setErr(String(e));
  // Run and Continue spin until the panel shows the live run, or that the run ended (it can end before it shows live);
  // Stop spins until the live run is gone.
  const pending = usePending<"start" | "resume" | "ranForMe" | "stop">((k, runId) => k === "stop" ? !live
    : !!(live && run) || !!runs?.some((r) => r.id === runId && r.endedAt != null));
  const start = () => { setErr(null); pending.act("start", () => startRun(task.id, agentId || null), fail); };
  const resume = () => {
    if (!run) return;
    setErr(null);
    pending.act("resume", () => continueRun(run.id, note).then((id) => { setNote(""); return id; }), fail);
  };
  // Run this for me: you ran what the agent asked for; its run continues with a note that says so.
  const ranForMe = () => { setErr(null); pending.act("ranForMe", () => continueAfterRunForMe(task.id), fail); };
  const stop = () => { if (run) pending.act("stop", () => stopRun(run.id), fail); };
  const continuable = !!run && canContinue(run);
  const asks = task.hold ? task.runForMe ?? [] : [];
  const copy = async () => {
    if (!run?.worktreePath || !run.sessionId) return;
    try { await navigator.clipboard.writeText(resumeCommand(run.worktreePath, run.sessionId)); setCopied(true); setTimeout(() => setCopied(false), 2000); }
    catch { setErr("Couldn't copy to the clipboard"); }
  };

  if (live && run) {
    return (
      <section className="run-card live" aria-label="Agent run">
        <div className="run-head">
          <span className="pulse" aria-hidden /><span className="title">Live run</span>
          <span className="who"><Avatar name={run.agentName} kind="agent" size="sm" />{run.agentName}</span><span className="chip-id">{run.id.slice(-8)}</span>
          <span className="right"><Elapsed since={run.startedAt ?? run.createdAt} />
            <button className="btn sm ghost" onClick={copy} disabled={!run.sessionId}><Terminal className="icon" />{copied ? "Copied" : "Open in terminal"}</button>
            <BusyButton className="btn sm danger" pending={pending} name="stop" busyLabel="Stopping…" icon={<Square className="icon" />} onClick={stop}>Stop</BusyButton></span>
        </div>
        <Stream events={events} />
        <div className="run-foot">
          <span>{result && result.kind === "result" ? `${formatTokens(result.input_tokens)} in · ${formatTokens(result.output_tokens)} out` : "Working…"}</span>
          {result && result.kind === "result" && result.cost_usd != null && <span>{formatCost(Math.round(result.cost_usd * 1e6))}</span>}
          {run.branch && <span className="mono">{run.branch}</span>}
        </div>
        {err && <div className="run-summary"><span className="error" style={{ color: "var(--danger)" }}>{err}</span></div>}
      </section>
    );
  }

  // A long hold reason (usually the agent's whole summary, which the comments show) shows its end, about two lines; hover shows all.
  const reasonEnd = task.holdReason ? textEnd(task.holdReason, 160) : null;
  // GA-70: a question that is with the Team Lead isn't yours yet.
  const holdLine = task.withLead
    ? `${run?.agentName ?? "The agent"} asked a question, and the Team Lead is looking at it: it answers and the agent carries on, or it asks you in the Inbox.`
    : `This card is on hold${task.holdReason ? ` (${(reasonEnd ?? task.holdReason).replace(/\.$/, "")})` : ""}. ${asks.length
      ? "Run what the agent asks for below, then press Done, continue." : "Clear the hold to run an agent."}`;
  const holdTitle = reasonEnd ? task.holdReason ?? undefined : undefined;
  const icon = !run ? null : run.status === "succeeded" && run.outcome !== "no_result" ? <Check className="icon" style={{ color: "var(--success)" }} />
    : run.status === "cancelled" ? <X className="icon" style={{ color: "var(--text-3)" }} /> : <CircleAlert className="icon" style={{ color: "var(--danger)" }} />;
  return (
    <section className="run-card" aria-label="Agent run">
      <div className="run-head">
        {run ? <>{icon}{outcomeBadge(run)}<span className="chip-id">{run.id.slice(-8)}</span><span className="badge info">{triggerName(run)}</span>
          <span className="who">{run.agentName}</span></> : <span className="title">Agent run</span>}
        <span className="right">{run && <span>{relTime(run.endedAt ?? run.createdAt)}</span>}</span>
      </div>
      <div className="run-summary">
        {agents.length === 0 ? <span className="muted">No agents yet. Add them on the <a href="#/team">Team</a> page; then Run starts one on this card.</span>
          : !run ? <span className="muted" title={task.hold ? holdTitle : undefined}>{task.hold ? holdLine : "No agent has worked on this card yet. Run starts the agent chosen here (or the card's agent, else the first agent on its column) in its own git worktree."}</span>
          : <>
            {task.hold && <div className="muted" style={{ marginBottom: 6 }} title={holdTitle}>{holdLine}</div>}
            {run.summaryMd ? <MarkdownView md={run.summaryMd} /> : <span className="run-reason">{runReason(run) ?? "No summary."}</span>}
            {run.summaryMd && run.error && <div className="warn" style={{ color: "var(--warning)", fontSize: "var(--fs-sm)" }}>{run.error}</div>}
            {!!run.refused?.length && <Refused list={run.refused} />}
            {asks.length > 0 && <RunForMe commands={asks} agent={run.agentName}
              action={<BusyButton className="btn sm primary" pending={pending} name="ranForMe" busyLabel="Continuing…" icon={<StepForward className="icon" />}
                onClick={ranForMe}>Done, continue</BusyButton>} />}
            <div style={{ display: "flex", gap: 14, marginTop: 8, color: "var(--text-3)", fontSize: "var(--fs-sm)" }}>
              <span>{formatCost(run.costUsdMicros)}</span>{run.branch && <span className="mono">{run.branch}</span>}
              <button className="link" onClick={() => setShowLast((s) => !s)}>{showLast ? "Hide output" : "Show output"}</button></div>
          </>}
        {err && <div role="alert" style={{ color: "var(--danger)", marginTop: 6 }}>{err}</div>}
      </div>
      {showLast && run && <Stream events={events} />}
      {agents.length > 0 && continuable && (
        <div className="run-note">
          <input className="input" aria-label={`Note for ${run!.agentName}`} value={note} disabled={!!pending.busy} maxLength={4000}
            placeholder={`A note for ${run!.agentName} with Continue (optional), like: use the existing CSV writer`}
            onChange={(e) => setNote(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.nativeEvent.isComposing) { e.preventDefault(); resume(); } }} />
        </div>
      )}
      {agents.length > 0 && (
        <div className="run-actions">
          <span className="grow">{continuable ? `Continue picks up ${run!.agentName}'s session where it stopped${task.hold ? " and clears the hold" : ""}; Run starts fresh`
            : task.hold ? (task.withLead ? "With the Team Lead" : "On hold") : suggestedName ? `Run starts ${suggestedName} unless you pick another agent` : "No agent picks this card up by itself; pick one"}</span>
          <select className="select" aria-label="Agent" value={agentId} onChange={(e) => setAgentId(e.target.value)} disabled={!!pending.busy}>
            <option value="">{suggestedName ? `${suggestedName} (${suggested === task.assigneeId ? "assigned" : "on the column"})` : "Choose an agent"}</option>
            {agents.map((a) => <option key={a.actorId} value={a.actorId}>{a.name}</option>)}
          </select>
          <div className="run-buttons">
            {continuable && <BusyButton className="btn sm primary" pending={pending} name="resume" busyLabel="Continuing…" icon={<StepForward className="icon" />} onClick={resume}>Continue</BusyButton>}
            <BusyButton className={`btn sm${continuable ? "" : " primary"}`} pending={pending} name="start" busyLabel="Starting…" icon={<Play className="icon" />} onClick={start}
              disabled={!!task.hold || (!agentId && !suggestedName)}>Run</BusyButton>
          </div>
        </div>
      )}
    </section>
  );
}
