// The task page, Paperclip layout: header line, title, description, acceptance criteria, attachments, the agent
// run, tabs (Comments, Activity, Runs) and a closable properties panel (]). Card | Changes | Screenshots at the top
// (GA-112, GA-185): Changes shows what the card changed, Screenshots what its agents saw of the screens they built, each
// full width (Properties closes; ] still brings it back), at #/task/<id>/changes and #/task/<id>/screenshots. A card
// assigned to you has Open in editor in the top bar (GA-115): its worktree, made and prepared as for an agent, opens in
// the Editor, the card pinned to you.
import { useCallback, useEffect, useRef, useState } from "react";
import {
  Activity, Archive, ArchiveRestore, Copy, FileDiff, Images, Keyboard, LoaderCircle, MessageSquare, PanelRight, Pencil, Play, SquareCode, SquareKanban,
} from "lucide-react";
import {
  addComment, archiveTask, cardChanges, getProject, getTask, getTeam, handCard, listComments, listRuns, listScreenshots, listUsers, onRunsChanged,
  restoreTask, startByHand, taskActivity, updateTask,
} from "../api";
import { go, href } from "../router";
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
import { OpenInMenu } from "../components/editor/Menus";
import { rememberFolder } from "../lib/editor";
import { byHandTip, canDoByHand, couldntOpen, markCardSeen, openInEditorTitle } from "../lib/hand";
import { ChangesView } from "../components/changes/ChangesView";
import { Diffstat } from "../components/changes/Diffstat";
import { tabLabel } from "../lib/changes";
import { ScreenshotsView } from "../components/screenshots/ScreenshotsView";
import { roundCount } from "../lib/screenshots";
import { reveal } from "../lib/reveal";
import type { CardChanges, ScreenshotSet } from "../types";

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

/** The card's title. What you type stays in this box, so a keystroke redraws only it (GA-190); Enter or leaving the box
 *  saves it, Escape puts the saved title back, and an empty title isn't saved. */
function TitleInput({ title, readOnly, onSave }: { title: string; readOnly: boolean; onSave: (t: string) => void }) {
  const [value, setValue] = useState(title);
  useEffect(() => setValue(title), [title]);
  const cancelled = useRef(false);
  const save = () => {
    if (cancelled.current) { cancelled.current = false; return; }
    if (readOnly) return;
    const t = value.trim();
    if (!t) { setValue(title); return; }
    if (t !== title) onSave(t);
  };
  return (
    <input className="title-input" aria-label="Title" value={value} readOnly={readOnly} onChange={(e) => setValue(e.target.value)} onBlur={save}
      onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") { cancelled.current = true; setValue(title); e.currentTarget.blur(); } }} />
  );
}

/** Comments being written, by card: one stays while another tab of the card shows, or another page. */
const drafts = new Map<string, string>();

/** A new comment. Its text stays in this box, so a keystroke redraws only it (GA-190). Comment or Ctrl+Enter posts it;
 *  once posted the box is empty again. */
function CommentComposer({ taskId, onPost }: { taskId: string; onPost: (md: string) => Promise<boolean> }) {
  const [first] = useState(() => drafts.get(taskId) ?? "");
  const [text, setText] = useState(first);
  const [round, setRound] = useState(0);
  const write = (md: string) => { if (md) drafts.set(taskId, md); else drafts.delete(taskId); setText(md); };
  const post = async (md: string) => {
    if (!md.trim()) return;
    if (await onPost(md)) { drafts.delete(taskId); setText(""); setRound((n) => n + 1); }
  };
  return (
    <div className="composer" style={{ marginTop: 14 }}>
      <MarkdownEditor key={round} value={round ? "" : first} onChange={write} onSave={post} ariaLabel="New comment" minHeight={72} hint={`${modKey()}+Enter posts`}
        placeholder="Write a comment. @ links a task, project, client or agent." />
      <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 8 }}>
        <button className="btn primary" disabled={!text.trim()} onClick={() => post(text)}>Comment</button>
      </div>
    </div>
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

/** A card's screenshot sets (GA-185): when the page opens, and again when a run ends (they arrive before runs-changed). */
function useScreenshots(id: string) {
  const [data, setData] = useState<ScreenshotSet[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let un: (() => void) | undefined, alive = true;
    const reload = () => listScreenshots(id).then((d) => { if (alive) { setData(d); setError(null); } }).catch((e) => { if (alive) setError(String(e)); });
    reload();
    onRunsChanged(() => alive && reload()).then((f) => (alive ? (un = f) : f())).catch(() => {});
    return () => { alive = false; un?.(); };
  }, [id]);
  return { data, error };
}

/** `view` "changes": the Changes tab (GA-112); `run`: only that run's changes. "screenshots": the Screenshots tab (GA-185);
 *  `run`: only that run's set. `youId`: you, the person using this Gizai (Open in editor shows on a card assigned to you). */
export function TaskPage({ id, view = "card", run, youId }: { id: string; view?: "card" | "changes" | "screenshots"; run?: string; youId?: string }) {
  const { data: task, error } = useData(() => getTask(id), [id]);
  const { data: team } = useData(() => getTeam());
  const { data: people } = useData(() => listUsers(), [], { counts: false });
  const { data: comments } = useData(() => listComments(id), [id]);
  const { data: activity } = useData(() => taskActivity(id), [id]);
  const { data: runs } = useData(() => listRuns(id), [id]);
  const liveRun = useLiveRuns().find((r) => r.taskId === id);
  const live = !!liveRun;
  const [tab, setTab] = useState<"comments" | "activity" | "runs">("comments");
  const [err, setErr] = useState<string | null>(null);
  const [props, setPropsState] = useState(() => readPref("gizai-props") !== "closed");
  const setProps = (on: boolean) => { setPropsState(on); writePref("gizai-props", on ? "open" : "closed"); };
  // On Changes and Screenshots, Properties is closed until ] (or Show properties) brings it back; the Card tab keeps its own
  // choice.
  const changes = useCardChanges(id);
  const shots = useScreenshots(id);
  const { data: project } = useData(() => (task?.projectId ? getProject(task.projectId) : Promise.resolve(null)), [task?.projectId]);
  const onChanges = view === "changes";
  const onShots = view === "screenshots";
  const wide = view !== "card";
  const [propsOnChanges, setPropsOnChanges] = useState(false);
  const viewRef = useRef(view);
  viewRef.current = view;
  useEffect(() => { if (view !== "card") setPropsOnChanges(false); }, [view, run]);
  const thread = useRef<HTMLDivElement>(null);
  const [readReason, setReadReason] = useState(0);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, select, [contenteditable], .cm-editor, [role=dialog]") || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === "]" && viewRef.current !== "card") { e.preventDefault(); setPropsOnChanges((p) => !p); return; }
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
  // You look at the card (again as it changes): the Editor's card bar stops saying QA sent it back.
  useEffect(() => { markCardSeen(id); }, [id, runs]);
  // Your card by hand: where its worktree is, for Open in editor and Open in.
  const [opening, setOpening] = useState(false);
  const [handFolder, setHandFolder] = useState<string | null>(null);
  const shown = useRef(true); // still on this card: a worktree that took a while to prepare opens only then
  useEffect(() => { shown.current = true; return () => { shown.current = false; }; }, []);
  const byHandMine = !!youId && !!task?.byHand && task?.assigneeId === youId;
  useEffect(() => {
    if (!byHandMine) { setHandFolder(null); return; }
    let alive = true;
    handCard(id).then((c) => { if (alive) setHandFolder(c.folder); }).catch(() => { if (alive) setHandFolder(null); });
    return () => { alive = false; };
  }, [id, byHandMine]);

  if (error) return <div className="error-banner">{error}</div>;
  if (!task) return null;
  const fail = (e: unknown) => setErr(String(e));
  // Open in editor: a card started by hand just opens; else its worktree is made and prepared first (it can take a while).
  const agentAtWork = liveRun ? team?.members.find((m) => m.actorId === liveRun.agentId)?.name ?? "An agent" : null;
  const openInEditor = async () => {
    setErr(null);
    setOpening(true);
    try {
      const folder = (task.byHand && handFolder) || (await startByHand(id)).folder;
      rememberFolder(folder);
      if (shown.current) go({ page: "editor", id: folder });
    } catch (e) {
      if (shown.current) { setErr(couldntOpen(task.identifier, e)); setOpening(false); }
    }
  };
  // Archived (GA-43): read-only, with Restore. Only a card in Done can be archived, and not while an agent works on it.
  const archived = !!task.archivedAt;
  const archive = () => { setErr(null); archiveTask(id).catch(fail); };
  const restore = () => { setErr(null); restoreTask(id).catch(fail); };
  // A merge moves the card to the team's Deploy column (merged, not deployed yet), else to Done.
  const columns = [...(team?.states ?? [])].sort((a, b) => (a.sortKey < b.sortKey ? -1 : 1));
  const mergeTo = (columns.find((s) => s.category === "deploy") ?? columns.find((s) => s.category === "done"))?.name;
  const showProps = wide ? propsOnChanges : props;
  const openProps = (on: boolean) => (wide ? setPropsOnChanges(on) : setProps(on));
  const label = tabLabel(changes.data);
  // Screenshots (GA-185): the tab shows on a card whose project takes them (or whose own switch is on), and on one with sets.
  const shotsCount = shots.data ? roundCount(shots.data) : null;
  const shotsTab = onShots || (shots.data?.length ?? 0) > 0 || !!project?.screenshots?.on || task.screenshots === true;
  const agent = team?.members.find((m) => m.actorId === task.assigneeId && m.kind === "agent") ?? null;
  // Off for this card: Show properties opens Properties with the focus on its Screenshots switch.
  const showShotsSwitch = () => { openProps(true); reveal("props-screenshots"); };
  // View the run: the Card tab's Runs, at that run.
  const viewRun = (runId: string) => { setTab("runs"); go({ page: "task", id }); reveal(`run-${runId}`); };
  const post = async (md: string) => {
    try { await addComment(id, md); return true; } catch (e) { fail(e); return false; }
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
          {canDoByHand(task, youId) && (
            <button className="btn sm" disabled={live || opening} aria-busy={opening || undefined} title={openInEditorTitle(task, agentAtWork)} onClick={openInEditor}>
              {opening ? <><LoaderCircle className="icon spin" />Preparing the worktree…</> : <><SquareCode className="icon" />Open in editor</>}
            </button>
          )}
          {byHandMine && handFolder && !archived && <OpenInMenu folder={handFolder} name={task.identifier} path={null} line={null} fail={setErr} />}
          {archived ? <button className="btn primary sm" onClick={restore} title="Put it back at the bottom of Done"><ArchiveRestore className="icon" />Restore</button>
            : task.stateCategory === "done" && <button className="btn ghost sm" onClick={archive} disabled={live}
              title={live ? "An agent is working on this card" : "Take it off the board; the bin on the Tasks page lists it"}><Archive className="icon" />Archive</button>}
          {!showProps && <button className="btn ghost sm icon-only" aria-label="Show properties" title="Show properties (])" onClick={() => openProps(true)}><PanelRight className="icon" /></button>}
        </div>
      </div>
      <div className="tabs task-tabs" role="tablist" aria-label="The card, its changes and its screenshots">
        <a className="tab" role="tab" aria-selected={!wide} href={href({ page: "task", id })}><SquareKanban className="icon" />Card</a>
        <a className="tab" role="tab" aria-selected={onChanges} href={href({ page: "task", id, view: "changes" })}
          title={label === null ? undefined : "What this card changed"}>
          <FileDiff className="icon" />Changes
          {label === "none" ? <span className="n">No changes</span> : label && <> <Diffstat add={label.add} del={label.del} /></>}</a>
        {shotsTab && (
          <a className="tab" role="tab" aria-selected={onShots} href={href({ page: "task", id, view: "screenshots" })}
            title={shotsCount === null ? undefined : "What the agents saw of the screens they built"}>
            <Images className="icon" />Screenshots
            {shotsCount !== null && <span className="n">{shotsCount > 0 ? shotsCount : "None"}</span>}</a>
        )}
      </div>
      {err && <div className="error-banner" role="alert">{err}<button className="btn ghost sm" onClick={() => setErr(null)}>Dismiss</button></div>}
      <div className="split">
        {onChanges ? <ChangesView task={task} card={changes} runId={run} runs={runs} />
          : onShots ? <ScreenshotsView task={task} sets={shots.data} error={shots.error} runId={run} project={project} agent={agent}
            onShowProps={showShotsSwitch} onViewRun={viewRun} />
          : <div className="doc">
          <div className="doc-in">
            <div className="task-head">
              <StatusIcon category={task.stateCategory} hold={task.hold} /><PriorityIcon priority={task.priority} />
              <span className="id">{task.identifier}</span>
              {task.projectName && <a className="proj" href={href({ page: "project", id: task.projectId! })}><span className="dot" style={{ width: 8, height: 8, borderRadius: "50%", background: task.projectColor ?? "var(--text-3)" }} />{task.projectName}</a>}
              {live && <span className="badge live"><span className="pulse" />Live</span>}
              {task.byHand && <span className="badge" title={byHandTip(task)}><Keyboard className="icon sm" />By hand</span>}
              {task.hold && <span className="badge needs">On hold</span>}
              {archived && <span className="badge"><Archive className="icon sm" />Archived</span>}
            </div>
            {archived && (
              <div className="archived-note" role="note">
                <span className="grow">Archived {relTime(task.archivedAt!)}{task.archivedBy ? ` by ${task.archivedBy}` : ""}. It is read-only: Restore puts it back at the bottom of {task.stateName}.</span>
              </div>
            )}
            <TitleInput title={task.title} readOnly={archived} onSave={(t) => updateTask(id, { title: t }).catch(fail)} />
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
                {!archived && <CommentComposer key={id} taskId={id} onPost={post} />}
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
