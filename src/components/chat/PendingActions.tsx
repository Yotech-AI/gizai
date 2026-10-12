// GA-102: what the Team Lead couldn't do after a tool from outside Gizai, under the answer that tried it: each call with one
// plain line about what it does, a preview, and Approve and Discard; Approve all when there are several. Approve runs exactly
// that call, once, with every other rule in force; Discard runs nothing. Either way a note in the chat says what happened.
import { useState } from "react";
import { ArrowRight, Check, CircleAlert, X } from "lucide-react";
import { approvePendingAction, approvePendingActions, discardPendingAction } from "../../api";
import { foldDiff, outsideLabel } from "../../lib/pending";
import { usePending } from "../../lib/usePending";
import type { DiffLine, PendingAction, PendingPreview } from "../../types";
import { BusyButton } from "../BusyButton";

/** What changes against the current version, line by line; long unchanged runs fold until you open them. */
function Diff({ lines }: { lines: DiffLine[] }) {
  const [all, setAll] = useState(false);
  const parts = all ? lines : foldDiff(lines);
  return (
    <div className="pa-diff" aria-label="What changes">
      {parts.map((l, i) => l.kind === "fold"
        ? <button key={i} type="button" className="pa-fold" onClick={() => setAll(true)}>{l.count === 1 ? "1 unchanged line" : `${l.count} unchanged lines`}</button>
        : <div key={i} className={`pa-dl ${l.kind}`}><span className="pa-sign" aria-label={l.kind === "add" ? "added" : l.kind === "del" ? "removed" : undefined}>
            {l.kind === "add" ? "+" : l.kind === "del" ? "−" : ""}</span>{l.text || " "}</div>)}
    </div>
  );
}

/** What the call is about and what changes, the text it saves or adds, and why approving fails as things are now. */
function Preview({ p }: { p: PendingPreview }) {
  return (
    <>
      {p.fields.length > 0 && (
        <dl className="pa-fields">
          {p.fields.map((f, i) => (
            <div key={i}><dt>{f.label}</dt>
              <dd>{f.from != null ? <><span className="pa-from">{f.from}</span><ArrowRight className="icon sm" aria-label="becomes" />{f.value}</> : f.value}</dd></div>
          ))}
        </dl>
      )}
      {p.warning && <div className="pa-warn"><CircleAlert className="icon sm" /><span>{p.warning}</span></div>}
      {p.diff ? <Diff lines={p.diff} /> : p.text ? <pre className="pa-text" aria-label="Text">{p.text}</pre> : null}
    </>
  );
}

/** The calls one answer couldn't make, in the order it tried them. `onError`: why an Approve or Discard didn't go through. */
export function PendingActions({ actions, onError }: { actions: PendingAction[]; onError: (e: string) => void }) {
  // Busy from a click until the call has left the list (it was approved or discarded), and for Approve all until it has run
  // them all and they have left it, so nothing is clicked twice.
  const shown = (id: string) => actions.some((a) => a.id === id);
  const pending = usePending<string>((k, ran) => k === "all"
    ? Array.isArray(ran) && !(ran as PendingAction[]).some((a) => shown(a.id))
    : !shown(k.slice(k.indexOf(":") + 1)), actions[0]?.threadId);
  const fail = (e: unknown) => onError(String(e));
  const waiting = actions.filter((a) => a.status === "pending");
  const outside = actions[0]?.outsideTool ?? "";
  return (
    <section className="pending-actions" aria-label="Waiting for your OK">
      <div className="pa-head">
        <span className="badge needs">Waiting for your OK</span>
        <span className="pa-why" title={outside}>This answer used {outsideLabel(outside)}, a tool from outside Gizai, so Gizai didn't run {actions.length === 1 ? "this" : "these"}.</span>
        {waiting.length > 1 && (
          <BusyButton className="btn sm primary" pending={pending} name="all" busyLabel="Approving…" icon={<Check className="icon" />}
            title="Runs each of these, in this order" onClick={() => pending.act("all", () => approvePendingActions(waiting.map((a) => a.id)), fail)}>
            Approve all</BusyButton>
        )}
      </div>
      {actions.map((a) => (
        <div key={a.id} className="pa-card">
          <div className="pa-line"><b>{a.preview?.line ?? a.tool}</b><span className="pa-tool">{a.tool}</span></div>
          {a.preview && <Preview p={a.preview} />}
          <div className="pa-actions">
            {a.status === "running" ? <span className="faint">Approved: running…</span> : (
              <>
                <BusyButton className="btn sm primary" pending={pending} name={`approve:${a.id}`} busyLabel="Approving…" icon={<Check className="icon" />}
                  title="Runs exactly this call, once" onClick={() => pending.act(`approve:${a.id}`, () => approvePendingAction(a.id), fail)}>Approve</BusyButton>
                <BusyButton className="btn sm" pending={pending} name={`discard:${a.id}`} busyLabel="Discarding…" icon={<X className="icon" />}
                  title="Runs nothing" onClick={() => pending.act(`discard:${a.id}`, () => discardPendingAction(a.id), fail)}>Discard</BusyButton>
              </>
            )}
          </div>
        </div>
      ))}
    </section>
  );
}
