import { useEffect, useState } from "react";
import { Plus } from "lucide-react";
import { addState, addTeam, getTeam, listTeams, renameState } from "../api";
import { go, href } from "../router";
import { useData } from "../lib/useData";
import { useCurrentTeam } from "../lib/team";
import { relTime } from "../lib/format";
import { wakeupLabel } from "../lib/agents";
import { useLiveRuns } from "../lib/useLiveRuns";
import { useDrawer } from "../lib/drawers";
import type { Member, StateCategory, Team, WorkflowState } from "../types";
import { Avatar } from "../components/Avatar";
import { CATEGORIES, CATEGORY_NAMES, StatusIcon } from "../components/StatusIcon";
import { Drawer } from "../components/Drawer";
import { Field, FormSection } from "../components/Form";
import { OrgChart } from "../components/OrgChart";
import { useChatLive } from "../components/chat/useChat";

/** Who works a column: you in Review, else its agents, Auto or Manual. */
function workedBy(s: WorkflowState, members: Member[]): string {
  if (s.category === "review") return "You review and merge";
  if (["backlog", "done", "cancelled"].includes(s.category)) return "Nobody: no agents here";
  const names = (s.agentIds ?? []).map((id) => members.find((m) => m.actorId === id)?.name ?? "a removed agent");
  if (s.auto) return names.length ? `Auto: ${names.join(", ")}` : "Auto, but no agent is on it";
  return names.length ? `Manual: Run starts ${names[0]}` : "Manual: press Run and pick an agent";
}

const HINTS: Record<string, string> = {
  backlog: "Not picked up",
  testing: "Fail → back to In progress; 3 fails → hold",
  review: "Your gate: check and merge",
  deploy: "Merged, not deployed yet. Press Run for the DevOps Agent, or deploy it yourself and drag it to Done.",
  done: "Only people move cards here",
};

function Stage({ s, members, onError }: { s: WorkflowState; members: Member[]; onError: (m: string) => void }) {
  const [name, setName] = useState(s.name);
  useEffect(() => setName(s.name), [s.name]);
  const save = () => { const n = name.trim(); if (!n) setName(s.name); else if (n !== s.name) renameState(s.id, n).catch((e) => { setName(s.name); onError(String(e)); }); };
  return (
    <div className="stage">
      <div className="nm"><StatusIcon category={s.category} />
        <input className="stage-name" aria-label={`Rename ${s.name}`} value={name} onChange={(e) => setName(e.target.value)} onBlur={save}
          onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); if (e.key === "Escape") { setName(s.name); (e.target as HTMLInputElement).blur(); } }} />
      </div>
      <div className="by">{workedBy(s, members)}</div>
      <div className="lim">{HINTS[s.category] ?? ""}</div>
    </div>
  );
}

/** Add column: its name, the column it goes after and its type (what the column does). It starts Manual, without agents. */
function AddColumn({ team, onAdded, onCancel }: { team: Team; onAdded: () => void; onCancel: () => void }) {
  const states = [...team.states].sort((a, b) => (a.sortKey < b.sortKey ? -1 : 1));
  const firstType: StateCategory = states.some((s) => s.category === "deploy") ? "ready" : "deploy";
  const [name, setName] = useState("");
  const [after, setAfter] = useState(() => {
    const open = states.filter((s) => s.category !== "done" && s.category !== "cancelled");
    return (states.find((s) => s.category === "review") ?? open[open.length - 1] ?? states[states.length - 1])?.id ?? "";
  });
  const [category, setCategory] = useState<StateCategory>(firstType);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const add = async () => {
    setBusy(true); setErr(null);
    try {
      await addState(team.id, name.trim() || CATEGORY_NAMES[category], after, category);
      onAdded();
    } catch (e) { setErr(String(e)); setBusy(false); }
  };
  return (
    <div className="panel col-add">
      <div className="rule-add">
        <input className="input" aria-label="Column name" autoFocus style={{ width: 150 }} value={name} placeholder={CATEGORY_NAMES[category]}
          onChange={(e) => setName(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !busy) add(); if (e.key === "Escape") onCancel(); }} />
        <span>after</span>
        <select className="select" aria-label="After column" value={after} onChange={(e) => setAfter(e.target.value)}>
          {states.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
        </select>
        <span>type</span>
        <select className="select" aria-label="Column type" value={category}
          onChange={(e) => setCategory(e.target.value as StateCategory)}>
          {CATEGORIES.map((c) => <option key={c} value={c}>{CATEGORY_NAMES[c]}</option>)}
        </select>
        <span className="grow" />
        <button className="btn ghost" onClick={onCancel}>Cancel</button>
        <button className="btn primary" disabled={busy || !after} onClick={add}><Plus className="icon" />Add column</button>
      </div>
      {err && <div className="col-add-err" role="alert">{err}</div>}
    </div>
  );
}

function MemberCard({ m, live }: { m: Member; live: number }) {
  const agent = m.kind === "agent";
  const inner = (
    <>
      <div className="h"><Avatar name={m.name} kind={m.kind} size={agent ? "xl" : "lg"} role={m.roleKey} />
        <div><b>{m.name}</b><span className="fn">{agent ? `${m.roleKey === "qa" ? "QA" : m.roleKey} agent` : m.roleKey === "reviewer" ? "Reviewer · merges" : m.roleKey}</span></div></div>
      {agent && (
        <>
          <div className="now">
            {live > 0 ? <span className="badge live"><span className="pulse" />{live} live</span> : <span className={`state ${m.status === "active" ? "" : "paused"}`}>{m.status === "active" ? "idle" : "paused"}</span>}
            <span>{wakeupLabel(m.wakeup, m.heartbeatMinutes)}</span>
            {m.lastHeartbeatAt ? <span className="faint">· woke {relTime(m.lastHeartbeatAt)}</span> : null}
          </div>
          <div className="tags"><span className="label-pill">Claude Code</span>{m.model && <span className="label-pill">{m.model}</span>}<span className="label-pill">{m.permissionMode}</span></div>
        </>
      )}
    </>
  );
  return agent ? <a className="member" href={href({ page: "agent", id: m.actorId })}>{inner}</a> : <div className="member">{inner}</div>;
}

function NewTeamDrawer({ onClose, onCreated }: { onClose: () => void; onCreated: (id: string) => void }) {
  const [name, setName] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const make = async () => { try { onCreated(await addTeam(name)); } catch (e) { setErr(String(e)); } };
  return (
    <Drawer title="New team" subtitle="A new team gets the six usual columns and no agents. New projects still join your first team for now." onClose={onClose} dirty={!!name} error={err}
      actions={<><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={!name.trim()} onClick={make}>Create team</button></>}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); make(); }}>
        <FormSection title="Team"><Field label="Name" htmlFor="tm-name"><input id="tm-name" className="input" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Mobile team" /></Field></FormSection>
        <button type="submit" hidden />
      </form>
    </Drawer>
  );
}

export function TeamPage() {
  const [teamId, setTeamId] = useCurrentTeam();
  const { data: teams } = useData(() => listTeams());
  const { data: team, error, reload } = useData(() => getTeam(teamId), [teamId]);
  const open = useDrawer();
  const live = useLiveRuns();
  const chatLive = useChatLive();
  const [newTeam, setNewTeam] = useState(false);
  const [addingColumn, setAddingColumn] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  if (error) return <div className="error-banner">{error}</div>;
  if (!team) return null;
  const agents = team.members.filter((m) => m.kind === "agent");
  const people = team.members.filter((m) => m.kind !== "agent");
  const states = [...team.states].sort((a, b) => (a.sortKey < b.sortKey ? -1 : 1));
  return (
    <>
      <div className="topbar">
        <div className="crumbs"><span>Team</span><span className="sep">/</span>
          <select className="chip" aria-label="Team" value={team.id} onChange={(e) => { setTeamId(e.target.value); go({ page: "team" }); }}>
            {(teams ?? []).map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
          </select>
          <span className="faint">{people.length} {people.length === 1 ? "person" : "people"}, {agents.length} {agents.length === 1 ? "agent" : "agents"}</span>
        </div>
        <div className="actions">
          <button className="btn ghost" onClick={() => setNewTeam(true)}>New team</button>
          <button className="btn primary" onClick={() => open({ kind: "agent", teamId: team.id })}><Plus className="icon" />Add agent</button>
        </div>
      </div>
      {err && <div className="error-banner" role="alert">{err}<button className="btn ghost sm" onClick={() => setErr(null)}>Dismiss</button></div>}
      <div className="content">
        <div className="page">
          <section>
            <div className="section-head"><h3>Organisation</h3><span className="faint" style={{ fontSize: "var(--fs-sm)" }}>
              {agents.length === 0 ? "Start with the Team Lead: the agent you chat with. Click an empty place to add that agent." : "Click an agent to open it, or an empty place to add one"}</span></div>
            <div className="panel org-panel">
              <OrgChart members={team.members} teamId={team.id}
                working={(id) => live.some((r) => r.agentId === id) || (chatLive.length > 0 && !!agents.find((a) => a.actorId === id)?.chatEnabled)} />
            </div>
          </section>
          <section>
            <div className="section-head"><h3>People</h3></div>
            <div className="members">
              {people.map((m) => <MemberCard key={m.actorId} m={m} live={0} />)}
            </div>
          </section>
          <section>
            <div className="section-head"><h3>Workflow</h3><span className="faint" style={{ fontSize: "var(--fs-sm)" }}>Click a column name to rename it</span>
              {!addingColumn && <button className="btn ghost sm" style={{ marginLeft: "auto" }} onClick={() => setAddingColumn(true)}><Plus className="icon" />Add column</button>}</div>
            <div className="flow">{states.map((s) => <Stage key={s.id} s={s} members={team.members} onError={setErr} />)}</div>
            {addingColumn && <AddColumn key={team.id} team={team} onAdded={() => { setAddingColumn(false); reload(); }} onCancel={() => setAddingColumn(false)} />}
          </section>
        </div>
      </div>
      {newTeam && <NewTeamDrawer onClose={() => setNewTeam(false)} onCreated={(id) => { setNewTeam(false); setTeamId(id); }} />}
    </>
  );
}
