import { useEffect, useMemo, useRef, useState } from "react";
import { listClients, listProjects, listTasks } from "../api";
import { go } from "../router";
import { searchItems, type PaletteItem } from "../lib/palette";
import { Building2, FolderKanban, ListTodo, Search, SquarePen, type LucideIcon } from "lucide-react";

const ICON: Record<PaletteItem["kind"], LucideIcon> = { task: ListTodo, project: FolderKanban, client: Building2, action: SquarePen };
const KIND: Record<PaletteItem["kind"], string> = { task: "Task", project: "Project", client: "Client", action: "Action" };

export function CommandPalette({ open, onClose, onNewTask }: { open: boolean; onClose: () => void; onNewTask: () => void }) {
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<PaletteItem[]>([]);
  const [sel, setSel] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setQuery(""); setSel(0);
    input.current?.focus();
    Promise.all([listTasks({ openOnly: false }), listProjects(), listClients()]).then(([t, p, c]) => {
      setItems([
        { kind: "action", id: "new-task", label: "New task", hint: "N" },
        { kind: "action", id: "chat", label: "Chat with the Team Lead", hint: "chat" },
        ...t.map((x) => ({ kind: "task" as const, id: x.id, label: x.title, hint: x.identifier })),
        ...p.map((x) => ({ kind: "project" as const, id: x.id, label: x.name, hint: `${x.number} ${x.key}` })),
        ...c.map((x) => ({ kind: "client" as const, id: x.id, label: x.name, hint: x.city ?? "" })),
      ]);
    }).catch(() => setItems([]));
  }, [open]);

  const results = useMemo(() => searchItems(query, items), [query, items]);
  if (!open) return null;

  const choose = (it: PaletteItem | undefined) => {
    if (!it) return;
    onClose();
    if (it.kind === "action" && it.id === "chat") go({ page: "chat" });
    else if (it.kind === "action") onNewTask();
    else go({ page: it.kind, id: it.id });
  };

  return (
    <>
      <div className="pal-backdrop" onClick={onClose} />
      <div className="pal" role="dialog" aria-label="Command palette">
        <div className="pal-in">
          <Search className="icon" />
          <input ref={input} value={query} placeholder="Type a task id, title, project or client…" aria-label="Search"
            onChange={(e) => { setQuery(e.target.value); setSel(0); }}
            onKeyDown={(e) => {
              if (e.key === "Escape") onClose();
              else if (e.key === "ArrowDown") { e.preventDefault(); setSel((s) => Math.min(s + 1, results.length - 1)); }
              else if (e.key === "ArrowUp") { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
              else if (e.key === "Enter") choose(results[sel]);
            }} />
        </div>
        <div className="pal-list">
          {results.length === 0 && <div className="pal-group">Nothing matches “{query}”</div>}
          {results.map((it, i) => {
            const I = ICON[it.kind];
            return (
              <button key={`${it.kind}-${it.id}`} className={`pal-item${i === sel ? " active" : ""}`} onMouseEnter={() => setSel(i)} onClick={() => choose(it)}>
                <I className="icon" />
                {it.kind === "task" && <span className="id">{it.hint}</span>}
                <span>{it.label}</span>
                {it.kind !== "task" && it.hint && <span className="faint">{it.hint}</span>}
                <span className="kind">{KIND[it.kind]}</span>
              </button>
            );
          })}
        </div>
      </div>
    </>
  );
}
