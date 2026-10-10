// The Memory page (GA-68): where people find, read and manage what the Team Lead and the agents remember. On the left
// the file explorer (a folder tree that remembers which folders are open; new note, new folder, rename, drag to move)
// with the search (words, "phrases", path: and tag:) above it; in the centre the note (NotePane) or, with none open,
// Recently changed (who wrote what, so agents' additions can be reviewed) or what memory is; on the right the note's
// panel. #/memory shows every note (the Team Lead's view), #/memory/shared the shared folders and
// #/memory/agent/<id> one agent's own folder (router.ts).
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { DndContext, DragOverlay, PointerSensor, pointerWithin, useDraggable, useDroppable, useSensor, useSensors, type DragEndEvent } from "@dnd-kit/core";
import {
  Brain, ChevronDown, ChevronRight, FilePlus, FileText, Folder, FolderOpen, FolderPlus, Hash, PanelRight, Pencil, Search, X,
} from "lucide-react";
import { getTeam, listTasks, memoryCreate, memoryMove, memoryNotes, memoryRecent, memorySearch } from "../api";
import { go, href, type Route } from "../router";
import { useData } from "../lib/useData";
import { useCurrentTeam } from "../lib/team";
import { relTime } from "../lib/format";
import {
  buildTree, findFolder, folderOf, foldersTo, hasTag, highlight, inScope, isNoteFolder, isOwnFolder, LEAD, leadOf, linkedNote, memoryScope, moveTarget, newNotePath,
  notesIn, noteTemplate, NOTE_TYPES, rebase, scopedQuery, scopeRoots, searchWords, section, titleOf, titleProblem, today, TYPE_FOLDER, TYPE_HINT,
  TYPE_NAME, withoutFrontmatter, type MemoryScope, type NoteType, type TreeFolder, type WikiLink,
} from "../lib/memory";
import { NotePane } from "../components/memory/NotePane";
import { NoteView, type NoteLinks } from "../components/memory/NoteView";
import { Drawer } from "../components/Drawer";
import type { MemoryChange, MemoryHit, MemoryNote } from "../types";

const OPEN_KEY = "gizai.memory.open";
const FOLDERS_KEY = "gizai.memory.folders";
const PANEL_KEY = "gizai.memory.panel";

const readList = (key: string): string[] => { try { const v = JSON.parse(localStorage.getItem(key) ?? "[]"); return Array.isArray(v) ? v.filter((x) => typeof x === "string") : []; } catch { return []; } };
const writeList = (key: string, list: string[]) => localStorage.setItem(key, JSON.stringify([...new Set(list)]));

type NewNote = { title?: string; folder?: string; type?: NoteType };
type Hover = { link: WikiLink; from: MemoryNote | null; at: DOMRect };

export function MemoryPage({ route, youId }: { route: Route; youId: string }) {
  const [teamId] = useCurrentTeam();
  const team = useData(() => getTeam(teamId), [teamId]);
  const all = useData(() => memoryNotes(true));
  const cards = useData(() => listTasks({ openOnly: false }).then((ts) => new Set(ts.map((t) => t.identifier.toUpperCase()))).catch(() => new Set<string>()));
  const agents = (team.data?.members ?? []).filter((m) => m.kind === "agent");
  const lead = leadOf(agents);
  const agent = route.scope && route.scope !== "shared" ? agents.find((a) => a.actorId === route.scope) ?? null : null;
  const notes = all.data ?? [];
  // An agent's folder by its name; before the team is there (or for an agent that is gone), by its notes.
  const own = route.scope && !agent ? notes.find((n) => n.ownerId === route.scope && n.path.startsWith("Agents/"))?.path.split("/")[1] : undefined;
  const scope = memoryScope(route.scope, agent?.name ?? own);
  const scoped = useMemo(() => notes.filter((n) => inScope(n, scope)), [notes, route.scope, agent?.name]); // eslint-disable-line react-hooks/exhaustive-deps

  const [tag, setTag] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [newNote, setNewNote] = useState<NewNote | null>(null);
  const [fresh, setFresh] = useState<string | null>(null); // a note just made: it opens in the editor
  const [jump, setJump] = useState<{ id: string; heading: string } | null>(null);
  const [panel, setPanel] = useState(() => localStorage.getItem(PANEL_KEY) !== "off");
  const [err, setErr] = useState<string | null>(null);
  const [hover, setHover] = useState<Hover | null>(null);
  const timers = useRef<{ show?: number; hide?: number }>({});
  useEffect(() => { setTag(null); setQuery(""); }, [route.scope]);

  const scopeName = scope.kind === "agent" ? agent?.name ?? own ?? "Agent" : scope.kind === "shared" ? "Shared notes" : lead?.name ?? "All notes";
  const open = (note: MemoryNote, heading?: string) => {
    setHover(null);
    setJump(heading ? { id: note.id, heading } : null);
    go({ page: "memory", scope: inScope(note, scope) ? route.scope : undefined, id: note.id });
  };
  const links: NoteLinks = {
    notes, cards: cards.data ?? new Set(),
    open,
    create: (l, from) => {
      setHover(null);
      const path = newNotePath(l.target, from ? folderOf(from.path) : scopeRoots(scope)[0] ?? "");
      setNewNote({ title: titleOf(path), folder: folderOf(path) });
    },
    hover: (l, from, at) => {
      window.clearTimeout(timers.current.show);
      if (l && at) {
        window.clearTimeout(timers.current.hide);
        timers.current.show = window.setTimeout(() => setHover({ link: l, from, at }), 350);
      } else {
        timers.current.hide = window.setTimeout(() => setHover(null), 250);
      }
    },
  };
  const create = async (path: string, body: string) => {
    const saved = await memoryCreate(path, body);
    setFresh(saved.id);
    setNewNote(null);
    go({ page: "memory", scope: route.scope, id: saved.id });
  };
  const current = route.id ? notes.find((n) => n.id === route.id) ?? null : null;

  return (
    <>
      <div className="topbar">
        <div className="crumbs">
          <a href={href({ page: "memory", scope: route.scope })}>Memory</a><span className="sep">/</span>
          {current ? <>
            <a href={href({ page: "memory", scope: route.scope })}>{scopeName}</a><span className="sep">/</span>
            {folderOf(current.path).split("/").filter(Boolean).map((f, i) => <span key={i} className="crumb-part"><span>{f}</span><span className="sep">/</span></span>)}
            <b>{titleOf(current.path)}</b>
          </> : <b>{scopeName}</b>}
        </div>
        <div className="actions">
          {route.id && (
            <button className={`btn ghost icon-only mem-toggle${panel ? " on" : ""}`} aria-pressed={panel} aria-label={panel ? "Hide the note's panel" : "Show the note's panel"}
              title={panel ? "Hide the note's panel" : "Show the note's panel"} onClick={() => { setPanel(!panel); localStorage.setItem(PANEL_KEY, panel ? "off" : "on"); }}>
              <PanelRight className="icon" />
            </button>
          )}
          <button className="btn primary" onClick={() => setNewNote({ folder: current ? folderOf(current.path) : undefined })}><FilePlus className="icon" />New note</button>
        </div>
      </div>
      {(all.error || err) && <div className="error-banner" role="alert">{all.error ?? err}<button className="btn ghost sm" onClick={() => setErr(null)}>Dismiss</button></div>}
      <div className="split memory">
        <Files scope={scope} scopeName={scopeName} notes={scoped} selected={route.id ?? null} tag={tag} onTag={setTag} query={query} onQuery={setQuery}
          onOpen={open} onNewNote={(folder) => setNewNote({ folder })} onError={setErr} />
        {route.id ? (
          <NotePane key={route.id} id={route.id} notes={notes} scopeNotes={scoped} links={links} tagFilter={tag} onTag={setTag} panel={panel}
            editing={fresh === route.id} heading={jump?.id === route.id ? jump.heading : undefined} />
        ) : (
          <div className="mem-note">
            <div className="doc-in mem-home">
              {all.data && scoped.length === 0
                ? <MemoryEmpty scope={scope} onNew={() => setNewNote({})} />
                : <RecentChanges scope={scope} youId={youId} onOpen={open} />}
            </div>
          </div>
        )}
      </div>
      {newNote && <NewNoteDrawer scope={scope} notes={notes} start={newNote} onClose={() => setNewNote(null)} onCreate={create} />}
      {hover && <NotePreview hover={hover} links={links} onStay={() => window.clearTimeout(timers.current.hide)} onLeave={() => links.hover(null, null, null)} />}
    </>
  );
}

// ---- The file explorer ---------------------------------------------------------------------------------------------

type DragItem = { kind: "note" | "folder"; path: string; id?: string };
type Renaming = { kind: "note" | "folder"; path: string } | { kind: "new-folder"; path: string };

function Files({ scope, scopeName, notes, selected, tag, onTag, query, onQuery, onOpen, onNewNote, onError }: {
  scope: MemoryScope; scopeName: string; notes: MemoryNote[]; selected: string | null; tag: string | null; onTag: (t: string | null) => void;
  query: string; onQuery: (q: string) => void; onOpen: (n: MemoryNote) => void; onNewNote: (folder?: string) => void; onError: (e: string | null) => void;
}) {
  const [openSet, setOpenSet] = useState<Set<string>>(() => new Set(readList(OPEN_KEY)));
  const [made, setMade] = useState<string[]>(() => readList(FOLDERS_KEY)); // empty folders made here
  const [renaming, setRenaming] = useState<Renaming | null>(null);
  const [dragging, setDragging] = useState<DragItem | null>(null);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 5 } }));
  const shown = tag ? notes.filter((n) => hasTag(n.bodyMd, tag)) : notes;
  const roots = scopeRoots(scope);
  const extra = made.filter((f) => roots.some((r) => f.toLowerCase() === r.toLowerCase() || f.toLowerCase().startsWith(`${r.toLowerCase()}/`)));
  const tree = buildTree(shown, tag ? [] : [...roots.filter((r) => r !== "Agents" || scope.kind === "all"), ...extra]);
  const top = scope.kind === "agent" ? findFolder(tree, scope.folder) ?? { path: scope.folder, name: titleOf(scope.folder), folders: [], notes: [], count: 0 } : tree;
  const sel = selected ? notes.find((n) => n.id === selected) : undefined;

  // The open note's folders are open.
  useEffect(() => {
    if (!sel) return;
    const want = foldersTo(folderOf(sel.path));
    setOpenSet((s) => (want.every((f) => s.has(f)) ? s : new Set([...s, ...want])));
  }, [sel?.path]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { writeList(OPEN_KEY, [...openSet]); }, [openSet]);
  const isOpen = (path: string) => (scope.kind === "agent" && path === scope.folder) || openSet.has(path) || !!tag;
  const toggle = (path: string) => setOpenSet((s) => { const n = new Set(s); if (n.has(path)) n.delete(path); else n.add(path); return n; });
  const keepMade = (list: string[]) => { setMade(list); writeList(FOLDERS_KEY, list); };

  const moveNote = async (n: { id: string; path: string }, to: string) => { await memoryMove(n.id, to); };
  const moveFolder = async (from: string, to: string) => {
    const inside = notesIn(findFolder(buildTree(notes, made), from) ?? { path: from, name: "", folders: [], notes: [], count: 0 });
    for (const n of inside) await memoryMove(n.id, rebase(n.path, from, to));
    keepMade([...made.filter((f) => f.toLowerCase() !== from.toLowerCase() && !f.toLowerCase().startsWith(`${from.toLowerCase()}/`)),
      ...made.filter((f) => f.toLowerCase() === from.toLowerCase() || f.toLowerCase().startsWith(`${from.toLowerCase()}/`)).map((f) => rebase(`${f}/x`, from, to).slice(0, -2))]);
    setOpenSet((s) => new Set([...s].map((f) => (f.toLowerCase() === from.toLowerCase() ? to : rebase(`${f}/x`, from, to).slice(0, -2)))));
  };
  const run = (job: Promise<unknown>) => job.then(() => onError(null)).catch((e) => onError(String(e)));

  const onDragEnd = (e: DragEndEvent) => {
    setDragging(null);
    const item = e.active.data.current as DragItem | undefined;
    const folder = e.over?.data.current?.folder as string | undefined;
    if (!item || folder === undefined) return;
    const to = moveTarget(item, folder);
    if (!to) return;
    if (item.kind === "note" && item.id) run(moveNote({ id: item.id, path: item.path }, to));
    else run(moveFolder(item.path, to));
    setOpenSet((s) => new Set([...s, folder]));
  };
  const rename = (r: Renaming, name: string) => {
    setRenaming(null);
    const t = name.trim();
    if (r.kind === "new-folder") {
      if (!t) return;
      const problem = titleProblem(t);
      if (problem) { onError(problem); return; }
      const path = `${r.path}/${t}`;
      keepMade([...made, path]);
      setOpenSet((s) => new Set([...s, r.path, path]));
      return;
    }
    if (!t || t === titleOf(r.path)) return;
    const problem = titleProblem(t);
    if (problem) { onError(problem); return; }
    const to = folderOf(r.path) ? `${folderOf(r.path)}/${t}` : t;
    if (r.kind === "note") { const n = notes.find((x) => x.path === r.path); if (n) run(moveNote(n, to)); }
    else run(moveFolder(r.path, to));
  };

  const ctx: TreeCtx = { isOpen, toggle, selected, onOpen, onNewNote, renaming, setRenaming, rename, made, removeMade: (p) => keepMade(made.filter((f) => f !== p)) };
  return (
    <aside className="mem-files" aria-label="Notes">
      <div className="mem-search">
        <Search className="icon sm" />
        <input className="input" type="search" aria-label="Search notes" placeholder="Search, path: or tag:" title={'Words and "phrases"; path:Folder keeps the notes in a folder, tag:name those with a tag'} value={query}
          onChange={(e) => onQuery(e.target.value)} onKeyDown={(e) => { if (e.key === "Escape") onQuery(""); }} />
      </div>
      {query.trim() ? <SearchResults scope={scope} query={query} selected={selected} onOpen={onOpen} /> : <>
        <div className="mem-files-head">
          <span className="ellipsis" title={scopeName}>{scopeName}</span>
          <button className="btn ghost sm icon-only" aria-label="New note" title="New note" onClick={() => onNewNote(sel ? folderOf(sel.path) : undefined)}><FilePlus className="icon" /></button>
          <button className="btn ghost sm icon-only" aria-label="New folder" title="New folder"
            onClick={() => { const at = sel ? folderOf(sel.path) : scope.kind === "agent" ? scope.folder : roots.find((r) => r !== "Agents") ?? ""; setRenaming({ kind: "new-folder", path: at }); setOpenSet((s) => new Set([...s, at])); }}>
            <FolderPlus className="icon" />
          </button>
        </div>
        {tag && (
          <div className="mem-filter">
            <span className="mem-tag on"><Hash className="icon sm" />{tag}</span><span className="faint">{shown.length} {shown.length === 1 ? "note" : "notes"}</span>
            <button className="btn ghost sm icon-only" aria-label="Show every note" title="Show every note" onClick={() => onTag(null)}><X className="icon" /></button>
          </div>
        )}
        <DndContext sensors={sensors} collisionDetection={pointerWithin} onDragStart={(e) => setDragging((e.active.data.current as DragItem) ?? null)}
          onDragEnd={onDragEnd} onDragCancel={() => setDragging(null)}>
          <div className="mem-tree" role="tree" aria-label="Folders and notes">
            {scope.kind === "agent" ? <FolderRow folder={top} depth={0} ctx={ctx} root /> : top.folders.map((f) => <FolderRow key={f.path} folder={f} depth={0} ctx={ctx} />)}
            {scope.kind !== "agent" && top.notes.map((n) => <NoteRow key={n.id} note={n} depth={0} ctx={ctx} />)}
            {tag && shown.length === 0 && <p className="faint mem-pad">No note here has this tag.</p>}
          </div>
          <DragOverlay dropAnimation={null}>
            {dragging && <div className="mem-drag">{dragging.kind === "note" ? <FileText className="icon sm" /> : <Folder className="icon sm" />}{titleOf(dragging.path)}</div>}
          </DragOverlay>
        </DndContext>
      </>}
    </aside>
  );
}

type TreeCtx = {
  isOpen: (path: string) => boolean; toggle: (path: string) => void; selected: string | null; onOpen: (n: MemoryNote) => void;
  onNewNote: (folder?: string) => void; renaming: Renaming | null; setRenaming: (r: Renaming | null) => void; rename: (r: Renaming, name: string) => void;
  made: string[]; removeMade: (path: string) => void;
};

/** An input in the tree for a name: Enter or leaving it keeps it, Escape doesn't. */
function NameInput({ value, label, onDone }: { value: string; label: string; onDone: (name: string | null) => void }) {
  const [v, setV] = useState(value);
  const done = useRef(false);
  const finish = (name: string | null) => { if (done.current) return; done.current = true; onDone(name); };
  return <input className="input mem-name" autoFocus aria-label={label} value={v} onChange={(e) => setV(e.target.value)} onFocus={(e) => e.target.select()}
    onBlur={() => finish(v)} onKeyDown={(e) => { if (e.key === "Enter") finish(v); if (e.key === "Escape") finish(null); }} onPointerDown={(e) => e.stopPropagation()} />;
}

function FolderRow({ folder, depth, ctx, root }: { folder: TreeFolder; depth: number; ctx: TreeCtx; root?: boolean }) {
  const own = isOwnFolder(folder.path);
  const drag = useDraggable({ id: `folder:${folder.path}`, data: { kind: "folder", path: folder.path } satisfies DragItem, disabled: !own });
  const drop = useDroppable({ id: `into:${folder.path}`, data: { folder: folder.path }, disabled: !isNoteFolder(folder.path) });
  const open = ctx.isOpen(folder.path);
  const r = ctx.renaming;
  const renamingThis = r?.kind === "folder" && r.path === folder.path;
  const empty = folder.count === 0 && ctx.made.includes(folder.path);
  const Icon = open ? FolderOpen : Folder;
  return (
    <div role="treeitem" aria-expanded={open} aria-label={folder.name}>
      <div ref={(el) => { drop.setNodeRef(el); drag.setNodeRef(el); }} {...drag.listeners} {...drag.attributes} role="button" tabIndex={0}
        className={`mem-item folder${drop.isOver ? " drop" : ""}${drag.isDragging ? " dragging" : ""}`} style={{ paddingLeft: 6 + depth * 14 }}
        onClick={() => { if (!renamingThis) ctx.toggle(folder.path); }}
        onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); ctx.toggle(folder.path); } if (e.key === "F2" && own) ctx.setRenaming({ kind: "folder", path: folder.path }); }}
        onDoubleClick={(e) => { if (own) { e.stopPropagation(); ctx.setRenaming({ kind: "folder", path: folder.path }); } }}>
        {root ? <Brain className="icon sm" /> : open ? <ChevronDown className="icon sm chev" /> : <ChevronRight className="icon sm chev" />}
        {!root && <Icon className="icon sm" />}
        {renamingThis ? <NameInput value={folder.name} label="Folder name" onDone={(n) => (n === null ? ctx.setRenaming(null) : ctx.rename(r!, n))} />
          : <span className="ellipsis">{root ? titleOf(folder.path) : folder.name}</span>}
        <span className="mem-acts" onPointerDown={(e) => e.stopPropagation()} onClick={(e) => e.stopPropagation()}>
          {isNoteFolder(folder.path) && <button className="btn ghost sm icon-only" aria-label={`New note in ${folder.name}`} title="New note here" onClick={() => ctx.onNewNote(folder.path)}><FilePlus className="icon" /></button>}
          {isNoteFolder(folder.path) && <button className="btn ghost sm icon-only" aria-label={`New folder in ${folder.name}`} title="New folder here"
            onClick={() => { ctx.setRenaming({ kind: "new-folder", path: folder.path }); if (!open) ctx.toggle(folder.path); }}><FolderPlus className="icon" /></button>}
          {own && <button className="btn ghost sm icon-only" aria-label={`Rename ${folder.name}`} title="Rename" onClick={() => ctx.setRenaming({ kind: "folder", path: folder.path })}><Pencil className="icon" /></button>}
          {empty && <button className="btn ghost sm icon-only" aria-label={`Remove the empty folder ${folder.name}`} title="Remove this empty folder" onClick={() => ctx.removeMade(folder.path)}><X className="icon" /></button>}
        </span>
        <span className="faint mem-count">{folder.count || ""}</span>
      </div>
      {(open || root) && (
        <div role="group">
          {r?.kind === "new-folder" && r.path === folder.path && (
            <div className="mem-item folder" style={{ paddingLeft: 6 + (depth + 1) * 14 }}>
              <Folder className="icon sm" /><NameInput value="" label="New folder's name" onDone={(n) => (n === null ? ctx.setRenaming(null) : ctx.rename(r, n))} />
            </div>
          )}
          {folder.folders.map((f) => <FolderRow key={f.path} folder={f} depth={depth + 1} ctx={ctx} />)}
          {folder.notes.map((n) => <NoteRow key={n.id} note={n} depth={depth + 1} ctx={ctx} />)}
          {folder.count === 0 && folder.folders.length === 0 && r?.path !== folder.path && <div className="faint mem-item empty" style={{ paddingLeft: 6 + (depth + 1) * 14 + 18 }}>No notes yet</div>}
        </div>
      )}
    </div>
  );
}

function NoteRow({ note, depth, ctx }: { note: MemoryNote; depth: number; ctx: TreeCtx }) {
  const drag = useDraggable({ id: `note:${note.id}`, data: { kind: "note", path: note.path, id: note.id } satisfies DragItem });
  const r = ctx.renaming;
  const renamingThis = r?.kind === "note" && r.path === note.path;
  const on = ctx.selected === note.id;
  return (
    <div ref={drag.setNodeRef} {...drag.listeners} {...drag.attributes} role="treeitem" aria-selected={on} tabIndex={0} title={note.path}
      className={`mem-item note${on ? " on" : ""}${drag.isDragging ? " dragging" : ""}`} style={{ paddingLeft: 6 + depth * 14 + 18 }}
      onClick={() => { if (!renamingThis) ctx.onOpen(note); }}
      onKeyDown={(e) => { if (e.key === "Enter") ctx.onOpen(note); if (e.key === "F2") ctx.setRenaming({ kind: "note", path: note.path }); }}
      onDoubleClick={(e) => { e.stopPropagation(); ctx.setRenaming({ kind: "note", path: note.path }); }}>
      <FileText className="icon sm" />
      {renamingThis ? <NameInput value={titleOf(note.path)} label="Note title" onDone={(n) => (n === null ? ctx.setRenaming(null) : ctx.rename(r!, n))} />
        : <span className="ellipsis">{titleOf(note.path)}</span>}
      <span className="mem-acts" onPointerDown={(e) => e.stopPropagation()} onClick={(e) => e.stopPropagation()}>
        <button className="btn ghost sm icon-only" aria-label={`Rename ${titleOf(note.path)}`} title="Rename" onClick={() => ctx.setRenaming({ kind: "note", path: note.path })}><Pencil className="icon" /></button>
      </span>
    </div>
  );
}

// ---- Search ---------------------------------------------------------------------------------------------------------

function Marked({ text, words }: { text: string; words: string[] }) {
  return <>{highlight(text, words).map((p, i) => (p.hit ? <mark key={i}>{p.text}</mark> : <span key={i}>{p.text}</span>))}</>;
}

function SearchResults({ scope, query, selected, onOpen }: { scope: MemoryScope; query: string; selected: string | null; onOpen: (n: MemoryNote) => void }) {
  const [hits, setHits] = useState<MemoryHit[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const q = scopedQuery(query.trim(), scope);
  useEffect(() => {
    let alive = true;
    const t = window.setTimeout(() => {
      memorySearch(q, 50).then((h) => { if (alive) { setHits(h); setError(null); } }).catch((e) => { if (alive) { setHits([]); setError(String(e)); } });
    }, 150);
    return () => { alive = false; window.clearTimeout(t); };
  }, [q]);
  const words = searchWords(query);
  return (
    <div className="mem-results" role="list" aria-label="Search results">
      {error ? <p className="faint mem-pad">{error}</p>
        : hits === null ? <p className="faint mem-pad">Searching…</p>
        : hits.length === 0 ? <p className="faint mem-pad">No note matches. Every word must be in a note's path or text; path:Folder and tag:name narrow it down.</p>
        : <div className="faint mem-pad">{hits.length === 50 ? "The first 50 notes" : `${hits.length} ${hits.length === 1 ? "note" : "notes"}`}</div>}
      {(hits ?? []).map((h) => (
        <button key={h.note.id} role="listitem" className={`mem-hit${selected === h.note.id ? " on" : ""}`} onClick={() => onOpen(h.note)}>
          <span className="mem-hit-title"><FileText className="icon sm" /><span className="ellipsis"><Marked text={titleOf(h.note.path)} words={words} /></span></span>
          <span className="faint ellipsis">{folderOf(h.note.path)}</span>
          {h.snippet && <span className="mem-snippet"><Marked text={h.snippet} words={words} /></span>}
        </button>
      ))}
    </div>
  );
}

// ---- With no note open ----------------------------------------------------------------------------------------------

function MemoryEmpty({ scope, onNew }: { scope: MemoryScope; onNew: () => void }) {
  return (
    <div className="empty mem-empty">
      <Brain className="icon" />
      <b>{scope.kind === "agent" ? "No notes in this agent's folder yet" : "No notes yet"}</b>
      <p>Memory is what the Team Lead and the agents keep for later: decisions and their reasons, preferences, gotchas and how
        things are done here. Each agent's run gets the notes that matter for its card, as data, never as instructions, and
        agents add what they learned to their own folder.</p>
      <p>A note is Markdown with [[links]] to other notes, #tags and properties at the top. Shared notes go in folders like
        Standards, Decisions and Lessons; each agent and the Team Lead have a folder of their own.</p>
      <button className="btn primary" onClick={onNew}><FilePlus className="icon" />New note</button>
    </div>
  );
}

type Who = "all" | "agents" | "you";

function RecentChanges({ scope, youId, onOpen }: { scope: MemoryScope; youId: string; onOpen: (n: MemoryNote) => void }) {
  const recent = useData(() => memoryRecent(60));
  const [who, setWho] = useState<Who>("all");
  const list = (recent.data ?? []).filter((c) => inScope(c.note, scope))
    .filter((c) => who === "all" || (who === "agents" ? c.authorKind === "agent" : c.authorId === youId)).slice(0, 30);
  const by = (c: MemoryChange) => (c.authorId === youId ? "You" : c.authorName ?? "Someone");
  return (
    <section className="mem-recent" aria-label="Recently changed">
      <div className="mem-recent-head">
        <h2>Recently changed</h2>
        <div className="chips" role="group" aria-label="Written by">
          {(["all", "agents", "you"] as Who[]).map((w) => (
            <button key={w} className={`chip${who === w ? " on" : ""}`} aria-pressed={who === w} onClick={() => setWho(w)}>{{ all: "Everyone", agents: "Agents", you: "You" }[w]}</button>
          ))}
        </div>
      </div>
      <p className="faint">Who wrote each note last, and when: review what the agents add here.</p>
      {recent.error && <div className="error-banner">{recent.error}</div>}
      {recent.data && list.length === 0 && <p className="faint">{who === "agents" ? "No agent wrote a note here yet." : who === "you" ? "You haven't written a note here yet." : "Nothing changed yet."}</p>}
      <div className="mem-recent-list">
        {list.map((c) => (
          <div key={c.note.id} className="mem-change">
            <button className="mem-change-note" title={c.note.path} onClick={() => onOpen(c.note)}>
              <FileText className="icon sm" /><span className="ellipsis">{titleOf(c.note.path)}</span><span className="faint ellipsis">{folderOf(c.note.path)}</span>
            </button>
            <span className="mem-change-who">
              <span className={c.authorKind === "agent" ? "mem-agent" : undefined}>{by(c)}</span>
              {c.runId && <>{" "}in a run{c.taskIdentifier && c.taskId ? <> on <a href={href({ page: "task", id: c.taskId })} className="mono">{c.taskIdentifier}</a></> : null}</>}
            </span>
            <span className="faint" title={new Date(c.at).toLocaleString()}>{relTime(c.at)} · v{c.version}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

// ---- New note ---------------------------------------------------------------------------------------------------------

/** The folders a new note can go in on this page: its memory folders and the folders in them. */
function folderChoices(scope: MemoryScope, notes: MemoryNote[], extra: string[]): string[] {
  const roots = scopeRoots(scope).filter((r) => r !== "Agents");
  const tree = buildTree(notes.filter((n) => inScope(n, scope)), [...roots, ...readList(FOLDERS_KEY), ...extra]);
  const out: string[] = [];
  const walk = (f: TreeFolder) => { if (f.path && isNoteFolder(f.path) && (inScope({ path: `${f.path}/x`, scope: "shared", ownerId: null }, scope) || scope.kind === "agent")) out.push(f.path); f.folders.forEach(walk); };
  walk(tree);
  return out.filter((f) => scope.kind !== "agent" || f.toLowerCase() === scope.folder.toLowerCase() || f.toLowerCase().startsWith(`${scope.folder.toLowerCase()}/`));
}

function NewNoteDrawer({ scope, notes, start, onClose, onCreate }: {
  scope: MemoryScope; notes: MemoryNote[]; start: NewNote; onClose: () => void; onCreate: (path: string, body: string) => Promise<void>;
}) {
  const [type, setType] = useState<NoteType>(start.type ?? "note");
  const [title, setTitle] = useState(start.title ?? "");
  const choices = folderChoices(scope, notes, start.folder && isNoteFolder(start.folder) ? [start.folder] : []);
  const typeFolder = (t: NoteType) => { const f = TYPE_FOLDER[t]; return f && choices.includes(f) ? f : null; };
  const [folder, setFolder] = useState(() => (start.folder && choices.some((c) => c.toLowerCase() === start.folder!.toLowerCase()) ? start.folder : typeFolder(start.type ?? "note") ?? (scope.kind === "agent" ? scope.folder : choices.includes(LEAD) ? LEAD : choices[0] ?? "")));
  const [picked, setPicked] = useState(!!start.folder);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const problem = title.trim() ? titleProblem(title) : null;
  const path = `${folder}/${title.trim()}`;
  const taken = !!title.trim() && notes.some((n) => n.path.toLowerCase() === path.toLowerCase());
  const submit = async () => {
    if (!title.trim() || problem || taken || !folder || busy) return;
    setBusy(true);
    try { await onCreate(path, noteTemplate(type, title, today())); } catch (e) { setErr(String(e)); setBusy(false); }
  };
  return (
    <Drawer title="New note" subtitle="A short note of its type, with the right properties and headings to fill in." onClose={onClose} dirty={!!title.trim()} error={err}
      actions={<><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={!title.trim() || !!problem || taken || !folder || busy} onClick={submit}>Make the note</button></>}>
      <div className="fields">
        <div className="field wide">
          <label htmlFor="mem-type">Type</label>
          <select id="mem-type" className="select" value={type} onChange={(e) => {
            const t = e.target.value as NoteType;
            setType(t);
            if (!picked) setFolder((f) => typeFolder(t) ?? f);
          }}>
            {NOTE_TYPES.map((t) => <option key={t} value={t}>{TYPE_NAME[t]}</option>)}
          </select>
          <span className="hint">{TYPE_HINT[type]}</span>
        </div>
        <div className="field wide">
          <label htmlFor="mem-title">Title</label>
          <input id="mem-title" className="input" autoFocus value={title} placeholder="Like: Rust style" onChange={(e) => setTitle(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") submit(); }} />
          {problem ? <span className="error">{problem}</span> : taken ? <span className="error">{path} exists already.</span> : null}
        </div>
        <div className="field wide">
          <label htmlFor="mem-folder">Folder</label>
          <select id="mem-folder" className="select" value={folder} onChange={(e) => { setFolder(e.target.value); setPicked(true); }}>
            {choices.map((f) => <option key={f} value={f}>{f}</option>)}
          </select>
          <span className="hint">Agents read the shared folders; each agent's folder is its own.</span>
        </div>
      </div>
      <div className="field wide mem-template">
        <span className="label">It starts as</span>
        <pre className="mono">{noteTemplate(type, title.trim() || "Title", today())}</pre>
      </div>
    </Drawer>
  );
}

// ---- The preview on hover -------------------------------------------------------------------------------------------

function NotePreview({ hover, links, onStay, onLeave }: { hover: Hover; links: NoteLinks; onStay: () => void; onLeave: () => void }) {
  const { link, from, at } = hover;
  const note = link.target ? linkedNote(links.notes, link.target, from) : from;
  const room = window.innerHeight - at.bottom;
  const style: CSSProperties = {
    left: Math.max(8, Math.min(at.left, window.innerWidth - 440)),
    ...(room < 320 && at.top > room ? { bottom: window.innerHeight - at.top + 6 } : { top: at.bottom + 6 }),
  };
  const body = note ? (link.heading ? section(note.bodyMd, link.heading) ?? withoutFrontmatter(note.bodyMd) : withoutFrontmatter(note.bodyMd)) : "";
  const cut = body.length > 1500 ? `${body.slice(0, 1500)}…` : body;
  return createPortal(
    <div className="pop note-preview" role="tooltip" style={style} onMouseEnter={onStay} onMouseLeave={onLeave}>
      {note ? <>
        <div className="note-preview-head"><FileText className="icon sm" /><b className="ellipsis">{titleOf(note.path)}</b><span className="faint ellipsis">{folderOf(note.path)}</span></div>
        <div className="note-preview-body">
          {cut.trim() ? <NoteView md={cut} from={note} links={{ ...links, hover: () => {} }} frontmatter /> : <p className="faint">This note is empty.</p>}
        </div>
      </> : <>
        <div className="note-preview-head"><FileText className="icon sm" /><b className="ellipsis">{link.target}</b></div>
        <p className="faint">No note called {link.target} yet.</p>
        <button className="btn sm" onClick={() => links.create(link, from)}><FilePlus className="icon" />Make it</button>
      </>}
    </div>,
    document.body,
  );
}

