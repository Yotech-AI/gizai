// The Editor's terminal panel (GA-137, TF2; design/terminal/terminal.html §4, §5 and §7, research §5.1 and §5.2): the
// Terminal button in the top bar, and the panel in the code column, between the code and the status bar.
// - The button shows and hides the panel; when the folder has no terminal yet it starts one in the folder's root. It is
//   off (aria-disabled, so its tooltip says why) while a terminal can't start there and none is open there yet.
// - The panel's tabs are the folder's terminals, the same set as the Terminal page's, with +, Own page and Hide. Its height
//   is a share of the column, the same in every folder; whether it shows is kept per folder (src/lib/terminalPanel.ts).
// - One view per terminal, through TF0's store: Own page moves it to #/terminal/<id>; Show in Editor (TF1's sidebar and
//   page) puts it in this panel, which then opens on its tab.
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ChevronDown, Maximize2, Plus, Terminal as TerminalIcon, X } from "lucide-react";
import { terminalClose, terminalOpen, terminalRename } from "../../api";
import type { TerminalSession } from "../../types";
import { go } from "../../router";
import { nextAfterClose, stepTab } from "../../lib/editor";
import { noteOpened, showIn, useShownHere, useShownIn, useTerminals, useTerminalsLoaded, type TerminalPlace } from "../../lib/terminals";
import {
  clampPanel, FIRST_SHARE, keepShare, keepShown, loadShare, loadShown, SHARE_STEP, shareFor, tabTitle,
} from "../../lib/terminalPanel";
import { TerminalView } from "../terminal/TerminalView";

/** The tab each folder's panel had open, while Gizai runs, so coming back to a folder shows the same terminal. */
const lastTab = new Map<string, string>();
/** A card's terminal named after the card ("GA-118", "GA-118 2"): its identifier in mono, as the sidebar shows it. */
const CARD_NAME = /^[A-Z][A-Z0-9]*-\d+( \d+)?$/;

export type TerminalPanelState = ReturnType<typeof useTerminalPanel>;

/** The panel's state for the Editor's folder: its terminals, which one shows, whether the panel shows, and its actions.
 *  `off` is why a terminal can't start there now, or null; `fail` shows an error in the Editor's banner; `focusCode`
 *  gives the focus back to the code when the panel hides with the focus in it. */
export function useTerminalPanel(folder: string, path: string, off: string | null, fail: (message: string) => void, focusCode: () => void) {
  const all = useTerminals();
  const loaded = useTerminalsLoaded();
  const here = useMemo(() => all.filter((s) => s.folder === folder), [all, folder]);
  const place = useMemo<TerminalPlace>(() => ({ kind: "panel", folder }), [folder]);
  // a terminal put in this panel (Show in Editor, or the panel's own tab)
  const placed = useShownHere(place);
  const placedIds = placed ? placed.split("\n") : [];  const [open, setOpen] = useState(() => loadShown(folder) || placedIds.length > 0);
  const [active, setActive] = useState<string | null>(() => placedIds[placedIds.length - 1] ?? lastTab.get(folder) ?? null);
  /** The next terminal to show takes the focus: after the button, a key, a tab or +; not when the page opens. */
  const [focusNext, setFocusNext] = useState(false);
  const [starting, setStarting] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);

  const ids = here.map((s) => s.id);
  const current = active && ids.includes(active) ? active : ids[0] ?? null;
  const visible = open && !!current;  useEffect(() => { if (current) lastTab.set(folder, current); }, [folder, current]);

  // Show in Editor put one of this folder's terminals in the panel: the panel opens on its tab.
  const seen = useRef(placed);
  useEffect(() => {
    const before = new Set(seen.current ? seen.current.split("\n") : []);
    seen.current = placed;
    const fresh = placedIds.filter((id) => !before.has(id));
    const id = fresh[fresh.length - 1];
    if (!id) return;
    setActive(id);
    if (!open) { setOpen(true); keepShown(folder, true); setFocusNext(true); }
  }, [placed]); // eslint-disable-line react-hooks/exhaustive-deps

  // Its terminal closed: the next tab shows; the last one closed hides the panel.
  const before = useRef<string[]>(ids);
  useEffect(() => {
    const was = before.current;
    before.current = ids;
    if (active && !ids.includes(active) && was.includes(active)) {
      const next = nextAfterClose(was.filter((x) => ids.includes(x) || x === active), active);
      setActive(next);
    }
    if (loaded && was.length > 0 && ids.length === 0 && open) { setOpen(false); keepShown(folder, false); }
  }, [ids.join("\n")]); // eslint-disable-line react-hooks/exhaustive-deps

  const show = (id?: string) => {
    if (id) setActive(id);
    setOpen(true);
    keepShown(folder, true);
    setFocusNext(true);
  };
  const hide = () => {
    const inside = !!panelRef.current && panelRef.current.contains(document.activeElement);
    setOpen(false);
    keepShown(folder, false);
    if (inside) requestAnimationFrame(focusCode);
  };
  /** A new terminal in the folder's root, in a new tab (+, Ctrl+Shift+`, the button with none open yet). */
  const start = () => {
    if (off || starting) return;
    setStarting(true);
    // 0 × 0: the backend starts it at 80 × 24, and the panel fits it to its place at once
    terminalOpen(folder, 0, 0, null)
      .then((s) => { noteOpened(s); show(s.id); })
      .catch((e) => fail(`Couldn't start a terminal in ${path}: ${String(e).replace(/^Error:\s*/, "")}`))
      .finally(() => setStarting(false));
  };
  /** The Terminal button and Ctrl+`: shows the panel (starting a terminal when the folder has none), or hides it. */
  const toggle = () => {
    if (visible) hide();
    else if (here.length) show();
    else start();
  };
  /** Ctrl+Shift+`: a new tab; while none can start, the panel with the terminals it has. */
  const newTab = () => {
    if (!off) start();
    else if (here.length) show();
  };
  const pick = (id: string) => { setActive(id); setFocusNext(true); };
  /** The Terminal button is off while a terminal can't start and none is open here; with one open it shows and hides. */
  const buttonOff = !!off && here.length === 0;
  return { folder, place, here, current, visible, off, buttonOff, starting, focusNext, panelRef, toggle, newTab, start, hide, pick, fail };
}

/** The Terminal button in the Editor's top bar, between Fetch and Open in (design §4). */
export function TerminalButton({ p }: { p: TerminalPanelState }) {
  const title = p.buttonOff ? p.off ?? "" : p.visible ? "Hide the terminal (Ctrl+`)" : "Show the terminal (Ctrl+`)";
  return (
    <button className="btn ghost sm term-btn" aria-pressed={p.visible} aria-disabled={p.buttonOff || undefined} title={title}
      onClick={() => { if (!p.buttonOff) p.toggle(); }}>
      <TerminalIcon className="icon" />Terminal
    </button>
  );
}

/** The panel: the drag handle, the folder's terminals as tabs with +, Own page and Hide, and the terminal. */
export function TerminalPanel({ p, name }: { p: TerminalPanelState; name: string }) {
  const [share, setShare] = useState(loadShare);
  const [asking, setAsking] = useState<{ s: TerminalSession; x: number; y: number } | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const s = p.here.find((x) => x.id === p.current);
  const ids = p.here.map((x) => x.id);
  const close = (t: TerminalSession, el: HTMLElement | null) => {
    if (t.busy && !t.ended) {
      const r = el?.getBoundingClientRect();
      setAsking({ s: t, x: r?.left ?? 200, y: (r?.bottom ?? 100) + 4 });
      return;
    }
    terminalClose(t.id).catch((e) => p.fail(`Couldn't close ${t.name}: ${String(e).replace(/^Error:\s*/, "")}`));
  };
  const ownPage = () => {
    if (!s) return;
    showIn(s.id, { kind: "page" });
    go({ page: "terminal", id: s.id });
  };
  return (
    <div ref={p.panelRef} className="term-panel" style={{ "--term-share": `${share}%` } as React.CSSProperties}
      onKeyDown={(e) => {
        // Ctrl+PgDn and Ctrl+PgUp: the next and previous terminal tab (the terminal lets them through)
        if (e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey && (e.key === "PageDown" || e.key === "PageUp") && ids.length > 1) {
          e.preventDefault();
          const next = stepTab(ids, p.current, e.key === "PageDown" ? 1 : -1);
          if (next) p.pick(next);
        }
      }}>
      <Handle panel={p.panelRef} share={share} setShare={setShare} />
      <div className="ed-tabs term-tabs" role="tablist" aria-label={`Terminals in ${name}`}>
        {p.here.map((t) => (
          <TermTab key={t.id} s={t} on={t.id === p.current} ids={ids} renaming={renaming === t.id}
            pick={p.pick} close={close} rename={(on) => setRenaming(on ? t.id : null)} fail={p.fail} />
        ))}
        <button className={`btn ghost sm icon-only${p.off ? " term-btn" : ""}`} aria-label={`New terminal in ${name}`}
          title={p.off ?? `New terminal in ${name} (Ctrl+Shift+\`)`} aria-disabled={!!p.off || p.starting || undefined}
          onClick={() => { if (!p.off) p.start(); }}>
          <Plus className="icon" />
        </button>
        <span className="acts">
          <button className="btn ghost sm" title={`Open ${s?.name ?? name} on its own page`} disabled={!s} onClick={ownPage}><Maximize2 className="icon" />Own page</button>
          <button className="btn ghost sm" title="Hide the terminal (Ctrl+`): its shells keep running" onClick={p.hide}><ChevronDown className="icon" />Hide</button>
        </span>
      </div>
      {p.current ? <TerminalView key={p.current} id={p.current} place={p.place} autoFocus={p.focusNext} /> : <div className="term-place" />}
      {asking && createPortal(
        <CloseAsk at={asking} s={asking.s} cancel={() => setAsking(null)}
          close={() => { const t = asking.s; setAsking(null); terminalClose(t.id).catch((e) => p.fail(`Couldn't close ${t.name}: ${String(e).replace(/^Error:\s*/, "")}`)); }} />,
        document.body,
      )}
    </div>
  );
}

/** The drag handle, the panel's top border (design §7): drag it between the limits (at least 120px of code and of panel),
 *  a double-click gives 30% back, ↑ and ↓ move it 5% when it has the focus. The share is kept when you let go. */
function Handle({ panel, share, setShare }: { panel: React.RefObject<HTMLDivElement | null>; share: number; setShare: (n: number) => void }) {
  const drag = useRef<{ y: number; h: number; code: number; col: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const latest = useRef(share);
  latest.current = share;
  /** The panel's, the code's and the column's heights now. */
  const measure = () => {
    const el = panel.current;
    const col = el?.parentElement;
    const code = col?.querySelector<HTMLElement>(":scope > .ed-code");
    if (!el || !col || !code) return null;
    return { h: el.getBoundingClientRect().height, code: code.getBoundingClientRect().height, col: col.getBoundingClientRect().height };
  };
  const set = (n: number, keep: boolean) => { latest.current = n; setShare(n); if (keep) keepShare(n); };
  const end = () => {
    if (!drag.current) return;
    drag.current = null;
    ref.current?.classList.remove("is-drag");
    panel.current?.parentElement?.classList.remove("is-dragging");
    keepShare(latest.current);
  };
  useEffect(() => end, []); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div ref={ref} className="term-handle" role="separator" aria-orientation="horizontal" aria-label="Terminal panel's height"
      aria-valuenow={Math.round(share)} aria-valuemin={0} aria-valuemax={100} tabIndex={0}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        const m = measure();
        if (!m) return;
        e.preventDefault();
        try { e.currentTarget.setPointerCapture(e.pointerId); } catch { /* the pointer is gone already */ }
        drag.current = { y: e.clientY, ...m };
        e.currentTarget.classList.add("is-drag");
        panel.current?.parentElement?.classList.add("is-dragging");
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        set(shareFor(clampPanel(d.h + d.y - e.clientY, d.h, d.code), d.col), false);
      }}
      onPointerUp={end} onPointerCancel={end} onLostPointerCapture={end}
      onDoubleClick={() => set(FIRST_SHARE, true)}
      onKeyDown={(e) => {
        if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
        e.preventDefault();
        const m = measure();
        if (!m) return;
        const by = ((e.key === "ArrowUp" ? 1 : -1) * SHARE_STEP * m.col) / 100;
        set(shareFor(clampPanel(m.h + by, m.h, m.code), m.col), true);
      }} />
  );
}

/** A terminal's tab (design §3 and §7): its icon, its name, what runs in it, ×. A double-click renames it in place; a
 *  middle click or × closes it, asking under the tab first while a program runs; ← and → move between tabs. While its view
 *  is on its own page, its icon is Own page's. */
function TermTab({ s, on, ids, renaming, pick, close, rename, fail }: {
  s: TerminalSession; on: boolean; ids: string[]; renaming: boolean;
  pick: (id: string) => void; close: (s: TerminalSession, el: HTMLElement | null) => void; rename: (on: boolean) => void; fail: (m: string) => void;
}) {
  const shownAt = useShownIn(s.id);
  const away = shownAt?.kind === "page";
  const [name, setName] = useState(s.name);
  const done = useRef(false);
  const keep = () => {
    if (done.current) return;
    done.current = true;
    rename(false);
    if (name.trim() !== s.name) terminalRename(s.id, name.trim()).catch((e) => fail(`Couldn't rename ${s.name}: ${String(e).replace(/^Error:\s*/, "")}`));
  };
  const Icon = away ? Maximize2 : TerminalIcon;
  return (
    <div className="ed-tab term-tab" role="tab" data-term={s.id} tabIndex={on ? 0 : -1} aria-selected={on} title={tabTitle(s, away)}
      onClick={() => pick(s.id)}
      onDoubleClick={() => { done.current = false; setName(s.name); rename(true); }}
      onAuxClick={(e) => { if (e.button === 1) { e.preventDefault(); close(s, e.currentTarget); } }}
      onMouseDown={(e) => { if (e.button === 1) e.preventDefault(); }}
      onKeyDown={(e) => {
        if (renaming) return;
        if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
          e.preventDefault();
          const next = stepTab(ids, s.id, e.key === "ArrowRight" ? 1 : -1);
          if (!next) return;
          pick(next);
          (e.currentTarget.parentElement?.querySelector(`[data-term="${CSS.escape(next)}"]`) as HTMLElement | null)?.focus();
        } else if (e.key === "F2") { e.preventDefault(); done.current = false; setName(s.name); rename(true); }
      }}
      ref={(el) => { if (el && on) el.scrollIntoView({ block: "nearest", inline: "nearest" }); }}>
      <Icon className="icon" />
      {renaming ? (
        <input className="input name-in" value={name} maxLength={40} aria-label="Terminal's name" autoFocus spellCheck={false}
          onFocus={(e) => e.currentTarget.select()} onChange={(e) => setName(e.target.value)} onBlur={keep}
          onClick={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") { e.preventDefault(); keep(); }
            else if (e.key === "Escape") { e.preventDefault(); done.current = true; rename(false); }
          }} />
      ) : <span className="name">{s.taskId && CARD_NAME.test(s.name) ? <span className="card-id">{s.name}</span> : s.name}</span>}
      {s.busy && !s.ended ? <span className="prog">{s.busy}</span> : s.ended ? <span className="state">ended</span> : null}
      <button className="x" aria-label={`Close ${s.name}`} title="Close (middle click)" tabIndex={-1}
        onClick={(e) => { e.stopPropagation(); close(s, e.currentTarget.parentElement); }}><X className="icon" /></button>
    </div>
  );
}

/** Closing a terminal while a program runs in it, under its tab (design §3): Cancel, Close; Close has the focus. */
function CloseAsk({ at, s, close, cancel }: { at: { x: number; y: number }; s: TerminalSession; close: () => void; cancel: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const cancelRef = useRef(cancel);
  cancelRef.current = cancel;
  useEffect(() => {
    (ref.current?.querySelector(".btn.danger") as HTMLButtonElement | null)?.focus();
    const down = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) cancelRef.current(); };
    window.addEventListener("mousedown", down);
    return () => window.removeEventListener("mousedown", down);
  }, []);
  return (
    <div ref={ref} className="pop ed-ask" role="alertdialog" aria-label={`Close ${s.name}`}
      style={{ position: "fixed", left: Math.min(at.x, window.innerWidth - 392), top: at.y }}
      onKeyDown={(e) => {
        if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); cancel(); }
        else if (e.key === "Enter") { e.preventDefault(); e.stopPropagation(); close(); }
      }}>
      <p>{s.busy} is still running in {s.name}. Close it and end {s.busy}?</p>
      <div className="acts">
        <button className="btn ghost sm" onClick={cancel}>Cancel</button>
        <button className="btn danger sm" onClick={close}>Close</button>
      </div>
    </div>
  );
}
