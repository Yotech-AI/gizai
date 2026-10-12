// The folder picker (GA-113; design §2): from + on the sidebar's Editor heading, and Open a folder on the Editor page.
// A popover like the @ picker, through a portal (the sidebar clips what overflows it), its top where it was asked for,
// moved up as far as it must to fit the window. Projects (their linked folders), one group per Editor folder ("In
// ~/Herd": the folders directly inside it that aren't a project's), and Cards (worktrees, yours by hand first). One box
// filters them all; ↑ and ↓ choose, Enter opens, Escape closes.
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { FolderCode, Keyboard, Search } from "lucide-react";
import { editorFolders, getSettings, listProjects, listTasks } from "../../api";
import type { EditorFolder, Project, Task } from "../../types";
import { go, href } from "../../router";
import { pickerGroups, rememberFolder } from "../../lib/editor";
import { StatusIcon } from "../StatusIcon";

/** Opens a folder in the Editor: to the top of the sidebar's list, then its page. */
export function openFolder(id: string) {
  rememberFolder(id);
  go({ page: "editor", id });
}

/** Who works on a card's worktree now, from the backend's reason ("QA Agent is working on GA-118" → "QA Agent is working"). */
export const workingWho = (why: string | null) => (why ?? "An agent is working").replace(/ on \S+$/, "");

export function FolderPicker({ at, onClose }: { at: { x: number; y: number }; onClose: () => void }) {
  const [folders, setFolders] = useState<EditorFolder[] | null>(null);
  const [roots, setRoots] = useState<string[]>([]);
  const [tasks, setTasks] = useState<Map<string, Task>>(new Map());
  const [projects, setProjects] = useState<Map<string, Project>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const [top, setTop] = useState(at.y);
  const ref = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    input.current?.focus();
    editorFolders().then(setFolders).catch((e) => { setFolders([]); setError(String(e)); });
    getSettings().then((s) => setRoots(s.editorRoots)).catch(() => {});
    listTasks({ openOnly: false }).then((ts) => setTasks(new Map(ts.map((t) => [t.id, t])))).catch(() => {});
    listProjects().then((ps) => setProjects(new Map(ps.map((p) => [p.id, p])))).catch(() => {});
  }, []);
  useEffect(() => {
    const down = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) onClose(); };
    window.addEventListener("mousedown", down);
    return () => window.removeEventListener("mousedown", down);
  }, [onClose]);
  useLayoutEffect(() => {
    const h = ref.current?.getBoundingClientRect().height ?? 0;
    setTop(Math.max(8, Math.min(at.y, window.innerHeight - h - 8)));
  }, [at.y, folders]);

  const byHand = (f: EditorFolder) => !!(f.taskId && tasks.get(f.taskId)?.byHand);
  const groups = useMemo(() => pickerGroups(folders ?? [], roots, query, byHand), [folders, roots, query, tasks]); // eslint-disable-line react-hooks/exhaustive-deps
  const flat = groups.flatMap((g) => g.folders);
  const choose = (f: EditorFolder | undefined) => { if (!f) return; onClose(); openFolder(f.id); };
  const rootNames = roots.length ? roots.join(", ") : "your Editor folders";

  let i = -1;
  return createPortal(
    <div ref={ref} className="pop ed-picker" role="dialog" aria-label="Open a folder" style={{ position: "fixed", left: at.x, top }}
      onKeyDown={(e) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); } }}>
      <div className="ed-picker-in">
        <Search className="icon sm" />
        <input ref={input} className="input" value={query} placeholder="Find a folder" aria-label="Find a folder"
          onChange={(e) => { setQuery(e.target.value); setSel(0); }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") { e.preventDefault(); setSel((s) => Math.min(s + 1, flat.length - 1)); }
            else if (e.key === "ArrowUp") { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
            else if (e.key === "Enter") { e.preventDefault(); choose(flat[sel]); }
          }} />
      </div>
      <div className="ed-picker-list" role="listbox" aria-label="Folders">
        {error ? <div className="pop-empty">{error}</div>
          : !folders ? <div className="pop-empty">Loading…</div>
          : folders.length === 0 ? <div className="pop-empty">No folders yet. Link a project to its folder (project page → Edit), or say where your projects are in Settings → General → Editor folders.</div>
          : flat.length === 0 ? <div className="pop-empty">No folder found</div>
          : groups.map((g) => (
            <div key={g.title} role="group" aria-label={g.title}>
              <div className="pop-label">{g.title}</div>
              {g.folders.map((f) => {
                i += 1;
                const me = i;
                const task = f.taskId ? tasks.get(f.taskId) : undefined;
                const project = f.projectId ? projects.get(f.projectId) : undefined;
                return (
                  <button key={f.id} className={`opt${me === sel ? " active" : ""}`} role="option" aria-selected={me === sel} title={f.path}
                    onMouseEnter={() => setSel(me)} onClick={() => choose(f)}>
                    {f.kind === "project" ? <span className="dot" style={{ width: 9, height: 9, borderRadius: "50%", flex: "none", background: project?.color ?? "var(--text-3)" }} />
                      : f.kind === "card" ? <span className="st"><StatusIcon category={task?.stateCategory ?? "in_progress"} hold={task?.hold} /></span>
                      : <FolderCode className="icon sm" />}
                    {f.kind === "card" ? <><span className="card-id">{f.name}</span><span className="label">{task?.title ?? ""}</span></> : <span className="label">{f.kind === "project" ? project?.name ?? f.name : f.name}</span>}
                    {f.kind === "card" && f.readOnly ? <span className="stand"><span className="pulse" />{workingWho(f.readOnlyWhy)}</span>
                      : f.kind === "card" && task?.byHand ? <span className="stand"><Keyboard className="icon sm" />by hand</span>
                      : f.kind !== "card" ? <span className="where">{f.path}</span> : null}
                  </button>
                );
              })}
            </div>
          ))}
      </div>
      <div className="ed-picker-foot">
        Projects' folders, the folders in {rootNames} and card worktrees. More places: <a href={href({ page: "settings", id: "general" })} onClick={onClose}>Settings → General → Editor folders</a>.
      </div>
    </div>,
    document.body,
  );
}
