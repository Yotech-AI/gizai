// The Terminal page (GA-135, TF0; design §3), simple for now: #/terminal starts a new terminal in a folder you pick in
// the Editor's folder picker, titled "New terminal in", and lists the terminals open now; #/terminal/<id> is that
// terminal under a plain top bar (Terminal / name). TF1 adds the tabs, the full top bar, the ended states and the sidebar.
import { useState } from "react";
import { CircleAlert, Plus, Terminal as TerminalIcon } from "lucide-react";
import { terminalOpen } from "../api";
import type { EditorFolder } from "../types";
import { go, href } from "../router";
import { FolderPicker } from "../components/editor/FolderPicker";
import { TerminalView } from "../components/terminal/TerminalView";
import { noteOpened, useTerminals, useTerminalsLoaded } from "../lib/terminals";

export function TerminalPage({ id }: { id?: string }) {
  return id ? <OneTerminal key={id} id={id} /> : <NewTerminal />;
}

/** New terminal: the folder picker, then the new terminal's page. */
function NewTerminal() {
  const list = useTerminals();
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const start = (f: EditorFolder) => {
    setError(null);
    // 0 × 0: the backend starts it at 80 × 24, and the page fits it to its place at once
    terminalOpen(f.id, 0, 0, null)
      .then((s) => { noteOpened(s); go({ page: "terminal", id: s.id }); })
      .catch((e) => setError(`Couldn't start a terminal in ${f.path}: ${String(e)}`));
  };
  return (
    <div className="term-page">
      <div className="topbar"><div className="crumbs"><b>Terminal</b></div></div>
      {error && <div className="ed-bar file failed" role="alert"><CircleAlert className="icon" /><span className="say">{error}</span></div>}
      <div className="ed-empty">
        <div className="empty">
          <TerminalIcon className="icon" />
          <b>{list.length ? "Terminals" : "No terminal open"}</b>
          <span>Start a shell in a project's folder, a folder in ~/Herd or a card's worktree.</span>
          <button className="btn primary term-new" onClick={(e) => { const r = e.currentTarget.getBoundingClientRect(); setAt({ x: r.left, y: r.bottom + 6 }); }}>
            <Plus className="icon" />New terminal
          </button>
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
      {at && <FolderPicker at={at} title="New terminal in" onPick={start} onClose={() => setAt(null)} />}
    </div>
  );
}

/** One terminal on its page, under a plain top bar. */
function OneTerminal({ id }: { id: string }) {
  const s = useTerminals().find((x) => x.id === id);
  const loaded = useTerminalsLoaded();
  return (
    <div className="term-page">
      <div className="topbar">
        <div className="crumbs ed-crumbs"><a href={href({ page: "terminal" })}>Terminal</a><span className="sep">/</span><b>{s?.name ?? ""}</b></div>
      </div>
      {s ? <TerminalView id={id} place={{ kind: "page" }} autoFocus />
        : loaded ? (
          <div className="ed-empty">
            <div className="empty">
              <TerminalIcon className="icon" />
              <b>This terminal isn't open any more.</b>
              <a className="btn" href={href({ page: "terminal" })}><Plus className="icon" />New terminal</a>
            </div>
          </div>
        ) : null}
    </div>
  );
}
