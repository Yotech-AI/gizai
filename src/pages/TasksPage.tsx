// Tasks (and the Inbox): Paperclip-style list grouped by column, or the board. The Inbox is always the list. View options are remembered on this device.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowUpDown, Check, Columns3, Layers, List, ListFilter, Plus, Trash2, X } from "lucide-react";
import { dismissChat, getTeam, listChatThreads, listProjects, listTasks, listUsers, moveTask, restoreTask, archiveTask } from "../api";
import { go, href } from "../router";
import { useData } from "../lib/useData";
import { useLiveRuns } from "../lib/useLiveRuns";
import { chatLabel, needsYou, waitingChats } from "../lib/inbox";
import { relTime } from "../lib/format";
import type { ChatThread } from "../types";
import { filterCount, filterTasks, groupTasks, sortTasks, type Filter, type GroupBy, type SortBy } from "../lib/taskView";
import type { Task } from "../types";
import { Board } from "../components/Board";
import { TaskList } from "../components/TaskList";
import { Popover } from "../components/Popover";
import { ArchivedCards } from "../components/ArchivedCards";

/** A message at the bottom right; with `undo`, an Undo button (after archiving a card). */
type Toast = { text: string; undo?: () => void };

type View = "list" | "board";
function readPref<T>(k: string, fallback: T): T { try { const v = localStorage.getItem(k); return v === null ? fallback : (JSON.parse(v) as T); } catch { return fallback; } }
function writePref(k: string, v: unknown) { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* private mode */ } }
function usePref<T>(k: string, fallback: T): [T, (v: T) => void] {
  const [v, setV] = useState<T>(() => readPref(k, fallback));
  return [v, (n: T) => { setV(n); writePref(k, n); }];
}

/** The top of the Inbox: the chats the Team Lead started that wait for you. A click opens the chat; × dismisses it. */
function LeadChats({ chats, onDismiss }: { chats: ChatThread[]; onDismiss: (id: string) => void }) {
  return (
    <section className="lead-chats" aria-label="From the Team Lead">
      <div className="section-head"><h3>From the Team Lead</h3><span className="faint">{chats.length}</span></div>
      <div className="panel">
        {chats.map((c) => (
          <div key={c.id} className="panel-row lead-chat">
            <a className="grow ellipsis" href={href({ page: "chat", id: c.id })}><span className="badge needs">{chatLabel(c)}</span> {c.title}</a>
            {(c.tasks ?? []).map((t) => <span key={t} className="id">{t}</span>)}
            <span className="faint">{relTime(c.updatedAt)}</span>
            <button className="btn ghost sm icon-only" aria-label={`Dismiss ${c.title}`} title="Dismiss" onClick={() => onDismiss(c.id)}><X className="icon" /></button>
          </div>
        ))}
      </div>
    </section>
  );
}

const SORTS: [SortBy, string][] =[["updated", "Last updated"], ["priority", "Priority"], ["title", "Title"], ["id", "ID"], ["manual", "Board order"]];
const GROUPS: [GroupBy, string][] = [["status", "Column"], ["assignee", "Assignee"], ["project", "Project"], ["priority", "Priority"], ["none", "No grouping"]];

export function TasksPage({ initialView, onNewTask, inboxFor }: { initialView?: View; onNewTask: (stateId?: string) => void; inboxFor?: string }) {
  const inbox = inboxFor !== undefined;
  // The board is the default view (2026-10-07); the key changed so an earlier "list" choice doesn't hide that.
  // The Inbox has no board (GA-2): it lists what needs you, so a board of columns only confuses.
  const [pickedView, setView] = usePref<View>("gizai-tasks-view", initialView ?? "board");
  const view: View = inbox ? "list" : pickedView;
  const [sort, setSort] = usePref<SortBy>("gizai-task-sort", "updated");
  const [group, setGroup] = usePref<GroupBy>("gizai-task-group", "status");
  const [filter, setFilterState] = useState<Filter>(() => ({ ...readPref<Filter>("gizai-task-filter", {}), projectId: (() => { try { return localStorage.getItem("gizai-task-project"); } catch { return null; } })() }));
  const setFilter = (f: Filter) => {
    setFilterState(f);
    writePref("gizai-task-filter", { ...f, projectId: undefined, text: undefined });
    try { if (f.projectId) localStorage.setItem("gizai-task-project", f.projectId); else localStorage.removeItem("gizai-task-project"); } catch { /* ignore */ }
  };
  const { data: team, error: teamErr } = useData(() => getTeam());
  const { data: projects } = useData(() => listProjects());
  const { data: people } = useData(() => listUsers());
  const { data: fetched, error } = useData(() => listTasks({}));
  const { data: threads, reload: reloadThreads } = useData(() => (inbox ? listChatThreads() : Promise.resolve([] as ChatThread[])), [inbox]);
  const chats = useMemo(() => (inbox ? waitingChats(threads ?? []) : []), [inbox, threads]);
  const dismiss = (id: string) => dismissChat(id).then(reloadThreads).catch((e) => setToast({ text: `Couldn't dismiss the chat: ${e}` }));
  const [tasks, setTasks] = useState<Task[]>([]);
  useEffect(() => { if (fetched) setTasks(fetched); }, [fetched]);
  const tasksRef = useRef(tasks);
  tasksRef.current = tasks;
  const [toast, setToast] = useState<Toast | null>(null);
  useEffect(() => { if (!toast) return; const t = setTimeout(() => setToast(null), toast.undo ? 8000 : 5000); return () => clearTimeout(t); }, [toast]);
  const [bin, setBin] = useState(false);
  useEffect(() => { if (projects && filter.projectId && !projects.some((p) => p.id === filter.projectId)) setFilter({ ...filter, projectId: null }); }, [projects]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!inbox && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "b") { e.preventDefault(); setView(view === "list" ? "board" : "list"); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const states = useMemo(() => [...(team?.states ?? [])].sort((a, b) => (a.sortKey < b.sortKey ? -1 : 1)), [team]);
  const live = useLiveRuns();
  const working = useMemo(() => new Map(live.map((r) => [r.taskId, team?.members.find((m) => m.actorId === r.agentId)?.name ?? "An agent"])), [live, team]);
  const shown = useMemo(() => {
    const base = inbox ? tasks.filter((t) => needsYou(t, inboxFor!)) : tasks;
    return sortTasks(filterTasks(base, filter), sort);
  }, [tasks, filter, sort, inbox, inboxFor]);
  const groups = useMemo(() => groupTasks(shown, group, states), [shown, group, states]);

  const onMove = useCallback(async (id: string, stateId: string, sortKey: string) => {
    const before = tasksRef.current;
    const st = states.find((s) => s.id === stateId);
    setTasks((ts) => ts.map((t) => t.id === id ? { ...t, stateId, sortKey, stateName: st?.name ?? t.stateName, stateCategory: st?.category ?? t.stateCategory } : t));
    try { await moveTask(id, stateId, sortKey); }
    catch (e) { setTasks(before); setToast({ text: `Couldn't move ${before.find((x) => x.id === id)?.identifier ?? "the task"}: ${e}` }); }
  }, [states]);

  // Archive (a card in Done): it leaves the board at once; Undo in the toast restores it to the bottom of Done.
  const onArchive = useCallback(async (t: Task) => {
    const before = tasksRef.current;
    setTasks((ts) => ts.filter((x) => x.id !== t.id));
    try {
      await archiveTask(t.id);
      const undo = () => {
        setToast(null);
        restoreTask(t.id).catch((e) => setToast({ text: `Couldn't restore ${t.identifier}: ${e}` }));
      };
      setToast({ text: `${t.identifier} archived`, undo });
    } catch (e) { setTasks(before); setToast({ text: `Couldn't archive ${t.identifier}: ${e}` }); }
  }, []);

  const project = projects?.find((p) => p.id === filter.projectId);
  const nFilters = filterCount(filter);
  const agents = (team?.members ?? []).filter((m) => m.kind === "agent");
  const noProjects = projects !== null && projects.length === 0;

  return (
    <>
      <div className="topbar">
        <div className="crumbs">
          {project && <><a href={`#/project/${project.id}`}>{project.name}</a><span className="sep">/</span></>}
          <b>{inbox ? "Inbox" : "Tasks"}</b><span className="faint">{fetched ? shown.length + chats.length : ""}</span>
        </div>
      </div>
      <div className="toolbar">
        <button className="btn" onClick={() => onNewTask()}><Plus className="icon" />New task</button>
        <input className="input search-input" type="search" aria-label="Search tasks" placeholder="Search ID or title" value={filter.text ?? ""} onChange={(e) => setFilter({ ...filter, text: e.target.value })} />
        <span className="spacer" />
        {!inbox && <div className="seg" role="group" aria-label="View">
          <button aria-pressed={view === "board"} aria-label="Board" title="Board (Ctrl+B)" onClick={() => setView("board")}><Columns3 className="icon" /></button>
          <button aria-pressed={view === "list"} aria-label="List" title="List (Ctrl+B)" onClick={() => setView("list")}><List className="icon" /></button>
        </div>}
        <Popover label="Filters" align="right" button={() => <button className={`btn ghost${nFilters ? " on" : ""}`}><ListFilter className="icon" />{nFilters ? `Filters: ${nFilters}` : "Filters"}</button>}>
          {() => (<>
            <div className="pop-label">Project</div>
            <button className="opt" onClick={() => setFilter({ ...filter, projectId: null })}>All projects{!filter.projectId && <Check className="icon tick" />}</button>
            {(projects ?? []).map((p) => <button key={p.id} className="opt" onClick={() => setFilter({ ...filter, projectId: p.id })}>
              <span className="dot" style={{ width: 9, height: 9, borderRadius: "50%", background: p.color ?? "var(--text-3)" }} />{p.name}{filter.projectId === p.id && <Check className="icon tick" />}</button>)}
            <div className="sep" /><div className="pop-label">Labels</div>
            {(team?.labels ?? []).map((l) => { const on = filter.labelIds?.includes(l.id); return (
              <button key={l.id} className="opt" onClick={() => setFilter({ ...filter, labelIds: on ? filter.labelIds!.filter((x) => x !== l.id) : [...(filter.labelIds ?? []), l.id] })}>
                <span className="dot" style={{ width: 8, height: 8, borderRadius: "50%", background: l.color ?? "var(--text-3)" }} />{l.name}{on && <Check className="icon tick" />}</button>); })}
            <div className="sep" /><div className="pop-label">Assignee</div>
            <button className="opt" onClick={() => setFilter({ ...filter, assigneeId: null })}>Anyone{!filter.assigneeId && <Check className="icon tick" />}</button>
            <button className="opt" onClick={() => setFilter({ ...filter, assigneeId: "none" })}>Unassigned{filter.assigneeId === "none" && <Check className="icon tick" />}</button>
            {[...(people ?? []).map((p) => ({ id: p.id, name: p.name })), ...agents.map((a) => ({ id: a.actorId, name: a.name }))].map((a) =>
              <button key={a.id} className="opt" onClick={() => setFilter({ ...filter, assigneeId: a.id })}>{a.name}{filter.assigneeId === a.id && <Check className="icon tick" />}</button>)}
          </>)}
        </Popover>
        {nFilters > 0 && <button className="btn ghost sm icon-only" aria-label="Clear filters" title="Clear filters" onClick={() => setFilter({})}><X className="icon" /></button>}
        <Popover label="Sort" align="right" button={() => <button className="btn ghost"><ArrowUpDown className="icon" />Sort</button>}>
          {(close) => SORTS.map(([k, l]) => <button key={k} className="opt" onClick={() => { setSort(k); close(); }}>{l}{sort === k && <Check className="icon tick" />}</button>)}
        </Popover>
        {view === "list" && <Popover label="Group" align="right" button={() => <button className="btn ghost"><Layers className="icon" />Group</button>}>
          {(close) => GROUPS.map(([k, l]) => <button key={k} className="opt" onClick={() => { setGroup(k); close(); }}>{l}{group === k && <Check className="icon tick" />}</button>)}
        </Popover>}
        {!inbox && <button className="btn ghost icon-only" aria-label="Archived cards" title="Archived cards" onClick={() => setBin(true)}><Trash2 className="icon" /></button>}
      </div>
      {(error || teamErr) && <div className="error-banner">{error ?? teamErr}</div>}
      {noProjects ? (
        <div className="content"><div className="page-pad"><div className="empty"><b>No projects yet.</b><span>Tasks live in a project. Create one, then press N to add a task.</span><button className="btn primary" onClick={() => go({ page: "projects" })}>Go to Projects</button></div></div></div>
      ) : !team || !fetched ? null : view === "list" ? (
        <div className="content">
          {chats.length > 0 && <div className="page-pad lead-chats-pad"><LeadChats chats={chats} onDismiss={dismiss} /></div>}
          <TaskList key={`${group}-${inbox}`} groups={groups} showHeads={group !== "none"} live={working} onAdd={group === "status" && !inbox ? (key) => onNewTask(key) : undefined}
            empty={inbox ? (chats.length > 0 ? null : <div className="empty"><b>Nothing needs you.</b><span>Cards on hold, cards waiting for your review or deploy, and the Team Lead's questions show up here.</span></div>)
              : <div className="empty"><b>No tasks here.</b><span>{nFilters ? "Nothing matches these filters." : "Press N to add one."}</span></div>} />
        </div>
      ) : (
        <div className="board-wrap">
          {chats.length > 0 && <LeadChats chats={chats} onDismiss={dismiss} />}
          {states.length > 0 && <Board tasks={shown} states={states} onMove={onMove} onOpen={(id) => go({ page: "task", id })} onAdd={(sid) => onNewTask(sid)}
            onArchive={onArchive} working={working} members={team.members} />}
        </div>
      )}
      {bin && <ArchivedCards projectId={filter.projectId} projectName={project?.name} onClose={() => setBin(false)} />}
      {toast && <div className="toast" role="status">{toast.text}{toast.undo && <button className="btn sm" onClick={toast.undo}>Undo</button>}</div>}
    </>
  );
}
