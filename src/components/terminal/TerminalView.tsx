// A place for a terminal (GA-135, TF0; design §3 and §5): the Terminal page has one, and the Editor's terminal panel
// (TF2). When this place shows the terminal, the terminal's one view moves in, fits it and sends the size; when another
// place asked for it later, this place says where it is, with Show here. Find (Ctrl+Shift+F, Cmd+F on macOS) floats at
// the top right, so the terminal keeps its size; the Terminal page's ⋯ → Find opens it too (findAsk, GA-136).
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArrowDown, ArrowUp, CaseSensitive, Maximize2, Regex, WholeWord, X } from "lucide-react";
import type { TerminalSession } from "../../types";
import {
  attachView, detachView, endFind, findIn, fitView, focusTerminal, folderNameOf, leaveIn, onFindResults, placeKey, platformOf,
  samePlace, showIn, termKey, useShownIn, useTerminals, useViewMade, type FindOptions, type TerminalPlace,
} from "../../lib/terminals";

/** Where the terminal shows instead of here (design §8): "gizai 2 is on its own page." */
export function awayText(s: TerminalSession, shown: TerminalPlace | null): string {
  if (shown?.kind === "page") return `${s.name} is on its own page.`;
  if (shown?.kind === "panel") return `${s.name} is in ${folderNameOf(s)}'s Editor.`;
  return `${s.name} isn't shown anywhere.`;
}

export function TerminalView({ id, place, autoFocus = false, findAsk = 0 }: {
  id: string; place: TerminalPlace; autoFocus?: boolean;
  /** Counts up each time the place's own Find is chosen (the page's ⋯ menu): find opens. */
  findAsk?: number;
}) {
  const s = useTerminals().find((x) => x.id === id);
  const shown = useShownIn(id);
  const made = useViewMade(id);
  const here = samePlace(shown, place);
  const slot = useRef<HTMLDivElement>(null);
  const [finding, setFinding] = useState(false);
  const findBox = useRef<HTMLInputElement>(null);
  const key = placeKey(place);
  function openFind() {
    setFinding(true);
    requestAnimationFrame(() => { findBox.current?.focus(); findBox.current?.select(); });
  }

  // this place asks for the terminal when it shows, and gives it back when it goes
  useLayoutEffect(() => {
    showIn(id, place);
    return () => leaveIn(id, place);
  }, [id, key]); // eslint-disable-line react-hooks/exhaustive-deps

  useLayoutEffect(() => {
    const el = slot.current;
    if (!here || !made || !el) return;
    attachView(id, el).then((ok) => { if (ok && autoFocus) focusTerminal(id); });
    // fit and send the size at most every 50 ms while the place changes size
    let timer = 0;
    const ro = new ResizeObserver(() => { if (!timer) timer = window.setTimeout(() => { timer = 0; fitView(id); }, 50); });
    ro.observe(el);
    return () => { ro.disconnect(); window.clearTimeout(timer); detachView(id, el); };
  }, [id, here, made]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => { if (!here) setFinding(false); }, [here]);
  const findAsked = useRef(findAsk);
  useEffect(() => {
    if (findAsk === findAsked.current) return;
    findAsked.current = findAsk;
    if (here) openFind();
  }, [findAsk]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!s) return <div className="term-place" />;
  if (!here) {
    return (
      <div className="term-place">
        <div className="term-away">
          <Maximize2 className="icon" />
          <span>{awayText(s, shown)}</span>
          <button className="btn sm" onClick={() => showIn(id, place)}>Show here</button>
        </div>
      </div>
    );
  }
  return (
    <div className={`term-place${s.ended ? " ended" : ""}`}
      onKeyDown={(e) => {
        if (termKey(e, platformOf(navigator.userAgent || ""), false) === "find") { e.preventDefault(); e.stopPropagation(); openFind(); }
      }}>
      <div ref={slot} className="term-slot" />
      {finding && <TerminalFind id={id} input={findBox} onClose={() => { setFinding(false); endFind(id); focusTerminal(id); }} />}
    </div>
  );
}

/** Find in the terminal's scrollback (design §3): the query in mono, Match case, Words and Regular expression inside
 *  the box, "3 of 12", previous and next, Close. Enter finds the next, Shift+Enter the previous, Escape closes. */
function TerminalFind({ id, input, onClose }: { id: string; input: React.RefObject<HTMLInputElement | null>; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [opts, setOpts] = useState<FindOptions>({ caseSensitive: false, wholeWord: false, regex: false });
  const [result, setResult] = useState<{ index: number; count: number } | null>(null);
  useEffect(() => onFindResults(id, setResult), [id]);
  // the box has the focus when it opens (from the menu too, where it appears after the focus was asked for)
  useEffect(() => { input.current?.focus(); input.current?.select(); }, []); // eslint-disable-line react-hooks/exhaustive-deps
  const valid = !opts.regex || (() => { try { new RegExp(query); return true; } catch { return false; } })();
  const run = (q: string, o: FindOptions, how: "next" | "previous" | "typed") => {
    if (!q || (o.regex && !(() => { try { new RegExp(q); return true; } catch { return false; } })())) { findIn(id, "", o); setResult(null); return; }
    if (!findIn(id, q, o, how)) setResult({ index: -1, count: 0 });
  };
  const toggle = (k: keyof FindOptions) => { const o = { ...opts, [k]: !opts[k] }; setOpts(o); run(query, o, "typed"); };
  const count = !query ? "" : !valid ? "Not a valid regular expression" : !result || result.count === 0 ? "No results"
    : result.index < 0 ? `${result.count.toLocaleString("en-US")}+ results` : `${(result.index + 1).toLocaleString("en-US")} of ${result.count.toLocaleString("en-US")}`;
  return (
    <div className="term-find" role="search" aria-label="Find in the terminal">
      <div className="in">
        <input ref={input} className="input" value={query} placeholder="Find" aria-label="Find" spellCheck={false}
          onChange={(e) => { setQuery(e.target.value); run(e.target.value, opts, "typed"); }}
          onKeyDown={(e) => {
            if (e.key === "Enter") { e.preventDefault(); run(query, opts, e.shiftKey ? "previous" : "next"); }
            else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); }
          }} />
        <span className="ed-toggles">
          <button className="ed-toggle" aria-pressed={!!opts.caseSensitive} aria-label="Match case" title="Match case" onClick={() => toggle("caseSensitive")}><CaseSensitive className="icon" /></button>
          <button className="ed-toggle" aria-pressed={!!opts.wholeWord} aria-label="Words" title="Words" onClick={() => toggle("wholeWord")}><WholeWord className="icon" /></button>
          <button className="ed-toggle" aria-pressed={!!opts.regex} aria-label="Regular expression" title="Regular expression" onClick={() => toggle("regex")}><Regex className="icon" /></button>
        </span>
      </div>
      <span className={`count${query && (!valid || !result || result.count === 0) ? " none" : ""}`} role="status">{count}</span>
      <button className="btn ghost sm icon-only" aria-label="Previous match" title="Previous match (Shift+Enter)" onClick={() => run(query, opts, "previous")}><ArrowUp className="icon" /></button>
      <button className="btn ghost sm icon-only" aria-label="Next match" title="Next match (Enter)" onClick={() => run(query, opts, "next")}><ArrowDown className="icon" /></button>
      <button className="btn ghost sm icon-only" aria-label="Close" title="Close (Escape)" onClick={onClose}><X className="icon" /></button>
    </div>
  );
}
