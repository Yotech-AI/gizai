// An agent's page (Paperclip's agent page): who it is, what it is doing, how its runs went, and its settings.
import { useState } from "react";
import { Check, CircleAlert, MessagesSquare, Pause, Pencil, Play, Plus, X } from "lucide-react";
import { agentNextTask, agentRuns, agentStats, getAgent, getTeam, listTasks, setAgentStatus, startRun } from "../api";
import { href } from "../router";
import { useData } from "../lib/useData";
import { useLiveRuns } from "../lib/useLiveRuns";
import { usePending } from "../lib/usePending";
import { relTime } from "../lib/format";
import { roleLabel } from "../lib/agents";
import { andList, boardOrder } from "../lib/columns";
import { useCurrentTeam } from "../lib/team";
import { cliName } from "../lib/clis";
import { useClis } from "../lib/useClis";
import { formatCost, dayRate } from "../lib/runs";
import { useDrawer } from "../lib/drawers";
import type { DayStat, Run } from "../types";
import { Avatar } from "../components/Avatar";
import { BusyButton } from "../components/BusyButton";
import { MarkdownView } from "../components/MarkdownView";
import { outcomeBadge } from "../components/RunPanel";
import { RunHistory } from "../components/RunHistory";

const TRIGGER: Record<string, string> = { manual: "Manual", routed: "Heartbeat", assigned: "Assigned", chat: "Chat", board_check: "Board check", nudge: "Continue" };
const md = (ms: number) => { const d = new Date(ms); return `${d.getUTCMonth() + 1}/${d.getUTCDate()}`; };

function Bars({ days, mode }: { days: DayStat[]; mode: "activity" | "success" }) {
  const max = Math.max(1, ...days.map((d) => d.succeeded + d.failed + d.other));
  return (
    <>
      <div className="bars" role="img" aria-label={mode === "activity" ? "Runs per day" : "Success rate per day"}>
        {days.map((d) => {
          const total = d.succeeded + d.failed + d.other;
          if (total === 0) return <div key={d.dayStart} className="day"><div className="seg-none" style={{ height: "3%" }} /></div>;
          if (mode === "success") {
            const rate = dayRate(d);
            if (rate === null) return <div key={d.dayStart} className="day"><div className="seg-none" style={{ height: "3%" }} /></div>;
            return <div key={d.dayStart} className="day" title={`${md(d.dayStart)}: ${Math.round(rate * 100)}%`}><div className={rate >= 0.8 ? "seg-ok" : rate >= 0.5 ? "seg-warn" : "seg-fail"} style={{ height: `${Math.max(4, rate * 100)}%` }} /></div>;
          }
          return (
            <div key={d.dayStart} className="day" title={`${md(d.dayStart)}: ${d.succeeded} succeeded, ${d.failed} failed`}>
              <div className="seg-ok" style={{ height: `${(d.succeeded / max) * 100}%` }} />
              <div className="seg-fail" style={{ height: `${(d.failed / max) * 100}%` }} />
              {d.other > 0 && <div className="seg-none" style={{ height: `${(d.other / max) * 100}%` }} />}
            </div>
          );
        })}
      </div>
      <div className="axis"><span>{md(days[0].dayStart)}</span><span>{md(days[Math.floor(days.length / 2)].dayStart)}</span><span>{md(days[days.length - 1].dayStart)}</span></div>
    </>
  );
}

export function AgentPage({ id }: { id: string }) {
  const { data: agent, error } = useData(() => getAgent(id), [id]);
  const { data: days } = useData(() => agentStats(id, 14), [id]);
  const { data: runs } = useData(() => agentRuns(id, 20), [id]);
  const clis = useClis();
  const { data: tasks } = useData(() => listTasks({}));
  const [teamId] = useCurrentTeam();
  const { data: team } = useData(() => getTeam(teamId), [teamId]);
  const live = useLiveRuns().filter((r) => r.agentId === id);
  const open = useDrawer();
  const [msg, setMsg] = useState<string | null>(null);
  // Run spins until the agent shows as working, or its run ended (it can end before it shows), or there was nothing to
  // pick up (the call returns null).
  const pending = usePending<"run">((_, runId) => live.length > 0 || runId === null || !!runs?.some((r) => r.id === runId && r.endedAt != null));
  if (error) return <div className="error-banner">{error}</div>;
  if (!agent) return null;
  const taskOf = (r: Run) => tasks?.find((t) => t.id === r.taskId);
  // The columns it is on decide when it works (Team → Workflow); shown when the agent is on the team picked there.
  const columns = team?.members.some((m) => m.actorId === id) ? boardOrder(team.states).filter((s) => s.agentIds?.includes(id)) : null;
  const onColumns = !columns ? "" : columns.length ? ` · on ${andList(columns.map((s) => `${s.name} (${s.auto ? "Auto" : "Manual"})`))}` : " · on no column";
  const state = live.length ? "running" : agent.status === "paused" ? "paused" : "idle";
  const last = runs?.[0];
  const monthStart = (() => { const n = new Date(); return Date.UTC(n.getUTCFullYear(), n.getUTCMonth(), 1); })();
  const spent = (runs ?? []).filter((r) => r.createdAt >= monthStart).reduce((s, r) => s + r.costUsdMicros, 0);
  const total = (days ?? []).reduce((s, d) => s + d.succeeded + d.failed, 0);
  const ok = (days ?? []).reduce((s, d) => s + d.succeeded, 0);
  const runNext = () => pending.act("run", async () => {
    setMsg(null);
    const task = await agentNextTask(id);
    if (!task) { setMsg(`Nothing to pick up: no card waits for ${agent.name} in an Auto column right now.`); return null; }
    return startRun(task, id);
  }, (e) => setMsg(String(e)));
  return (
    <>
      <div className="topbar"><div className="crumbs"><a href={href({ page: "team" })}>Team</a><span className="sep">/</span><b>{agent.name}</b></div></div>
      <div className="content">
        <div className="page">
          <div className="entity-head">
            <Avatar name={agent.name} kind="agent" size="xl" role={agent.roleKey} />
            <div className="names"><h1>{agent.name}</h1>
              <p>{roleLabel(agent.roleKey)}{agent.chatEnabled ? " · answers on the Chat page" : ""}{agent.chatEnabled && agent.boardCheckMinutes ? ` · checks the board every ${agent.boardCheckMinutes} min` : ""} · {cliName(agent.adapter, clis)}{agent.model ? ` (${agent.model})` : ""}{onColumns}</p></div>
            <div className="actions">
              {agent.chatEnabled && <a className="btn" href={href({ page: "chat" })}><MessagesSquare className="icon" />Open chat</a>}
              <button className="btn" onClick={() => open({ kind: "task", assigneeId: id })}><Plus className="icon" />Assign task</button>
              <BusyButton className="btn" pending={pending} name="run" busyLabel="Starting…" icon={<Play className="icon" />} onClick={runNext} disabled={state !== "idle"}>Run</BusyButton>
              <button className="btn" onClick={() => setAgentStatus(id, agent.status === "active" ? "paused" : "active").catch((e) => setMsg(String(e)))}>
                {agent.status === "active" ? <><Pause className="icon" />Pause</> : <><Play className="icon" />Resume</>}</button>
              <span className={`state ${state}`}>{state}</span>
              <button className="btn ghost" onClick={() => open({ kind: "agent", id })}><Pencil className="icon" />Settings</button>
            </div>
          </div>
          {msg && <div className="error-banner" style={{ margin: 0 }} role="status">{msg}</div>}
          {agent.boardCheckPaused && (
            <div className="error-banner" style={{ margin: 0 }} role="status">The board check stopped: {agent.boardCheckPaused} Change or resume {agent.name} to start it again.{" "}
              <button className="link" onClick={() => setAgentStatus(id, "active").catch((e) => setMsg(String(e)))}>Start the board check again</button></div>
          )}

          <section>
            <div className="section-head"><h3>{live.length ? "Working now" : "Latest run"}</h3>{last?.taskId && <a className="link" href={href({ page: "task", id: last.taskId })}>Open task</a>}</div>
            {live.length > 0 ? (
              <div className="run-card live"><div className="run-head"><span className="pulse" /><span className="title">Live</span>
                {live.map((r) => { const t = tasks?.find((x) => x.id === r.taskId); return <a key={r.runId} className="who" href={href({ page: "task", id: r.taskId })}><span className="id">{t?.identifier}</span>{t?.title}</a>; })}</div></div>
            ) : last ? (
              <div className="run-card">
                <div className="run-head">{last.status === "succeeded" && last.outcome !== "no_result" ? <Check className="icon" style={{ color: "var(--success)" }} /> : last.status === "cancelled" ? <X className="icon" style={{ color: "var(--text-3)" }} /> : <CircleAlert className="icon" style={{ color: "var(--danger)" }} />}
                  {outcomeBadge(last)}<span className="chip-id">{last.id.slice(-8)}</span><span className="badge info">{TRIGGER[last.trigger] ?? last.trigger}</span>
                  {taskOf(last) && <span className="muted"><span className="id">{taskOf(last)!.identifier}</span> {taskOf(last)!.title}</span>}
                  <span className="right">{relTime(last.endedAt ?? last.createdAt)}</span></div>
                <div className="run-summary">{last.summaryMd ? <MarkdownView md={last.summaryMd} /> : <span className="muted">{last.error ?? "No summary."}</span>}</div>
              </div>
            ) : <div className="empty"><b>No runs yet.</b><span>{agent.name} starts when you press Run, or by itself on the cards of the Auto columns it is on (Team → Workflow).</span></div>}
          </section>

          {days && days.length > 0 && (
            <div className="stats">
              <div className="stat-card"><h4>Run activity</h4><span className="sub">Last 14 days · {total} runs</span><Bars days={days} mode="activity" />
                <div className="legend"><span><i style={{ background: "var(--success)" }} />Succeeded</span><span><i style={{ background: "var(--danger)" }} />Failed</span></div></div>
              <div className="stat-card"><h4>Success rate</h4><span className="sub">Last 14 days · {total ? Math.round((ok / total) * 100) : 0}%</span><Bars days={days} mode="success" /></div>
              <div className="stat-card"><h4>Spend this month</h4><span className="sub">{agent.budgetUsdMicros ? `Budget ${formatCost(agent.budgetUsdMicros)}` : "No budget set"}</span>
                <div className="t-metric" style={{ marginTop: 14 }}>{formatCost(spent)}</div>
                {agent.budgetUsdMicros ? <div style={{ height: 6, borderRadius: 3, background: "var(--line)", marginTop: 10, overflow: "hidden" }}>
                  <div style={{ height: "100%", width: `${Math.min(100, (spent / agent.budgetUsdMicros) * 100)}%`, background: spent >= agent.budgetUsdMicros ? "var(--danger)" : spent >= 0.8 * agent.budgetUsdMicros ? "var(--warning)" : "var(--success)" }} /></div> : null}</div>
            </div>
          )}

          <section>
            <div className="section-head"><h3>Recent runs</h3></div>
            <div className="panel">
              {(runs ?? []).map((r) => { const t = taskOf(r); return (
                <a key={r.id} className="panel-row" href={r.taskId ? href({ page: "task", id: r.taskId }) : r.trigger === "chat" ? href({ page: "chat" }) : undefined}
                  title={r.trigger === "board_check" ? r.summaryMd ?? r.error ?? undefined : undefined}>
                  {t && <span className="id">{t.identifier}</span>}
                  {r.trigger === "board_check" ? <span className="grow ellipsis"><span className="badge info">Board check</span> <span className="muted">{(r.summaryMd ?? r.error ?? "").split("\n")[0]}</span></span>
                    : <span className="grow">{t?.title ?? (r.trigger === "chat" ? "Chat answer" : "A deleted task")}</span>}
                  {outcomeBadge(r)}<span className="faint">{formatCost(r.costUsdMicros)}</span><span className="faint" style={{ width: 80, textAlign: "right" }}>{relTime(r.createdAt)}</span>
                </a>); })}
              {runs && runs.length === 0 && <div className="panel-row faint">No runs yet.</div>}
            </div>
          </section>

          {(runs ?? []).some((r) => r.trigger === "board_check") && (
            <section>
              <div className="section-head"><h3>Board checks</h3></div>
              <RunHistory runs={(runs ?? []).filter((r) => r.trigger === "board_check")} />
            </section>
          )}

          <section>
            <div className="section-head"><h3>Instructions</h3><button className="link" onClick={() => open({ kind: "agent", id })}>Edit</button></div>
            <div className="panel" style={{ padding: "14px 16px" }}>{agent.instructionsMd ? <MarkdownView md={agent.instructionsMd} /> : <span className="faint">The role's standard instructions.</span>}</div>
          </section>
        </div>
      </div>
    </>
  );
}
