import { useEffect, useState } from "react";
import { listOldWorktrees, removeOldWorktrees } from "../api";
import type { OldWorktree } from "../types";
import { formatBytes } from "../lib/format";

/** Settings → Data: the worktrees of Done and Cancelled cards with their disk use. Remove asks first. */
export function OldWorktrees() {
  const [list, setList] = useState<OldWorktree[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  /** The cards whose worktrees wait for your confirmation. */
  const [asking, setAsking] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);
  const load = () => listOldWorktrees().then((l) => { setList(l); setErr(null); }).catch((e) => setErr(String(e)));
  useEffect(() => { load(); }, []);
  const remove = async (ids: string[]) => {
    setBusy(true);
    try {
      const out = await removeOldWorktrees(ids);
      const removed = out.filter((r) => r.removed).length;
      const kept = out.filter((r) => !r.removed).map((r) => `${r.identifier}: ${r.note}.`);
      setMsg([`Removed ${removed} ${removed === 1 ? "worktree" : "worktrees"}.`, ...kept].join(" "));
    } catch (e) { setMsg(String(e)); }
    setBusy(false);
    setAsking(null);
    load();
  };
  if (err) return <span className="warn">{err}</span>;
  if (!list) return <span className="faint">Counting disk use…</span>;
  const removable = list.filter((w) => !w.live);
  const size = (ws: OldWorktree[]) => formatBytes(ws.reduce((n, w) => n + w.bytes, 0));
  const askingFor = list.filter((w) => asking?.includes(w.taskId));
  return (
    <div className="old-worktrees">
      {list.length === 0 ? <span className="faint">No Done or Cancelled card has a worktree.</span> : (
        <ul>
          {list.map((w) => (
            <li key={w.taskId}>
              <span className="ow-card"><b className="mono">{w.identifier}</b> {w.title}
                <span className="faint"> · {w.projectName} · {w.category === "done" ? "Done" : "Cancelled"}{w.uncommitted > 0 ? ` · ${w.uncommitted} uncommitted ${w.uncommitted === 1 ? "change" : "changes"}` : ""}{w.live ? " · an agent is working in it" : ""}</span></span>
              <span className="ow-size mono">{formatBytes(w.bytes)}</span>
              <button className="btn ghost sm" disabled={busy || w.live} title={w.path} onClick={() => setAsking([w.taskId])}>Remove</button>
            </li>
          ))}
        </ul>
      )}
      {asking && askingFor.length > 0 ? (
        <div className="ow-confirm" role="alertdialog" aria-label="Remove worktrees">
          <span>{askingFor.length === 1 ? `Remove ${askingFor[0].identifier}'s worktree (${size(askingFor)})?` : `Remove ${askingFor.length} worktrees (${size(askingFor)})?`} Its folder is deleted; committed work stays on its branch, and a branch with commits main doesn't have is kept. One with uncommitted changes stays.</span>
          <span className="ow-actions"><button className="btn ghost sm" disabled={busy} onClick={() => setAsking(null)}>Keep</button>
            <button className="btn danger sm" disabled={busy} onClick={() => remove(asking)}>{busy ? "Removing…" : "Remove"}</button></span>
        </div>
      ) : removable.length > 1 && (
        <div><button className="btn sm" disabled={busy} onClick={() => setAsking(removable.map((w) => w.taskId))}>Remove all {removable.length} ({size(removable)})</button></div>
      )}
      {msg && <span className="hint" role="status">{msg}</span>}
    </div>
  );
}
