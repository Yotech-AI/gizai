// The task page, Paperclip layout: header line, title, description, acceptance criteria, attachments, the agent
// run, tabs (Comments, Activity, Runs) and a closable properties panel (]). Card | Changes at the top (GA-112): Changes
// shows what the card changed, full width (Properties closes; ] still brings it back), at #/task/<id>/changes.
import { useCallback, useEffect, useRef, useState } from "react";
import { Activity, Archive, ArchiveRestore, Copy, FileDiff, MessageSquare, PanelRight, Pencil, Play, SquareKanban } from "lucide-react";
import { addComment, archiveTask, cardChanges, getTask, getTeam, listComments, listRuns, listUsers, onRunsChanged, restoreTask, taskActivity, updateTask } from "../api";
import { href } from "../router";
import { useData } from "../lib/useData";
import { relTime } from "../lib/format";
import { modKey } from "../lib/keys";
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
import { ChangesView } from "../components/changes/ChangesView";
import { Diffstat } from "../components/changes/Diffstat";
import { tabLabel } from "../lib/changes";
import type { CardChanges } from "../types";

function readPref(k: string): string | null { try { return localStorage.getItem(k); } catch { return null; } }
function writePref(k: string, v: string) { try { localStorage.setItem(k, v); } catch { /* private mode */ } }

/** Shows Markdown; a click (or Edit) switches to the editor with its toolbar. Saves on blur or Ctrl+Enter, Escape cancels.
 *  `readOnly` (an archived card) only shows it. */
function EditableMarkdown({ label, value, placeholder, hint, onSave, readOnly }: {
  label: string; value: string; placeholder: string; hint?: string; onSave: (md: string) => Promise<void>; readOnly?: boolean;
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
  if (readOnly) {
    return (
      <section className="block md-block">
        <div className="block-head"><h3>{label}</h3></div>
        <div className="md-static">{value.trim() ? <MarkdownView md={value} /> : <span className="faint">None.</span>}</div>
      </section>
    );
  }
  return (
    <section className="block md-block">
      <div className="block-head">
        <h3>{label}</h3>{hint && <span className="faint" style={{ fontSize: "var(--fs-sm)" }}>{hint}</span>}
        <span className="right">{!editing && <button className="btn ghost sm" onClick={() => setEditing(true)}><Pencil className="icon" />Edit</button>}</span>
      </div>
      {editing ? (
        <div className="md-edit-box">
          <MarkdownEditor value={value} autoFocus ariaLabel={label} placeholder={placeholder} minHeight={120} hint={`${modKey()}+Enter saves · Esc cancels`}
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

/** A card's changes (its files and counts, without their diffs): when the page opens, and again when a run starts or
 *  ends. */
function useCardChanges(id: string) {
  const [data, setData] = useState<CardChanges | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(() => {
    cardChanges(id).then((d) => { setData(d); setError(null); }).catch((e) => { setData(null); setError(String(e)); });
  }, [id]);
  useEffect(() => {
    reload();
    let un: (() => void) | undefined, alive = true;
    onRunsChanged(() => alive && reload()).then((f) => (alive ? (un = f) : f())).catch(() => {});
    return () => { alive = false; un?.(); };
  }, [reload]);
  return { data, error, reload };
}

/** `view` "changes": the Changes tab (GA-112); `run`: only that run's changes. */
export function TaskPage({ id, view = "card", run }: { id: string; view?: "card" | "changes"; run?: string }) {
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
  // On Changes, Properties is closed until ] (or Show properties) brings it back; the Card tab keeps its own choice.
  const changes = useCardChanges(id);
  const onChanges = view === "changes";
  const [propsOnChanges, setPropsOnChanges] = useState(false);
  const viewRef = useRef(view);
  viewRef.current = view;
  useEffect(() => { if (view === "changes") setPropsOnChanges(false); }, [view, run]);
  const thread = useRef<HTMLDivElement>(null);
  const [readReason, setReadReason] = useState(0);
  useEffect(() => { if (task) setTitle(task.title); }, [task?.title]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, select, [contenteditable], .cm-editor, [role=dialog]") || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === "]" && viewRef.current === "changes") { e.preventDefault(); setPropsOnChanges((p) => !p); return; }
      if (e.key === "]") { e.preventDefault(); setPropsState((p) => { writePref("gizai-props", p ? "closed" : "open"); return !p; }); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  // Read in comments (the Hold row): the Comments tab, at the latest comment that holds the hold reason (an agent's summary
  // is posted as one), else at the thread.
  useEffect(() => {
    if (!readReason) return;
    const reason = task?.holdReason?.trim();
    const c = reason ? [...(comments ?? [])].reverse().find((x) => x.bodyMd.includes(reason)) : undefined;
    ((c && document.getElementById(`comment-${c.id}`)) || thread.current)?.scrollIntoView({ block: "start" });
  }, [readReason]); // eslint-disable-line react-hooks/exhaustive-deps

  if (error) return <div className="error-banner">{error}</div>;
  if (!task) return null;
  const fail = (e: unknown) => setErr(String(e));
  // Archived (GA-43): read-only, with Restore. Only a card in Done can be archived, and not while an agent works on it.
  const archived = !!task.archivedAt;
  const archive = () => { setErr(null); archiveTask(id).catch(fail); };
  const restore = () => { setErr(null); restoreTask(id).catch(fail); };
  // A merge moves the card to the team's Deploy column (merged, not deployed yet), else to Done.
  const columns = [...(team?.states ?? [])].sort((a, b) => (a.sortKey < b.sortKey ? -1 : 1));
  const mergeTo = (columns.find((s) => s.category === "deploy") ?? columns.find((s) => s.category === "done"))?.name;
  const saveTitle = () => {
    if (archived) return;
    const t = title.trim();
    if (!t) { setTitle(task.title); return; }
    if (t !== task.title) updateTask(id, { title: t }).catch(fail);
  };
  const showProps = onChanges ? propsOnChanges : props;
  const openProps = (on: boolean) => (onChanges ? setPropsOnChanges(on) : setProps(on));
  const label = tabLabel(changes.data);
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
          {archived ? <button className="btn primary sm" onClick={restore} title="Put it back at the bottom of Done"><ArchiveRestore className="icon" />Restore</button>
            : task.stateCategory === "done" && <button className="btn ghost sm" onClick={archive} disabled={live}
              title={live ? "An agent is working on this card" : "Take it off the board; the bin on the Tasks page lists it"}><Archive className="icon" />Archive</button>}
          {!showProps && <button className="btn ghost sm icon-only" aria-label="Show properties" title="Show properties (])" onClick={() => openProps(true)}><PanelRight className="icon" /></button>}
        </div>
      </div>
      <div className="tabs task-tabs" role="tablist" aria-label="The card and its changes">
        <a className="tab" role="tab" aria-selected={!onChanges} href={href({ page: "task", id })}><SquareKanban className="icon" />Card</a>
        <a className="tab" role="tab" aria-selected={onChanges} href={href({ page: "task", id, view: "changes" })}
          title={label === null ? undefined : "What this card changed"}>
          <FileDiff className="icon" />Changes
          {label === "none" ? <span className="n">No changes</span> : label && <> <Diffstat add={label.add} del={label.del} /></>}</a>
      </div>
      {err && <div className="error-banner" role="alert">{err}<button className="btn ghost sm" onClick={() => setErr(null)}>Dismiss</button></div>}
      <div className="split">
        {onChanges ? <ChangesView task={task} card={changes} runId={run} runs={runs} /> : <div className="doc">
          <div className="doc-in">
            <div className="task-head">
              <StatusIcon category={task.stateCategory} hold={task.hold} /><PriorityIcon priority={task.priority} />
              <span className="id">{task.identifier}</span>
              {task.projectName && <a className="proj" href={href({ page: "project", id: task.projectId! })}><span className="dot" style={{ width: 8, height: 8, borderRadius: "50%", background: task.projectColor ?? "var(--text-3)" }} />{task.projectName}</a>}
              {live && <span className="badge live"><span className="pulse" />Live</span>}
              {task.hold && <span className="badge needs">On hold</span>}
              {archived && <span className="badge"><Archive className="icon sm" />Archived</span>}
            </div>
            {archived && (
              <div className="archived-note" role="note">
                <span className="grow">Archived {relTime(task.archivedAt!)}{task.archivedBy ? ` by ${task.archivedBy}` : ""}. It is read-only: Restore puts it back at the bottom of {task.stateName}.</span>
              </div>
            )}
            <input className="title-input" aria-label="Title" value={title} readOnly={archived} onChange={(e) => setTitle(e.target.value)} onBlur={saveTitle}
              onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); if (e.key === "Escape") { setTitle(task.title); (e.target as HTMLInputElement).blur(); } }} />
            <EditableMarkdown label="Description" value={task.descriptionMd} readOnly={archived}
              placeholder="Describe the task. Markdown works: headings, bold, checklists. @ links a task, project, client or agent."
              onSave={(md) => updateTask(id, { descriptionMd: md }).catch(fail)} />
            <EditableMarkdown label="Acceptance criteria" value={task.acceptanceMd ?? ""} hint="QA checks these one by one" readOnly={archived}
              placeholder="- [ ] What must be true when this task is done"
              onSave={(md) => updateTask(id, { acceptanceMd: md }).catch(fail)} />
            <section className="block">
              <div className="block-head"><h3>Attachments</h3><span className="faint" style={{ fontSize: "var(--fs-sm)" }}>Screenshots, exports, specs</span></div>
              <FileDrop ownerType="task" ownerId={task.id} readOnly={archived} />
            </section>
            {team && !archived && <RunPanel task={task} team={team} />}
            {!archived && <PullPanel task={task} live={live} mergeTo={mergeTo} changes={changes.data} />}

            <div className="tabs" role="tablist" ref={thread}>
              {([["comments", MessageSquare, "Comments", comments?.length], ["activity", Activity, "Activity", undefined], ["runs", Play, "Runs", runs?.length]] as const).map(([k, I, l, n]) => (
                <button key={k} className="tab" role="tab" aria-selected={tab === k} onClick={() => setTab(k)}><I className="icon" />{l}{n ? <span className="n">{n}</span> : null}</button>
              ))}
            </div>

            {tab === "comments" && (
              <div>
                {(comments ?? []).map((c) => (
                  <div className="comment" key={c.id} id={`comment-${c.id}`}>
                    <Avatar name={c.authorName} kind={c.authorKind} size="lg" />
                    <div className="body">
                      <div className="by"><b>{c.authorName}</b><span className="faint" title={new Date(c.createdAt).toLocaleString("en-GB")}>{relTime(c.createdAt)}</span>{c.runId && <span className="badge info">from a run</span>}</div>
                      <MarkdownView md={c.bodyMd} />
                    </div>
                  </div>
                ))}
                {comments && comments.length === 0 && <p className="faint">No comments yet.</p>}
                {!archived && <div className="composer" style={{ marginTop: 14 }}>
                  <MarkdownEditor key={composerKey} value="" onChange={setDraft} onSave={post} ariaLabel="New comment" minHeight={72} hint={`${modKey()}+Enter posts`}
                    placeholder="Write a comment. @ links a task, project, client or agent." />
                  <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 8 }}>
                    <button className="btn primary" disabled={!draft.trim()} onClick={() => post(draft)}>Comment</button>
                  </div>
                </div>}
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
        </div>}
        {showProps && team && people && <Properties task={task} team={team} people={people} onError={setErr} onClose={() => openProps(false)} readOnly={archived}
          changes={changes.data}
          onReadComments={() => { setTab("comments"); setReadReason((n) => n + 1); }} />}
      </div>
    </>
  );
}
