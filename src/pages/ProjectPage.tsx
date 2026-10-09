import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { FileText, ListTodo, Pencil, Plus } from "lucide-react";
import { checkRepo, getProject, listDocs, type RepoCheck } from "../api";
import { href } from "../router";
import { useData } from "../lib/useData";
import { linkLabel } from "../lib/provider";
import { relTime } from "../lib/format";
import { useDrawer } from "../lib/drawers";
import { FileDrop } from "../components/FileDrop";
import { MarkdownView } from "../components/MarkdownView";

export function ProjectPage({ id }: { id: string }) {
  const { data: p, error } = useData(() => getProject(id), [id]);
  const { data: docs } = useData(() => listDocs(id), [id]);
  const open = useDrawer();
  const [repo, setRepo] = useState<RepoCheck | null>(null);
  useEffect(() => { if (p?.repoPath) checkRepo(p.repoPath).then(setRepo).catch(() => setRepo(null)); else setRepo(null); }, [p?.repoPath]);
  if (error) return <div className="error-banner">{error}</div>;
  if (!p) return null;
  const total = p.openTasks + p.doneTasks;
  const showTasks = () => { try { localStorage.setItem("gizai-task-project", p.id); } catch { /* ignore */ } };
  return (
    <>
      <div className="topbar"><div className="crumbs"><a href={href({ page: "projects" })}>Projects</a><span className="sep">/</span><b>{p.name}</b></div></div>
      <div className="content">
        <div className="page">
          <div className="entity-head">
            <span className="avatar xl" style={{ borderRadius: "var(--radius-l)", background: p.color ?? "var(--selected)", color: "var(--bg)", fontWeight: 800 }}>{p.key.slice(0, 2)}</span>
            <div className="names"><h1>{p.name}</h1>
              <p><span className="mono">{p.number}</span> · tasks {p.key}-… · {p.clientId ? <a href={href({ page: "client", id: p.clientId })} style={{ color: "inherit" }}>{p.clientName}</a> : "Internal"} · {p.status}</p></div>
            <div className="actions">
              <a className="btn" href="#/tasks" onClick={showTasks}><ListTodo className="icon" />Open tasks</a>
              <button className="btn" onClick={() => open({ kind: "task", projectId: p.id })}><Plus className="icon" />New task</button>
              <button className="btn" onClick={() => open({ kind: "project", id: p.id })}><Pencil className="icon" />Edit</button>
            </div>
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "minmax(0, 1.4fr) minmax(320px, 1fr)", gap: 20, alignItems: "start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
              <section><div className="section-head"><h3>Goal</h3><button className="link" onClick={() => open({ kind: "project", id: p.id })}>Edit</button></div>
                <div className="panel" style={{ padding: "14px 16px" }}>{p.goalMd?.trim() ? <MarkdownView md={p.goalMd} /> : <span className="faint">No goal written yet. Agents read it with every task.</span>}</div></section>
              <section><div className="section-head"><h3>Docs</h3><button className="link" onClick={() => open({ kind: "doc", projectId: p.id })}><Plus className="icon sm" />New doc</button></div>
                <div className="panel">
                  {docs?.length ? docs.map((d) => (
                    <a key={d.id} className="doc-row" href={href({ page: "doc", id: d.id })}><FileText className="icon" style={{ color: "var(--text-3)" }} /><span className="grow">{d.title}</span>
                      <span className="faint">v{d.currentVersion} · {relTime(d.updatedAt)}</span></a>
                  )) : <div className="doc-row faint">Requirements, meeting notes, decisions: write them here in Markdown. Agents can read them.</div>}
                </div></section>
              <section><div className="section-head"><h3>Files</h3></div><FileDrop ownerType="project" ownerId={p.id} /></section>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
              <section><div className="section-head"><h3>Tasks</h3><a className="link" href="#/tasks" onClick={showTasks}>Open tasks</a></div>
                <div className="panel" style={{ padding: 16 }}>
                  {total === 0 ? <span className="faint">No tasks yet. Press N to add one.</span> : <>
                    <div className="t-metric">{p.doneTasks} / {total}</div><div className="faint" style={{ fontSize: "var(--fs-sm)" }}>tasks done · {p.openTasks} open</div>
                    <div style={{ height: 6, borderRadius: 3, background: "var(--line)", marginTop: 10, overflow: "hidden" }}><div style={{ height: "100%", width: `${(100 * p.doneTasks) / total}%`, background: "var(--success)" }} /></div></>}
                </div></section>
              <section><div className="section-head"><h3>Repository</h3></div>
                <div className="panel kv">
                  <span className="k">Local path</span><span className="mono" style={{ wordBreak: "break-all" }}>{p.repoPath ?? <span className="faint">Not linked</span>}</span>
                  <span className="k">{linkLabel(p.repoUrl)}</span><span className="mono" style={{ wordBreak: "break-all" }}>{p.repoUrl
                    ? <a href={p.repoUrl} onClick={(e) => { e.preventDefault(); openUrl(p.repoUrl!).catch(() => {}); }}>{p.repoUrl.replace(/^https:\/\//, "")}</a>
                    : <span className="faint">Not linked: cards start from the local branch</span>}</span>
                  <span className="k">Main branch</span><span className="mono">{p.defaultBranch}</span>
                  {p.repoPath && <><span className="k">Status</span><span>{repo == null ? "Checking…" : !repo.isGit ? <span style={{ color: "var(--danger)" }}>Not a git repository</span> : <>On <span className="mono">{repo.branch ?? "?"}</span>{repo.dirty ? <span style={{ color: "var(--warning)" }}> · uncommitted changes</span> : " · clean"}</>}</span></>}
                </div></section>
              <section><div className="section-head"><h3>Budget</h3></div>
                <div className="panel kv">
                  <span className="k">Amount</span><span>{p.budgetAmountMinor != null ? `€${(p.budgetAmountMinor / 100).toLocaleString("en-GB")}` : <span className="faint">—</span>}</span>
                  <span className="k">Hours</span><span>{p.budgetHours ?? <span className="faint">—</span>}</span>
                </div></section>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
