// Tasks (and the Inbox): Paperclip-style list grouped by column, or the board. View options are remembered on this device.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowUpDown, Check, Columns3, Layers, List, ListFilter, Plus, X } from "lucide-react";
import { getTeam, listProjects, listTasks, listUsers, moveTask } from "../api";
import { go } from "../router";
import { useData } from "../lib/useData";
import { useLiveRuns } from "../lib/useLiveRuns";
import { needsYou } from "../lib/inbox";
import { filterCount, filterTasks, groupTasks, sortTasks, type Filter, type GroupBy, type SortBy } from "../lib/taskView";
import type { Task } from "../types";
import { Board } from "../components/Board";
import { TaskList } from "../components/TaskList";
import { Popover } from "../components/Popover";

type View = "list" | "board";
function readPref<T>(k: string, fallback: T): T { try { const v = localStorage.getItem(k); return v === null ? fallback : (JSON.parse(v) as T); } catch { return fallback; } }
function writePref(k: string, v: unknown) { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* private mode */ } }
function usePref<T>(k: string, fallback: T): [T, (v: T) => void] {
  const [v, setV] = useState<T>(() => readPref(k, fallback));
  return [v, (n: T) => { setV(n); writePref(k, n); }];
}

const SORTS: [SortBy, string][] = [["updated", "Last updated"], ["priority", "Priority"], ["title", "Title"], ["id", "ID"], ["manual", "Board order"]];
const GROUPS: [GroupBy, string][] = [["status", "Column"], ["assignee", "Assignee"], ["project", "Project"], ["priority", "Priority"], ["none", "No grouping"]];

export function TasksPage({ initialView, onNewTask, inboxFor }: { initialView?: View; onNewTask: (stateId?: string) => void; inboxFor?: string }) {
  const inbox = inboxFor !== undefined;
  // The board is the default view (2026-10-07); the key changed so an earlier "list" choice doesn't hide that.
  const [view, setView] = usePref<View>(inbox ? "gizai-inbox-view" : "gizai-tasks-view", initialView ?? "board");
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
  const [tasks, setTasks] = useState<Task[]>([]);
  useEffect(() => { if (fetched) setTasks(fetched); }, [fetched]);
  const tasksRef = useRef(tasks);
  tasksRef.current = tasks;
  const [toast, setToast] = useState<string | null>(null);
  useEffect(() => { if (!toast) return; const t = setTimeout(() => setToast(null), 5000); return () => clearTimeout(t); }, [toast]);
  useEffect(() => { if (projects && filter.projectId && !projects.some((p) => p.id === filter.projectId)) setFilter({ ...filter, projectId: null }); }, [projects]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "b") { e.preventDefault(); setView(view === "list" ? "board" : "list"); }
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
    catch (e) { setTasks(before); setToast(`Couldn't move ${before.find((x) => x.id === id)?.identifier ?? "the task"}: ${e}`); }
  }, [states]);

  const project = projects?.find((p) => p.id === filter.projectId);
  const nFilters = filterCount(filter);
  const agents = (team?.members ?? []).filter((m) => m.kind === "agent");
  const noProjects = projects !== null && projects.length === 0;

  return (
    <>
      <div className="topbar">
        <div className="crumbs">
          {project && <><a href={`#/project/${project.id}`}>{project.name}</a><span className="sep">/</span></>}
          <b>{inbox ? "Inbox" : "Tasks"}</b><span className="faint">{fetched ? shown.length : ""}</span>
        </div>
      </div>
      <div className="toolbar">
        <button className="btn" onClick={() => onNewTask()}><Plus className="icon" />New task</button>
        <input className="input search-input" type="search" aria-label="Search tasks" placeholder="Search ID or title" value={filter.text ?? ""} onChange={(e) => setFilter({ ...filter, text: e.target.value })} />
        <span className="spacer" />
        <div className="seg" role="group" aria-label="View">
          <button aria-pressed={view === "board"} aria-label="Board" title="Board (Ctrl+B)" onClick={() => setView("board")}><Columns3 className="icon" /></button>
          <button aria-pressed={view === "list"} aria-label="List" title="List (Ctrl+B)" onClick={() => setView("list")}><List className="icon" /></button>
        </div>
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
      </div>
      {(error || teamErr) && <div className="error-banner">{error ?? teamErr}</div>}
      {noProjects ? (
        <div className="content"><div className="page-pad"><div className="empty"><b>No projects yet.</b><span>Tasks live in a project. Create one, then press N to add a task.</span><button className="btn primary" onClick={() => go({ page: "projects" })}>Go to Projects</button></div></div></div>
      ) : !team || !fetched ? null : view === "list" ? (
        <div className="content">
          <TaskList key={`${group}-${inbox}`} groups={groups} showHeads={group !== "none"} live={working} onAdd={group === "status" && !inbox ? (key) => onNewTask(key) : undefined}
            empty={inbox ? <div className="empty"><b>Nothing needs you.</b><span>Cards on hold and cards waiting for your review show up here.</span></div>
              : <div className="empty"><b>No tasks here.</b><span>{nFilters ? "Nothing matches these filters." : "Press N to add one."}</span></div>} />
        </div>
      ) : (
        <div className="board-wrap">
          {states.length > 0 && <Board tasks={shown} states={states} onMove={onMove} onOpen={(id) => go({ page: "task", id })} onAdd={(sid) => onNewTask(sid)} working={working} />}
        </div>
      )}
      {toast && <div className="toast" role="status">{toast}</div>}
    </>
  );
}
