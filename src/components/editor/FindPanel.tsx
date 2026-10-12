// Find and replace in the file (Ctrl+F; GA-113, design §3): @codemirror/search's panel, drawn with the app's controls.
// The find box with Match case, Words and Regular expression inside it, "3 of 12", previous and next, Close; under it
// the replace box with Replace and Replace all. Enter finds the next, Shift+Enter the previous, Escape closes; in the
// replace box Enter replaces and Ctrl+Enter replaces all. The keys of the search keymap (F3, Ctrl+G…) work as before.
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import { CaseSensitive, ChevronDown, ChevronUp, Regex, WholeWord, X } from "lucide-react";
import {
  SearchQuery, closeSearchPanel, findNext, findPrevious, getSearchQuery, replaceAll, replaceNext, setSearchQuery,
} from "@codemirror/search";
import type { EditorState } from "@codemirror/state";
import type { EditorView, Panel, ViewUpdate } from "@codemirror/view";

/** Counting stops here, so a huge file stays quick: "1,000+". */
const COUNT_UP_TO = 1000;

/** Where the selection is among the matches, and how many there are; null for an empty or invalid query. */
function countOf(state: EditorState): { current: number; total: number; more: boolean } | null {
  const q = getSearchQuery(state);
  if (!q.search || !q.valid) return null;
  const sel = state.selection.main;
  const cursor = q.getCursor(state) as Iterator<{ from: number; to: number }>;
  let total = 0, current = 0;
  for (let r = cursor.next(); !r.done; r = cursor.next()) {
    total += 1;
    if (r.value.from === sel.from && r.value.to === sel.to) current = total;
    if (total >= COUNT_UP_TO) return { current, total, more: true };
  }
  return { current, total, more: false };
}

function FindPanel({ view, tick }: { view: EditorView; tick: number }) {
  const start = getSearchQuery(view.state);
  const [q, setQ] = useState({ search: start.search, replace: start.replace, caseSensitive: start.caseSensitive, wholeWord: start.wholeWord, regexp: start.regexp });
  const find = useRef<HTMLInputElement>(null);
  // the query can change from outside the panel (Ctrl+F on a new selection, Ctrl+D)
  useEffect(() => {
    const now = getSearchQuery(view.state);
    if (now.search !== q.search || now.replace !== q.replace || now.caseSensitive !== q.caseSensitive || now.wholeWord !== q.wholeWord || now.regexp !== q.regexp) {
      setQ({ search: now.search, replace: now.replace, caseSensitive: now.caseSensitive, wholeWord: now.wholeWord, regexp: now.regexp });
    }
  }, [tick]); // eslint-disable-line react-hooks/exhaustive-deps
  const put = (next: typeof q) => { setQ(next); view.dispatch({ effects: setSearchQuery.of(new SearchQuery(next)) }); };
  const toggle = (k: "caseSensitive" | "wholeWord" | "regexp") => put({ ...q, [k]: !q[k] });
  const n = countOf(view.state);
  const valid = new SearchQuery(q).valid;
  const count = !q.search ? "" : !valid ? "Not a valid regular expression" : !n || n.total === 0 ? "No results"
    : `${n.current ? `${n.current.toLocaleString("en-US")} of ` : ""}${n.total.toLocaleString("en-US")}${n.more ? "+" : ""}${n.current ? "" : n.total === 1 ? " result" : " results"}`;
  const readOnly = view.state.readOnly;
  return (
    <>
      <div className="in">
        <input ref={find} className="input" main-field="true" value={q.search} placeholder="Find" aria-label="Find"
          onChange={(e) => put({ ...q, search: e.target.value })}
          onKeyDown={(e) => {
            if (e.key === "Enter") { e.preventDefault(); (e.shiftKey ? findPrevious : findNext)(view); }
            else if (e.key === "Escape") { e.preventDefault(); closeSearchPanel(view); }
          }} />
        <div className="ed-toggles">
          <button className="ed-toggle" aria-pressed={q.caseSensitive} aria-label="Match case" title="Match case" onClick={() => toggle("caseSensitive")}><CaseSensitive className="icon" /></button>
          <button className="ed-toggle" aria-pressed={q.wholeWord} aria-label="Words" title="Words" onClick={() => toggle("wholeWord")}><WholeWord className="icon" /></button>
          <button className="ed-toggle" aria-pressed={q.regexp} aria-label="Regular expression" title="Regular expression" onClick={() => toggle("regexp")}><Regex className="icon" /></button>
        </div>
      </div>
      <div className="row">
        <span className="count" role="status">{count}</span>
        <button className="btn ghost sm icon-only" aria-label="Previous match (Shift+F3)" title="Previous match (Shift+F3)" onClick={() => findPrevious(view)}><ChevronUp className="icon" /></button>
        <button className="btn ghost sm icon-only" aria-label="Next match (F3)" title="Next match (F3)" onClick={() => findNext(view)}><ChevronDown className="icon" /></button>
      </div>
      <button className="btn ghost sm icon-only close" aria-label="Close (Escape)" title="Close (Escape)" onClick={() => closeSearchPanel(view)}><X className="icon" /></button>
      {!readOnly && <>
        <div className="in plain">
          <input className="input" value={q.replace} placeholder="Replace" aria-label="Replace"
            onChange={(e) => put({ ...q, replace: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === "Enter") { e.preventDefault(); (e.ctrlKey || e.metaKey ? replaceAll : replaceNext)(view); }
              else if (e.key === "Escape") { e.preventDefault(); closeSearchPanel(view); }
            }} />
        </div>
        <div className="row">
          <button className="btn sm" onClick={() => replaceNext(view)}>Replace</button>
          <button className="btn sm" onClick={() => replaceAll(view)}>Replace all</button>
        </div>
      </>}
    </>
  );
}

/** The panel for `search({ createPanel })`: React in the panel's own element, drawn again as the text, the selection or
 *  the query change. */
export function findPanel(view: EditorView): Panel {
  const dom = document.createElement("div");
  dom.className = "ed-find";
  dom.setAttribute("role", "search");
  const root = createRoot(dom);
  let tick = 0;
  const draw = () => { tick += 1; const t = tick; try { flushSync(() => root.render(<FindPanel view={view} tick={t} />)); } catch { root.render(<FindPanel view={view} tick={t} />); } };
  draw();
  return {
    dom,
    top: true,
    mount: () => { (dom.querySelector("[main-field]") as HTMLInputElement | null)?.select(); },
    update: (u: ViewUpdate) => {
      if (u.docChanged || u.selectionSet || u.transactions.some((tr) => tr.effects.some((e) => e.is(setSearchQuery)))) draw();
    },
    destroy: () => { setTimeout(() => root.unmount()); },
  };
}
