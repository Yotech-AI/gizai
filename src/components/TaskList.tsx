// The task list (design system: TaskList): rows grouped by column, Paperclip-style. J/K move, Enter opens.
import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, Plus, User } from "lucide-react";
import type { Task } from "../types";
import type { Group } from "../lib/taskView";
import { href, go } from "../router";
import { relTime } from "../lib/format";
import { StatusIcon } from "./StatusIcon";
import { Avatar } from "./Avatar";

const WEEK = 7 * 24 * 3600 * 1000;
export function shortDate(ms: number, now = Date.now()): string {
  return now - ms < WEEK ? relTime(ms, now) : new Date(ms).toLocaleDateString("en-GB", { month: "short", day: "numeric" });
}

export function TaskList({ groups, showHeads, live, onAdd, empty }: {
  groups: Group<Task>[]; showHeads: boolean; live: Map<string, string>; onAdd?: (groupKey: string) => void; empty?: React.ReactNode;
}) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  // Done and Cancelled start collapsed, once the groups are known (tasks can arrive after the first render).
  const seeded = useRef(false);
  useEffect(() => {
    if (seeded.current || groups.length === 0) return;
    seeded.current = true;
    setCollapsed(new Set(groups.filter((g) => g.category === "done" || g.category === "cancelled").map((g) => g.key)));
  }, [groups]);
  const [focus, setFocus] = useState<string | null>(null);
  const visible = useMemo(() => groups.flatMap((g) => (showHeads && collapsed.has(g.key) ? [] : g.tasks)), [groups, collapsed, showHeads]);
  const live_ = useRef({ visible, focus });
  live_.current = { visible, focus };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, select, [contenteditable], .cm-editor, [role=dialog], [role=menu]") || e.ctrlKey || e.metaKey || e.altKey) return;
      const { visible, focus } = live_.current;
      const i = visible.findIndex((x) => x.id === focus);
      if (e.key === "j" || e.key === "k") {
        e.preventDefault();
        const next = visible[Math.max(0, Math.min(visible.length - 1, i + (e.key === "j" ? 1 : -1)))] ?? visible[0];
        if (next) { setFocus(next.id); document.getElementById(`row-${next.id}`)?.scrollIntoView({ block: "nearest" }); }
      } else if (e.key === "Enter" && focus) { e.preventDefault(); go({ page: "task", id: focus }); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  if (groups.every((g) => g.tasks.length === 0)) return <div className="page-pad">{empty}</div>;
  const toggle = (key: string) => setCollapsed((s) => { const n = new Set(s); n.has(key) ? n.delete(key) : n.add(key); return n; });
  return (
    <div className="task-list">
      {groups.map((g) => (
        <div key={g.key} role="group" aria-label={g.label}>
          {showHeads && (
            <div className="group-head" aria-expanded={!collapsed.has(g.key)} onClick={() => toggle(g.key)}>
              <ChevronDown className="icon sm chev" />
              {g.category ? <StatusIcon category={g.category} /> : g.color ? <span className="dot" style={{ width: 9, height: 9, borderRadius: "50%", background: g.color }} /> : null}
              <span className="name">{g.label}</span><span className="n">{g.tasks.length}</span>
              {onAdd && g.category && <span className="add"><button className="btn ghost sm icon-only" aria-label={`New task in ${g.label}`} title={`New task in ${g.label}`}
                onClick={(e) => { e.stopPropagation(); onAdd(g.key); }}><Plus className="icon" /></button></span>}
            </div>
          )}
          {!(showHeads && collapsed.has(g.key)) && g.tasks.map((t) => (
            <a key={t.id} id={`row-${t.id}`} className={`task-row${focus === t.id ? " focus" : ""}`} href={href({ page: "task", id: t.id })} onMouseEnter={() => setFocus(t.id)}>
              <StatusIcon category={t.stateCategory} hold={t.hold} />
              <span className="id">{t.identifier}</span>
              <span className="title"><span>{t.title}</span></span>
              <span className="labels">{t.labels.map((l) => <span key={l.id} className="label-pill"><span className="dot" style={{ background: l.color ?? "var(--text-3)" }} />{l.name}</span>)}</span>
              {t.assigneeName ? <span className="who"><Avatar name={t.assigneeName} kind={t.assigneeKind} size="sm" /><span className="ellipsis">{t.assigneeName}</span></span>
                : <span className="who none"><User className="icon sm" />Assignee</span>}
              <span>{live.has(t.id) ? <span className="badge live" title={`${live.get(t.id)} is working`}><span className="pulse" />Live</span> : t.hold ? <span className="badge needs">On hold</span> : null}</span>
              <span className="date">{shortDate(t.updatedAt)}</span>
            </a>
          ))}
        </div>
      ))}
    </div>
  );
}
