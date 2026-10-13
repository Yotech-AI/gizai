// The folder picker (GA-113; design §2): from + on the sidebar's Editor heading, and Open a folder on the Editor page.
// A popover like the @ picker, through a portal (the sidebar clips what overflows it), its top left corner where it was
// asked for, moved up and left as far as it must to fit the window. Projects (their linked folders), one group per Editor
// folder ("In ~/Herd": the folders directly inside it that aren't a project's), and Cards (worktrees, yours by hand first:
// a card's title on up to two lines, its full title in the row's tooltip; GA-146). One box filters them all; ↑ and ↓
// choose, Enter opens, Escape closes. The Terminal page (GA-135) and the sidebar's Terminal (GA-136) use it titled "New
// terminal in", with their own `onPick` (design/terminal §2): there a card's worktree an agent works on is off, with why
// as its tooltip; a card that isn't pinned to you asks first under its row, with Edit anyway; the folder the page shows is
// on top and chosen; a note can sit on top of the list (20 terminals open); and a terminal that couldn't start says why
// under its row, with the picker still open.
import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { FolderCode, Info, Keyboard, Search } from "lucide-react";
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

export function FolderPicker({ at, onClose, title, onPick, current, note, you }: {
  at: { x: number; y: number }; onClose: () => void;
  /** Its title over the search box ("New terminal in"); without one it opens the folder in the Editor. With a promise, the
   *  picker stays open until it settles, and shows the sentence it resolves with (why it couldn't) under the folder's row. */
  title?: string; onPick?: ((f: EditorFolder) => void) | ((f: EditorFolder) => Promise<string | null>);
  /** The folder the page shows: on top, and chosen. */
  current?: string;
  /** A note on top of the list ("20 terminals are open: …"). */
  note?: string | null;
  /** You: with `onPick`, a card's worktree that isn't pinned to you asks first, with Edit anyway. */
  you?: string;
}) {
  const [folders, setFolders] = useState<EditorFolder[] | null>(null);
  const [roots, setRoots] = useState<string[]>([]);
  const [tasks, setTasks] = useState<Map<string, Task>>(new Map());
  const [projects, setProjects] = useState<Map<string, Project>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const [pos, setPos] = useState({ left: at.x, top: at.y });
  const [asking, setAsking] = useState<string | null>(null);
  const [failed, setFailed] = useState<{ id: string; why: string } | null>(null);
  const [busy, setBusy] = useState(false);
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
    const r = ref.current?.getBoundingClientRect();
    setPos({ left: Math.max(8, Math.min(at.x, window.innerWidth - (r?.width ?? 0) - 8)), top: Math.max(8, Math.min(at.y, window.innerHeight - (r?.height ?? 0) - 8)) });
  }, [at.x, at.y, folders, asking, failed]);

  const byHand = (f: EditorFolder) => !!(f.taskId && tasks.get(f.taskId)?.byHand);
  const groups = useMemo(() => currentFirst(pickerGroups(folders ?? [], roots, query, byHand), current), [folders, roots, query, tasks, current]); // eslint-disable-line react-hooks/exhaustive-deps
  const flat = groups.flatMap((g) => g.folders);
  // the cards' identifiers share one column, as wide as the longest (in characters: the mono font), so every title starts
  // at the same place (GA-146); from all the cards, so it stays put while you type
  const idChars = useMemo(() => Math.max(0, ...(folders ?? []).filter((f) => f.kind === "card").map((f) => f.name.length)), [folders]);
  // a terminal can't start in a card's worktree while an agent works on it (the backend refuses it too)
  const off = (f: EditorFolder) => !!onPick && f.readOnly;
  // nor, without asking, in one that isn't pinned to you: a shell there can change its files, as an edit can
  const ask = (f: EditorFolder) => (onPick && you && f.kind === "card" && !f.readOnly ? notYoursAsk(f, f.taskId ? tasks.get(f.taskId) : undefined, you) : null);
  const choose = (f: EditorFolder | undefined, anyway = false) => {
    if (!f || off(f) || busy) return;
    if (!anyway && ask(f)) { setAsking(f.id); setFailed(null); return; }
    setAsking(null);
    if (!onPick) { onClose(); openFolder(f.id); return; }
    const r: unknown = onPick(f);
    if (!(r instanceof Promise)) { onClose(); return; }
    setBusy(true);
    setFailed(null);
    (r as Promise<string | null>).then((why) => { setBusy(false); if (why) setFailed({ id: f.id, why }); else onClose(); })
      .catch((e) => { setBusy(false); setFailed({ id: f.id, why: String(e) }); });
  };
  const rootNames = roots.length ? roots.join(", ") : "your Editor folders";

  let i = -1;
  return createPortal(
    <div ref={ref} className="pop ed-picker" role="dialog" aria-label={title ?? "Open a folder"} style={{ position: "fixed", left: pos.left, top: pos.top }}
      onKeyDown={(e) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); } }}>
      {title && <div className="ed-picker-title">{title}</div>}
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
      {note && <div className="ed-picker-note" role="status"><Info className="icon" /><span>{note}</span></div>}
      <div className="ed-picker-list" role="listbox" aria-label="Folders" style={{ "--id-chars": idChars } as React.CSSProperties}>
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
                const asks = ask(f);
                const says = off(f) ? `${f.readOnlyWhy ?? "An agent is working on it"}: a terminal there waits until its run ends.` : asks ? asks.slice(0, asks.indexOf(":")) : f.path;
                return (
                  <Fragment key={f.id}>
                  <button className={`opt${me === sel ? " active" : ""}`} role="option" aria-selected={me === sel} data-kind={f.kind}
                    title={f.kind === "card" && task?.title ? `${task.title}\n${says}` : says}
                    aria-disabled={off(f) || undefined}
                    onMouseEnter={() => setSel(me)} onClick={() => choose(f)}>
                    {f.kind === "project" ? <span className="dot" style={{ width: 9, height: 9, borderRadius: "50%", flex: "none", background: project?.color ?? "var(--text-3)" }} />
                      : f.kind === "card" ? <span className="st"><StatusIcon category={task?.stateCategory ?? "in_progress"} hold={task?.hold} /></span>
                      : <FolderCode className="icon sm" />}
                    {f.kind === "card" ? <><span className="card-id">{f.name}</span><span className="label">{task?.title ?? ""}</span></> : <span className="label">{f.kind === "project" ? project?.name ?? f.name : f.name}</span>}
                    {f.kind === "card" && f.readOnly ? <span className="stand"><span className="pulse" />{workingWho(f.readOnlyWhy)}</span>
                      : f.kind === "card" && task?.byHand ? <span className="stand"><Keyboard className="icon sm" />by hand</span>
                      : f.kind !== "card" ? <span className="where">{f.path}</span> : null}
                  </button>
                  {asking === f.id && asks && (
                    <div className="ed-confirm" role="alertdialog" aria-label={f.name}
                      onKeyDown={(e) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); setAsking(null); input.current?.focus(); } }}>
                      <span className="grow">{asks}</span>
                      <button className="btn sm" autoFocus disabled={busy} onClick={() => choose(f, true)}>Edit anyway</button>
                      <button className="btn ghost sm" onClick={() => { setAsking(null); input.current?.focus(); }}>Cancel</button>
                    </div>
                  )}
                  {failed?.id === f.id && <div className="ed-row-note error" role="alert">{failed.why}</div>}
                  </Fragment>
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

/** The folder the page shows comes first: its group moves to the top, with it first there, so Enter picks it. */
function currentFirst<G extends { title: string; folders: EditorFolder[] }>(groups: G[], current: string | undefined): G[] {
  const i = current ? groups.findIndex((g) => g.folders.some((f) => f.id === current)) : -1;
  if (i < 0) return groups;
  const g = groups[i]!;
  const moved = { ...g, folders: [...g.folders.filter((f) => f.id === current), ...g.folders.filter((f) => f.id !== current)] };
  return [moved, ...groups.slice(0, i), ...groups.slice(i + 1)];
}

/** Choosing a card's worktree that isn't pinned to you asks first (design/terminal §2): "GA-112 is Frontend Agent's card:
 *  a shell there can change its files, as an edit can." A card pinned to you (by hand) starts at once: null. */
export function notYoursAsk(f: Pick<EditorFolder, "name">, task: Pick<Task, "byHand" | "assigneeId" | "assigneeName"> | undefined, you: string): string | null {
  if (task?.byHand && task.assigneeId === you) return null;
  const whose = task?.assigneeName ? `is ${task.assigneeName}'s card` : "isn't assigned to anyone";
  return `${f.name} ${whose}: a shell there can change its files, as an edit can.`;
}
