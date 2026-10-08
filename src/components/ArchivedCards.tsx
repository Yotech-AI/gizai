// The bin (GA-43): the archived cards, the most recently archived first, with when and by whom. Search by ID or title;
// the Tasks page's project filter applies. A card opens its task page, read-only, with Restore.
import { useMemo, useState } from "react";
import { listArchivedTasks } from "../api";
import { href } from "../router";
import { useData } from "../lib/useData";
import { relTime } from "../lib/format";
import { filterTasks } from "../lib/taskView";
import { Drawer } from "./Drawer";

export function ArchivedCards({ projectId, projectName, onClose }: { projectId?: string | null; projectName?: string | null; onClose: () => void }) {
  const { data, error } = useData(() => listArchivedTasks(projectId ?? null), [projectId]);
  const [q, setQ] = useState("");
  const shown = useMemo(() => filterTasks(data ?? [], { text: q }), [data, q]);
  return (
    <Drawer title="Archived cards" subtitle={`${projectName ?? "All projects"}, the most recently archived first`} onClose={onClose} error={error}>
      <input className="input bin-search" type="search" autoFocus aria-label="Search archived cards" placeholder="Search ID or title"
        value={q} onChange={(e) => setQ(e.target.value)} />
      {data && data.length === 0 && <div className="empty"><b>No archived cards{projectName ? ` in ${projectName}` : ""}.</b>
        <span>Archive a card in Done to put it here. It keeps its ID, comments, runs and branch, and Restore puts it back in Done.</span></div>}
      {data && data.length > 0 && shown.length === 0 && <div className="empty"><b>Nothing matches.</b><span>Search by ID, like GA-12, or by words in the title.</span></div>}
      <div role="list" aria-label="Archived cards">
        {shown.map((t) => (
          <a key={t.id} role="listitem" className="bin-row" href={href({ page: "task", id: t.id })} onClick={onClose}>
            <span className="id">{t.identifier}</span>
            <span className="title" title={t.title}>{t.title}</span>
            <span className="proj">{t.projectName && <><span className="dot" style={{ flex: "none", width: 8, height: 8, borderRadius: "50%", background: t.projectColor ?? "var(--text-3)" }} /><span>{t.projectName}</span></>}</span>
            <span className="when" title={t.archivedAt ? new Date(t.archivedAt).toLocaleString("en-GB") : undefined}>
              Archived {t.archivedAt ? relTime(t.archivedAt) : ""}{t.archivedBy ? ` by ${t.archivedBy}` : ""}</span>
          </a>
        ))}
      </div>
    </Drawer>
  );
}
