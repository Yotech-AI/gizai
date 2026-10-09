// The Usage page (GA-33): what the agents use, tokens and API cost, in total with a bar per day, per agent and per project.
// The three tabs share the period switch; gizai-core sums the runs and chat turns (usage.rs), so the tabs add up to the
// same total.
import { useState, type ReactNode } from "react";
import { ChartColumn, FolderKanban, MessagesSquare, Users, type LucideIcon } from "lucide-react";
import { usageSummary } from "../api";
import { href } from "../router";
import { useData } from "../lib/useData";
import {
  COST_NOTE, DEFAULT_PERIOD, INPUT_LABEL, PERIODS, barHeights, dayLabel, formatTokenCount, formatUsageCost, fullCount, periodDays,
  periodPhrase, runsLabel, shareBasis, shareOf, unknownCostNote,
} from "../lib/usage";
import type { Usage, UsageDay, UsagePeriod, UsageTotals } from "../types";
import { roleIcon } from "../components/Avatar";

type Tab = "total" | "agents" | "projects";
const TABS: [Tab, LucideIcon, string][] = [["total", ChartColumn, "Total"], ["agents", Users, "Agents"], ["projects", FolderKanban, "Projects"]];

/** The API cost of runs, with why part of it is unknown on hover. */
function Cost({ t }: { t: UsageTotals }) {
  return <span className={t.unknownCostRuns > 0 ? "usage-unknown" : undefined} title={unknownCostNote(t.unknownCostRuns)}>{formatUsageCost(t)}</span>;
}

function Tokens({ n }: { n: number }) {
  return <span title={`${fullCount(n)} tokens`}>{formatTokenCount(n)}</span>;
}

function Metrics({ t }: { t: UsageTotals }) {
  return (
    <div className="stats">
      <div className="stat-card"><h4>API cost</h4><span className="sub">An estimate at API prices, not a bill</span>
        <div className="t-metric usage-metric"><Cost t={t} /></div></div>
      <div className="stat-card"><h4>{INPUT_LABEL}</h4><span className="sub">Cache reads and writes count as input</span>
        <div className="t-metric usage-metric"><Tokens n={t.inputTokens} /></div></div>
      <div className="stat-card"><h4>Output tokens</h4><span className="sub">What the models wrote</span>
        <div className="t-metric usage-metric"><Tokens n={t.outputTokens} /></div></div>
      <div className="stat-card"><h4>Runs and chat turns</h4><span className="sub">Of all agents</span>
        <div className="t-metric usage-metric">{t.runs}</div><span className="sub">{runsLabel(t)}</span></div>
    </div>
  );
}

/** A bar per UTC day: the API cost, or the input and output tokens stacked. */
function DayBars({ days, show }: { days: UsageDay[]; show: "cost" | "tokens" }) {
  if (days.length === 0) return null;
  const heights = barHeights(days.map((d) => (show === "cost" ? d.totals.costUsdMicros : d.totals.inputTokens + d.totals.outputTokens)));
  const tip = (d: UsageDay) => `${dayLabel(d.dayStart)}: ${formatUsageCost(d.totals)} · ${formatTokenCount(d.totals.inputTokens)} input, ${formatTokenCount(d.totals.outputTokens)} output tokens`;
  const axis = days.length === 1 ? [days[0]] : days.length === 2 ? [days[0], days[1]] : [days[0], days[Math.floor(days.length / 2)], days[days.length - 1]];
  return (
    <>
      <div className="bars usage-bars" role="img" aria-label={show === "cost" ? "API cost per day" : "Tokens per day"}>
        {days.map((d, i) => {
          const h = heights[i];
          if (h === 0) return <div key={d.dayStart} className="day" title={tip(d)}><div className="seg-none" style={{ height: "3%" }} /></div>;
          if (show === "cost") return <div key={d.dayStart} className="day" title={tip(d)}><div className="seg-cost" style={{ height: `${h}%` }} /></div>;
          const all = d.totals.inputTokens + d.totals.outputTokens;
          return (
            <div key={d.dayStart} className="day" title={tip(d)}>
              <div className="seg-in" style={{ height: `${(h * d.totals.inputTokens) / all}%` }} />
              {d.totals.outputTokens > 0 && <div className="seg-out" style={{ height: `${Math.max(1, (h * d.totals.outputTokens) / all)}%` }} />}
            </div>
          );
        })}
      </div>
      <div className={`axis${axis.length === 1 ? " one" : ""}`}>{axis.map((d) => <span key={d.dayStart}>{dayLabel(d.dayStart)}</span>)}</div>
    </>
  );
}

function TotalTab({ u }: { u: Usage }) {
  return (
    <>
      <Metrics t={u.total} />
      <div className="usage-charts">
        <div className="stat-card"><h4>API cost per day</h4><span className="sub">{formatUsageCost(u.total)} in {u.days.length === 1 ? "1 day" : `${u.days.length} days`}</span>
          <DayBars days={u.days} show="cost" /></div>
        <div className="stat-card"><h4>Tokens per day</h4><span className="sub">{formatTokenCount(u.total.inputTokens + u.total.outputTokens)} tokens</span>
          <DayBars days={u.days} show="tokens" />
          <div className="legend"><span><i className="seg-in" />Input (incl. cache)</span><span><i className="seg-out" />Output</span></div></div>
      </div>
    </>
  );
}

type Row = { id: string; name: ReactNode; note?: string; totals: UsageTotals };

/** The agents or the projects with their usage, and the total they add up to. */
function UsageTable({ rows, total, nameHeader, label }: { rows: Row[]; total: UsageTotals; nameHeader: string; label: string }) {
  const basis = shareBasis(total);
  return (
    <div className="panel usage-panel">
      <table className="grid usage-table" aria-label={label}>
        <thead>
          <tr>
            <th>{nameHeader}</th><th>Runs</th><th className="num">{INPUT_LABEL}</th><th className="num">Output tokens</th><th className="num">API cost</th>
            <th className="share" title={basis === "cost" ? "Share of the API cost" : "Share of the tokens: no run reported a cost"}>Share</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => {
            const share = shareOf(r.totals, total, basis);
            return (
              <tr key={r.id}>
                <td><span className="usage-name">{r.name}</span>{r.note && <span className="usage-note-inline faint">{r.note}</span>}</td>
                <td className="muted">{runsLabel(r.totals)}</td>
                <td className="num"><Tokens n={r.totals.inputTokens} /></td>
                <td className="num"><Tokens n={r.totals.outputTokens} /></td>
                <td className="num"><Cost t={r.totals} /></td>
                <td className="share"><span className="usage-share"><span className="track"><i style={{ width: `${share}%` }} /></span>{Math.round(share)}%</span></td>
              </tr>
            );
          })}
        </tbody>
        <tfoot>
          <tr>
            <td>Total</td>
            <td className="muted">{runsLabel(total)}</td>
            <td className="num"><Tokens n={total.inputTokens} /></td>
            <td className="num"><Tokens n={total.outputTokens} /></td>
            <td className="num"><Cost t={total} /></td>
            <td className="share" />
          </tr>
        </tfoot>
      </table>
    </div>
  );
}

function AgentsTab({ u }: { u: Usage }) {
  const rows: Row[] = u.agents.map((a) => {
    const Role = roleIcon(a.roleKey);
    return {
      id: a.agentId, totals: a.totals,
      name: <a href={href({ page: "agent", id: a.agentId })}><Role className="icon" />{a.name}</a>,
      note: a.totals.chatTurns > 0 ? "chat turns included" : undefined,
    };
  });
  return <UsageTable rows={rows} total={u.total} nameHeader="Agent" label="Usage per agent" />;
}

function ProjectsTab({ u }: { u: Usage }) {
  const rows: Row[] = u.projects.map((p) => ({
    id: p.projectId, totals: p.totals,
    name: <a href={href({ page: "project", id: p.projectId })}><span className="dot" style={{ background: p.color ?? "var(--text-3)" }} />{p.name}<span className="id">{p.key}</span></a>,
  }));
  rows.push({ id: "chat", totals: u.chat, name: <><MessagesSquare className="icon" />Chat (no project)</>, note: "the Team Lead's chat turns and board checks" });
  if (u.noProject.runs > 0) rows.push({ id: "no-project", totals: u.noProject, name: <><FolderKanban className="icon" />Cards without a project</> });
  return <UsageTable rows={rows} total={u.total} nameHeader="Project" label="Usage per project" />;
}

export function UsagePage() {
  const [period, setPeriod] = useState<UsagePeriod>(DEFAULT_PERIOD);
  const [tab, setTab] = useState<Tab>("total");
  const { data: u, error } = useData(() => usageSummary(period), [period]);
  return (
    <>
      <div className="topbar"><div className="crumbs"><b>Usage</b>{u && <span className="faint">{periodDays(u.since, u.until)}</span>}</div></div>
      <div className="toolbar">
        <div className="chips" role="group" aria-label="Period">{PERIODS.map((p) => (
          <button key={p.key} className={`chip${period === p.key ? " on" : ""}`} aria-pressed={period === p.key} onClick={() => setPeriod(p.key)}>{p.label}</button>))}</div>
        <span className="spacer" />
        <span className="faint usage-utc" title="Days and months start at midnight UTC, like the agents' monthly budgets">UTC days</span>
      </div>
      {error && <div className="error-banner">{error}</div>}
      <div className="content">
        <div className="page usage-page">
          <div className="tabs" role="tablist">
            {TABS.map(([k, I, l]) => (
              <button key={k} className="tab" role="tab" aria-selected={tab === k} onClick={() => setTab(k)}><I className="icon" />{l}</button>
            ))}
          </div>
          {u && u.total.runs === 0 ? (
            <div className="empty"><b>No runs {periodPhrase(period)}.</b>
              <span>The agents' runs and the Team Lead's chat turns show here with their tokens and API cost.</span></div>
          ) : u && (
            <div role="tabpanel" className="usage-tab">
              {tab === "total" && <TotalTab u={u} />}
              {tab === "agents" && <AgentsTab u={u} />}
              {tab === "projects" && <ProjectsTab u={u} />}
            </div>
          )}
          <p className="usage-foot faint">{COST_NOTE} Input tokens include cache reads and writes. A run without a cost (Codex, Gemini and other CLIs report none) counts its tokens, and its cost shows as unknown.</p>
        </div>
      </div>
    </>
  );
}
