// The Changes view of a card (Code Editor F1, GA-112; D1's design/code-editor/changes.html): on the left the changed
// files under their folders, with where the changes come from on top; on the right every file's diff below each other.
// Both scroll on their own; the current file in the list follows the scroll, and a click in the list scrolls its file
// to the top. Keys while the view has the focus: n and p the next and previous file, Alt+↓ and Alt+↑ the next and
// previous change, [ folds the list, Esc goes back to Card. With `runId`, only what that run committed.
import { Fragment, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  ArrowLeft, ArrowRight, ChevronDown, ChevronRight, CircleAlert, FileDiff, GitBranch, PanelLeftClose, PanelLeftOpen, Play, RefreshCw, Search,
  Settings, SquareKanban,
} from "lucide-react";
import type { EditorView } from "@codemirror/view";
import { getChunks } from "@codemirror/merge";
import { runChanges, runCommits } from "../../api";
import { go, href } from "../../router";
import { count, files as filesText, filterFiles, groupFiles, runLine, sourceLine, uncommitted } from "../../lib/changes";
import { commitCount } from "../../lib/runs";
import type { CardChanges, Commit, FileChange, Run, Task } from "../../types";
import { Diffstat } from "./Diffstat";
import { FileBox, StatusLetter, type DiffSource } from "./FileBox";

/** How far outside the screen a file starts loading its diff. */
const NEAR = "1200px 0px 1200px 0px";

type Loaded = { data: CardChanges | null; error: string | null; reload: () => void };

export function ChangesView({ task, card, runId, runs }: { task: Task; card: Loaded; runId?: string; runs?: Run[] | null }) {
  const run = useRunChanges(runId);
  const shown = runId ? run : card;
  const back = () => go({ page: "task", id: task.id });
  if (shown.error) return <Failed error={shown.error} retry={shown.reload} taskId={task.id} />;
  const c = shown.data;
  if (!c) {
    return (
      <div className="chg"><aside className="chg-files" aria-label="Changed files"><div className="chg-sum">
        <div className="src"><span>{runId ? "One run's changes" : "Changes"}</span></div><div className="tot faint">Loading the changed files…</div>
      </div></aside><div className="chg-diffs" /></div>
    );
  }
  if (c.source === "none") {
    return <Empty icon={<GitBranch className="icon lg" />} title="No changes yet" text="The first run makes this card's branch."
      action={<button className="btn" onClick={back}><SquareKanban className="icon" />Back to Card</button>} />;
  }
  if (!c.files.length) {
    return <Empty icon={<FileDiff className="icon lg" />} title="No changes" text={c.note ?? `This card's branch has no changes against ${c.baseLabel}.`}
      action={runId ? <a className="btn" href={href({ page: "task", id: task.id, view: "changes" })}><ArrowLeft className="icon" />All changes</a> : undefined} />;
  }
  const r = runId ? (runs ?? []).find((x) => x.id === runId) ?? null : null;
  return <Changes key={runId ?? "card"} task={task} c={c} run={runId ? { id: runId, run: r, commits: run.commits } : null} onBack={back} />;
}

/** One run's changes and its commits. */
function useRunChanges(runId?: string): Loaded & { commits: Commit[] | null } {
  const [data, setData] = useState<CardChanges | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [commits, setCommits] = useState<Commit[] | null>(null);
  const [n, setN] = useState(0);
  useEffect(() => {
    if (!runId) return;
    let alive = true;
    setData(null); setError(null);
    runChanges(runId).then((d) => alive && setData(d)).catch((e) => alive && setError(String(e)));
    runCommits(runId).then((d) => alive && setCommits(d)).catch(() => alive && setCommits(null));
    return () => { alive = false; };
  }, [runId, n]);
  return { data, error, reload: () => setN((x) => x + 1), commits };
}

function Empty({ icon, title, text, action }: { icon: React.ReactNode; title: string; text: string; action?: React.ReactNode }) {
  return <div className="chg-empty"><div className="empty">{icon}<b>{title}</b><span>{withArrows(text)}</span>{action}</div></div>;
}

/** A sentence with its arrows drawn as Lucide's arrow-right (Atkinson Hyperlegible has no →). */
function withArrows(text: string): React.ReactNode {
  const parts = text.split("→");
  return parts.length === 1 ? text : parts.map((p, i) => <span key={i}>{i > 0 && <ArrowRight className="icon sm" aria-label="then" style={{ verticalAlign: -2 }} />}{p}</span>);
}

/** Nothing to find (merged before Gizai saved its range, no merge commit here, gh not logged in), or a failure. */
function Failed({ error, retry, taskId }: { error: string; retry: () => void; taskId: string }) {
  const m = /^Gizai can't find what \S+ changed: (.*)$/s.exec(error);
  if (m) {
    const bitbucket = /Bitbucket/.test(m[1]);
    const text = m[1].charAt(0).toUpperCase() + m[1].slice(1);
    return <Empty icon={<CircleAlert className="icon lg" />} title="Gizai can't find what this card changed" text={/\.$/.test(text) ? text : text + "."}
      action={/Settings/.test(m[1]) ? <a className="btn" href={href({ page: "settings", id: "github" })}><Settings className="icon" />Open {bitbucket ? "Bitbucket" : "GitHub"} settings</a>
        : <a className="btn" href={href({ page: "task", id: taskId })}><SquareKanban className="icon" />Back to Card</a>} />;
  }
  return (
    <div className="error-banner" role="alert"><CircleAlert className="icon" style={{ flex: "none" }} /><span>{withArrows(error)}</span>
      <button className="btn ghost sm" onClick={retry}><RefreshCw className="icon" />Try again</button></div>
  );
}

type RunInfo = { id: string; run: Run | null; commits: Commit[] | null };

function Changes({ task, c, run, onBack }: { task: Task; c: CardChanges; run: RunInfo | null; onBack: () => void }) {
  const [query, setQuery] = useState("");
  const [listFolded, setListFolded] = useState(false);
  const [closedDirs, setClosedDirs] = useState<Set<string>>(new Set());
  const [current, setCurrent] = useState<string | null>(c.files[0]?.path ?? null);
  const [near, setNear] = useState<Set<string>>(new Set());
  const root = useRef<HTMLDivElement>(null);
  const diffs = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);
  const views = useRef(new Map<string, EditorView>());
  // a click in the list keeps its file at the top while the files above it load and grow, until you scroll yourself
  const pinned = useRef<{ path: string; until: number } | null>(null);

  const shown = useMemo(() => filterFiles(c.files, query), [c.files, query]);
  const groups = useMemo(() => groupFiles(shown), [shown]);
  // the boxes in the list's order: folders first, then the top level
  const order = useMemo(() => groups.flatMap((g) => g.files), [groups]);
  const source: DiffSource = useMemo(() => ({ taskId: task.id, from: c.source === "pull" ? "" : c.from ?? "", to: c.to }), [task.id, c]);
  const onView = useCallback((path: string, view: EditorView | null) => { if (view) views.current.set(path, view); else views.current.delete(path); }, []);

  const boxOf = (path: string) => inner.current?.querySelector<HTMLElement>(`.chg-box[data-path="${CSS.escape(path)}"]`) ?? null;
  const scrollToBox = (path: string, smooth = false) => {
    const box = boxOf(path), d = diffs.current;
    if (!box || !d) return;
    d.scrollTo({ top: Math.max(0, box.offsetTop - 8), behavior: smooth ? "smooth" : "auto" });
  };
  const goTo = (path: string) => {
    setCurrent(path);
    pinned.current = { path, until: Date.now() + 2000 };
    scrollToBox(path);
  };

  // files near the screen load their diffs
  useEffect(() => {
    const d = diffs.current;
    if (!d) return;
    const io = new IntersectionObserver((entries) => {
      const add = entries.filter((e) => e.isIntersecting).map((e) => (e.target as HTMLElement).dataset.path!);
      if (add.length) setNear((s) => (add.every((p) => s.has(p)) ? s : new Set([...s, ...add])));
    }, { root: d, rootMargin: NEAR });
    for (const box of d.querySelectorAll(".chg-box")) io.observe(box);
    return () => io.disconnect();
  }, [order]);
  // the pinned file stays at the top while boxes above it change height
  useEffect(() => {
    const el = inner.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const p = pinned.current;
      if (p && Date.now() < p.until) scrollToBox(p.path); else pinned.current = null;
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  const unpin = () => { pinned.current = null; };

  // the current file follows the scroll: the first box that reaches under the sticky headers' line
  const onScroll = () => {
    const d = diffs.current;
    if (!d || pinned.current) return;
    const top = d.getBoundingClientRect().top + 40;
    const box = [...d.querySelectorAll<HTMLElement>(".chg-box")].find((b) => b.getBoundingClientRect().bottom > top);
    if (box?.dataset.path && box.dataset.path !== current) setCurrent(box.dataset.path);
  };
  useLayoutEffect(() => {
    if (!current) return;
    root.current?.querySelector(`.chg-file[data-path="${CSS.escape(current)}"]`)?.scrollIntoView({ block: "nearest" });
  }, [current]);
  // the view has the keys from the start
  useEffect(() => { root.current?.focus({ preventScroll: true }); }, []);

  /** Alt+↓ / Alt+↑: the next or previous change below or above the sticky header, across files; a file that hasn't
   *  loaded yet counts as one change at its top. */
  const stepChange = (dir: 1 | -1) => {
    const d = diffs.current;
    if (!d) return;
    const line = d.getBoundingClientRect().top + 48;
    const tops: number[] = [];
    for (const f of order) {
      const box = boxOf(f.path);
      if (!box) continue;
      const view = views.current.get(f.path);
      const chunks = view && getChunks(view.state)?.chunks;
      if (view && chunks?.length) for (const ch of chunks) tops.push(view.documentTop + view.lineBlockAt(ch.fromB).top);
      else tops.push(box.getBoundingClientRect().top + 40);
    }
    const target = dir > 0 ? tops.find((y) => y > line + 2) : [...tops].reverse().find((y) => y < line - 2);
    if (target !== undefined) d.scrollBy({ top: target - line, behavior: "smooth" });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.nativeEvent.defaultPrevented || e.ctrlKey || e.metaKey) return;
    const t = e.target as HTMLElement;
    if (t.closest("input, textarea, select, [role=dialog]")) {
      if (e.key === "Escape" && t.closest(".chg-filter")) { e.preventDefault(); (t as HTMLInputElement).blur(); root.current?.focus({ preventScroll: true }); }
      return;
    }
    unpin();
    if (e.altKey && (e.key === "ArrowDown" || e.key === "ArrowUp")) { e.preventDefault(); stepChange(e.key === "ArrowDown" ? 1 : -1); return; }
    if (e.altKey) return;
    if (e.key === "n" || e.key === "p") {
      e.preventDefault();
      const at = order.findIndex((f) => f.path === current);
      const next = order[Math.max(0, Math.min(order.length - 1, at + (e.key === "n" ? 1 : -1)))];
      if (next) goTo(next.path);
    } else if (e.key === "[") {
      e.preventDefault();
      setListFolded((x) => !x);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onBack();
    }
  };

  const toggleDir = (dir: string) => setClosedDirs((s) => { const n = new Set(s); if (n.has(dir)) n.delete(dir); else n.add(dir); return n; });
  const unc = uncommitted(c);
  const src = sourceLine(c, task.branch);
  const foldList = <button className="btn ghost sm icon-only" title="Fold the file list ([)" aria-label="Fold the file list" onClick={() => setListFolded(true)}><PanelLeftClose className="icon" /></button>;
  const total = <><Diffstat add={c.additions} del={c.deletions} /> · {query.trim() ? `${count(shown.length)} of ${filesText(c.files.length)}` : filesText(c.files.length)}</>;
  const head = run ? <RunHead task={task} c={c} run={run} total={total} foldList={foldList} /> : (
    <>
      <div className="src"><span>{src.since ? <>Since <span className="mono">{src.since}</span></> : src.text}</span>{foldList}</div>
      <div className="tot">{total}</div>
      {unc > 0 && <div className="unc"><span className="chg-dot" />{filesText(unc)} not committed yet</div>}
    </>
  );

  return (
    <div className={`chg${listFolded ? " list-folded" : ""}`} tabIndex={-1} ref={root} onKeyDown={onKeyDown}
      onPointerDown={(e) => { if (!(e.target as HTMLElement).closest("input, button, a, .cm-editor")) root.current?.focus({ preventScroll: true }); }}>
      <div className="chg-rail"><button className="btn ghost sm icon-only" title="Show the file list ([)" aria-label="Show the file list" onClick={() => setListFolded(false)}><PanelLeftOpen className="icon" /></button></div>
      <aside className="chg-files" aria-label="Changed files">
        <div className="chg-sum">{head}</div>
        <label className="chg-filter"><Search className="icon sm" /><input className="input" placeholder="Filter files…" aria-label="Filter files" value={query} onChange={(e) => setQuery(e.target.value)} /></label>
        <div className="chg-tree">
          {groups.map((g) => {
            const open = !closedDirs.has(g.dir);
            return (
              <Fragment key={g.dir || "/"}>
                {g.dir && <button className="chg-dir" aria-expanded={open} title={g.dir} onClick={() => toggleDir(g.dir)}>
                  {open ? <ChevronDown className="icon" /> : <ChevronRight className="icon" />}<span>{g.dir}</span>{!open && <span className="n">{filesText(g.files.length)}</span>}</button>}
                <div className="chg-group">{g.files.map((f) => <FileRow key={f.path} f={f} on={f.path === current} onClick={() => goTo(f.path)} />)}</div>
              </Fragment>
            );
          })}
          {!shown.length && <div className="chg-nomatch">No file matches <b>“{query.trim()}”</b>. <button className="link" onClick={() => setQuery("")}>Clear the filter</button></div>}
        </div>
        <div className="chg-keys">
          <span><span className="kbd">n</span> <span className="kbd">p</span> next, previous file</span>
          <span><span className="kbd">Alt ↓</span> <span className="kbd">Alt ↑</span> next, previous change</span>
          <span><span className="kbd">[</span> fold this list</span><span><span className="kbd">Esc</span> back to Card</span>
        </div>
      </aside>
      <div className="chg-diffs" ref={diffs} onScroll={onScroll} onWheel={unpin} onTouchStart={unpin}>
        <div ref={inner}>
          {order.map((f) => <FileBox key={f.path} f={f} source={source} near={near.has(f.path)} onView={onView} />)}
        </div>
      </div>
    </div>
  );
}

function FileRow({ f, on, onClick }: { f: FileChange; on: boolean; onClick: () => void }) {
  const name = f.path.slice(f.path.lastIndexOf("/") + 1);
  const was = f.oldPath ? f.oldPath.slice(f.oldPath.lastIndexOf("/") + 1) : null;
  const renameOnly = (f.status === "renamed" || f.status === "copied") && !f.binary && !f.additions && !f.deletions;
  const counts = f.binary ? <span className="bin">Binary</span> : renameOnly ? <span className="bin">{f.status === "copied" ? "Copied" : "Renamed"}</span>
    : <Diffstat add={f.status === "deleted" ? null : f.additions ?? 0} del={f.deletions ?? 0} />;
  return (
    <button className={`chg-file${on ? " on" : ""}`} data-path={f.path} title={f.oldPath ? `${f.oldPath} → ${f.path}` : f.path} onClick={onClick} aria-current={on || undefined}>
      <StatusLetter f={f} />
      <span className="nm">{was && <><span className="was">{was} </span><ArrowRight className="arrow" aria-label="to" /> </>}{name}</span>
      <span className="meta">{counts}{f.uncommitted && <span className="chg-dot" title="Not committed yet" aria-label="Not committed yet" />}</span>
    </button>
  );
}

/** One run's heading: whose run and when, its commits, and All changes. */
function RunHead({ task, c, run, total, foldList }: { task: Task; c: CardChanges; run: RunInfo; total: React.ReactNode; foldList: React.ReactNode }) {
  const line = run.run ? runLine(run.run) : null;
  const commits = run.commits;
  return (
    <div className="chg-run">
      <div className="src"><span><Play className="icon sm" />{line ? line.who : "One run"}</span>{foldList}</div>
      <div className="tot">{line && <>{line.when} · </>}{commits && <>{commitCount(commits.length)} · </>}{total}</div>
      {commits && commits.length > 0 && (
        <ul className="chg-commits">{commits.map((x) => <li key={x.sha}><span className="mono" title={x.sha}>{x.sha.slice(0, 7)}</span><span>{x.subject}</span></li>)}</ul>
      )}
      {c.note && !commits && <div className="faint">{c.note}</div>}
      <a className="btn sm chg-back" href={href({ page: "task", id: task.id, view: "changes" })}><ArrowLeft className="icon" />All changes</a>
    </div>
  );
}
