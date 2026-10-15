// The Terminal page (GA-135, TF0; GA-136, TF1; design/terminal §3). #/terminal is an empty page with New terminal (the
// Editor's folder picker, titled "New terminal in"), and the terminals open now. #/terminal/<id> is one terminal:
// - the top bar: Terminal / its name (a double-click or F2 renames it in place), the folder in mono, the shell and its size,
//   Show in Editor, and ⋯ (Rename, Find, Clear, Copy folder path, Close);
// - its folder's terminals as tabs, with + for another one there (also Ctrl+Shift+`); Ctrl+PgDn and Ctrl+PgUp switch tabs;
// - a bar under the tabs when the shell ended with another code than 0, with Start again (a new shell in the same folder
//   with the same name) and Close, or when a shell couldn't start, with Try again and Close;
// - the terminal, or TF0's placeholder with Show here while the Editor's panel shows it.
// A terminal that closes while its page shows (× , exit, the sidebar) gives way to the tab next to it.
import { useEffect, useRef, useState } from "react";
import { CircleAlert, Ellipsis, FolderCode, Info, Plus, RotateCcw, Terminal as TerminalIcon } from "lucide-react";
import { editorFolders } from "../api";
import type { EditorFolder, TerminalSession } from "../types";
import { go, href, type Route } from "../router";
import { modKey } from "../lib/keys";
import { onWindowFocus } from "../lib/windowFocus";
import {
  asksToClose, clearTerminal, closeQuestion, endedSay, folderNameOf, nextAfterClose, sentence, stepTerminal, useTerminalSize, useTerminals,
  useTerminalsLoaded,
} from "../lib/terminals";
import { ContextMenu, type MenuItem } from "../components/editor/Menus";
import { TerminalView } from "../components/terminal/TerminalView";
import { TerminalTabs } from "../components/terminal/TerminalTabs";
import { closeTerminal, NameInput, NewTerminalPicker, showInEditor, startTerminal, TabCloseAsk } from "../components/terminal/TerminalParts";

export function TerminalPage({ id, youId }: { id?: string; youId: string }) {
  return id ? <OneTerminal id={id} youId={youId} /> : <NoTerminal youId={youId} />;
}

/** Where a click asked for the folder picker: under the button. */
const under = (e: React.MouseEvent<HTMLElement>) => { const r = e.currentTarget.getBoundingClientRect(); return { x: r.left, y: r.bottom + 6 }; };

/** Goes to a route without leaving the page it replaces in the history (a closed terminal's address). */
const replace = (r: Route) => window.location.replace(href(r));

/** #/terminal: New terminal, and the terminals open now. */
function NoTerminal({ youId }: { youId: string }) {
  const list = useTerminals();
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return (
    <div className="term-page">
      <div className="topbar"><div className="crumbs"><b>Terminal</b></div></div>
      <div className="ed-empty">
        <div className="empty">
          <TerminalIcon className="icon" />
          <b>{list.length ? "Terminals" : "No terminal open"}</b>
          <span>Start a shell in a project's folder, a folder in ~/Herd or a card's worktree.</span>
          <button className="btn primary term-new" onClick={(e) => setAt(under(e))}><Plus className="icon" />New terminal</button>
          {list.length > 0 && (
            <div className="term-open">
              {list.map((s) => (
                <a key={s.id} href={href({ page: "terminal", id: s.id })} title={`${s.folderPath} · ${s.shell}`}>
                  <TerminalIcon className="icon" /><span>{s.name}</span>
                  <span className="where">{s.ended ? "ended" : s.busy ?? s.folderPath}</span>
                </a>
              ))}
            </div>
          )}
        </div>
      </div>
      {at && <NewTerminalPicker at={at} you={youId} onClose={() => setAt(null)} />}
    </div>
  );
}

/** A shell that couldn't start (from +, Start again or Try again): why, and what Try again and Close do. */
type Failed = { at: string; why: string; retry: () => void; close: () => void };

/** One terminal on its page. */
function OneTerminal({ id, youId }: { id: string; youId: string }) {
  const list = useTerminals();
  const loaded = useTerminalsLoaded();
  const s = list.find((x) => x.id === id);
  const size = useTerminalSize(id);
  const [renaming, setRenaming] = useState(false);
  const [more, setMore] = useState<{ x: number; y: number } | null>(null);
  const [asking, setAsking] = useState<{ x: number; y: number; question: string } | null>(null);
  const [findAsk, setFindAsk] = useState(0);
  const [failed, setFailed] = useState<Failed | null>(null);
  const [starting, setStarting] = useState(false);
  const [picker, setPicker] = useState<{ x: number; y: number } | null>(null);
  const [folder, setFolder] = useState<EditorFolder | null>(null);
  const moreBtn = useRef<HTMLButtonElement>(null);

  // the folder's tabs as they were, so the tab next to this one shows when it closes
  const seen = useRef<{ id: string; folder: string; tabs: string[] } | null>(null);
  if (s) seen.current = { id, folder: s.folder, tabs: list.filter((x) => x.folder === s.folder).map((x) => x.id) };
  useEffect(() => {
    const was = seen.current;
    if (s || !loaded || !was || was.id !== id) return;
    seen.current = null;
    const next = nextAfterClose(was.tabs, id, list.filter((x) => x.folder === was.folder).map((x) => x.id));
    replace(next ? { page: "terminal", id: next } : { page: "terminal" });
  }, [s, loaded, id, list]);

  // whether an agent works on the folder (a card's worktree) now: then + is off, with why
  const folderId = s?.folder ?? null;
  useEffect(() => {
    if (!folderId) return;
    let alive = true;
    const load = () => { editorFolders().then((fs) => { if (alive) setFolder(fs.find((f) => f.id === folderId) ?? null); }).catch(() => {}); };
    load();
    const stop = onWindowFocus(load);
    return () => { alive = false; stop(); };
  }, [folderId]);
  const newOff = folder?.readOnly ? `${folder.readOnlyWhy ?? "An agent is working on it"}: a terminal there waits until its run ends.` : null;

  useEffect(() => { setRenaming(false); setMore(null); setAsking(null); }, [id]);

  /** Starts a terminal in this folder (+, Ctrl+Shift+`), or another in place of an ended one (Start again), and shows it. */
  const start = (s: TerminalSession, again: TerminalSession | null) => {
    if (starting) return;
    setStarting(true);
    setFailed(null);
    startTerminal(s.folder, again ? again.name : null)
      .then((n) => {
        setStarting(false);
        go({ page: "terminal", id: n.id });
        if (again) closeTerminal(again.id); // after the new one is there, so the page never loses its place
      })
      .catch((e) => {
        setStarting(false);
        setFailed({ at: s.id, why: sentence(String(e)), retry: () => start(s, again), close: () => (again ? closeTerminal(again.id) : setFailed(null)) });
      });
  };
  const close = (s: TerminalSession) => {
    if (!asksToClose(s)) { closeTerminal(s.id); return; }
    const r = document.querySelector(`.term-tabs [data-term="${CSS.escape(s.id)}"]`)?.getBoundingClientRect();
    setAsking({ x: r?.left ?? 240, y: (r?.bottom ?? 80) + 4, question: closeQuestion(s) });
  };

  // the page's keys: Ctrl+PgDn and Ctrl+PgUp (they come through from the terminal), Ctrl+Shift+` and F2
  const keys = useRef<(e: KeyboardEvent) => void>(() => {});
  keys.current = (e) => {
    if (!s || e.defaultPrevented) return;
    const ctrl = e.ctrlKey && !e.metaKey && !e.altKey;
    if (ctrl && !e.shiftKey && (e.key === "PageDown" || e.key === "PageUp")) {
      e.preventDefault();
      const next = stepTerminal(list.filter((x) => x.folder === s.folder).map((x) => x.id), id, e.key === "PageDown" ? 1 : -1);
      if (next && next !== id) go({ page: "terminal", id: next });
    } else if (ctrl && e.shiftKey && e.code === "Backquote") {
      e.preventDefault();
      if (!newOff) start(s, null);
    } else if (e.key === "F2" && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
      const t = e.target as HTMLElement | null;
      if (t?.closest?.(".xterm, input, textarea, [contenteditable], .side, [role=tab], [role=dialog], [role=menu]")) return;
      e.preventDefault();
      setRenaming(true);
    }
  };
  useEffect(() => {
    const on = (e: KeyboardEvent) => keys.current(e);
    window.addEventListener("keydown", on);
    return () => window.removeEventListener("keydown", on);
  }, []);

  if (!s) {
    return (
      <div className="term-page">
        <div className="topbar"><div className="crumbs"><a href={href({ page: "terminal" })}>Terminal</a></div><div className="actions" /></div>
        {loaded && seen.current?.id !== id && (
          <div className="ed-empty">
            <div className="empty">
              <TerminalIcon className="icon" />
              <b>This terminal isn't open any more.</b>
              <button className="btn" onClick={(e) => setPicker(under(e))}><Plus className="icon" />New terminal</button>
            </div>
          </div>
        )}
        {picker && <NewTerminalPicker at={picker} you={youId} onClose={() => setPicker(null)} />}
      </div>
    );
  }

  const name = folderNameOf(s);
  const cols = size?.cols ?? s.cols;
  const rows = size?.rows ?? s.rows;
  const isMac = modKey() === "Cmd";
  const items: MenuItem[] = [
    { label: "Rename", kbd: "F2", run: () => setRenaming(true) },
    { label: "Find", kbd: isMac ? "Cmd F" : "Ctrl Shift F", run: () => setFindAsk((n) => n + 1) },
    { label: "Clear", title: "Empties the scrollback", run: () => clearTerminal(s.id) },
    { label: "Copy folder path", run: () => { navigator.clipboard?.writeText(s.folder).catch(() => {}); } },
    { sep: true },
    { label: "Close", run: () => close(s) },
  ];
  const bar = failed && failed.at === id ? (
    <div className="ed-bar file failed" role="alert">
      <CircleAlert className="icon" />
      <span className="say">{failed.why}</span>
      <span className="acts">
        <button className="btn sm" disabled={starting} onClick={failed.retry}><RotateCcw className="icon" />Try again</button>
        <button className="btn sm ghost" onClick={failed.close}>Close</button>
      </span>
    </div>
  ) : s.ended ? (
    <div className="ed-bar file" role="status">
      <Info className="icon" />
      <span className="say">{endedSay(s.ended)}</span>
      <span className="acts">
        <button className="btn sm" disabled={starting} onClick={() => start(s, s)}><RotateCcw className="icon" />Start again</button>
        <button className="btn sm ghost" onClick={() => closeTerminal(s.id)}>Close</button>
      </span>
    </div>
  ) : null;

  return (
    <div className="term-page">
      <div className="topbar">
        <div className="crumbs ed-crumbs">
          <a href={href({ page: "terminal" })}>Terminal</a><span className="sep">/</span>
          {renaming ? <NameInput session={s} onDone={() => setRenaming(false)} />
            : <b title="Double-click to rename (F2)" onDoubleClick={() => setRenaming(true)}>{s.name}</b>}
        </div>
        <div className="term-where">
          <span className="path" title={s.folderPath}>{s.folderPath}</span>
          <span className="sep">·</span><span>{s.shell}</span>
          <span className="sep">·</span><span className="size" title={`${cols} columns, ${rows} rows`}>{cols} × {rows}</span>
        </div>
        <div className="actions">
          <button className="btn sm" title={`Show ${s.name} in ${name}'s Editor, in its terminal panel`} onClick={() => showInEditor(s)}>
            <FolderCode className="icon" />Show in Editor
          </button>
          <button ref={moreBtn} className="btn ghost sm icon-only" aria-label="More" title="More" aria-haspopup="menu" aria-expanded={!!more}
            onClick={() => { const r = moreBtn.current?.getBoundingClientRect(); setMore(more ? null : { x: (r?.right ?? 0) - 240, y: (r?.bottom ?? 0) + 4 }); }}>
            <Ellipsis className="icon" />
          </button>
        </div>
      </div>
      <TerminalTabs folder={s.folder} active={id} onOpen={(t) => go({ page: "terminal", id: t })} onNew={() => start(s, null)} newOff={newOff} />
      {bar}
      <TerminalView key={id} id={id} place={{ kind: "page" }} autoFocus findAsk={findAsk} />
      {more && <ContextMenu at={more} label="More" items={items} onClose={() => setMore(null)} />}
      {asking && !s.ended && <TabCloseAsk name={s.name} question={asking.question} at={asking} onCancel={() => setAsking(null)} onClose={() => { setAsking(null); closeTerminal(s.id); }} />}
    </div>
  );
}
