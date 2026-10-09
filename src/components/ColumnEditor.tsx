// Team → Workflow: the team's columns in board order. Each column shows its agents (chips with ×, "+ Agent", or drop an
// agent card from the organisation chart on it), Auto or Manual and its next column, Review where merged cards go, and
// one line that says what happens. Columns are dragged by their grip into a new place; the bin asks first.
// The Team page's DndContext runs the drags (agents and columns); this file draws what they drop on.
import { useEffect, useState } from "react";
import { SortableContext, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { GripVertical, Plus, Trash2, X } from "lucide-react";
import { addColumnAgent, addState, columnRemoval, removeColumnAgent, removeState, renameState, setColumn } from "../api";
import { useLiveRuns } from "../lib/useLiveRuns";
import { andList, agentNames, boardOrder, columnLine, KINDS, lastOfKind, mergedTarget, takesAgents } from "../lib/columns";
import type { ColumnRemoval, Member, Team, WorkflowState } from "../types";
import { Avatar } from "./Avatar";
import { Popover } from "./Popover";
import { StatusIcon } from "./StatusIcon";

/** What a dragged column carries; `takesAgents` lets an agent card drop only on columns that take agents. */
export type ColumnDrag = { type: "column"; takesAgents: boolean; name: string };

/** One error at a time, under the column it belongs to. */
export type ColumnError = { id: string; text: string } | null;

function AgentChip({ m, column, onError }: { m: Member | undefined; column: WorkflowState; onError: (t: string) => void }) {
  const name = m?.name ?? "A removed agent";
  const paused = !!m && m.status !== "active";
  return (
    <span className={`agent-chip${paused ? " paused" : ""}`} title={paused ? `${name} is paused` : name}>
      <Avatar name={name} kind="agent" size="sm" /><span className="nm">{name}</span>
      {m && <button className="chip-x" aria-label={`Take ${name} off ${column.name}`} title={`Take ${name} off ${column.name}`}
        onClick={() => removeColumnAgent(column.id, m.actorId).catch((e) => onError(String(e)))}><X className="icon" /></button>}
    </span>
  );
}

/** "+ Agent": the team's agents that aren't on this column yet. */
function AddAgentMenu({ column, agents, onError }: { column: WorkflowState; agents: Member[]; onError: (t: string) => void }) {
  const on = new Set(column.agentIds ?? []);
  const free = agents.filter((a) => !on.has(a.actorId));
  return (
    <Popover label={`Agents for ${column.name}`} button={() => (
      <button className="btn ghost sm add-agent" aria-label={`Add an agent to ${column.name}`} title={`Put an agent on ${column.name}`}><Plus className="icon" />Agent</button>)}>
      {(close) => (<>
        <div className="pop-label">Put on {column.name}</div>
        {agents.length === 0 ? <div className="pop-empty">No agents yet: add one in the organisation chart</div>
          : free.length === 0 ? <div className="pop-empty">Every agent is on {column.name} already</div>
          : free.map((a) => (
            <button key={a.actorId} className="opt" onClick={() => { close(); addColumnAgent(column.id, a.actorId).catch((e) => onError(String(e))); }}>
              <Avatar name={a.name} kind="agent" size="sm" />{a.name}{a.status !== "active" && <span className="faint">paused</span>}</button>))}
      </>)}
    </Popover>
  );
}

/** The bin's confirm: how many cards (archived ones included) go where, and which columns get relinked or lose their link. */
function RemoveConfirm({ s, states, members, info, onClose, onError }: {
  s: WorkflowState; states: WorkflowState[]; members: Member[]; info: ColumnRemoval; onClose: () => void; onError: (t: string) => void;
}) {
  const others = boardOrder(states).filter((x) => x.id !== s.id);
  const [target, setTarget] = useState(info.defaultTarget ?? others[0]?.id ?? "");
  const [busy, setBusy] = useState(false);
  const next = states.find((x) => x.id === s.nextStateId);
  const agents = agentNames(s, members);
  const remove = async () => {
    setBusy(true);
    try { await removeState(s.id, target); onClose(); } catch (e) { onError(String(e)); setBusy(false); }
  };
  const live = info.cards - info.archived;
  return (
    <div className="wf-confirm" role="alertdialog" aria-label={`Remove ${s.name}`}>
      <b>Remove {s.name}?</b>
      {info.blocked ? <span>{info.blocked}.</span> : (<>
        {info.cards === 0 ? <span>It has no cards.</span> : (
          <span className="wf-confirm-move">
            {info.archived === 0 ? `Its ${live === 1 ? "card goes" : `${live} cards go`} to`
              : live === 0 ? `Its ${info.archived === 1 ? "archived card goes" : `${info.archived} archived cards go`} to`
              : `Its ${info.cards} cards (${info.archived} archived) go to`}
            <select className="select" aria-label={`Cards of ${s.name} go to`} value={target} onChange={(e) => setTarget(e.target.value)}>
              {others.map((x) => <option key={x.id} value={x.id}>{x.name}{x.id === info.defaultTarget ? " (the column before)" : ""}</option>)}
            </select>
          </span>
        )}
        {info.relinked.length > 0 && <span>{andList(info.relinked)} {info.relinked.length === 1 ? "links" : "link"} to {next?.name ?? "its next column"} instead.</span>}
        {info.unlinked.length > 0 && <span>{andList(info.unlinked)} {info.unlinked.length === 1 ? "loses its" : "lose their"} next column and {info.unlinked.length === 1 ? "turns" : "turn"} Manual if Auto.</span>}
        {agents.length > 0 && <span>{andList(agents)} {agents.length === 1 ? "comes" : "come"} off it.</span>}
      </>)}
      <span className="wf-confirm-actions">
        <button className="btn ghost sm" onClick={onClose}>Keep</button>
        {!info.blocked && <button className="btn danger sm" disabled={busy || !target} onClick={remove}>{busy ? "Removing…" : "Remove column"}</button>}
      </span>
    </div>
  );
}

function ColumnRow({ s, states, members, agents, removal, error, onError, dropping }: {
  s: WorkflowState; states: WorkflowState[]; members: Member[]; agents: Member[]; removal?: ColumnRemoval | null; error: string | null;
  onError: (text: string | null) => void;
  /** An agent card is dragged over this column (and it takes agents). */
  dropping: boolean;
}) {
  const takes = takesAgents(s.category);
  const data: ColumnDrag = { type: "column", takesAgents: takes, name: s.name };
  const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({ id: s.id, data });
  const [name, setName] = useState(s.name);
  useEffect(() => setName(s.name), [s.name]);
  const [asking, setAsking] = useState<ColumnRemoval | null>(null);
  const fail = (e: unknown) => onError(String(e));
  const run = (p: Promise<unknown>) => { onError(null); p.catch(fail); };
  const rename = () => {
    const n = name.trim();
    if (!n) { setName(s.name); return; }
    if (n !== s.name) { onError(null); renameState(s.id, n).catch((e) => { setName(s.name); fail(e); }); }
  };
  const order = boardOrder(states);
  const blocked = removal?.blocked ?? lastOfKind(s, states);
  const openBin = () => { onError(null); columnRemoval(s.id).then(setAsking).catch(fail); };
  const merged = s.category === "review" ? mergedTarget(s, states) : undefined;
  return (
    <div ref={setNodeRef} className={`wf-col${dropping ? " drop" : ""}${isDragging ? " dragging" : ""}`} data-col={s.name} aria-label={`Column ${s.name}`}
      style={{ transform: CSS.Translate.toString(transform), transition }}>
      <div className="wf-main">
        <button ref={setActivatorNodeRef} className="wf-grip" aria-label={`Move ${s.name}`} title="Drag to a new place (or Space, then the arrow keys)" {...attributes} {...listeners}>
          <GripVertical className="icon" /></button>
        <div className="wf-name"><StatusIcon category={s.category} />
          <input className="stage-name" aria-label={`Rename ${s.name}`} value={name} onChange={(e) => setName(e.target.value)} onBlur={rename}
            onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); if (e.key === "Escape") { setName(s.name); (e.target as HTMLInputElement).blur(); } }} />
        </div>
        <div className="wf-agents">
          {takes ? (<>
            {(s.agentIds ?? []).map((id) => <AgentChip key={id} m={members.find((m) => m.actorId === id)} column={s} onError={fail} />)}
            <AddAgentMenu column={s} agents={agents} onError={fail} />
          </>) : s.category === "review" ? <span className="wf-you">You review and merge</span>
            : <span className="faint">No agents</span>}
        </div>
        <div className="wf-flow">
          {takes && (
            <div className="seg" role="group" aria-label={`Auto or Manual for ${s.name}`}>
              <button aria-pressed={!!s.auto} title={s.nextStateId ? "Its agents take its cards by themselves" : "Auto needs a next column: pick one first"}
                onClick={() => { if (!s.auto) run(setColumn(s.id, { auto: true })); }}>Auto</button>
              <button aria-pressed={!s.auto} title="Only Run starts a card" onClick={() => { if (s.auto) run(setColumn(s.id, { auto: false })); }}>Manual</button>
            </div>
          )}
          {(takes || s.category === "review") && (
            <label className="wf-next">
              <span>{s.category === "review" ? "Merged cards go to" : "Next"}</span>
              <select className="select" aria-label={s.category === "review" ? `Merged cards from ${s.name} go to` : `Next column after ${s.name}`} value={s.nextStateId ?? ""}
                onChange={(e) => run(setColumn(s.id, { nextStateId: e.target.value }))}>
                <option value="">{s.category === "review" ? `Not set (${merged?.name ?? "Done"})` : "None"}</option>
                {order.map((x) => <option key={x.id} value={x.id}>{x.name}{x.id === s.id ? " (this column)" : ""}</option>)}
              </select>
            </label>
          )}
        </div>
        <span className="wf-bin" title={blocked ?? `Remove ${s.name}`}>
          <button className="btn ghost sm icon-only" aria-label={`Remove ${s.name}`} disabled={!!blocked || !!asking} onClick={openBin}><Trash2 className="icon" /></button>
        </span>
      </div>
      <div className="wf-line">{columnLine(s, states, members)}</div>
      {error && <div className="wf-err" role="alert">{error}</div>}
      {asking && <RemoveConfirm s={s} states={states} members={members} info={asking} onClose={() => setAsking(null)} onError={fail} />}
    </div>
  );
}

/** Add column: a name, a kind in plain words and the column it goes after. It starts Manual, without agents or a next column. */
function AddColumn({ team, onDone }: { team: Team; onDone: () => void }) {
  const states = boardOrder(team.states);
  const [kind, setKind] = useState("work");
  const [name, setName] = useState("");
  const [after, setAfter] = useState(() => {
    const open = states.filter((s) => s.category !== "done" && s.category !== "cancelled");
    return (states.find((s) => s.category === "review") ?? open[open.length - 1] ?? states[states.length - 1])?.id ?? "";
  });
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const usual = KINDS.find((k) => k.kind === kind)?.name ?? "";
  const add = async () => {
    setBusy(true); setErr(null);
    try { await addState(team.id, name.trim() || usual, after, kind); onDone(); } catch (e) { setErr(String(e)); setBusy(false); }
  };
  return (
    <div className="panel col-add" role="group" aria-label="Add column">
      <div className="rule-add">
        <input className="input" aria-label="Column name" autoFocus style={{ width: 170 }} value={name} placeholder={usual}
          onChange={(e) => setName(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !busy) add(); if (e.key === "Escape") onDone(); }} />
        <span>a</span>
        <select className="select" aria-label="Kind of column" value={kind} onChange={(e) => setKind(e.target.value)}>
          {KINDS.map((k) => <option key={k.kind} value={k.kind}>{k.label}</option>)}
        </select>
        <span>column, after</span>
        <select className="select" aria-label="After column" value={after} onChange={(e) => setAfter(e.target.value)}>
          {states.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
        </select>
        <span className="grow" />
        <button className="btn ghost" onClick={onDone}>Cancel</button>
        <button className="btn primary" disabled={busy || !after} onClick={add}><Plus className="icon" />Add column</button>
      </div>
      {err && <div className="col-add-err" role="alert">{err}</div>}
    </div>
  );
}

export function ColumnEditor({ team, order, error, onError, overId, adding, onAddingDone }: {
  team: Team;
  /** The column ids in board order (the Team page keeps it, so a dragged column stays put while it saves). */
  order: string[];
  error: ColumnError; onError: (e: ColumnError) => void;
  /** The column an agent card is dragged over now. */
  overId: string | null;
  adding: boolean; onAddingDone: () => void;
}) {
  const byId = new Map(team.states.map((s) => [s.id, s]));
  const states = order.map((id) => byId.get(id)).filter((s): s is WorkflowState => !!s);
  const agents = team.members.filter((m) => m.kind === "agent" && m.status !== "archived").sort((a, b) => a.name.localeCompare(b.name));
  // Why each bin is off (a card an agent works on, the last Backlog or Done column), asked again after every change.
  const [removals, setRemovals] = useState<Record<string, ColumnRemoval>>({});
  const live = useLiveRuns().map((r) => r.runId).join(",");
  const key = team.states.map((s) => `${s.id}:${s.category}:${s.nextStateId ?? ""}`).join("|");
  useEffect(() => {
    let alive = true;
    const ids = key.split("|").map((k) => k.split(":")[0]).filter(Boolean);
    Promise.all(ids.map((id) => columnRemoval(id).then((r) => [id, r] as const).catch(() => null)))
      .then((rs) => { if (alive) setRemovals(Object.fromEntries(rs.filter((r): r is readonly [string, ColumnRemoval] => !!r))); });
    return () => { alive = false; };
  }, [key, live]);
  return (
    <>
      <SortableContext items={order} strategy={verticalListSortingStrategy}>
        <div className="wf" role="list" aria-label="Columns">
          {states.map((s) => (
            <div role="listitem" key={s.id}>
              <ColumnRow s={s} states={states} members={team.members} agents={agents} removal={removals[s.id]} dropping={overId === s.id}
                error={error?.id === s.id ? error.text : null} onError={(text) => onError(text ? { id: s.id, text } : error?.id === s.id ? null : error)} />
            </div>
          ))}
        </div>
      </SortableContext>
      {adding && <AddColumn key={team.id} team={team} onDone={onAddingDone} />}
    </>
  );
}
