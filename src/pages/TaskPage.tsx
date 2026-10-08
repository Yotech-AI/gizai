// The task page, Paperclip layout: header line, title, description, acceptance criteria, attachments, the agent
// run, tabs (Comments, Activity, Runs) and a closable properties panel (]).
import { useEffect, useRef, useState } from "react";
import { Activity, Copy, MessageSquare, PanelRight, Pencil, Play } from "lucide-react";
import { addComment, getTask, getTeam, listComments, listRuns, listUsers, taskActivity, updateTask } from "../api";
import { href } from "../router";
import { useData } from "../lib/useData";
import { relTime } from "../lib/format";
import { describeChange } from "../lib/activity";
import { useLiveRuns } from "../lib/useLiveRuns";
import { PriorityIcon, StatusIcon } from "../components/StatusIcon";
import { Avatar } from "../components/Avatar";
import { MarkdownEditor } from "../components/MarkdownEditor";
import { MarkdownView } from "../components/MarkdownView";
import { RunHistory } from "../components/RunHistory";
import { Properties } from "../components/Properties";
import { FileDrop } from "../components/FileDrop";
import { RunPanel } from "../components/RunPanel";
import { PullPanel } from "../components/PullPanel";

function readPref(k: string): string | null { try { return localStorage.getItem(k); } catch { return null; } }
function writePref(k: string, v: string) { try { localStorage.setItem(k, v); } catch { /* private mode */ } }

/** Shows Markdown; a click (or Edit) switches to the editor with its toolbar. Saves on blur or Ctrl+Enter, Escape cancels. */
function EditableMarkdown({ label, value, placeholder, hint, onSave }: {
  label: string; value: string; placeholder: string; hint?: string; onSave: (md: string) => Promise<void>;
}) {
  const [editing, setEditingState] = useState(false);
  const closed = useRef(false); // Ctrl+Enter, blur and Escape can all fire while the editor closes: act once
  const setEditing = (on: boolean) => { if (on) closed.current = false; setEditingState(on); };
  const save = async (md: string) => {
    if (closed.current) return;
    closed.current = true;
    setEditing(false);
    if (md !== value) await onSave(md);
  };
  const cancel = () => { closed.current = true; setEditing(false); };
  return (
    <section className="block md-block">
      <div className="block-head">
        <h3>{label}</h3>{hint && <span className="faint" style={{ fontSize: "var(--fs-sm)" }}>{hint}</span>}
        <span className="right">{!editing && <button className="btn ghost sm" onClick={() => setEditing(true)}><Pencil className="icon" />Edit</button>}</span>
      </div>
      {editing ? (
        <div className="md-edit-box">
          <MarkdownEditor value={value} autoFocus ariaLabel={label} placeholder={placeholder} minHeight={120} hint="Ctrl+Enter saves · Esc cancels"
            onSave={save} onBlur={save} onCancel={cancel} />
        </div>
      ) : (
        <div className="md-click" role="button" tabIndex={0} aria-label={`Edit ${label.toLowerCase()}`}
          onClick={() => setEditing(true)} onKeyDown={(e) => { if (e.key === "Enter") setEditing(true); }}>
          {value.trim() ? <MarkdownView md={value} /> : <span className="faint">{placeholder}</span>}
        </div>
      )}
    </section>
  );
}

export function TaskPage({ id }: { id: string }) {
  const { data: task, error } = useData(() => getTask(id), [id]);
  const { data: team } = useData(() => getTeam());
  const { data: people } = useData(() => listUsers());
  const { data: comments } = useData(() => listComments(id), [id]);
  const { data: activity } = useData(() => taskActivity(id), [id]);
  const { data: runs } = useData(() => listRuns(id), [id]);
  const live = useLiveRuns().some((r) => r.taskId === id);
  const [tab, setTab] = useState<"comments" | "activity" | "runs">("comments");
  const [title, setTitle] = useState("");
  const [draft, setDraft] = useState("");
  const [composerKey, setComposerKey] = useState(0);
  const [err, setErr] = useState<string | null>(null);
  const [props, setPropsState] = useState(() => readPref("gizai-props") !== "closed");
  const setProps = (on: boolean) => { setPropsState(on); writePref("gizai-props", on ? "open" : "closed"); };
  useEffect(() => { if (task) setTitle(task.title); }, [task?.title]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, select, [contenteditable], .cm-editor, [role=dialog]") || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === "]") { e.preventDefault(); setPropsState((p) => { writePref("gizai-props", p ? "closed" : "open"); return !p; }); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (error) return <div className="error-banner">{error}</div>;
  if (!task) return null;
  const fail = (e: unknown) => setErr(String(e));
  // A merge moves the card to the team's Deploy column (merged, not deployed yet), else to Done.
  const columns = [...(team?.states ?? [])].sort((a, b) => (a.sortKey < b.sortKey ? -1 : 1));
  const mergeTo = (columns.find((s) => s.category === "deploy") ?? columns.find((s) => s.category === "done"))?.name;
  const saveTitle = () => {
    const t = title.trim();
    if (!t) { setTitle(task.title); return; }
    if (t !== task.title) updateTask(id, { title: t }).catch(fail);
  };
  const post = async (md: string) => {
    if (!md.trim()) return;
    try { await addComment(id, md); setDraft(""); setComposerKey((k) => k + 1); } catch (e) { fail(e); }
  };

  return (
    <>
      <div className="topbar">
        <div className="crumbs">
          <a href={href({ page: "tasks" })}>Tasks</a><span className="sep">/</span>
          {task.projectId && <><a href={href({ page: "project", id: task.projectId })}>{task.projectName}</a><span className="sep">/</span></>}
          <b className="mono">{task.identifier}</b>
        </div>
        <div className="actions">
          <button className="btn ghost sm" onClick={() => navigator.clipboard?.writeText(task.identifier).catch(() => {})} title="Copy the task ID"><Copy className="icon" />Copy ID</button>
          {!props && <button className="btn ghost sm icon-only" aria-label="Show properties" title="Show properties (])" onClick={() => setProps(true)}><PanelRight className="icon" /></button>}
        </div>
      </div>
      {err && <div className="error-banner" role="alert">{err}<button className="btn ghost sm" onClick={() => setErr(null)}>Dismiss</button></div>}
      <div className="split">
        <div className="doc">
          <div className="doc-in">
            <div className="task-head">
              <StatusIcon category={task.stateCategory} hold={task.hold} /><PriorityIcon priority={task.priority} />
              <span className="id">{task.identifier}</span>
              {task.projectName && <a className="proj" href={href({ page: "project", id: task.projectId! })}><span className="dot" style={{ width: 8, height: 8, borderRadius: "50%", background: task.projectColor ?? "var(--text-3)" }} />{task.projectName}</a>}
              {live && <span className="badge live"><span className="pulse" />Live</span>}
              {task.hold && <span className="badge needs">On hold</span>}
            </div>
            <input className="title-input" aria-label="Title" value={title} onChange={(e) => setTitle(e.target.value)} onBlur={saveTitle}
              onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); if (e.key === "Escape") { setTitle(task.title); (e.target as HTMLInputElement).blur(); } }} />
            <EditableMarkdown label="Description" value={task.descriptionMd}
              placeholder="Describe the task. Markdown works: headings, bold, checklists, KADE-12 refs, @mentions."
              onSave={(md) => updateTask(id, { descriptionMd: md }).catch(fail)} />
            <EditableMarkdown label="Acceptance criteria" value={task.acceptanceMd ?? ""} hint="QA checks these one by one"
              placeholder="- [ ] What must be true when this task is done"
              onSave={(md) => updateTask(id, { acceptanceMd: md }).catch(fail)} />
            <section className="block">
              <div className="block-head"><h3>Attachments</h3><span className="faint" style={{ fontSize: "var(--fs-sm)" }}>Screenshots, exports, specs</span></div>
              <FileDrop ownerType="task" ownerId={task.id} />
            </section>
            {team && <RunPanel task={task} team={team} />}
            <PullPanel task={task} live={live} mergeTo={mergeTo} />

            <div className="tabs" role="tablist">
              {([["comments", MessageSquare, "Comments", comments?.length], ["activity", Activity, "Activity", undefined], ["runs", Play, "Runs", runs?.length]] as const).map(([k, I, l, n]) => (
                <button key={k} className="tab" role="tab" aria-selected={tab === k} onClick={() => setTab(k)}><I className="icon" />{l}{n ? <span className="n">{n}</span> : null}</button>
              ))}
            </div>

            {tab === "comments" && (
              <div>
                {(comments ?? []).map((c) => (
                  <div className="comment" key={c.id}>
                    <Avatar name={c.authorName} kind={c.authorKind} size="lg" />
                    <div className="body">
                      <div className="by"><b>{c.authorName}</b><span className="faint" title={new Date(c.createdAt).toLocaleString("en-GB")}>{relTime(c.createdAt)}</span>{c.runId && <span className="badge info">from a run</span>}</div>
                      <MarkdownView md={c.bodyMd} />
                    </div>
                  </div>
                ))}
                {comments && comments.length === 0 && <p className="faint">No comments yet.</p>}
                <div className="composer" style={{ marginTop: 14 }}>
                  <MarkdownEditor key={composerKey} value="" onChange={setDraft} onSave={post} ariaLabel="New comment" minHeight={72} hint="Ctrl+Enter posts"
                    placeholder="Write a comment. Mention @someone or a task like KADE-12." />
                  <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 8 }}>
                    <button className="btn primary" disabled={!draft.trim()} onClick={() => post(draft)}>Comment</button>
                  </div>
                </div>
              </div>
            )}
            {tab === "activity" && (
              <ul className="activity">
                {(activity ?? []).slice().reverse().map((a, i) => (
                  <li key={i}><b>{a.actorName ?? "Gizai"}</b><span className="grow">{describeChange(a)}</span><span className="faint" title={new Date(a.at).toLocaleString("en-GB")}>{relTime(a.at)}</span></li>
                ))}
                {activity && activity.length === 0 && <li className="faint">Nothing yet.</li>}
              </ul>
            )}
            {tab === "runs" && <RunHistory runs={runs} />}
          </div>
        </div>
        {props && team && people && <Properties task={task} team={team} people={people} onError={setErr} onClose={() => setProps(false)} />}
      </div>
    </>
  );
}
