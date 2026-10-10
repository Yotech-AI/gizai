// The agent run on a task (design system: RunCard): live transcript with Stop and Open in terminal while it runs;
// otherwise the last run's result and a Run button with an agent picker.
import { useEffect, useRef, useState } from "react";
import { Check, CircleAlert, Play, Square, StepForward, Terminal, X } from "lucide-react";
import { continueAfterRunForMe, continueRun, listRuns, onRunEvent, runEvents, startRun, stopRun, suggestAgent } from "../api";
import { useData } from "../lib/useData";
import { useLiveRuns } from "../lib/useLiveRuns";
import { usePending } from "../lib/usePending";
import { badgeOf, canContinue, elapsed, formatCost, formatTokens, mergeEvents, noteIsGood, resumeCommand, runReason, triggerName } from "../lib/runs";
import { relTime, textEnd } from "../lib/format";
import type { Refusal, Run, SeqEvent, Task, Team } from "../types";
import { Avatar } from "./Avatar";
import { BusyButton } from "./BusyButton";
import { MarkdownView } from "./MarkdownView";
import { RunForMe } from "./RunForMe";

export function outcomeBadge(r: Run) {
  const b = badgeOf(r);
  return <span className={`badge ${b.cls}`}>{b.text}</span>;
}

export function Stream({ events }: { events: SeqEvent[] }) {
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => { end.current?.scrollIntoView({ block: "nearest" }); }, [events.length]);
  return (
    <div className="run-stream" aria-live="polite">
      {events.map(({ seq, event: e }) => {
        switch (e.kind) {
          case "init": return <div key={seq} className="faint">Started{e.model ? ` · ${e.model}` : ""}</div>;
          case "text": { const t = e.text.replace(/^GIZAI_RESULT:.*$/m, "").trim(); return t ? <div key={seq} style={{ fontFamily: "var(--font-sans)", color: "var(--text)" }}><MarkdownView md={t} /></div> : null; }
          case "tool_use": return <div key={seq} className="tool"><b>{e.name}</b> {e.summary}</div>;
          case "tool_result": return e.is_error ? <div key={seq} className="err">{e.preview}</div> : null;
          case "result": return <div key={seq} className={e.is_error ? "err" : "ok"}>{e.is_error ? `Ended with ${e.subtype}` : "Done"} · {e.num_turns} turns</div>;
          case "note": return noteIsGood(e.text) ? <div key={seq} className="ok">{e.text}</div> : <div key={seq} style={{ color: "var(--warning)" }}>{e.text}</div>;
          case "refused": return <div key={seq} className="err">Refused: <b>{e.tool}</b> {e.input}{e.reason ? ` (${e.reason})` : ""}</div>;
          case "mcp_servers": return <div key={seq} className="faint">MCP servers: {e.servers.map((s) => `${s.name} (${s.status === "connected" ? "connected" : s.status || "failed"})`).join(", ")}</div>;
          default: return e.raw_type?.startsWith("cap_exceeded") ? <div key={seq} className="err">{e.raw_type.endsWith(":time") ? "Stopped at the time limit" : e.raw_type.endsWith(":tools") ? "Stopped at the tool-call limit" : "Stopped at the time or tool-call limit"}</div> : null;
        }
      })}
      <div ref={end} />
    </div>
  );
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
  const [now, setNow] = useState(Date.now());
  useEffect(() => { if (!live) return; const t = setInterval(() => setNow(Date.now()), 1000); return () => clearInterval(t); }, [live]);
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
          <span className="right">{elapsed(run.startedAt ?? run.createdAt, now)}
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
  const holdLine = `This card is on hold${task.holdReason ? ` (${(reasonEnd ?? task.holdReason).replace(/\.$/, "")})` : ""}. ${asks.length
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
            : task.hold ? "On hold" : suggestedName ? `Run starts ${suggestedName} unless you pick another agent` : "No agent picks this card up by itself; pick one"}</span>
          <div className="run-controls">
            <select className="select" aria-label="Agent" value={agentId} onChange={(e) => setAgentId(e.target.value)} disabled={!!pending.busy}>
              <option value="">{suggestedName ? `${suggestedName} (${suggested === task.assigneeId ? "assigned" : "on the column"})` : "Choose an agent"}</option>
              {agents.map((a) => <option key={a.actorId} value={a.actorId}>{a.name}</option>)}
            </select>
            {continuable && <BusyButton className="btn sm primary" pending={pending} name="resume" busyLabel="Continuing…" icon={<StepForward className="icon" />} onClick={resume}>Continue</BusyButton>}
            <BusyButton className={`btn sm${continuable ? "" : " primary"}`} pending={pending} name="start" busyLabel="Starting…" icon={<Play className="icon" />} onClick={start}
              disabled={!!task.hold || (!agentId && !suggestedName)}>Run</BusyButton>
          </div>
        </div>
      )}
    </section>
  );
}
