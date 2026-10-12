// Behind main in the Editor (GA-114, F3; design design/code-editor/editor.html §5): the top bar's standing and Fetch, and
// the bar under the top bar: behind with Update, a refusal (Update off, the same sentence as its tooltip), Update running,
// how it ended with Show output, and a fetch that failed with Try again. src/lib/behind.ts works out which and its words.
import { Fragment } from "react";
import { ArrowDownToLine, CircleAlert, CircleCheck, CloudOff, GitBranch, LoaderCircle, RefreshCw, TriangleAlert, X } from "lucide-react";
import type { MainBar } from "../../lib/behind";

/** The top bar's "main · 3 behind origin/main · 1 ahead · 2 changed", the branch in mono. */
export function Standing({ branch, words, tip }: { branch: string | null; words: string[]; tip: string }) {
  if (!branch && !words.length) return <div className="ed-standing" />;
  return (
    <div className="ed-standing" title={tip || undefined}>
      {branch && <><GitBranch className="icon sm" /><span className="branch">{branch}</span></>}
      {words.map((w, i) => <Fragment key={w}>{(branch || i > 0) && <span className="sep">·</span>}<span>{w}</span></Fragment>)}
    </div>
  );
}

/** Fetch, in the top bar: fetches main now; "Fetching…" while it does. Off while Update runs. */
export function FetchButton({ title, busy, off, onClick }: { title: string; busy: boolean; off: boolean; onClick: () => void }) {
  return (
    <button className="btn ghost sm" title={title} disabled={busy || off} aria-busy={busy || undefined} onClick={onClick}>
      <RefreshCw className={busy ? "icon spin" : "icon"} />{busy ? "Fetching…" : "Fetch"}
    </button>
  );
}

/** The bar about main, under the top bar. */
export function BehindBar({ bar, output, fetching, update, fetch, toggleOutput, dismiss }: {
  bar: MainBar;
  /** Update's output shows (Show output). */
  output: boolean;
  fetching: boolean;
  update: () => void;
  fetch: () => void;
  toggleOutput: () => void;
  dismiss: () => void;
}) {
  const dismissBtn = <button className="btn ghost sm icon-only" aria-label="Dismiss" title="Dismiss" onClick={dismiss}><X className="icon" /></button>;
  switch (bar.kind) {
    case "behind":
    case "refused":
      return (
        <div className={`ed-bar ${bar.kind}`} role={bar.kind === "refused" ? "alert" : "status"}>
          {bar.kind === "behind" ? <ArrowDownToLine className="icon" /> : <TriangleAlert className="icon" />}
          <span className="say">{bar.lead && <b>{bar.lead}</b>}{bar.rest}</span>
          <span className="acts">
            <button className={`btn sm${bar.update === "on" ? " primary" : ""}`} disabled={bar.update === "off"} title={bar.tip} onClick={update}>
              <ArrowDownToLine className="icon" />Update
            </button>
          </span>
        </div>
      );
    case "updating":
      return (
        <div className="ed-bar" role="status">
          <LoaderCircle className="icon spin" />
          <span className="say">{bar.text}</span>
          <span className="acts"><button className="btn sm" disabled aria-busy="true"><LoaderCircle className="icon spin" />Updating…</button></span>
        </div>
      );
    case "busy":
      return (
        <div className="ed-bar refused" role="alert">
          <TriangleAlert className="icon" />
          <span className="say">{bar.text}</span>
        </div>
      );
    case "fetch-failed":
      return (
        <div className="ed-bar refused" role="alert">
          <CloudOff className="icon" />
          <span className="say">{bar.text}</span>
          <span className="acts">
            <button className="btn sm" disabled={fetching} aria-busy={fetching || undefined} onClick={fetch}>
              <RefreshCw className={fetching ? "icon spin" : "icon"} />{fetching ? "Fetching…" : "Try again"}
            </button>
          </span>
        </div>
      );
    default: {
      const kind = bar.kind === "done" ? "done" : bar.kind === "stopped" ? "failed" : "refused";
      const Icon = bar.kind === "done" ? CircleCheck : bar.kind === "stopped" ? CircleAlert : TriangleAlert;
      return (
        <div className={`ed-bar ${kind}`} role={kind === "done" ? "status" : "alert"}>
          <Icon className="icon" />
          <span className="say">{bar.text}</span>
          <span className="acts">
            {bar.output && <button className="btn ghost sm" aria-expanded={output} onClick={toggleOutput}>{output ? "Hide output" : "Show output"}</button>}
            {dismissBtn}
          </span>
          {output && bar.output && <pre className="update-output" aria-label="What the update printed">{bar.output}</pre>}
        </div>
      );
    }
  }
}
