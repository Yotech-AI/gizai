import { useEffect, useState } from "react";
import { listOldWorktrees, removeOldWorktreeAnyway, removeOldWorktrees } from "../api";
import type { OldWorktree } from "../types";
import { formatBytes } from "../lib/format";

/** How many of a worktree's uncommitted files show under its row. */
const SHOWN = 10;

const changes = (n: number) => `${n} uncommitted ${n === 1 ? "change" : "changes"}`;

/** Settings → Data: the worktrees of Done and Cancelled cards with their disk use and uncommitted files. Remove asks
 *  first; for a worktree with uncommitted changes it asks to Remove anyway, which puts them in git's stash first. */
export function OldWorktrees() {
  const [list, setList] = useState<OldWorktree[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  /** The cards whose worktrees wait for your confirmation. */
  const [asking, setAsking] = useState<string[] | null>(null);
  /** The card whose worktree with uncommitted changes waits for Remove anyway. */
  const [anyway, setAnyway] = useState<string | null>(null);
  /** Why a worktree stayed after Remove anyway, by card. */
  const [why, setWhy] = useState<Record<string, string>>({});
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
  const removeAnyway = async (taskId: string) => {
    setBusy(true);
    const stays = (note: string) => setWhy((m) => ({ ...m, [taskId]: note }));
    try {
      const r = await removeOldWorktreeAnyway(taskId);
      if (r.removed) {
        setMsg(`${r.identifier}: ${r.note}.`);
        setWhy((m) => { const next = { ...m }; delete next[taskId]; return next; });
      } else stays(r.note);
    } catch (e) { stays(String(e)); }
    setBusy(false);
    setAnyway(null);
    load();
  };
  const ask = (w: OldWorktree) => {
    if (w.uncommitted > 0) { setAsking(null); setAnyway(w.taskId); } else { setAnyway(null); setAsking([w.taskId]); }
  };
  if (err) return <span className="warn">{err}</span>;
  if (!list) return <span className="faint">Counting disk use…</span>;
  const removable = list.filter((w) => !w.live);
  const size = (ws: OldWorktree[]) => formatBytes(ws.reduce((n, w) => n + w.bytes, 0));
  const askingFor = list.filter((w) => asking?.includes(w.taskId));
  const anywayFor = list.find((w) => w.taskId === anyway && !w.live);
  return (
    <div className="old-worktrees">
      {list.length === 0 ? <span className="faint">No Done or Cancelled card has a worktree.</span> : (
        <ul>
          {list.map((w) => {
            const files = w.changes.slice(0, SHOWN);
            return (
              <li key={w.taskId}>
                <div className="ow-row">
                  <span className="ow-card"><b className="mono">{w.identifier}</b> {w.title}
                    <span className="faint"> · {w.projectName} · {w.category === "done" ? "Done" : "Cancelled"}{w.uncommitted > 0 ? ` · ${changes(w.uncommitted)}` : ""}{w.live ? " · an agent is working in it" : ""}</span></span>
                  <span className="ow-size mono">{formatBytes(w.bytes)}</span>
                  <button className="btn ghost sm" disabled={busy || w.live} title={w.path} onClick={() => ask(w)}>Remove</button>
                </div>
                {files.length > 0 && (
                  <ul className="ow-files" aria-label={`${w.identifier}'s uncommitted changes`}>
                    {files.map((f) => (
                      <li key={f.path}><span className={`ow-change ${f.change}`}>{f.change}</span><span className="ow-path mono">{f.path}</span></li>
                    ))}
                    {w.uncommitted > files.length && <li className="faint">and {w.uncommitted - files.length} more</li>}
                  </ul>
                )}
                {why[w.taskId] && <span className="warn" role="status">{w.identifier}: {why[w.taskId]}.</span>}
              </li>
            );
          })}
        </ul>
      )}
      {anywayFor ? (
        <div className="ow-confirm" role="alertdialog" aria-label="Remove a worktree with uncommitted changes">
          <span>{anywayFor.identifier} has {changes(anywayFor.uncommitted)}. Remove it anyway? Gizai puts {anywayFor.uncommitted === 1 ? "the change" : "the changes"} in git's stash first, so {anywayFor.uncommitted === 1 ? "it isn't" : "they aren't"} lost.</span>
          <span className="ow-actions"><button className="btn ghost sm" disabled={busy} onClick={() => setAnyway(null)}>Keep</button>
            <button className="btn danger sm" disabled={busy} onClick={() => removeAnyway(anywayFor.taskId)}>{busy ? "Removing…" : "Remove anyway"}</button></span>
        </div>
      ) : asking && askingFor.length > 0 ? (
        <div className="ow-confirm" role="alertdialog" aria-label="Remove worktrees">
          <span>{askingFor.length === 1 ? `Remove ${askingFor[0].identifier}'s worktree (${size(askingFor)})?` : `Remove ${askingFor.length} worktrees (${size(askingFor)})?`} Its folder is deleted; committed work stays on its branch, and a branch with commits main doesn't have is kept. One with uncommitted changes stays.</span>
          <span className="ow-actions"><button className="btn ghost sm" disabled={busy} onClick={() => setAsking(null)}>Keep</button>
            <button className="btn danger sm" disabled={busy} onClick={() => remove(asking)}>{busy ? "Removing…" : "Remove"}</button></span>
        </div>
      ) : removable.length > 1 && (
        <div><button className="btn sm" disabled={busy} onClick={() => { setAnyway(null); setAsking(removable.map((w) => w.taskId)); }}>Remove all {removable.length} ({size(removable)})</button></div>
      )}
      {msg && <span className="hint" role="status">{msg}</span>}
    </div>
  );
}
