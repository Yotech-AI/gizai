// The Editor's file tree (GA-113; design §3 and §9): one folder level at a time from the backend, folders first, git's
// mark on the right of each file and a dot on a folder with changes inside. Ignored folders are greyed and folded, and
// what is in them is read-only (a lock). One tab stop: ↑ and ↓ move, → and ← open and fold a folder, Enter opens, F2
// renames, Delete asks first, Shift+F10 or the context menu key opens the row's menu. New file and Rename type in
// place; a path makes its folders. Delete asks under its row, never in a dialog.
import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyEvent } from "react";
import {
  ChevronDown, ChevronRight, ChevronsDownUp, File, FileCode, FileImage, FileJson, FileKey, FilePlus, FileText, Folder, FolderOpen, FolderPlus, Lock,
} from "lucide-react";
import type { DirEntry, FolderGit } from "../../types";
import { holdsChanges, MARK_LETTER, MARK_TIP, markOf, nameOf, treeRows, type Level } from "../../lib/editor";
import { ContextMenu, type MenuItem } from "./Menus";

/** A file's icon by kind only (code, JSON, text, .env, image, other), all text-3: colour is for meaning. */
function fileIcon(name: string) {
  const n = name.toLowerCase();
  if (n === ".env" || n.startsWith(".env.")) return FileKey;
  if (/\.(json|jsonc|lock)$/.test(n)) return FileJson;
  if (/\.(png|jpe?g|gif|webp|svg|ico|bmp|avif)$/.test(n)) return FileImage;
  if (/\.(md|markdown|txt|log|rst|csv)$/.test(n)) return FileText;
  if (/\.(php|ts|tsx|js|jsx|mjs|cjs|rs|css|scss|html?|sql|ya?ml|toml|sh|bash|zsh|py|rb|go|java|kt|swift|c|h|cpp|vue|svelte|xml)$/.test(n) || n === "artisan") return FileCode;
  return File;
}

export type TreeActions = {
  toggle: (dir: string) => void;
  openFile: (path: string) => void;
  /** New file or folder at `path`; resolves with why not, or null when it is made. */
  create: (path: string, dir: boolean) => Promise<string | null>;
  rename: (from: string, to: string) => Promise<string | null>;
  remove: (path: string, dir: boolean) => Promise<string | null>;
  foldAll: () => void;
  menuExtra: (path: string, dir: boolean) => MenuItem[];
  copyPath: (path: string) => void;
};

type Editing = { kind: "file" | "folder"; dir: string } | { kind: "rename"; path: string };

export function FileTree({ title, levels, open, git, active, readOnly, readOnlyWhy, act }: {
  title: string; levels: Map<string, Level>; open: Set<string>; git: FolderGit | null; active: string | null;
  /** The whole folder can't change now (an agent at work, or another's card): no New, Rename or Delete. */
  readOnly: boolean; readOnlyWhy: string | null; act: TreeActions;
}) {
  const rows = treeRows(levels, open);
  const entries = rows.flatMap((r) => (r.kind === "entry" ? [r] : []));
  const [focus, setFocus] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [typed, setTyped] = useState("");
  const [asking, setAsking] = useState<DirEntry | null>(null);
  const [note, setNote] = useState<{ at: string; text: string } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; entry: DirEntry | null; inIgnored: boolean } | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const focusPath = focus && entries.some((r) => r.entry.path === focus) ? focus : entries[0]?.entry.path ?? null;

  useEffect(() => { setNote(null); }, [editing]);

  const startNew = (kind: "file" | "folder", dir: string) => {
    if (dir && !open.has(dir)) act.toggle(dir);
    setEditing({ kind, dir }); setTyped("");
  };
  const startRename = (e: DirEntry) => { setEditing({ kind: "rename", path: e.path }); setTyped(e.name); };
  const finish = async () => {
    if (!editing) return;
    const name = typed.trim();
    if (!name) { setEditing(null); return; }
    if (editing.kind === "rename") {
      const parent = editing.path.includes("/") ? editing.path.slice(0, editing.path.lastIndexOf("/")) : "";
      const to = name.startsWith("/") ? name.slice(1) : parent ? `${parent}/${name}` : name;
      if (to === editing.path) { setEditing(null); return; }
      const why = await act.rename(editing.path, to);
      if (why) setNote({ at: editing.path, text: why }); else setEditing(null);
    } else {
      const path = name.startsWith("/") ? name.slice(1) : editing.dir ? `${editing.dir}/${name}` : name;
      const why = await act.create(path.replace(/\/+$/, ""), editing.kind === "folder");
      if (why) setNote({ at: `new:${editing.dir}`, text: why }); else setEditing(null);
    }
  };

  const itemsFor = (entry: DirEntry | null, inIgnored: boolean): MenuItem[] => {
    const lockedHere = readOnly || inIgnored;
    // New makes its file in the folder itself: inside an ignored folder, which is read-only (the folder can still be
    // renamed or deleted).
    const lockedNew = lockedHere || (entry?.dir === true && entry.ignored);
    const why = readOnly ? readOnlyWhy ?? "Read-only" : "Ignored by git: read-only";
    const dir = entry ? (entry.dir ? entry.path : entry.path.includes("/") ? entry.path.slice(0, entry.path.lastIndexOf("/")) : "") : "";
    const items: MenuItem[] = [
      { label: "New file", disabled: lockedNew, title: lockedNew ? why : undefined, run: () => startNew("file", dir) },
      { label: "New folder", disabled: lockedNew, title: lockedNew ? why : undefined, run: () => startNew("folder", dir) },
    ];
    if (entry) {
      items.push(
        { label: "Rename", kbd: "F2", disabled: lockedHere, title: lockedHere ? why : undefined, run: () => startRename(entry) },
        { label: "Delete", kbd: "Delete", disabled: lockedHere, title: lockedHere ? why : undefined, run: () => setAsking(entry) },
        { sep: true },
        { label: "Copy path", run: () => act.copyPath(entry.path) },
      );
    }
    const extra = act.menuExtra(entry?.path ?? "", entry?.dir ?? true);
    if (extra.length) items.push({ sep: true }, ...extra);
    return items;
  };

  const onKey = (e: ReactKeyEvent) => {
    if ((e.target as HTMLElement).tagName === "INPUT" || (e.target as HTMLElement).tagName === "BUTTON") return;
    const i = entries.findIndex((r) => r.entry.path === focusPath);
    const row = entries[i];
    const move = (j: number) => {
      const to = entries[Math.max(0, Math.min(j, entries.length - 1))];
      if (!to) return;
      setFocus(to.entry.path);
      (box.current?.querySelector(`[data-path="${CSS.escape(to.entry.path)}"]`) as HTMLElement | null)?.focus();
    };
    if (e.key === "ArrowDown") { e.preventDefault(); move(i + 1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); move(i - 1); }
    else if (e.key === "Home") { e.preventDefault(); move(0); }
    else if (e.key === "End") { e.preventDefault(); move(entries.length - 1); }
    else if (!row) return;
    else if (e.key === "ArrowRight") {
      e.preventDefault();
      if (row.entry.dir && !open.has(row.entry.path)) act.toggle(row.entry.path);
      else if (row.entry.dir) move(i + 1);
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      if (row.entry.dir && open.has(row.entry.path)) act.toggle(row.entry.path);
      else { const up = row.entry.path.includes("/") ? row.entry.path.slice(0, row.entry.path.lastIndexOf("/")) : ""; const j = entries.findIndex((r) => r.entry.path === up); if (j >= 0) move(j); }
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (row.entry.dir) act.toggle(row.entry.path); else act.openFile(row.entry.path);
    } else if (e.key === "F2" && !readOnly && !row.inIgnored) { e.preventDefault(); startRename(row.entry); }
    else if (e.key === "Delete" && !readOnly && !row.inIgnored) { e.preventDefault(); setAsking(row.entry); }
    else if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
      e.preventDefault();
      const r = (e.target as HTMLElement).getBoundingClientRect();
      setMenu({ x: r.left + 24, y: r.bottom, entry: row.entry, inIgnored: row.inIgnored });
    }
  };

  const input = (depth: number, at: string, placeholder?: string) => (
    <>
      <div className="ed-row" style={{ ["--depth" as string]: depth }}>
        <span className="room" />
        <input className="input name-in" autoFocus value={typed} placeholder={placeholder} aria-label={placeholder ?? "New name"}
          onChange={(e) => { setTyped(e.target.value); setNote(null); }}
          onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Enter") { e.preventDefault(); void finish(); } if (e.key === "Escape") { e.preventDefault(); setEditing(null); } }}
          onBlur={() => { if (!typed.trim()) setEditing(null); }} />
      </div>
      {note?.at === at && <div className="ed-row-note error" style={{ ["--depth" as string]: depth }} role="alert">{note.text}</div>}
    </>
  );
  const newHere = (dir: string, depth: number) =>
    editing && editing.kind !== "rename" && editing.dir === dir
      ? input(depth, `new:${dir}`, editing.kind === "file" ? "Name, or a path like app/Exports/InvoicesCsv.php" : "Folder name, or a path")
      : null;

  return (
    <>
      <div className="ed-tree-head">
        <span title={title}>{title}</span>
        <button className="btn ghost sm icon-only" aria-label="New file" title={readOnly ? readOnlyWhy ?? "Read-only" : "New file"} disabled={readOnly} onClick={() => startNew("file", "")}><FilePlus className="icon" /></button>
        <button className="btn ghost sm icon-only" aria-label="New folder" title={readOnly ? readOnlyWhy ?? "Read-only" : "New folder"} disabled={readOnly} onClick={() => startNew("folder", "")}><FolderPlus className="icon" /></button>
        <button className="btn ghost sm icon-only" aria-label="Fold every folder" title="Fold every folder" onClick={act.foldAll}><ChevronsDownUp className="icon" /></button>
      </div>
      <div className="ed-tree" ref={box} role="tree" aria-label={`Files in ${title}`} onKeyDown={onKey}
        onContextMenu={(e) => { if (e.target === box.current) { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, entry: null, inIgnored: false }); } }}>
        {newHere("", 0)}
        {rows.map((r) => {
          if (r.kind === "note") return <div key={`note:${r.dir}`} className={`ed-row-note${r.error ? " error" : ""}`} style={{ ["--depth" as string]: r.depth }}>{r.text}</div>;
          const { entry, depth, inIgnored } = r;
          if (editing?.kind === "rename" && editing.path === entry.path) return <div key={entry.path}>{input(depth, entry.path)}</div>;
          const isOpen = entry.dir && open.has(entry.path);
          const mark = entry.dir ? null : markOf(entry.path, git, entry.git);
          const dot = entry.dir && !entry.ignored && holdsChanges(entry.path, git, entry.git);
          const Icon = entry.dir ? (isOpen ? FolderOpen : Folder) : fileIcon(entry.name);
          const tip = entry.ignored
            ? entry.dir ? "Ignored by git: read-only, and search skips it" : inIgnored ? "Ignored by git: read-only" : "Ignored by git: it opens and saves as usual, and isn't committed"
            : !entry.dir && entry.size > 1024 * 1024 ? "Read-only: over 1 MB" : dot ? "Changes inside" : undefined;
          return (
            <div key={entry.path}>
              <div className={`ed-row${entry.ignored || inIgnored ? " ignored" : ""}`} role="treeitem" data-path={entry.path}
                aria-expanded={entry.dir ? isOpen : undefined} aria-selected={entry.path === active} aria-level={depth + 1}
                tabIndex={entry.path === focusPath ? 0 : -1} title={tip} style={{ ["--depth" as string]: depth }}
                onFocus={() => setFocus(entry.path)}
                onClick={() => { setFocus(entry.path); if (entry.dir) act.toggle(entry.path); else act.openFile(entry.path); }}
                onContextMenu={(e) => { e.preventDefault(); setFocus(entry.path); setMenu({ x: e.clientX, y: e.clientY, entry, inIgnored }); }}>
                {entry.dir ? (isOpen ? <ChevronDown className="icon" /> : <ChevronRight className="icon" />) : <span className="room" />}
                <Icon className="icon" />
                <span className="name">{entry.name}</span>
                {!entry.dir && (inIgnored || entry.size > 1024 * 1024) && <Lock className="ro" aria-label="Read-only" />}
                {mark && <span className={`ed-mark ${mark}`} title={MARK_TIP[mark]} aria-label={MARK_TIP[mark]}>{MARK_LETTER[mark]}</span>}
                {dot && <span className="ed-dot" aria-label="Changes inside" />}
              </div>
              {note?.at === entry.path && !editing && <div className="ed-row-note error" style={{ ["--depth" as string]: depth }} role="alert">{note.text}</div>}
              {asking?.path === entry.path && (
                <div className="ed-confirm" style={{ ["--depth" as string]: depth }} role="alertdialog" aria-label="Delete"
                  onKeyDown={(e) => { e.stopPropagation(); if (e.key === "Escape") setAsking(null); }}>
                  <span className="grow">{entry.dir ? `Delete the folder ${entry.name} and everything in it?` : `Delete ${entry.name} from disk?`}</span>
                  <button className="btn danger sm" autoFocus onClick={async () => {
                    const why = await act.remove(entry.path, entry.dir);
                    setAsking(null);
                    if (why) setNote({ at: entry.path, text: why });
                  }}>Delete</button>
                  <button className="btn sm" onClick={() => setAsking(null)}>Cancel</button>
                </div>
              )}
              {entry.dir && isOpen && newHere(entry.path, depth + 1)}
            </div>
          );
        })}
      </div>
      {menu && <ContextMenu at={menu} label={menu.entry ? nameOf(menu.entry.path) : title} items={itemsFor(menu.entry, menu.inIgnored)} onClose={() => setMenu(null)} />}
    </>
  );
}
