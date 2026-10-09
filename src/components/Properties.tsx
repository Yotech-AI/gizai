// The task's properties panel (design system: PropertiesPanel). Values are popover pickers; changes save at once.
import { Check, X } from "lucide-react";
import { listTasks, moveTask, setTaskLabels, updateTask } from "../api";
import { href } from "../router";
import { relTime, textEnd } from "../lib/format";
import { keyBetween } from "../lib/sortKey";
import type { Person, Task, Team } from "../types";
import { PRIORITY_NAMES, PriorityIcon, StatusIcon } from "./StatusIcon";
import { Avatar } from "./Avatar";
import { Popover } from "./Popover";
import { NewLabel } from "./NewLabel";

const HOLD_NAMES: Record<string, string> = {
  needs_decision: "Needs your decision", stalled: "Stalled", merge_conflict: "Merge conflict",
  waiting_approval: "Waiting for approval", rate_limited: "Rate limited", blocked: "Blocked",
};
const date = (ms: number) => new Date(ms).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
/** A hold reason longer than this shows its end: about two lines of the value column (some 30 characters a line). */
const REASON_CHARS = 60;

/** `readOnly` (an archived card): the values show, but nothing can be changed. `onReadComments`: the Hold row's "Read in
 *  comments" under a shortened reason. */
export function Properties({ task, team, people, onError, onClose, onReadComments, readOnly }: {
  task: Task; team: Team; people: Person[]; onError: (msg: string) => void; onClose: () => void; onReadComments?: () => void; readOnly?: boolean;
}) {
  const run = (p: Promise<unknown>) => p.catch((e) => onError(String(e)));
  // A value that opens its picker, or only shows when the card is read-only.
  const pick = (label: string, cls: string, value: React.ReactNode, menu: (close: () => void) => React.ReactNode, style?: React.CSSProperties) => readOnly
    ? <span className={`v${cls}`} style={style}>{value}</span>
    : <Popover label={label} button={() => <span className={`v editable${cls}`} role="button" tabIndex={0} style={style}>{value}</span>}>{menu}</Popover>;
  const agents = team.members.filter((m) => m.kind === "agent");
  const states = [...team.states].sort((a, b) => (a.sortKey < b.sortKey ? -1 : 1));
  const moveToColumn = async (stateId: string) => {
    const others = (await listTasks({ projectId: task.projectId })).filter((t) => t.stateId === stateId && t.id !== task.id);
    await moveTask(task.id, stateId, keyBetween(others.map((t) => t.sortKey).sort().pop() ?? null, null));
  };
  const toggleLabel = (id: string) => {
    const ids = task.labels.map((l) => l.id);
    run(setTaskLabels(task.id, ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id]));
  };
  const row = (k: string, v: React.ReactNode) => <div className="prop-row"><span className="k">{k}</span>{v}</div>;
  const testing = task.testing !== false; // on unless the card says off
  // A long hold reason is usually the agent's whole summary, which the comments show: only its end (the questions) here.
  const reasonEnd = task.holdReason ? textEnd(task.holdReason, REASON_CHARS) : null;
  return (
    <aside className="props" aria-label="Properties">
      <div className="props-head">Properties<button className="btn ghost sm icon-only" aria-label="Close properties" title="Close properties (])" onClick={onClose}><X className="icon" /></button></div>
      <div className="props-body">
        {row("Status", pick("Status", "", <><StatusIcon category={task.stateCategory} />{task.stateName}</>,
          (close) => states.map((s) => <button key={s.id} className="opt" onClick={() => { close(); run(moveToColumn(s.id)); }}><StatusIcon category={s.category} />{s.name}{s.id === task.stateId && <Check className="icon tick" />}</button>)))}
        {row("Priority", pick("Priority", "", <><PriorityIcon priority={task.priority} />{PRIORITY_NAMES[task.priority]}</>,
          (close) => [1, 2, 3, 4, 0].map((p) => <button key={p} className="opt" onClick={() => { close(); run(updateTask(task.id, { priority: p })); }}><PriorityIcon priority={p} />{PRIORITY_NAMES[p]}{p === task.priority && <Check className="icon tick" />}</button>)))}
        {row("Testing", <label className={`v${readOnly ? "" : " editable"} check`} title={testing ? "On: the QA Agent tests the card before Review" : "Off: the card skips Testing and goes straight to Review"}>
          <input type="checkbox" checked={testing} disabled={readOnly} onChange={(e) => run(updateTask(task.id, { testing: e.target.checked }))} />Test before Review</label>)}
        {row("Labels", pick("Labels", task.labels.length ? "" : " none",
          task.labels.length ? task.labels.map((l) => <span key={l.id} className="label-pill"><span className="dot" style={{ background: l.color ?? "var(--text-3)" }} />{l.name}</span>) : "No labels",
          () => (<>
            {team.labels.map((l) => { const on = task.labels.some((x) => x.id === l.id); return (
              <button key={l.id} className="opt" onClick={() => toggleLabel(l.id)}><span className="dot" style={{ width: 8, height: 8, borderRadius: "50%", background: l.color ?? "var(--text-3)" }} />{l.name}{on && <Check className="icon tick" />}</button>); })}
            {team.labels.length > 0 && <div className="sep" />}
            <NewLabel labels={team.labels} onCreated={(l) => setTaskLabels(task.id, [...task.labels.map((x) => x.id), l.id])} />
          </>),
          { flexWrap: "wrap" }))}
        {row("Assignee", pick("Assignee", task.assigneeName ? "" : " none",
          task.assigneeName ? <><Avatar name={task.assigneeName} kind={task.assigneeKind} size="sm" />{task.assigneeName}</> : "Unassigned",
          (close) => (<>
            <button className="opt" onClick={() => { close(); run(updateTask(task.id, { assigneeId: "" })); }}>Unassigned{!task.assigneeId && <Check className="icon tick" />}</button>
            <div className="pop-label">People</div>
            {people.map((p) => <button key={p.id} className="opt" onClick={() => { close(); run(updateTask(task.id, { assigneeId: p.id })); }}><Avatar name={p.name} size="sm" />{p.name}{task.assigneeId === p.id && <Check className="icon tick" />}</button>)}
            {agents.length > 0 && <div className="pop-label">Agents</div>}
            {agents.map((a) => <button key={a.actorId} className="opt" onClick={() => { close(); run(updateTask(task.id, { assigneeId: a.actorId })); }}><Avatar name={a.name} kind="agent" size="sm" />{a.name}{task.assigneeId === a.actorId && <Check className="icon tick" />}</button>)}
          </>)))}
        {row("Project", task.projectId ? <a className="v editable" href={href({ page: "project", id: task.projectId })}><span className="dot" style={{ width: 9, height: 9, borderRadius: "50%", background: task.projectColor ?? "var(--text-3)" }} />{task.projectName}</a> : <span className="v none">None</span>)}
        {task.hold && row("Hold", <span className="v" style={{ flexDirection: "column", alignItems: "flex-start", gap: 6 }}>
          <span className="badge needs">{HOLD_NAMES[task.hold] ?? task.hold}</span>
          {task.holdReason && <span className="muted" style={{ fontSize: "var(--fs-sm)", overflowWrap: "anywhere" }} title={reasonEnd ? task.holdReason : undefined}>
            {reasonEnd ?? task.holdReason}</span>}
          {reasonEnd && onReadComments && <button className="link" style={{ color: "var(--accent)" }} onClick={onReadComments}>Read in comments</button>}
          {!readOnly && <button className="btn sm" onClick={() => run(updateTask(task.id, { hold: "" }))}>Clear hold</button>}</span>)}
        {row("Branch", task.branch ? <span className="v"><span className="id" style={{ color: "var(--text-2)", whiteSpace: "normal", wordBreak: "break-all" }}>{task.branch}</span></span> : <span className="v none">Set when an agent starts</span>)}
        <div className="props-sep" />
        {row("Created", <span className="v" title={new Date(task.createdAt).toLocaleString("en-GB")}>{date(task.createdAt)}</span>)}
        {row("Updated", <span className="v" title={new Date(task.updatedAt).toLocaleString("en-GB")}>{relTime(task.updatedAt)}</span>)}
        {task.archivedAt && row("Archived", <span className="v" title={new Date(task.archivedAt).toLocaleString("en-GB")}>
          {date(task.archivedAt)}{task.archivedBy ? ` by ${task.archivedBy}` : ""}</span>)}
      </div>
    </aside>
  );
}
