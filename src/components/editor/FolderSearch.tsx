// Search in the folder (GA-113; design §4), in the tree's place: the query with Match case, Words and Regular expression
// inside its box, searching as you type (after 300 ms) with git grep over the files git tracks and new files it doesn't
// ignore; then the results by file, each hit's line number and line with the match marked. A click or Enter opens the
// file at that line. At most 2,000 results and 10 seconds (the backend's limits).
import { useEffect, useRef, useState } from "react";
import { CaseSensitive, ChevronDown, ChevronRight, Info, Regex, WholeWord } from "lucide-react";
import { editorSearch } from "../../api";
import type { SearchHit, SearchResult } from "../../types";
import { hitParts, hitsByFile, nameOf, parentOf, searchSummary } from "../../lib/editor";

export type SearchState = { query: string; caseSensitive: boolean; wholeWord: boolean; regex: boolean };

export function FolderSearch({ folder, name, state, setState, onOpen, focusKey }: {
  folder: string; name: string; state: SearchState; setState: (s: SearchState) => void;
  onOpen: (hit: SearchHit) => void; focusKey: number;
}) {
  const [result, setResult] = useState<SearchResult | null>(null);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [folded, setFolded] = useState<Set<string>>(new Set());
  const [chosen, setChosen] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => { input.current?.focus(); input.current?.select(); }, [focusKey]);
  useEffect(() => {
    const q = state.query;
    if (!q.trim()) { setResult(null); setError(null); setSearching(false); return; }
    let alive = true;
    setSearching(true);
    const t = setTimeout(() => {
      editorSearch(folder, q, state.caseSensitive, state.wholeWord, state.regex)
        .then((r) => { if (alive) { setResult(r); setError(null); setFolded(new Set()); } })
        .catch((e) => { if (alive) { setResult(null); setError(searchError(String(e), state.regex)); } })
        .finally(() => { if (alive) setSearching(false); });
    }, 300);
    return () => { alive = false; clearTimeout(t); };
  }, [folder, state.query, state.caseSensitive, state.wholeWord, state.regex]);

  const toggle = (key: "caseSensitive" | "wholeWord" | "regex") => setState({ ...state, [key]: !state[key] });
  const groups = result ? hitsByFile(result.hits) : [];
  const open = (hit: SearchHit) => { setChosen(`${hit.path}:${hit.line}`); onOpen(hit); };
  return (
    <>
      <div className="ed-search-in">
        <input ref={input} className="input" value={state.query} placeholder={`Search in ${name}`} aria-label={`Search in ${name}`}
          onChange={(e) => setState({ ...state, query: e.target.value })}
          onKeyDown={(e) => { if (e.key === "Enter" && result?.hits[0]) { e.preventDefault(); open(result.hits[0]); } }} />
        <div className="ed-toggles">
          <button className="ed-toggle" aria-pressed={state.caseSensitive} aria-label="Match case" title="Match case" onClick={() => toggle("caseSensitive")}><CaseSensitive className="icon" /></button>
          <button className="ed-toggle" aria-pressed={state.wholeWord} aria-label="Words" title="Words" onClick={() => toggle("wholeWord")}><WholeWord className="icon" /></button>
          <button className="ed-toggle" aria-pressed={state.regex} aria-label="Regular expression" title="Regular expression" onClick={() => toggle("regex")}><Regex className="icon" /></button>
        </div>
      </div>
      <div className={`ed-search-sum${error ? " error" : ""}`} role="status">
        {!state.query.trim() ? <><Info className="icon" /><span>Searches the files git tracks and new files it doesn't ignore. Ignored files (node_modules/, vendor/, .env) are left out.</span></>
          : searching ? <span>Searching…</span>
          : error ? <span>{error}</span>
          : result ? <span>{searchSummary(result)}</span> : null}
      </div>
      <div className="ed-results" role="tree" aria-label="Results">
        {!searching && groups.map((g) => {
          const shut = folded.has(g.path);
          return (
            <div key={g.path}>
              <div className="ed-row" role="treeitem" aria-expanded={!shut} tabIndex={0} title={g.path}
                onClick={() => setFolded((s) => { const n = new Set(s); if (n.has(g.path)) n.delete(g.path); else n.add(g.path); return n; })}>
                {shut ? <ChevronRight className="icon" /> : <ChevronDown className="icon" />}
                <span className="name">{nameOf(g.path)}</span>
                <span className="where">{parentOf(g.path)}</span>
                <span className="count">{g.hits.length}</span>
              </div>
              {!shut && g.hits.map((h) => {
                const p = hitParts(h, state.query, state);
                return (
                  <div key={`${h.line}:${h.column}`} className="ed-hit" role="treeitem" tabIndex={0} aria-selected={chosen === `${h.path}:${h.line}`}
                    onClick={() => open(h)} onKeyDown={(e) => { if (e.key === "Enter") open(h); }}>
                    <span className="no">{h.line}</span>
                    <span className="tx">{p.before}{p.match ? <mark>{p.match}</mark> : null}{p.after}</span>
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </>
  );
}

/** git grep's refusal, as the panel says it: a bad regular expression, or what failed. */
function searchError(e: string, regex: boolean): string {
  const said = e.replace(/^Error:\s*/, "");
  if (regex && /regex|regular expression|unmatched|parenthes|brack/i.test(said)) {
    return `Not a valid regular expression: ${said.replace(/^.*?:\s*/, "").replace(/\.$/, "")}.`;
  }
  return `Search failed: ${said.replace(/\.$/, "")}.`;
}
