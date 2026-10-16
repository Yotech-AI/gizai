// The Screenshots tab of a card (GA-185, GA-181's frames 166:71, 168:479, 168:564 and the empty tab 169:*): one set per
// run, newest first, older runs folded after the newest two; a click on a picture opens the larger view. Full width like
// Changes. The pictures come through screenshotBytes as blob: URLs while they show (lib/shotUrls).
import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowRight, ChevronDown, ChevronRight, CircleAlert, GitCommitHorizontal, Image, Images, Info, PanelRight, Settings, TriangleAlert } from "lucide-react";
import { agentCliTools, agentMcp } from "../../api";
import { href } from "../../router";
import { BROWSER } from "../../lib/cliTools";
import {
  answerOf, compare, emptyReason, emptyTexts, formatOf, leftOutText, noneText, notTakenText, olderCount, olderText, PAIRING_TEXT, shortCommit,
  shotCount, SHOWN_SETS, sizeOf, summaryOf, themeOf, when, type Compared, type EmptyReason,
} from "../../lib/screenshots";
import { useNearScreen, useShotUrl } from "../../lib/shotUrls";
import { reveal } from "../../lib/reveal";
import { useDrawer } from "../../lib/drawers";
import type { Member, Project, Screenshot, ScreenshotSet, Task } from "../../types";
import { Avatar, roleIcon } from "../Avatar";
import { ShotViewer } from "./ShotViewer";

/** One thumbnail: the picture at its own proportions in its cell, its caption, format, size and theme, Changed, Unchanged or
 *  New, and its file name. The picture loads once it comes near the screen. */
function Thumb({ shot, pairing, onOpen }: { shot: Screenshot; pairing?: keyof typeof PAIRING_TEXT; onOpen: (el: HTMLElement) => void }) {
  const [ref, near] = useNearScreen<HTMLButtonElement>();
  const { url, error } = useShotUrl(shot, near);
  return (
    <li>
      <button ref={ref} type="button" className="shot" data-shot={shot.name} aria-label={`${shot.caption}. ${formatOf(shot)}, ${sizeOf(shot)}, ${themeOf(shot)}. Open the larger view`}
        onClick={(e) => onOpen(e.currentTarget)}>
        <span className="shot-cell">
          {url ? <img src={url} alt="" draggable={false} />
            : <span className="shot-ph" title={error ?? undefined}><Image className="icon" aria-hidden />{error ? "Couldn’t load this picture" : sizeOf(shot)}</span>}
        </span>
        <span className="shot-cap">{shot.caption}</span>
        <span className="shot-meta">{formatOf(shot)} · {sizeOf(shot)} · {themeOf(shot)}{pairing && <span className="badge">{PAIRING_TEXT[pairing]}</span>}</span>
        <span className="shot-file" title={shot.name}>{shot.name}</span>
      </button>
    </li>
  );
}

/** Shot/Note: what was left out (warning), a preview that didn't start (danger), or why a run took none (faint). */
function Note({ kind, children }: { kind: "left" | "preview" | "none"; children: string }) {
  const I = kind === "left" ? TriangleAlert : kind === "preview" ? CircleAlert : Info;
  return <p className={`shot-note ${kind}`}><I className="icon sm" aria-hidden /><span>{children}</span></p>;
}

/** One run's set: its heading (the agent, its answer, when, how many, the commit, the summary, View the run), its pictures
 *  and its notes. */
function SetBlock({ set, compared, onOpen, onViewRun }: {
  set: ScreenshotSet; compared: Compared | null; onOpen: (index: number, el: HTMLElement) => void; onViewRun: (runId: string) => void;
}) {
  const answer = answerOf(set);
  const none = noneText(set);
  const commit = shortCommit(set.commit);
  const Role = roleIcon(set.role);
  return (
    <section className="shot-set" aria-label={`${set.agentName}: ${answer.text}, ${when(set.endedAt)}, ${shotCount(set.shots.length)}`}>
      <div className="shot-head">
        <Avatar name={set.agentName} kind="agent" role={set.role} />
        <b className="who">{set.agentName}</b>
        {set.role && <Role className="icon role" aria-label={set.role} />}
        <span className={`badge ${answer.cls}`}>{answer.text}</span>
        <span className="meta"><span title={new Date(set.endedAt).toLocaleString("en-GB")}>{when(set.endedAt)}</span> · {shotCount(set.shots.length)}</span>
        {commit && <span className="commit" title={set.commit ?? undefined}><GitCommitHorizontal className="icon sm" aria-hidden />{commit}</span>}
        {compared && <span className="sum">{summaryOf(compared)}</span>}
        <button type="button" className="link view-run" onClick={() => onViewRun(set.runId)}>View the run<ArrowRight className="icon sm" aria-hidden /></button>
      </div>
      {set.shots.length > 0 && (
        <ul className="shot-grid">
          {set.shots.map((s, i) => <Thumb key={s.fileId} shot={s} pairing={compared?.pairing.get(s.fileId)} onOpen={(el) => onOpen(i, el)} />)}
        </ul>
      )}
      {none && <Note kind={none.kind === "preview" ? "preview" : "none"}>{none.text}</Note>}
      {set.shots.length > 0 && set.preview && <Note kind="preview">{`The preview didn’t start: ${set.preview.replace(/\.$/, "")}.`}</Note>}
      {set.leftOut.length > 0 && <Note kind="left">{leftOutText(set.leftOut)}</Note>}
      {compared && compared.notTaken.length > 0 && <Note kind="none">{notTakenText(compared.notTaken)}</Note>}
    </section>
  );
}

/** Whether the card's agent takes screenshots: its browser and its Take screenshots switch. */
function useAgentShots(agent: Member | null) {
  const [state, setState] = useState<{ id: string; browser: boolean; screenshots: boolean } | null>(null);
  const id = agent?.actorId ?? null, cli = agent?.adapter || "claude_code";
  useEffect(() => {
    if (!id) { setState(null); return; }
    let alive = true;
    Promise.all([agentMcp(id), agentCliTools(id, cli)]).then(([mcp, tools]) => {
      if (!alive) return;
      const browser = mcp.servers.some((s) => s.serverId === BROWSER && s.on) && !tools.browser.disabled;
      setState({ id, browser, screenshots: browser && !!tools.saved?.screenshots });
    }).catch(() => alive && setState({ id, browser: true, screenshots: true }));
    return () => { alive = false; };
  }, [id, cli]);
  return state && state.id === id ? state : null;
}

/** The empty tab: why there are none, with a link to the setting that changes it. */
function EmptyTab({ task, project, agent, onShowProps }: { task: Task; project: Project; agent: Member | null; onShowProps: () => void }) {
  const drawer = useDrawer();
  const shots = useAgentShots(agent);
  if (agent && !shots) return null; // the agent's switches are still loading
  const reason: EmptyReason = emptyReason({
    cardSwitch: task.screenshots, project: { name: project.name, on: project.screenshots.on, previewCommand: project.previewCommand },
    agent: agent && shots ? { id: agent.actorId, name: agent.name, browser: shots.browser, screenshots: shots.screenshots } : null,
  });
  const { title, text } = emptyTexts(reason);
  const action = reason.kind === "no-preview"
    ? <button type="button" className="btn" onClick={() => { drawer({ kind: "project", id: project.id }); reveal("project-preview", "p-preview"); }}><Settings className="icon" />Set up the preview</button>
    : reason.kind === "agent-off"
      ? <button type="button" className="btn" onClick={() => { drawer({ kind: "agent", id: reason.agentId }); reveal("agent-browser", "agent-screenshots"); }}><Settings className="icon" />Open {reason.agent} settings</button>
      : reason.kind === "card-off"
        ? <button type="button" className="btn" onClick={onShowProps}><PanelRight className="icon" />Show properties</button>
        : <a className="link accent" href={href({ page: "settings", id: "agents" })} onClick={() => reveal("settings-screenshots")}>Screenshot settings</a>;
  return (
    <div className="empty shot-empty" data-reason={reason.kind}>
      <Images className="icon" aria-hidden />
      <b>{title}</b>
      <span>{text}</span>
      {action}
    </div>
  );
}

/** The tab: `run` shows only that run's set (#/task/<id>/screenshots/<run>). */
export function ScreenshotsView({ task, sets, error, runId, project, agent, onShowProps, onViewRun }: {
  task: Task; sets: ScreenshotSet[] | null; error: string | null; runId?: string; project: Project | null; agent: Member | null;
  onShowProps: () => void; onViewRun: (runId: string) => void;
}) {
  const [older, setOlder] = useState(false);
  const [view, setView] = useState<{ set: ScreenshotSet; index: number } | null>(null);
  const opener = useRef<HTMLElement | null>(null);
  const all = useMemo(() => sets ?? [], [sets]);
  const one = runId ? all.filter((s) => s.runId === runId) : null;
  const shown = one ?? all.slice(0, SHOWN_SETS);
  const rest = one ? [] : all.slice(SHOWN_SETS);
  const compared = useMemo(() => new Map(all.map((s) => [s.runId, compare(all, s)])), [all]);
  // a set that changed under the larger view (a new run's) closes it, as its pictures may be gone
  useEffect(() => { if (view && !all.some((s) => s.runId === view.set.runId)) setView(null); }, [all, view]);
  const open = (set: ScreenshotSet, index: number, el: HTMLElement) => { opener.current = el; setView({ set, index }); };
  const close = () => { setView(null); requestAnimationFrame(() => opener.current?.focus()); };
  const block = (s: ScreenshotSet) => (
    <SetBlock key={s.runId} set={s} compared={compared.get(s.runId) ?? null} onOpen={(i, el) => open(s, i, el)} onViewRun={onViewRun} />
  );
  return (
    <div className="shots">
      <div className="shots-in">
        {error && <div className="error-banner" role="alert">{error}</div>}
        {sets && all.length === 0 && project && <EmptyTab task={task} project={project} agent={agent} onShowProps={onShowProps} />}
        {one && (
          <p className="shots-one">
            {one.length === 0 ? "This run has no screenshots here: they may have been cleared." : "One run’s screenshots."}{" "}
            <a className="link accent" href={href({ page: "task", id: task.id, view: "screenshots" })}>Show every run</a>
          </p>
        )}
        {shown.map(block)}
        {rest.length > 0 && (
          <button type="button" className="shots-older" aria-expanded={older} onClick={() => setOlder((o) => !o)}>
            {older ? <ChevronDown className="icon sm" aria-hidden /> : <ChevronRight className="icon sm" aria-hidden />}
            <b>{olderCount(rest.length)}</b><span className="faint">{olderText(rest)}</span>
          </button>
        )}
        {older && rest.map(block)}
      </div>
      {view && <ShotViewer sets={all} set={all.find((s) => s.runId === view.set.runId) ?? view.set} start={view.index} onClose={close} />}
    </div>
  );
}

