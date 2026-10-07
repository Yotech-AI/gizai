// A card's pull request on GitHub: its link and state, and Open pull request for a card in Review (pushes the branch
// with your git login and opens the pull request with gh). Opening the page of a card Gizai follows asks GitHub at
// once, so a pull request an agent opened shows straight away, and a merge moves the card to Done.
import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, GitMerge, GitPullRequest, GitPullRequestClosed, GitPullRequestDraft, Upload } from "lucide-react";
import { checkPullRequest, getProject, openPullRequest } from "../api";
import { useData } from "../lib/useData";
import { followsPull, pullAction, pullBadge, pullHint, pullNumber } from "../lib/pulls";
import type { Task } from "../types";

const ICONS = { open: GitPullRequest, draft: GitPullRequestDraft, merged: GitMerge, closed: GitPullRequestClosed };

export function PullPanel({ task, live }: { task: Task; live: boolean }) {
  const { data: project } = useData(() => (task.projectId ? getProject(task.projectId) : Promise.resolve(null)), [task.projectId]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [checkErr, setCheckErr] = useState<string | null>(null);
  const linked = !!project?.repoUrl && project.repoUrl.includes("github.com");
  const follow = linked && !!task.branch && followsPull(task) && !live;
  useEffect(() => {
    if (!follow) return;
    let alive = true;
    checkPullRequest(task.id).then(() => alive && setCheckErr(null)).catch((e) => alive && setCheckErr(String(e)));
    return () => { alive = false; };
  }, [task.id, follow]);
  useEffect(() => { setErr(null); setNote(null); }, [task.id]);

  const action = linked ? pullAction(task, live) : null;
  if (!task.prUrl && !action) return null;
  const open = async () => {
    setBusy(true); setErr(null); setNote(null);
    try { setNote((await openPullRequest(task.id)).note ?? null); } catch (e) { setErr(String(e)); } finally { setBusy(false); }
  };
  const state = (task.prState ?? "open") as keyof typeof ICONS;
  const Icon = ICONS[state] ?? GitPullRequest;
  const badge = pullBadge(task.prState);
  const n = task.prUrl ? pullNumber(task.prUrl) : null;
  return (
    <section className="run-card" aria-label="Pull request">
      <div className="run-head">
        <Icon className="icon" /><span className="title">Pull request</span>
        {task.prUrl && <>
          <a href={task.prUrl} title={task.prUrl} onClick={(e) => { e.preventDefault(); openUrl(task.prUrl!).catch(() => {}); }}>
            {n != null ? `#${n}` : "On GitHub"}<ExternalLink className="icon" style={{ width: 12, height: 12, marginLeft: 3 }} /></a>
          <span className={`badge ${badge.cls}`}>{badge.text}</span>
        </>}
        <span className="right">
          {action && <button className="btn sm primary" onClick={open} disabled={busy || !!action.why} title={action.why ?? undefined}>
            {action.label === "Push branch" ? <Upload className="icon" /> : <GitPullRequest className="icon" />}{busy ? "Pushing…" : action.label}</button>}
        </span>
      </div>
      <div className="run-summary">
        <span className="muted">{action?.why ?? pullHint(task, project?.defaultBranch ?? "main")}</span>
        {note && <div style={{ color: "var(--warning)", fontSize: "var(--fs-sm)", marginTop: 6 }}>{note}</div>}
        {err && <div role="alert" style={{ color: "var(--danger)", marginTop: 6 }}>{err}</div>}
        {checkErr && !err && <div className="muted" style={{ fontSize: "var(--fs-sm)", marginTop: 6 }}>Couldn't ask GitHub: {checkErr}</div>}
      </div>
    </section>
  );
}
