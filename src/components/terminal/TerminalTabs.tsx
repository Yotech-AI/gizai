// The terminal tabs (GA-136, TF1; design/terminal §3 and §7): the terminals of one folder, drawn as the Editor's file tabs
// but size-term-tab high. A tab: the terminal icon, its name, what runs in it (the program in mono, or "ended") and ×,
// which shows on hover and on the open tab. + starts another terminal in the same folder, without a picker; it is off
// while an agent works on the card, with why as its tooltip. A double-click (or F2) renames a tab in place; × or a middle
// click closes it, asking under the tab first while a program runs; ← and → move between tabs that have the focus.
import { useEffect, useRef, useState } from "react";
import { Plus, Terminal as TerminalIcon, X } from "lucide-react";
import type { TerminalSession } from "../../types";
import { asksToClose, closeQuestion, folderNameOf, showsCardId, stepTerminal, tabTitle, useTerminals } from "../../lib/terminals";
import { closeTerminal, NameInput, TabCloseAsk } from "./TerminalParts";

export function TerminalTabs({ folder, active, onOpen, onNew, newOff }: {
  /** The folder (an EditorFolder id) whose terminals these are. */
  folder: string;
  /** The terminal shown under the tabs. */
  active: string | null;
  onOpen: (id: string) => void;
  onNew: () => void;
  /** Why + can't start a terminal here now (an agent works on the card), or null. */
  newOff: string | null;
}) {
  const tabs = useTerminals().filter((s) => s.folder === folder);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [asking, setAsking] = useState<{ id: string; name: string; question: string; at: { x: number; y: number } } | null>(null);
  const strip = useRef<HTMLDivElement>(null);
  const focusNext = useRef<string | null>(null);
  const tabEl = (id: string) => strip.current?.querySelector<HTMLElement>(`[data-term="${CSS.escape(id)}"]`) ?? null;
  const name = tabs[0] ? folderNameOf(tabs[0]) : folder.split(/[\\/]/).filter(Boolean).pop() ?? folder;

  // ← and → move the open tab and the focus with it
  useEffect(() => {
    if (active && focusNext.current === active) { focusNext.current = null; tabEl(active)?.focus(); }
  }, [active]); // eslint-disable-line react-hooks/exhaustive-deps

  const close = (s: TerminalSession, el: HTMLElement | null) => {
    if (!asksToClose(s)) { closeTerminal(s.id); return; }
    const r = el?.getBoundingClientRect();
    setAsking({ id: s.id, name: s.name, question: closeQuestion(s), at: { x: r?.left ?? 0, y: (r?.bottom ?? 0) + 4 } });
  };
  // the question goes with its terminal (closed elsewhere, or its shell ended)
  const ask = asking && tabs.some((s) => s.id === asking.id && !s.ended) ? asking : null;

  return (
    <>
      <div ref={strip} className="ed-tabs term-tabs" role="tablist" aria-label={`Terminals in ${name}`}>
        {tabs.map((s) => {
          const on = s.id === active;
          const nameEl = renaming === s.id
            ? <NameInput session={s} onDone={() => { setRenaming(null); requestAnimationFrame(() => tabEl(s.id)?.focus()); }} />
            : <span className={`name${s.taskId && showsCardId(s.name, folderNameOf(s)) ? " card-id" : ""}`}>{s.name}</span>;
          return (
            <div key={s.id} className="ed-tab term-tab" role="tab" data-term={s.id} tabIndex={on ? 0 : -1} aria-selected={on} title={tabTitle(s)}
              ref={(el) => { if (el && on) el.scrollIntoView({ block: "nearest", inline: "nearest" }); }}
              onClick={() => { if (!on) onOpen(s.id); }}
              onDoubleClick={() => setRenaming(s.id)}
              onMouseDown={(e) => { if (e.button === 1) e.preventDefault(); }}
              onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); close(s, e.currentTarget); } }}
              onKeyDown={(e) => {
                if (e.target !== e.currentTarget) return;
                if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
                  e.preventDefault();
                  const next = stepTerminal(tabs.map((x) => x.id), s.id, e.key === "ArrowRight" ? 1 : -1);
                  if (!next || next === s.id) return;
                  focusNext.current = next;
                  onOpen(next);
                } else if (e.key === "F2") { e.preventDefault(); setRenaming(s.id); }
              }}>
              <TerminalIcon className="icon" />
              {nameEl}
              {s.ended ? <span className="state">ended</span> : s.busy ? <span className="prog">{s.busy}</span> : null}
              <button className="x" aria-label={`Close ${s.name}`} title="Close (middle click)" tabIndex={-1}
                onClick={(e) => { e.stopPropagation(); close(s, e.currentTarget.parentElement); }}><X className="icon" /></button>
            </div>
          );
        })}
        <button className="btn ghost sm icon-only" aria-label={`New terminal in ${name}`} aria-disabled={newOff ? true : undefined}
          title={newOff ?? `New terminal in ${name} (Ctrl+Shift+\`)`} onClick={() => { if (!newOff) onNew(); }}><Plus className="icon" /></button>
      </div>
      {ask && (
        <TabCloseAsk name={ask.name} question={ask.question} at={ask.at} onCancel={() => { setAsking(null); tabEl(ask.id)?.focus(); }}
          onClose={() => { setAsking(null); closeTerminal(ask.id); }} />
      )}
    </>
  );
}
