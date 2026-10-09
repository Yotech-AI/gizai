// Kanban board. The drag logic is the one the WebKitGTK spike proved (2026-10-06): pointerWithin, then
// closestCenter inside the hovered column, at most one column change per frame. Backlog, Done and
// Cancelled are narrow rails (still drop targets; click to open). At most 50 cards render per column.
import { memo, useCallback, useEffect, useRef, useState } from "react";
import {
  DndContext, MeasuringStrategy, PointerSensor, closestCenter, getFirstCollision, pointerWithin, useDroppable, useSensor, useSensors,
  type CollisionDetection, type DragEndEvent, type DragOverEvent, type UniqueIdentifier,
} from "@dnd-kit/core";
import { SortableContext, arrayMove, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import type { Member, Task, WorkflowState } from "../types";
import { dropKey, groupByColumn } from "../lib/board";
import { boardNote } from "../lib/columns";
import { pullBadge, pullLabel } from "../lib/pulls";
import { PriorityIcon, StatusIcon } from "./StatusIcon";
import { Avatar } from "./Avatar";
import { Archive, Plus } from "lucide-react";

const CAP = 50;
const RAIL_CATEGORIES = new Set(["backlog", "done", "cancelled"]);

type Cols = Record<string, string[]>;

const Card = memo(function Card({ task, onOpen, onArchive, working }: {
  task: Task; onOpen: (id: string) => void; working?: string;
  /** Cards in a Done column: the Archive button, shown on hover and focus. */
  onArchive?: (task: Task) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: task.id });
  return (
    <div ref={setNodeRef} className={"card" + (isDragging ? " dragging" : "")} data-card={task.identifier}
      style={{ transform: CSS.Transform.toString(transform), transition }} {...attributes} {...listeners}
      onClick={() => onOpen(task.id)} onKeyDown={(e) => { if (e.key === "Enter") onOpen(task.id); }}>
      <div className="top"><span className="id">{task.identifier}</span>
        {task.prUrl && <span className={`badge ${pullBadge(task.prState).cls}`} title={`Pull request: ${pullBadge(task.prState).text.toLowerCase()} on GitHub`}>{pullLabel(task.prUrl)}</span>}
        {task.hold && <span className="badge needs">On hold</span>}{task.priority > 0 && <PriorityIcon priority={task.priority} />}
        {onArchive && (
          // Its own pointer and key events stay here: pressing it neither starts a drag nor opens the card.
          <button className="btn ghost sm icon-only card-archive" aria-label={`Archive ${task.identifier}`} title="Archive"
            onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}
            onClick={(e) => { e.stopPropagation(); onArchive(task); }}><Archive className="icon" /></button>
        )}</div>
      <div className="title">{task.title}</div>
      {working && <div className="working"><span className="pulse" aria-hidden />{working} is working</div>}
      {(task.labels.length > 0 || task.assigneeName) && (
        <div className="meta">
          {task.labels.map((l) => <span key={l.id} className="label-pill"><span className="dot" style={{ background: l.color ?? "var(--text-3)" }} />{l.name}</span>)}
          {task.assigneeName && <Avatar name={task.assigneeName} kind={task.assigneeKind} size="sm" />}
        </div>
      )}
    </div>
  );
});

function Column({ state, ids, byId, rail, onToggleRail, onOpen, onAdd, onArchive, working, members }: {
  state: WorkflowState; ids: string[]; byId: Map<string, Task>; rail: boolean; working: Map<string, string>; members?: Member[];
  onToggleRail: (id: string) => void; onOpen: (id: string) => void; onAdd?: (stateId: string) => void; onArchive?: (task: Task) => void;
}) {
  // Only a card in Done can be archived (not Deploy, not Cancelled).
  const archive = state.category === "done" ? onArchive : undefined;
  const { setNodeRef } = useDroppable({ id: state.id });
  const [shown, setShown] = useState(CAP);
  if (rail) {
    return (
      <div ref={setNodeRef} className="col rail" data-col={state.name} onClick={() => onToggleRail(state.id)} title={`Show ${state.name}`}>
        <div className="col-head"><StatusIcon category={state.category} /> <span className="name">{state.name}</span> <span className="n">{ids.length}</span></div>
      </div>
    );
  }
  const visible = ids.slice(0, shown);
  // The note follows the column: your review, or who starts its cards (its agents when Auto, Run when Manual).
  const note = boardNote(state, members);
  return (
    <div className="col" data-col={state.name}>
      <div className="col-head">
        <StatusIcon category={state.category} title={state.name} /><span className="name">{state.name}</span>
        <span className="n">{ids.length}{state.wipLimit ? ` / ${state.wipLimit}` : ""}</span>
        <span className="add">
          {RAIL_CATEGORIES.has(state.category) && <button className="btn ghost sm" onClick={() => onToggleRail(state.id)}>Collapse</button>}
          {onAdd && <button className="btn ghost sm icon-only" aria-label={`New task in ${state.name}`} title={`New task in ${state.name}`} onClick={(e) => { e.stopPropagation(); onAdd(state.id); }}><Plus className="icon" /></button>}
        </span>
      </div>
      {note && <div className="col-note">{note}</div>}
      <SortableContext id={state.id} items={visible} strategy={verticalListSortingStrategy}>
        <div ref={setNodeRef} className="col-body">
          {visible.map((id) => { const t = byId.get(id); return t ? <Card key={id} task={t} onOpen={onOpen} onArchive={archive} working={working.get(id)} /> : null; })}
          {ids.length > shown && (
            <button className="show-more" onClick={() => setShown((s) => s + CAP)}>Show {Math.min(CAP, ids.length - shown)} more of {ids.length - shown}</button>
          )}
          {ids.length === 0 && <div className="col-empty">No tasks</div>}
        </div>
      </SortableContext>
    </div>
  );
}

export function Board({ tasks, states, onMove, onOpen, onAdd, onArchive, working = new Map(), members }: {
  tasks: Task[]; states: WorkflowState[];
  /** The team's members, so an Auto column's note names its agents. */
  members?: Member[];
  /** task id → name of the agent working on it now */
  working?: Map<string, string>;
  onMove: (taskId: string, stateId: string, sortKey: string) => void;
  onOpen: (taskId: string) => void;
  onAdd?: (stateId: string) => void;
  /** Archive a card in a Done column (its button shows on hover and focus). */
  onArchive?: (task: Task) => void;
}) {
  const stateIds = states.map((s) => s.id);
  const [cols, setCols] = useState<Cols>(() => groupByColumn(tasks, stateIds));
  const colsRef = useRef(cols);
  colsRef.current = cols;
  const dragging = useRef<{ id: string; from: string; index: number } | null>(null);
  const [openRails, setOpenRails] = useState<Set<string>>(new Set());
  const byId = new Map(tasks.map((t) => [t.id, t]));
  const byIdRef = useRef(byId);
  byIdRef.current = byId;

  // Follow the data unless a card is in the air.
  const stateKey = stateIds.join(",");
  useEffect(() => {
    if (!dragging.current) setCols(groupByColumn(tasks, stateKey.split(",")));
  }, [tasks, stateKey]);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 5 } }));
  const findCol = (id: string) => (id in colsRef.current ? id : Object.keys(colsRef.current).find((k) => colsRef.current[k].includes(id)));
  const isRail = (stateId: string) => {
    const s = states.find((x) => x.id === stateId);
    return !!s && RAIL_CATEGORIES.has(s.category) && !openRails.has(stateId);
  };
  const isRailRef = useRef(isRail);
  isRailRef.current = isRail;

  const lastOverId = useRef<UniqueIdentifier | null>(null);
  const recentlyMoved = useRef(false);
  const collision: CollisionDetection = useCallback((args) => {
    const hits = pointerWithin(args);
    let overId = getFirstCollision(hits, "id");
    if (overId != null) {
      const items = colsRef.current[String(overId)];
      if (items && items.length > 0 && !isRailRef.current(String(overId))) {
        const inCol = new Set(items);
        overId = closestCenter({ ...args, droppableContainers: args.droppableContainers.filter((c) => inCol.has(String(c.id))) })[0]?.id ?? overId;
      }
      lastOverId.current = overId;
      return [{ id: overId }];
    }
    if (recentlyMoved.current && dragging.current) lastOverId.current = dragging.current.id;
    return lastOverId.current ? [{ id: lastOverId.current }] : [];
  }, []);
  useEffect(() => { requestAnimationFrame(() => { recentlyMoved.current = false; }); }, [cols]);

  const onDragOver = ({ active, over }: DragOverEvent) => {
    if (!over || over.id === active.id) return;
    const from = findCol(String(active.id)), to = findCol(String(over.id));
    if (!from || !to || from === to || recentlyMoved.current) return;
    recentlyMoved.current = true;
    setCols((prev) => {
      const a = prev[from].filter((x) => x !== active.id);
      const overIdx = prev[to].indexOf(String(over.id));
      const b = [...prev[to]];
      b.splice(isRailRef.current(to) ? 0 : overIdx >= 0 ? overIdx : b.length, 0, String(active.id));
      return { ...prev, [from]: a, [to]: b };
    });
  };

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    const start = dragging.current;
    dragging.current = null;
    if (!start) return;
    const id = String(active.id);
    let next = colsRef.current;
    const to = findCol(id);
    if (over && to) {
      const overCol = findCol(String(over.id));
      if (overCol === to) {
        const oi = next[to].indexOf(id), ni = next[to].indexOf(String(over.id));
        if (ni >= 0 && oi !== ni) next = { ...next, [to]: arrayMove(next[to], oi, ni) };
      }
    }
    if (!to) { setCols(groupByColumn(tasks, stateIds)); return; }
    setCols(next);
    const index = next[to].indexOf(id);
    if (to === start.from && index === start.index) return;
    const key = dropKey(next[to], id, (x) => byIdRef.current.get(x)?.sortKey ?? "a0");
    onMove(id, to, key);
  };

  const toggleRail = useCallback((id: string) => setOpenRails((s) => { const n = new Set(s); n.has(id) ? n.delete(id) : n.add(id); return n; }), []);

  return (
    <DndContext sensors={sensors} collisionDetection={collision} measuring={{ droppable: { strategy: MeasuringStrategy.Always } }}
      onDragStart={({ active }) => {
        const from = findCol(String(active.id)) ?? "";
        dragging.current = { id: String(active.id), from, index: colsRef.current[from]?.indexOf(String(active.id)) ?? -1 };
      }}
      onDragCancel={() => { dragging.current = null; setCols(groupByColumn(tasks, stateIds)); }}
      onDragOver={onDragOver}
      onDragEnd={onDragEnd}>
      <div className="board">
        {states.map((s) => (
          <Column key={s.id} state={s} ids={cols[s.id] ?? []} byId={byId} rail={isRail(s.id)} onToggleRail={toggleRail} onOpen={onOpen} onAdd={onAdd} onArchive={onArchive} working={working} members={members} />
        ))}
      </div>
    </DndContext>
  );
}
