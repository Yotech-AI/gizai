// The Editor page (GA-113, F2 of the Code Editor; research §5.2, design design/code-editor/editor.html §3, §4, §7):
// #/editor/<folder>. The top bar is the folder's (breadcrumbs, branch and how many files changed, Open in); under it at
// most one bar about the folder (read-only, a card that isn't yours, the card bar of your card by hand and where Hand over
// sent it, GA-115); then the file tree or search in the folder beside the tabs and the code; under the tabs at most one
// bar about the open file (changed on disk, read-only, couldn't save); the status bar at the bottom is the open file's.
// Outside changes (research §5.3): every 3 seconds the open files' versions (an untouched tab reloads, a tab with changes
// here offers Reload and Keep mine); every 10 seconds and when the window gets the focus, git's status; on the focus also
// the tree's open folders. No file watcher.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { CircleCheck, FileCode, FolderCode, GitBranch, Lock, PanelLeft, PanelLeftClose, Search, TriangleAlert, X } from "lucide-react";
import type { EditorState, Text } from "@codemirror/state";
import { EditorView, type ViewUpdate } from "@codemirror/view";
import { gotoLine, openSearchPanel } from "@codemirror/search";
import {
  editorCreate, editorDelete, editorFolders, editorGit, editorList, editorRead, editorRename, editorVersions, editorWrite, getTask, onRowsChanged, openInApp,
} from "../api";
import type { DirEntry, EditorFolder, FolderGit, OpenedFile, SearchHit, Task } from "../types";
import { go, href } from "../router";
import {
  cursorLabel, diskAction, endingsLabel, forgetFolder, ignoredFolderOf, keepTabs, loadTabs, MARK_TIP, MARK_WORD, markOf, nameOf, nextAfterClose,
  parentOf, readOnlyBar, readOnlyStatus, rememberFolder, sizeLabel, stepTab, tabNames, unreadable, WHY, type Level,
} from "../lib/editor";
import { languageName } from "../lib/codeLanguages";
import { modKey } from "../lib/keys";
import { codeState, goToLine, lockEffect, replaceText, setLanguage, type CodeHandlers } from "../components/editor/code";
import { FileTree } from "../components/editor/FileTree";
import { FolderSearch, type SearchState } from "../components/editor/FolderSearch";
import { GoToFile } from "../components/editor/GoToFile";
import { FolderPicker } from "../components/editor/FolderPicker";
import { openInItems, OpenInMenu, useEditorApps } from "../components/editor/Menus";
import { CardBar } from "../components/editor/CardBar";

export function EditorPage({ id, youId }: { id?: string; youId: string }) {
  return id ? <FolderEditor key={id} folderId={id} youId={youId} /> : <NoFolder />;
}

/** #/editor with no folder open: what it is for, and Open a folder. */
function NoFolder() {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return (
    <div className="ed-page">
      <div className="topbar"><div className="crumbs"><b>Editor</b></div></div>
      <div className="ed-empty">
        <div className="empty">
          <FolderCode className="icon" />
          <b>No folder open</b>
          <span>Open a folder to work on its files: a project's folder, a folder in ~/Herd, or a card's worktree.</span>
          <button className="btn primary" onClick={(e) => { const r = e.currentTarget.getBoundingClientRect(); setAt({ x: r.left, y: r.bottom + 6 }); }}>Open a folder</button>
        </div>
      </div>
      {at && <FolderPicker at={at} onClose={() => setAt(null)} />}
    </div>
  );
}

/** What the page knows of an open tab. The code itself (an EditorState) and the text last read or saved are kept apart,
 *  by path, outside React. */
type Tab = {
  path: string;
  status: "loading" | "ready" | "error";
  error: string | null;
  file: OpenedFile | null;
  /** The version Gizai read or saved, which a save checks. */
  version: string;
  /** A newer version on disk you chose to keep yours over (Keep mine); the next poll leaves it alone. */
  seen: string | null;
  dirty: boolean;
  /** The file bar about the disk: changed while you edit it, Keep mine chosen, asking before saving over it, deleted
   *  while you edit it, or gone (no changes here). */
  disk: null | "changed" | "kept" | "confirm" | "deleted" | "gone";
  saving: boolean;
  saveError: string | null;
  /** When it last reloaded by itself (Reloaded: changed on disk, for 5 seconds). */
  reloaded: number;
};
const newTab = (path: string): Tab => ({ path, status: "loading", error: null, file: null, version: "", seen: null, dirty: false, disk: null, saving: false, saveError: null, reloaded: 0 });

function FolderEditor({ folderId, youId }: { folderId: string; youId: string }) {
  const [folder, setFolder] = useState<EditorFolder | null>(null);
  const [folderError, setFolderError] = useState<string | null>(null);
  const [task, setTask] = useState<Task | null>(null);
  const [editAnyway, setEditAnyway] = useState(false);
  const [levels, setLevels] = useState<Map<string, Level>>(new Map());
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [git, setGit] = useState<FolderGit | null>(null);
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [side, setSide] = useState<"files" | "search">("files");
  const [treeShown, setTreeShown] = useState(true);
  const [search, setSearch] = useState<SearchState>({ query: "", caseSensitive: false, wholeWord: false, regex: false });
  const [searchFocus, setSearchFocus] = useState(0);
  const [goto, setGoto] = useState(false);
  const [cursor, setCursor] = useState({ line: 1, col: 1, sel: 0 });
  const [banner, setBanner] = useState<string | null>(null);
  const [closing, setClosing] = useState<{ path: string; x: number; y: number } | null>(null);
  const [, setTick] = useState(0);
  const apps = useEditorApps();

  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const states = useRef(new Map<string, EditorState>());
  const saved = useRef(new Map<string, Text>());
  const shown = useRef<string | null>(null);
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const levelsRef = useRef(levels);
  levelsRef.current = levels;
  const openRef = useRef(open);
  openRef.current = open;
  const pending = useRef<{ path: string; line: number; column: number } | null>(null);

  const id = folderId;
  const patch = useCallback((path: string, p: Partial<Tab>) => setTabs((ts) => ts.map((t) => (t.path === path ? { ...t, ...p } : t))), []);

  // ---- the folder, and the card whose worktree it is ----
  const loadFolder = useCallback(() => editorFolders().then((list) => {
    const f = list.find((x) => x.id === id);
    if (f) { setFolder(f); setFolderError(null); return; }
    return editorList(id, "").then(
      () => setFolderError(`Gizai can't open ${id}: it isn't one of the Editor's folders any more.`),
      (e) => setFolderError(`Gizai can't open ${id}: ${String(e).replace(/^Error:\s*/, "").replace(/\.$/, "")}.`),
    );
  }).catch((e) => setFolderError(String(e))), [id]);
  useEffect(() => { void loadFolder(); rememberFolder(id); }, [id, loadFolder]);
  // The card, again with every write the backend reports (Hand over, QA sending it back, a move on the board).
  const taskId = folder?.taskId ?? null;
  useEffect(() => {
    if (!taskId) return;
    let alive = true;
    let un: (() => void) | undefined;
    const load = () => { getTask(taskId).then((t) => { if (alive) setTask(t); }).catch(() => { if (alive) setTask(null); }); };
    load();
    onRowsChanged(() => { if (alive) load(); }).then((f) => (alive ? (un = f) : f())).catch(() => {});
    return () => { alive = false; un?.(); };
  }, [taskId]);

  // A card's worktree is yours when it is pinned to you (done by hand); another's is read-only until Edit anyway.
  const cardMine = !!task && task.byHand && task.assigneeId === youId;
  // Hand over moved your card on: the card bar's place says where it went (until it is yours by hand again).
  const [handed, setHanded] = useState<string | null>(null);
  useEffect(() => { if (cardMine) setHanded(null); }, [cardMine]);
  const agentAtWork = !!folder?.readOnly;
  const notMine = folder?.kind === "card" && !agentAtWork && !cardMine;
  const folderLocked = agentAtWork || (notMine && !editAnyway);
  const lockedRef = useRef(folderLocked);
  lockedRef.current = folderLocked;
  const tabLocked = (t: Tab | undefined) => !!t && (ownLock(t.file) || folderLocked);

  // ---- the tree ----
  const loadLevel = useCallback((dir: string, quiet = false) => {
    if (!quiet) setLevels((m) => (m.has(dir) ? m : new Map(m).set(dir, "loading")));
    editorList(id, dir)
      .then((entries) => setLevels((m) => new Map(m).set(dir, entries)))
      .catch((e) => setLevels((m) => new Map(m).set(dir, { error: unreadable(String(e)) })));
  }, [id]);
  const refreshTree = useCallback(() => {
    loadLevel("", true);
    for (const dir of openRef.current) if (levelsRef.current.has(dir)) loadLevel(dir, true);
  }, [loadLevel]);
  useEffect(() => { loadLevel(""); }, [loadLevel]);
  const toggle = (dir: string) => setOpen((s) => {
    const n = new Set(s);
    if (n.has(dir)) n.delete(dir);
    else { n.add(dir); if (!levelsRef.current.has(dir) || !Array.isArray(levelsRef.current.get(dir))) loadLevel(dir); }
    return n;
  });
  /** Opens the folders `path` is in, so its row shows. */
  const reveal = (path: string) => {
    const dirs = path.split("/").slice(0, -1).map((_, i, a) => a.slice(0, i + 1).join("/"));
    setOpen((s) => { const n = new Set(s); for (const d of dirs) { n.add(d); if (!levelsRef.current.has(d)) loadLevel(d); } return n; });
  };
  const entryOf = (path: string): DirEntry | undefined => {
    const level = levels.get(parentOf(path));
    return Array.isArray(level) ? level.find((e) => e.path === path) : undefined;
  };

  // ---- git ----
  const refreshGit = useCallback(() => { editorGit(id).then(setGit).catch(() => {}); }, [id]);
  useEffect(() => {
    refreshGit();
    const t = setInterval(() => { if (!document.hidden) { refreshGit(); void loadFolder(); } }, 10_000);
    const focus = () => { refreshGit(); refreshTree(); void loadFolder(); };
    window.addEventListener("focus", focus);
    return () => { clearInterval(t); window.removeEventListener("focus", focus); };
  }, [refreshGit, refreshTree, loadFolder]);

  // ---- the code: one view, the open tab's state in it ----
  const handlers = useRef<CodeHandlers>({ save: () => {}, update: () => {} });
  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({ parent: host.current });
    view.current = v;
    return () => { v.destroy(); view.current = null; };
  }, []);
  handlers.current = {
    save: () => { if (shown.current) save(shown.current); },
    update: (u: ViewUpdate) => {
      const path = shown.current;
      if (!path) return;
      states.current.set(path, u.state);
      if (u.docChanged) {
        const base = saved.current.get(path);
        const dirty = !!base && !u.state.doc.eq(base);
        if (tabsRef.current.find((t) => t.path === path)?.dirty !== dirty) patch(path, { dirty });
      }
      if (u.docChanged || u.selectionSet) cursorFrom(u.state);
    },
  };
  const cursorFrom = (s: EditorState) => {
    const head = s.selection.main.head;
    const line = s.doc.lineAt(head);
    setCursor({ line: line.number, col: head - line.from + 1, sel: s.selection.ranges.reduce((n, r) => n + (r.to - r.from), 0) });
  };
  const activeTab = tabs.find((t) => t.path === active);
  const activeState = active ? states.current.get(active) : undefined;
  useEffect(() => {
    const v = view.current;
    if (!v) return;
    const s = active ? states.current.get(active) : undefined;
    if (!s) { shown.current = null; return; }
    if (shown.current !== active || v.state !== s) { shown.current = active; v.setState(s); cursorFrom(s); }
    const at = pending.current;
    if (at && at.path === active) { pending.current = null; requestAnimationFrame(() => goToLine(v, at.line, at.column)); }
  }, [active, activeTab?.status, activeState]);

  /** Gives a kept state a transaction's result: through the view when it shows that state. */
  const apply = (path: string, change: (s: EditorState) => import("@codemirror/state").Transaction) => {
    const s = states.current.get(path);
    if (!s) return;
    const tr = change(s);
    if (view.current && shown.current === path && view.current.state === s) view.current.dispatch(tr);
    else states.current.set(path, tr.state);
  };

  // ---- opening, reading, reloading ----
  const read = useCallback((path: string) => {
    editorRead(id, path).then((file) => {
      if (file.text !== null) {
        const s = codeState(file.text, ownLock(file) || lockedRef.current, handlers);
        states.current.set(path, s);
        saved.current.set(path, s.doc);
        void setLanguage(path, view.current, () => states.current.get(path), (n) => states.current.set(path, n)).then(() => setTick((x) => x + 1));
      }
      patch(path, { status: "ready", file, version: file.version, error: null, dirty: false, disk: null, seen: null });
    }).catch((e) => patch(path, { status: "error", error: String(e).replace(/^Error:\s*/, "") }));
  }, [id, patch]);

  const openFile = (path: string, at?: { line: number; column: number }) => {
    if (at) pending.current = { path, ...at };
    if (!tabsRef.current.some((t) => t.path === path)) {
      setTabs((ts) => [...ts, newTab(path)]);
      read(path);
    } else if (at && view.current && shown.current === path) {
      pending.current = null;
      goToLine(view.current, at.line, at.column);
    }
    setActive(path);
  };

  // The tabs kept for this folder, read once; and kept again as they change.
  useEffect(() => {
    const kept = loadTabs(id);
    if (!kept.paths.length) return;
    setTabs(kept.paths.map(newTab));
    kept.paths.forEach(read);
    setActive(kept.active);
  }, [id, read]);
  useEffect(() => { keepTabs(id, { paths: tabs.map((t) => t.path), active }); }, [id, tabs.length, tabs.map((t) => t.path).join("\n"), active]); // eslint-disable-line react-hooks/exhaustive-deps

  const reload = useCallback((path: string, byItself: boolean) => {
    editorRead(id, path).then((file) => {
      if (file.text === null) { patch(path, { file, version: file.version, seen: null, disk: null, dirty: false }); return; }
      const text = file.text;
      const s = states.current.get(path);
      if (!s) { read(path); return; }
      // typed in since the poll: ask, rather than reload over it
      if (byItself && tabsRef.current.find((x) => x.path === path)?.dirty) { patch(path, { disk: "changed" }); return; }
      apply(path, (st) => {
        const tr = replaceText(st, text);
        saved.current.set(path, tr.state.doc);
        return tr;
      });
      apply(path, (st) => st.update({ effects: lockEffect(ownLock(file) || lockedRef.current) }));
      patch(path, { file, version: file.version, seen: null, disk: null, dirty: false, saveError: null, reloaded: byItself ? Date.now() : 0 });
      if (byItself) setTimeout(() => setTick((x) => x + 1), 5100);
    }).catch((e) => patch(path, { status: "error", error: String(e).replace(/^Error:\s*/, "") }));
  }, [id, patch, read]); // eslint-disable-line react-hooks/exhaustive-deps

  // Outside changes: the open files' versions every 3 seconds.
  useEffect(() => {
    const t = setInterval(async () => {
      if (document.hidden) return;
      const snap = tabsRef.current.filter((x) => x.status === "ready" && !x.saving && x.file);
      if (!snap.length) return;
      const now = await editorVersions(id, snap.map((x) => x.path)).catch(() => null);
      if (!now) return;
      snap.forEach((x, i) => {
        const cur = tabsRef.current.find((y) => y.path === x.path);
        const onDisk = now[i];
        if (!cur || onDisk === undefined || cur.saving || cur.version !== x.version) return;
        const does = diskAction(cur, onDisk);
        if (does === "reload") reload(cur.path, true);
        else if (does === "ask" && cur.disk !== "changed") patch(cur.path, { disk: "changed" });
        else if (does === "gone" && cur.disk !== "gone") patch(cur.path, { disk: "gone" });
        else if (does === "gone-ask" && cur.disk !== "deleted") patch(cur.path, { disk: "deleted" });
        else if (does === "none" && (cur.disk === "gone" || cur.disk === "deleted")) patch(cur.path, { disk: null });
      });
    }, 3000);
    return () => clearInterval(t);
  }, [id, patch, reload]);

  // Read-only follows the folder: an agent starting or ending on the card, Edit anyway, Done editing.
  useEffect(() => {
    for (const t of tabsRef.current) if (t.file) apply(t.path, (s) => s.update({ effects: lockEffect(ownLock(t.file) || folderLocked) }));
  }, [folderLocked]); // eslint-disable-line react-hooks/exhaustive-deps

  // ---- saving ----
  const save = (path: string) => {
    const t = tabsRef.current.find((x) => x.path === path);
    const s = states.current.get(path);
    if (!t || !s || t.saving || tabLocked(t)) return;
    if (t.disk === "changed") return;
    if (t.disk === "kept") { patch(path, { disk: "confirm" }); return; }
    const version = t.disk === "confirm" ? t.seen ?? t.version : t.disk === "deleted" || t.disk === "gone" ? "" : t.version;
    const doc = s.doc;
    patch(path, { saving: true, saveError: null });
    editorWrite(id, path, doc.toString(), version).then((v) => {
      saved.current.set(path, doc);
      const now = states.current.get(path)?.doc;
      patch(path, { saving: false, version: v, seen: null, disk: null, saveError: null, dirty: !!now && !now.eq(doc) });
      refreshGit();
      if (t.disk === "deleted" || t.disk === "gone") loadLevel(parentOf(path), true);
    }).catch((e) => {
      const said = String(e).replace(/^Error:\s*/, "");
      if (/changed on disk/.test(said)) patch(path, { saving: false, disk: /isn't there any more/.test(said) ? "deleted" : "changed" });
      else patch(path, { saving: false, saveError: said });
    });
  };
  const keepMine = (path: string) => {
    editorVersions(id, [path]).then(([v]) => {
      if (v === "") patch(path, { disk: "deleted" });
      else patch(path, { disk: "kept", seen: v ?? null });
    }).catch((e) => patch(path, { saveError: String(e) }));
  };

  // ---- closing ----
  const close = (path: string) => {
    const paths = tabsRef.current.map((t) => t.path);
    setTabs((ts) => ts.filter((t) => t.path !== path));
    states.current.delete(path);
    saved.current.delete(path);
    if (shown.current === path) shown.current = null;
    setActive((a) => (a === path ? nextAfterClose(paths, path) : a));
    setClosing(null);
  };
  const askClose = (path: string, el?: HTMLElement | null) => {
    const t = tabsRef.current.find((x) => x.path === path);
    if (!t?.dirty) { close(path); return; }
    setActive(path);
    const r = (el ?? document.querySelector(`.ed-tab[data-path="${CSS.escape(path)}"]`))?.getBoundingClientRect();
    setClosing({ path, x: r?.left ?? 200, y: (r?.bottom ?? 100) + 4 });
  };

  // ---- the tree's actions ----
  const why = (e: unknown) => {
    const said = String(e).replace(/^Error:\s*/, "");
    if (/\.git\b/.test(said)) return "Gizai never writes in .git.";
    if (/\.\.|outside/.test(said)) return `That path leads outside ${folder?.name ?? "the folder"}.`;
    const there = /^(.+) is there already$/.exec(said);
    if (there) return `${nameOf(there[1]!)} is already there.`;
    return said.endsWith(".") ? said : `${said}.`;
  };
  const treeAct = {
    toggle,
    openFile: (p: string) => openFile(p),
    create: async (path: string, dir: boolean) => {
      try {
        await editorCreate(id, path, dir);
        loadLevel(parentOf(path), true);
        reveal(path);
        if (!dir) openFile(path);
        refreshGit();
        return null;
      } catch (e) { return why(e); }
    },
    rename: async (from: string, to: string) => {
      try {
        await editorRename(id, from, to);
        loadLevel(parentOf(from), true);
        loadLevel(parentOf(to), true);
        // Open tabs of the file (or of files in the folder) follow it.
        const moved = (p: string) => (p === from ? to : p.startsWith(`${from}/`) ? to + p.slice(from.length) : p);
        for (const map of [states.current, saved.current] as Map<string, unknown>[]) {
          for (const [k, v] of [...map]) if (moved(k) !== k) { map.delete(k); map.set(moved(k), v); }
        }
        if (shown.current) shown.current = moved(shown.current);
        setTabs((ts) => ts.map((t) => ({ ...t, path: moved(t.path) })));
        setActive((a) => (a ? moved(a) : a));
        // their versions on disk at the new path, so the next poll doesn't take the move for an outside change
        const now = tabsRef.current.map((t) => moved(t.path)).filter((p, i) => p !== tabsRef.current[i]!.path);
        if (now.length) editorVersions(id, now).then((vs) => now.forEach((p, i) => { if (vs[i]) patch(p, { version: vs[i]! }); })).catch(() => {});
        refreshGit();
        return null;
      } catch (e) { return why(e); }
    },
    remove: async (path: string, dir: boolean) => {
      try {
        await editorDelete(id, path);
        loadLevel(parentOf(path), true);
        for (const t of tabsRef.current) if (t.path === path || (dir && t.path.startsWith(`${path}/`))) if (!t.dirty) close(t.path); else patch(t.path, { disk: "deleted" });
        refreshGit();
        return null;
      } catch (e) { return why(e); }
    },
    foldAll: () => setOpen(new Set()),
    copyPath: (p: string) => { navigator.clipboard?.writeText(p).catch(() => {}); },
    menuExtra: (p: string, dir: boolean) => openInItems(apps, id, p && !dir ? p : null, null, setBanner),
  };

  // ---- keys: Ctrl+P, Ctrl+Shift+F, Ctrl+F, Ctrl+S, Ctrl+W, Ctrl+Tab, Ctrl+B ----
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      const k = e.key.toLowerCase();
      const paths = tabsRef.current.map((t) => t.path);
      if (k === "p" && !e.shiftKey) { e.preventDefault(); setGoto(true); }
      else if (k === "f" && e.shiftKey) { e.preventDefault(); setTreeShown(true); setSide("search"); setSearchFocus((n) => n + 1); }
      else if (k === "f" && !e.defaultPrevented && view.current && shown.current) { e.preventDefault(); view.current.focus(); openSearchPanel(view.current); }
      else if (k === "s" && !e.defaultPrevented) { e.preventDefault(); if (shown.current) save(shown.current); }
      else if (k === "w") { e.preventDefault(); const a = shown.current ?? paths[0]; if (a) askClose(a); }
      else if (e.key === "Tab") { e.preventDefault(); setActive((a) => stepTab(paths, a, e.shiftKey ? -1 : 1)); }
      else if (k === "b" && !e.shiftKey) { e.preventDefault(); setTreeShown((x) => !x); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const names = useMemo(() => tabNames(tabs.map((t) => t.path)), [tabs]);
  if (folderError) {
    return (
      <div className="ed-page">
        <div className="topbar"><div className="crumbs"><a href={href({ page: "editor" })}>Editor</a><span className="sep">/</span><b>{nameOf(id)}</b></div></div>
        <div className="ed-empty"><div className="empty"><b>{folderError}</b>
          <button className="btn" onClick={() => { forgetFolder(id); go({ page: "editor" }); }}>Remove from the list</button></div></div>
      </div>
    );
  }

  const name = folder?.name ?? nameOf(id);
  const owner = task?.assigneeName ?? null;
  const t = activeTab;
  const file = t?.file ?? null;
  const entry = t ? entryOf(t.path) : undefined;
  const ignoredIn = t ? ignoredFolderOf(t.path, levels) : null;
  const mark = t ? markOf(t.path, git) : null;
  const locked = tabLocked(t);
  const lockWhy = ownLock(file) ? file!.readOnlyWhy : agentAtWork ? folder?.readOnlyWhy ?? null : notMine ? `${name} is ${owner ? `${owner}'s card` : "another's card"}` : null;
  const firstApp = apps?.[0];
  const openInFirst = (path: string) => firstApp && <button className="btn sm" onClick={() => openInApp(firstApp.id, id, path, null).catch((e) => setBanner(String(e)))}>Open in {firstApp.name}</button>;
  const dirtyCount = tabs.filter((x) => x.dirty).length;

  return (
    <div className="ed-page">
      <div className="topbar">
        <div className="crumbs ed-crumbs"><a href={href({ page: "editor" })}>Editor</a><span className="sep">/</span><b title={folder?.path ?? id}>{name}</b></div>
        <div className="ed-standing" title={[git?.branch, git && git.uncommitted ? `${git.uncommitted} changed` : null].filter(Boolean).join(" · ")}>
          {git?.branch && <><GitBranch className="icon sm" /><span className="branch">{git.branch}</span></>}
          {git && git.uncommitted > 0 && <>{git.branch && <span className="sep">·</span>}<span>{git.uncommitted.toLocaleString("en-US")} changed</span></>}
        </div>
        <div className="actions" style={{ marginLeft: "auto" }}>
          {folder && <OpenInMenu folder={id} name={name} path={t?.path ?? null} line={t ? cursor.line : null} fail={setBanner} />}
        </div>
      </div>
      {banner && <div className="error-banner" role="alert" style={{ margin: 0 }}>{banner}<button className="btn ghost sm icon-only" aria-label="Dismiss" style={{ marginLeft: 8 }} onClick={() => setBanner(null)}><X className="icon" /></button></div>}

      {agentAtWork ? (
        <div className="ed-bar" role="status">
          <span className="pulse" />
          <span className="say">{dirtyCount > 0
            ? `${folder?.readOnlyWhy ?? "An agent is working on it"}: its worktree is read-only again. ${dirtyCount === 1 ? "1 tab has" : `${dirtyCount} tabs have`} changes that weren't saved: copy what you need, then close ${dirtyCount === 1 ? "it" : "them"}.`
            : `${folder?.readOnlyWhy ?? "An agent is working on it"}: its worktree is read-only until the run ends.`}</span>
          {folder?.taskId && <span className="acts"><button className="btn sm" onClick={() => go({ page: "task", id: folder.taskId! })}>Open card</button></span>}
        </div>
      ) : handed ? (
        <div className="ed-bar done" role="status">
          <CircleCheck className="icon" />
          <span className="say">{handed}</span>
          {folder?.taskId && <span className="acts"><button className="btn ghost sm" onClick={() => go({ page: "task", id: folder.taskId! })}>Open card</button></span>}
        </div>
      ) : cardMine && task ? (
        <CardBar task={task} folder={id} git={git} onChanged={() => { refreshGit(); refreshTree(); }} onHanded={setHanded} />
      ) : notMine && !editAnyway ? (
        <div className="ed-bar" role="status">
          <Lock className="icon" />
          <span className="say">{owner ? `${name} is ${owner}'s card` : `${name} isn't assigned to anyone`}: its worktree is read-only here. Edit anyway puts your changes in it, uncommitted, for {owner ? "the agent's" : "its"} next run.</span>
          <span className="acts">
            <button className="btn sm" onClick={() => setEditAnyway(true)}>Edit anyway</button>
            {folder?.taskId && <button className="btn sm" onClick={() => go({ page: "task", id: folder.taskId! })}>Open card</button>}
          </span>
        </div>
      ) : notMine && editAnyway ? (
        <div className="ed-bar" role="status">
          <TriangleAlert className="icon" />
          <span className="say">You're editing {name}'s worktree. {owner ? `${owner}'s` : "Its"} next run gets your changes, uncommitted.</span>
          <span className="acts"><button className="btn sm" onClick={() => setEditAnyway(false)}>Done editing</button></span>
        </div>
      ) : null}

      <div className="ed">
        <aside className="ed-side" hidden={!treeShown} aria-label="Files and search">
          <div className="ed-side-top">
            <div className="seg" role="group" aria-label="Files or search">
              <button aria-pressed={side === "files"} onClick={() => setSide("files")}>Files</button>
              <button aria-pressed={side === "search"} title={`Search in the folder (${modKey()}+Shift+F)`} onClick={() => { setSide("search"); setSearchFocus((n) => n + 1); }}><Search className="icon sm" />Search</button>
            </div>
            <button className="btn ghost sm icon-only" aria-label={`Fold the file tree (${modKey()}+B)`} title={`Fold the file tree (${modKey()}+B)`} onClick={() => setTreeShown(false)}><PanelLeftClose className="icon" /></button>
          </div>
          {side === "files" ? (
            <>
              <button className="ed-goto" onClick={() => setGoto(true)}><Search className="icon sm" />Go to file<span className="kbd">{modKey()} P</span></button>
              <FileTree title={name} levels={levels} open={open} git={git} active={active} readOnly={folderLocked} readOnlyWhy={lockWhy} act={treeAct} />
            </>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}
              onKeyDown={(e) => { if (e.key === "Escape") { e.preventDefault(); setSide("files"); } }}>
              <FolderSearch folder={id} name={name} state={search} setState={setSearch} focusKey={searchFocus}
                onOpen={(h: SearchHit) => { reveal(h.path); openFile(h.path, { line: h.line, column: h.column }); }} />
            </div>
          )}
        </aside>

        <div className="ed-main">
          <div className="ed-tabs" role="tablist" aria-label="Open files">
            {!treeShown && <button className="btn ghost sm icon-only" aria-label={`Show the file tree (${modKey()}+B)`} title={`Show the file tree (${modKey()}+B)`} onClick={() => setTreeShown(true)}><PanelLeft className="icon" /></button>}
            {tabs.map((x) => {
              const n = names.get(x.path);
              const m = markOf(x.path, git);
              const ro = tabLocked(x);
              const ign = !!entryOf(x.path)?.ignored || !!ignoredFolderOf(x.path, levels);
              return (
                <div key={x.path} className={`ed-tab${x.dirty ? " unsaved" : ""}${ign ? " ignored" : ""}`} role="tab" data-path={x.path} tabIndex={x.path === active ? 0 : -1}
                  aria-selected={x.path === active} aria-label={`${n?.name ?? nameOf(x.path)}${x.dirty ? ", not saved" : ""}`} title={x.path}
                  onClick={() => setActive(x.path)}
                  onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); askClose(x.path, e.currentTarget); } }}
                  onMouseDown={(e) => { if (e.button === 1) e.preventDefault(); }}
                  onKeyDown={(e) => {
                    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
                      e.preventDefault();
                      const next = stepTab(tabs.map((y) => y.path), x.path, e.key === "ArrowRight" ? 1 : -1);
                      setActive(next);
                      (e.currentTarget.parentElement?.querySelector(`[data-path="${CSS.escape(next ?? "")}"]`) as HTMLElement | null)?.focus();
                    }
                  }}
                  ref={(el) => { if (el && x.path === active) el.scrollIntoView({ block: "nearest", inline: "nearest" }); }}>
                  {ro && <Lock className="ro" aria-label="Read-only" />}
                  <span className="name">{n?.name ?? nameOf(x.path)}</span>
                  {n?.dir && <span className="dir">{n.dir}</span>}
                  {m && <span className={`ed-mark ${m}`} title={MARK_TIP[m]} aria-label={MARK_TIP[m]}>{m === "modified" ? "M" : m === "added" ? "A" : m === "renamed" ? "R" : m === "untracked" ? "U" : m === "conflicted" ? "C" : "D"}</span>}
                  <button className="x" aria-label={`Close ${n?.name ?? nameOf(x.path)}`} title={`Close (${modKey()}+W)`} tabIndex={-1}
                    onClick={(e) => { e.stopPropagation(); askClose(x.path, e.currentTarget.parentElement); }}><X className="icon" /></button>
                </div>
              );
            })}
          </div>

          {t && <FileBar t={t} name={nameOf(t.path)} locked={locked} folderBar={agentAtWork || notMine} ignoredIn={ignoredIn} size={entry?.size ?? null}
            reload={() => reload(t.path, false)} keepMine={() => keepMine(t.path)} save={() => save(t.path)}
            cancel={() => patch(t.path, { disk: "kept" })} closeTab={() => close(t.path)} openIn={<OpenInMenu folder={id} name={nameOf(t.path)} path={t.path} line={cursor.line} fail={setBanner} />} />}

          <div className="ed-code" style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0, position: "relative" }}>
            <div ref={host} className="ed-host" style={{ flex: 1, minHeight: 0, display: activeState ? "flex" : "none", flexDirection: "column" }} />
            {!activeState && (
              <div className="ed-empty">
                {!t ? <span className="faint">Open a file from the tree, or press {modKey()}+P to go to one.</span>
                  : t.status === "loading" ? <span className="faint">Loading…</span>
                  : t.status === "error" ? <div className="empty"><b>{t.error}</b><button className="btn sm" onClick={() => close(t.path)}>Close tab</button></div>
                  : t.disk === "gone" ? <div className="empty"><b>{t.path} isn't there any more.</b><button className="btn sm" onClick={() => close(t.path)}>Close tab</button></div>
                  : file?.tooLarge ? <div className="empty"><FileCode className="icon" /><b>Too big to show here{entry ? ` (${sizeLabel(entry.size)})` : ""}.</b>{openInFirst(t.path)}</div>
                  : file?.binary ? <div className="empty"><FileCode className="icon" /><b>A binary file{entry ? ` (${sizeLabel(entry.size)})` : ""}: Gizai shows text only.</b>{openInFirst(t.path)}</div>
                  : null}
              </div>
            )}
          </div>

          <div className="ed-status" role="status" aria-label="Status bar">
            {t && <>
              <span className="path" title={t.path}>{t.path}</span>
              {entry?.ignored && !ignoredIn ? <span className="item">Ignored by git</span> : mark ? <span className={`item git ${mark}`}>{MARK_WORD[mark]}</span> : null}
              <span className="grow" />
              {activeState && <button className="item" title={`Go to line (${modKey()}+Alt+G)`} onClick={() => { if (view.current) gotoLine(view.current); }}>{cursorLabel(cursor.line, cursor.col, cursor.sel)}</button>}
              {file?.text != null && <span className="item" title="Line endings, kept as the file has them">{endingsLabel(file.crlf, file.bom)}</span>}
              {file?.text != null && <span className="item">{languageName(t.path)}</span>}
              {t.saving ? <span className="item">Saving…</span>
                : t.saveError ? <span className="item bad">Couldn't save</span>
                : locked && file?.text != null ? <span className="item"><Lock className="icon" />{readOnlyStatus(lockWhy)}</span>
                : t.dirty ? <button className="item unsaved" title={`Save (${modKey()}+S)`} onClick={() => save(t.path)}>Not saved</button>
                : Date.now() - t.reloaded < 5000 ? <span className="item">Reloaded: changed on disk</span>
                : file?.text != null ? <span className="item">Saved</span> : null}
            </>}
          </div>
        </div>
      </div>

      {goto && <GoToFile folder={id} name={name} onClose={() => setGoto(false)} onOpen={(p) => { reveal(p); openFile(p); }} />}
      {closing && createPortal(
        <CloseAsk at={closing} name={nameOf(closing.path)}
          save={() => { const p = closing.path; save(p); setClosing(null); waitSaved(p, () => close(p)); }}
          discard={() => close(closing.path)} cancel={() => setClosing(null)} />,
        document.body,
      )}
    </div>
  );

  /** Closes the tab once its save went through (Save and close); a refused save leaves it open with its bar. */
  function waitSaved(path: string, then: () => void) {
    const check = (n: number) => setTimeout(() => {
      const x = tabsRef.current.find((y) => y.path === path);
      if (!x) return;
      if (!x.saving && !x.dirty && !x.saveError && !x.disk) then();
      else if (x.saving && n < 100) check(n + 1);
    }, 100);
    check(0);
  }
}

/** Whether the file itself is read-only (over 1 MB, ignored, not UTF-8…), apart from its folder (an agent at work, which
 *  the folder's bar says and which ends with the run). */
const ownLock = (file: OpenedFile | null | undefined) => !!file?.readOnly && (Object.values(WHY) as string[]).includes(file.readOnlyWhy ?? "");

/** The open file's bar, under the tabs: changed or deleted on disk, Keep mine and its second Ctrl+S, couldn't save, and
 *  read-only. At most one: the first that applies. */
function FileBar({ t, name, locked, folderBar, ignoredIn, size, reload, keepMine, save, cancel, closeTab, openIn }: {
  t: Tab; name: string; locked: boolean; folderBar: boolean; ignoredIn: string | null; size: number | null;
  reload: () => void; keepMine: () => void; save: () => void; cancel: () => void; closeTab: () => void; openIn: React.ReactNode;
}) {
  const ctrl = modKey();
  if (t.disk === "changed") return (
    <div className="ed-bar file refused" role="alert">
      <TriangleAlert className="icon" />
      <span className="say"><span className="mono">{t.path}</span> changed on disk while you were editing it.</span>
      <span className="acts">
        <button className="btn sm" title="Show the version on disk: your unsaved changes here are lost" onClick={reload}>Reload</button>
        <button className="btn sm" title="Keep your version: saving it replaces the one on disk" onClick={keepMine}>Keep mine</button>
      </span>
    </div>
  );
  if (t.disk === "kept") return (
    <div className="ed-bar file refused" role="status">
      <TriangleAlert className="icon" />
      <span className="say">Keeping your version of {name}. The one on disk is newer: {ctrl}+S asks once more before saving over it.</span>
      <span className="acts"><button className="btn sm" title="Show the version on disk: your unsaved changes here are lost" onClick={reload}>Reload</button></span>
    </div>
  );
  if (t.disk === "confirm") return (
    <div className="ed-bar file refused" role="alert">
      <TriangleAlert className="icon" />
      <span className="say">Save over the newer {name} on disk? Press {ctrl}+S again.</span>
      <span className="acts"><button className="btn sm primary" onClick={save}>Save over it</button><button className="btn sm" onClick={cancel}>Cancel</button></span>
    </div>
  );
  if (t.disk === "deleted") return (
    <div className="ed-bar file refused" role="alert">
      <TriangleAlert className="icon" />
      <span className="say"><span className="mono">{t.path}</span> was deleted on disk.</span>
      <span className="acts"><button className="btn sm" onClick={save}>Save it again</button><button className="btn sm" onClick={closeTab}>Close tab</button></span>
    </div>
  );
  if (t.disk === "gone") return (
    <div className="ed-bar file refused" role="status">
      <TriangleAlert className="icon" />
      <span className="say"><span className="mono">{t.path}</span> isn't there any more.</span>
      <span className="acts"><button className="btn sm" onClick={closeTab}>Close tab</button></span>
    </div>
  );
  if (t.saveError) return (
    <div className="ed-bar file failed" role="alert">
      <TriangleAlert className="icon" />
      <span className="say">Couldn't save {name}: {t.saveError.replace(/\.$/, "")}. Your changes are still here.</span>
      <span className="acts"><button className="btn sm" onClick={save}>Try again</button></span>
    </div>
  );
  const ro = locked && !folderBar && t.file?.text != null ? readOnlyBar(t.file?.readOnlyWhy ?? null, size, ignoredIn) : null;
  if (ro) return (
    <div className="ed-bar file" role="status">
      <Lock className="icon" />
      <span className="say">{ro}</span>
      {t.file?.readOnlyWhy !== WHY.ignored && <span className="acts">{openIn}</span>}
    </div>
  );
  return null;
}

/** Closing a tab that isn't saved: Save and close, Close without saving, Cancel. */
function CloseAsk({ at, name, save, discard, cancel }: { at: { x: number; y: number }; name: string; save: () => void; discard: () => void; cancel: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    (ref.current?.querySelector("button") as HTMLButtonElement | null)?.focus();
    const down = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) cancel(); };
    window.addEventListener("mousedown", down);
    return () => window.removeEventListener("mousedown", down);
  }, [cancel]);
  return (
    <div ref={ref} className="pop ed-ask" role="alertdialog" aria-label={`Close ${name}`}
      style={{ position: "fixed", left: Math.min(at.x, window.innerWidth - 392), top: at.y }}
      onKeyDown={(e) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); cancel(); } }}>
      <p>{name} has changes that aren't saved.</p>
      <div className="acts">
        <button className="btn sm primary" onClick={save}>Save and close</button>
        <button className="btn sm" onClick={discard}>Close without saving</button>
        <button className="btn sm" onClick={cancel}>Cancel</button>
      </div>
    </div>
  );
}
