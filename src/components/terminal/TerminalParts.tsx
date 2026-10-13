// What the sidebar's Terminal section and the Terminal page share (GA-136, TF1; design/terminal §1 to §3): New terminal
// (the Editor's folder picker, titled "New terminal in"), the name's box for Rename, the question before closing a
// terminal a program runs in (under a row, or under a tab), and Show in Editor.
import { useEffect, useRef, useState } from "react";
import { terminalClose, terminalOpen, terminalRename } from "../../api";
import type { EditorFolder, TerminalSession } from "../../types";
import { go } from "../../router";
import { manyNote, noteChanged, noteOpened, sentence, showIn, useTerminals } from "../../lib/terminals";
import { FolderPicker } from "../editor/FolderPicker";

/** Starts a terminal in the folder (an EditorFolder id); `name` null gives the folder's name, numbered. 0 × 0: the
 *  backend starts it at 80 × 24, and the place that shows it fits it at once. It is in the list at once. */
export function startTerminal(folder: string, name: string | null = null): Promise<TerminalSession> {
  return terminalOpen(folder, 0, 0, name).then((s) => { noteOpened(s); return s; });
}

/** Starts a terminal in the folder and opens its page; resolves with why it couldn't ("Couldn't start zsh in …: …."),
 *  or null. */
export function startAndOpen(folder: string, name: string | null = null): Promise<string | null> {
  return startTerminal(folder, name)
    .then((s) => { go({ page: "terminal", id: s.id }); return null; })
    .catch((e) => sentence(String(e)));
}

/** Closes the terminal: its shell ends, and everything it started (the backend takes up to about 3 s). */
export function closeTerminal(id: string) {
  terminalClose(id).catch(() => {});
}

/** Show in Editor, on the row's menu and the page's top bar (TF2 has the same rule, word for word): the folder in the
 *  Editor, with this terminal in its panel. */
export function showInEditor(session: TerminalSession) {
  showIn(session.id, { kind: "panel", folder: session.folder });
  go({ page: "editor", id: session.folder });
}

/** New terminal (design §2): the Editor's folder picker titled "New terminal in", the folder the page shows on top and
 *  chosen, the note from 20 open terminals; a choice starts a terminal there and opens its page. */
export function NewTerminalPicker({ at, current, you, onClose }: {
  at: { x: number; y: number }; current?: string; you: string; onClose: () => void;
}) {
  const open = useTerminals().length;
  return (
    <FolderPicker at={at} title="New terminal in" current={current} you={you} note={manyNote(open)} onClose={onClose}
      onPick={(f: EditorFolder) => startAndOpen(f.id)} />
  );
}

/** Rename in place, on a row, a tab or the page's name: the name, selected; Enter keeps it (as does leaving the box),
 *  Escape cancels. An empty name gives the default back (the backend's rule); at most 40 characters. */
export function NameInput({ session, onDone }: { session: TerminalSession; onDone: (kept: boolean) => void }) {
  const [value, setValue] = useState(session.name);
  const ended = useRef(false);
  const finish = (keep: boolean) => {
    if (ended.current) return;
    ended.current = true;
    if (keep && value.trim() !== session.name) terminalRename(session.id, value).then(noteChanged).catch(() => {});
    onDone(keep);
  };
  return (
    <input className="input name-in" value={value} maxLength={40} aria-label="Terminal's name" spellCheck={false} autoFocus
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setValue(e.target.value)}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") { e.preventDefault(); finish(true); }
        else if (e.key === "Escape") { e.preventDefault(); finish(false); }
      }}
      onBlur={() => finish(true)}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onAuxClick={(e) => e.stopPropagation()} />
  );
}

/** The question under a sidebar row (the tree's .ed-confirm): "npm is still running in gizai 2. Close it and end npm?"
 *  (closeQuestion, as it was when asked) with Close (it has the focus) and Cancel; Enter closes, Escape cancels. */
export function RowCloseAsk({ name, question, onClose, onCancel }: { name: string; question: string; onClose: () => void; onCancel: () => void }) {
  return (
    <div className="ed-confirm" role="alertdialog" aria-label={`Close ${name}`}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape") { e.preventDefault(); onCancel(); }
        else if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) { e.preventDefault(); onClose(); }
      }}>
      <span className="grow">{question}</span>
      <button className="btn danger sm" autoFocus onClick={onClose}>Close</button>
      <button className="btn ghost sm" onClick={onCancel}>Cancel</button>
    </div>
  );
}

/** The same question under a tab, as the Editor asks before closing a file that isn't saved: Cancel and Close, Close
 *  with the focus; a click outside cancels. */
export function TabCloseAsk({ name, question, at, onClose, onCancel }: {
  name: string; question: string; at: { x: number; y: number }; onClose: () => void; onCancel: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const cancel = useRef(onCancel);
  cancel.current = onCancel;
  useEffect(() => {
    (ref.current?.querySelector(".btn.danger") as HTMLButtonElement | null)?.focus();
    const down = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) cancel.current(); };
    window.addEventListener("mousedown", down);
    return () => window.removeEventListener("mousedown", down);
  }, []);
  return (
    <div ref={ref} className="pop ed-ask" role="alertdialog" aria-label={`Close ${name}`}
      style={{ position: "fixed", left: Math.max(8, Math.min(at.x, window.innerWidth - 392)), top: at.y }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape") { e.preventDefault(); onCancel(); }
        else if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) { e.preventDefault(); onClose(); }
      }}>
      <p>{question}</p>
      <div className="acts">
        <button className="btn ghost sm" onClick={onCancel}>Cancel</button>
        <button className="btn danger sm" onClick={onClose}>Close</button>
      </div>
    </div>
  );
}
