// The card bar (GA-115, F4 of the Code Editor; design design/code-editor/editor.html §6): on the Editor page of a card you
// do by hand, in the folder bar's place. The column's glyph, the identifier and title, the branch and how far it is, then
// Commit… (a small box with the message and the files it commits), Push and Hand over (to Testing, or to Review with the
// card's Testing switch off). The next step's button is primary; one that can't act is off, with why as its tooltip. What
// a button did shows in place of how far it is for a few seconds; what went wrong takes the bar's place until dismissed.
// QA sent the card back: the bar says so (needs) until you open the card. Its class is hand, since .card is the board's.
import { useEffect, useRef, useState } from "react";
import { Check, CircleAlert, GitBranch, GitCommitHorizontal, LoaderCircle, Send, Upload, X } from "lucide-react";
import { editorMain, getProject, getTeam, handCard, handCommit, handOver, handPush, listRuns } from "../../api";
import type { FolderGit, GitMark, HandCard, Project, Run, Task, Team } from "../../types";
import { go } from "../../router";
import { MARK_LETTER, MARK_TIP } from "../../lib/editor";
import { modKey } from "../../lib/keys";
import {
  backFromQa, backSay, cardSeenAt, committedSay, filesWord, handButtons, handedSay, handOverLabel, handState, plainError, testersOf,
  type HandButton, type HandStand,
} from "../../lib/hand";
import { StatusIcon } from "../StatusIcon";

/** The most files Commit…'s box lists. */
const LISTED = 8;

export function CardBar({ task, folder, git, onChanged, onHanded }: {
  task: Task;
  /** The card's worktree (an EditorFolder id). */
  folder: string;
  /** git's status of the worktree, as the page last read it: the bar reads where the card stands again with each. */
  git: FolderGit | null;
  /** A commit or a push changed git's status: the page reads it again. */
  onChanged: () => void;
  /** Hand over moved the card on: what to say in the bar's place. */
  onHanded: (said: string) => void;
}) {
  const [hand, setHand] = useState<HandCard | null>(null);
  const [handErr, setHandErr] = useState<string | null>(null);
  const [own, setOwn] = useState<number | null>(null);
  const [project, setProject] = useState<Project | null>(null);
  const [team, setTeam] = useState<Team | null>(null);
  const [runs, setRuns] = useState<Run[] | null>(null);
  const [busy, setBusy] = useState<"commit" | "push" | "over" | null>(null);
  const [said, setSaid] = useState<string | null>(null);
  const [failed, setFailed] = useState<{ text: string; retry?: () => void } | null>(null);
  const [box, setBox] = useState(false);
  const [message, setMessage] = useState(`${task.identifier}: ${task.title}`);

  // Where it stands, again with each read of git's status (every 10 seconds, on the focus, after a save or a commit).
  useEffect(() => {
    let alive = true;
    handCard(task.id).then((h) => { if (alive) { setHand(h); setHandErr(null); } }).catch((e) => { if (alive) setHandErr(plainError(e)); });
    editorMain(folder).then((m) => { if (alive) setOwn(m.ahead); }).catch(() => {});
    return () => { alive = false; };
  }, [task.id, folder, git]);
  // Its project (a link to push to?) and team (who tests it).
  useEffect(() => {
    let alive = true;
    if (task.projectId) {
      getProject(task.projectId).then((p) => {
        if (!alive) return;
        setProject(p);
        return getTeam(p.teamId ?? null).then((t) => { if (alive) setTeam(t); });
      }).catch(() => {});
    }
    return () => { alive = false; };
  }, [task.projectId]);
  // Its runs, for QA sending it back: again as the card changes.
  useEffect(() => {
    let alive = true;
    listRuns(task.id).then((r) => { if (alive) setRuns(r); }).catch(() => {});
    return () => { alive = false; };
  }, [task]);
  // What a button did shows for a few seconds.
  useEffect(() => {
    if (!said) return;
    const t = setTimeout(() => setSaid(null), 6000);
    return () => clearTimeout(t);
  }, [said]);

  const id = task.identifier;
  const branch = hand?.branch ?? task.branch ?? git?.branch ?? "";
  const stand: HandStand = { changed: git?.uncommitted ?? hand?.changed ?? 0, unpushed: hand?.unpushed ?? 0, own, link: project ? !!project.repoUrl : true };
  const b = handButtons(id, branch, stand, { testing: task.testing, testers: testersOf(team), project: project?.name ?? task.projectName ?? "Its project" });
  const back = backFromQa(task, runs, cardSeenAt(task.id));

  const act = async (what: "commit" | "push" | "over", run: () => Promise<void>, retry?: () => void) => {
    setBusy(what);
    setFailed(null);
    setSaid(null);
    try { await run(); } catch (e) { setBox(false); setFailed({ text: plainError(e), retry }); } finally { setBusy(null); }
  };
  const commit = () => act("commit", async () => {
    const files = stand.changed;
    const c = await handCommit(task.id, message);
    setBox(false);
    setMessage(`${id}: ${task.title}`);
    setSaid(committedSay(files, c.sha));
    onChanged();
  });
  const push = () => act("push", async () => {
    setSaid(await handPush(task.id));
    onChanged();
  }, () => push());
  const over = () => act("over", async () => {
    const column = await handOver(task.id);
    onHanded(handedSay(column, team));
  });

  if (failed) return (
    <div className="ed-bar failed" role="alert">
      <CircleAlert className="icon" />
      <span className="say">{failed.text}</span>
      <span className="acts">
        {failed.retry && <button className="btn sm" onClick={failed.retry}><Upload className="icon" />Try again</button>}
        <button className="btn ghost sm icon-only" aria-label="Dismiss" title="Dismiss" onClick={() => setFailed(null)}><X className="icon" /></button>
      </span>
    </div>
  );

  // The buttons are off while one of them works, and until Gizai knows where the card stands.
  const off = (x: HandButton) => x.off || busy !== null || !hand;
  const why = (x: HandButton) => (!hand && handErr ? handErr : x.title);
  const primary = (x: HandButton) => (x.primary && !back ? " primary" : "");
  const files = Object.entries(git?.files ?? {}).sort(([a], [b2]) => (a < b2 ? -1 : 1));
  const buttons = (
    <>
      <span className="pop-wrap">
        <button className={`btn sm${primary(b.commit)}`} disabled={off(b.commit)} title={why(b.commit)} aria-haspopup="dialog" aria-expanded={box}
          onClick={() => setBox((x) => !x)}>
          {busy === "commit" ? <><LoaderCircle className="icon spin" />Committing…</> : <><GitCommitHorizontal className="icon" />Commit…</>}
        </button>
        {box && <CommitBox message={message} setMessage={setMessage} files={files} count={stand.changed} busy={busy === "commit"}
          onCommit={commit} onClose={() => setBox(false)} />}
      </span>
      <button className={`btn sm${primary(b.push)}`} disabled={off(b.push)} title={why(b.push)} onClick={push}>
        {busy === "push" ? <><LoaderCircle className="icon spin" />Pushing…</> : <><Upload className="icon" />Push</>}
      </button>
      <button className={`btn sm${primary(b.over)}`} disabled={off(b.over)} title={why(b.over)} onClick={over}>
        {busy === "over" ? <><LoaderCircle className="icon spin" />Handing over…</> : <><Send className="icon" />{handOverLabel(task.testing)}</>}
      </button>
    </>
  );

  if (back) return (
    <div className="ed-bar needs" role="status">
      <StatusIcon category={task.stateCategory} hold={task.hold} title={task.stateName} />
      <span className="say">{backSay(back, id)}</span>
      <span className="acts">
        <button className="btn sm primary" onClick={() => go({ page: "task", id: task.id })}>Open card</button>
        {buttons}
      </span>
    </div>
  );

  return (
    <div className="ed-bar hand" role="status" aria-label={`${id} by hand`}>
      <StatusIcon category={task.stateCategory} hold={task.hold} title={task.stateName} />
      <span className="say">
        <span className="ed-card-id">{id}</span>
        <span className="ed-card-title" title={task.title}>{task.title}</span>
        {branch && <span className="ed-card-branch" title={branch}><GitBranch className="icon sm" /><span>{branch}</span></span>}
        <span className="ed-card-state">{said ? <><Check className="icon sm" /> {said}</> : hand ? handState(stand) : handErr ?? ""}</span>
      </span>
      <span className="acts">{buttons}</span>
    </div>
  );
}

/** Commit…'s box, under its button: the message ("GA-123: <title>" to start with), the files it commits (every change, as
 *  git sees it), Cancel and Commit. Ctrl+Enter commits; Escape and a click outside close it, keeping the message. */
function CommitBox({ message, setMessage, files, count, busy, onCommit, onClose }: {
  message: string; setMessage: (m: string) => void; files: [string, GitMark][]; count: number; busy: boolean; onCommit: () => void; onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const area = useRef<HTMLTextAreaElement>(null);
  // the message has the focus, the caret after what is filled in
  useEffect(() => { const a = area.current; if (a) { a.focus(); a.setSelectionRange(a.value.length, a.value.length); } }, []);
  useEffect(() => {
    // the box and its button are one: a click on the button toggles it
    const down = (e: MouseEvent) => { if (!ref.current?.parentElement?.contains(e.target as Node)) onClose(); };
    window.addEventListener("mousedown", down);
    return () => window.removeEventListener("mousedown", down);
  }, [onClose]);
  const empty = !message.trim();
  return (
    <div ref={ref} className="pop right ed-commit" role="dialog" aria-label="Commit"
      onKeyDown={(e) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); } }}>
      <div className="field">
        <label htmlFor="ed-commit-message">Commit message</label>
        <textarea ref={area} id="ed-commit-message" className="textarea" value={message} onChange={(e) => setMessage(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); if (!empty && !busy) onCommit(); } }} />
      </div>
      {files.length > 0 && (
        <ul aria-label="What it commits">
          {files.slice(0, LISTED).map(([p, m]) => (
            <li key={p}><span className={`ed-mark ${m}`} title={MARK_TIP[m]} aria-label={MARK_TIP[m]}>{MARK_LETTER[m]}</span><span title={p}>{p}</span></li>
          ))}
          {files.length > LISTED && <li><span className="ed-mark" /><span>and {(files.length - LISTED).toLocaleString("en-US")} more</span></li>}
        </ul>
      )}
      <p style={{ marginTop: files.length ? -4 : 0, fontSize: "var(--fs-sm)", color: "var(--text-2)" }}>Commits every change, as git sees it: {filesWord(count)}.</p>
      <div className="acts">
        <button className="btn ghost sm" onClick={onClose}>Cancel</button>
        <button className="btn primary sm" disabled={empty || busy} title={empty ? "Write a commit message first." : `Commit (${modKey()}+Enter)`} onClick={onCommit}>
          {busy ? <><LoaderCircle className="icon spin" />Committing…</> : <><GitCommitHorizontal className="icon" />Commit <span className="kbd">{modKey()} Enter</span></>}
        </button>
      </div>
    </div>
  );
}
