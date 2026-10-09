// A card's pull request on GitHub or Bitbucket (the project's link says which): its link and state, and Open pull request for a
// card in Review. On GitHub that pushes the branch over SSH or HTTPS, as Settings → GitHub says, and opens the pull request with gh;
// on Bitbucket it pushes over SSH and opens it through Bitbucket's API (Settings → Bitbucket). Opening the page of a card Gizai
// follows asks GitHub or Bitbucket at once, so a pull request an agent opened shows straight away, and a merge moves the card to
// Deploy (or Done when the team has no Deploy column).
import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, GitMerge, GitPullRequest, GitPullRequestClosed, GitPullRequestDraft, Upload } from "lucide-react";
import { checkPullRequest, getProject, openPullRequest } from "../api";
import { useData } from "../lib/useData";
import { hasPulls, HOST_NAMES, pullHostOf } from "../lib/provider";
import { followsPull, pullAction, pullBadge, pullHint, pullNumber } from "../lib/pulls";
import type { Task } from "../types";

const ICONS = { open: GitPullRequest, draft: GitPullRequestDraft, merged: GitMerge, closed: GitPullRequestClosed };

/** `mergeTo`: the column a merge moves the card to (the team's Deploy column; Done when left out). */
export function PullPanel({ task, live, mergeTo }: { task: Task; live: boolean; mergeTo?: string }) {
  const { data: project } = useData(() => (task.projectId ? getProject(task.projectId) : Promise.resolve(null)), [task.projectId]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [pushed, setPushed] = useState<string | null>(null);
  const [checkErr, setCheckErr] = useState<string | null>(null);
  const linked = hasPulls(project?.repoUrl);
  // GitHub or Bitbucket: the pull request's own link once there is one, else the project's
  const host = pullHostOf(task.prUrl || project?.repoUrl);
  const on = HOST_NAMES[host];
  const follow = linked && !!task.branch && followsPull(task) && !live;
  useEffect(() => {
    if (!follow) return;
    let alive = true;
    checkPullRequest(task.id).then(() => alive && setCheckErr(null)).catch((e) => alive && setCheckErr(String(e)));
    return () => { alive = false; };
  }, [task.id, follow]);
  useEffect(() => { setErr(null); setNote(null); setPushed(null); }, [task.id]);

  const action = linked ? pullAction(task, live) : null;
  if (!task.prUrl && !action) return null;
  const open = async () => {
    setBusy(true); setErr(null); setNote(null); setPushed(null);
    try {
      const pr = await openPullRequest(task.id);
      setNote(pr.note ?? null);
      setPushed(`Pushed ${task.branch} to ${on}${pr.number != null ? `; pull request #${pr.number} is ${pr.state}` : ""}.`);
    } catch (e) { setErr(String(e)); } finally { setBusy(false); }
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
            {n != null ? `#${n}` : `On ${on}`}<ExternalLink className="icon" style={{ width: 12, height: 12, marginLeft: 3 }} /></a>
          <span className={`badge ${badge.cls}`}>{badge.text}</span>
        </>}
        <span className="right">
          {action && <button className="btn sm primary" onClick={open} disabled={busy || !!action.why} title={action.why ?? undefined}>
            {action.label === "Push branch" ? <Upload className="icon" /> : <GitPullRequest className="icon" />}{busy ? "Pushing…" : action.label}</button>}
        </span>
      </div>
      <div className="run-summary">
        <span className="muted">{action?.why ?? pullHint(task, project?.defaultBranch ?? "main", mergeTo, host)}</span>
        {pushed && <div style={{ color: "var(--success)", fontSize: "var(--fs-sm)", marginTop: 6 }}>{pushed}</div>}
        {note && <div style={{ color: "var(--warning)", fontSize: "var(--fs-sm)", marginTop: 6 }}>{note}</div>}
        {err && <div role="alert" style={{ color: "var(--danger)", marginTop: 6 }}>{err}</div>}
        {checkErr && !err && <div className="muted" style={{ fontSize: "var(--fs-sm)", marginTop: 6 }}>Couldn't ask {on}: {checkErr}</div>}
      </div>
    </section>
  );
}
