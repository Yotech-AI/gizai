// How the task list groups, sorts and filters (Paperclip-style view options), as pure functions.
import { fold } from "./palette";

export type ViewTask = {
  id: string; identifier: string; title: string; stateId: string; stateName: string; stateCategory: string; priority: number; sortKey: string;
  updatedAt: number; labels: { id: string; name: string; color?: string | null }[]; assigneeId?: string | null; assigneeName?: string | null;
  projectId?: string | null; projectName?: string | null; projectColor?: string | null; hold?: string | null;
};
export type GroupBy = "status" | "assignee" | "project" | "priority" | "none";
export type SortBy = "updated" | "priority" | "title" | "id" | "manual";
export type Filter = { projectId?: string | null; labelIds?: string[]; assigneeId?: string | null; text?: string };
export type Group<T> = { key: string; label: string; category?: string; color?: string | null; tasks: T[] };

const PRIORITY_ORDER = [1, 2, 3, 4, 0];
const PRIORITY_LABEL: Record<number, string> = { 0: "None", 1: "Urgent", 2: "High", 3: "Medium", 4: "Low" };

export function groupTasks<T extends ViewTask>(tasks: T[], by: GroupBy, states: { id: string; name: string; category: string }[]): Group<T>[] {
  if (by === "none") return [{ key: "all", label: "All tasks", tasks }];
  const groups = new Map<string, Group<T>>();
  const add = (key: string, init: () => Omit<Group<T>, "tasks">, task: T) => {
    if (!groups.has(key)) groups.set(key, { ...init(), tasks: [] });
    groups.get(key)!.tasks.push(task);
  };
  for (const task of tasks) {
    if (by === "status") {
      const st = states.find((x) => x.id === task.stateId);
      add(task.stateId, () => ({ key: task.stateId, label: st?.name ?? task.stateName, category: st?.category ?? task.stateCategory }), task);
    }
    else if (by === "assignee") add(task.assigneeId ?? "", () => ({ key: task.assigneeId ?? "", label: task.assigneeName ?? "Unassigned" }), task);
    else if (by === "project") add(task.projectId ?? "", () => ({ key: task.projectId ?? "", label: task.projectName ?? "No project", color: task.projectColor }), task);
    else add(String(task.priority), () => ({ key: String(task.priority), label: PRIORITY_LABEL[task.priority] ?? "None" }), task);
  }
  const list = [...groups.values()];
  if (by === "status") {
    const order = states.map((s) => s.id);
    return list.sort((a, b) => order.indexOf(a.key) - order.indexOf(b.key));
  }
  if (by === "priority") return list.sort((a, b) => PRIORITY_ORDER.indexOf(Number(a.key)) - PRIORITY_ORDER.indexOf(Number(b.key)));
  return list.sort((a, b) => (a.key === "" ? 1 : b.key === "" ? -1 : a.label.localeCompare(b.label)));
}

export function sortTasks<T extends ViewTask>(tasks: T[], by: SortBy): T[] {
  const out = [...tasks];
  const num = (id: string) => Number(id.split("-").pop()) || 0;
  switch (by) {
    case "updated": return out.sort((a, b) => b.updatedAt - a.updatedAt);
    case "priority": return out.sort((a, b) => PRIORITY_ORDER.indexOf(a.priority) - PRIORITY_ORDER.indexOf(b.priority) || b.updatedAt - a.updatedAt);
    case "title": return out.sort((a, b) => a.title.localeCompare(b.title));
    case "id": return out.sort((a, b) => a.identifier.split("-")[0].localeCompare(b.identifier.split("-")[0]) || num(a.identifier) - num(b.identifier));
    default: return out.sort((a, b) => (a.sortKey < b.sortKey ? -1 : a.sortKey > b.sortKey ? 1 : 0));
  }
}

export function filterTasks<T extends ViewTask>(tasks: T[], f: Filter): T[] {
  const q = f.text ? fold(f.text) : "";
  return tasks.filter((t) =>
    (!f.projectId || t.projectId === f.projectId) &&
    (!f.labelIds?.length || t.labels.some((l) => f.labelIds!.includes(l.id))) &&
    (!f.assigneeId || (f.assigneeId === "none" ? !t.assigneeId : t.assigneeId === f.assigneeId)) &&
    (!q || fold(`${t.identifier} ${t.title}`).includes(q)));
}

/** How many filter conditions are on (for the "Filters: N" button). */
export function filterCount(f: Filter): number {
  return (f.projectId ? 1 : 0) + (f.labelIds?.length ? 1 : 0) + (f.assigneeId ? 1 : 0) + (f.text?.trim() ? 1 : 0);
}
