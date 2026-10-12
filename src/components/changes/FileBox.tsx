// One changed file in the Changes view (Code Editor F1, GA-112): its header (it sticks while the diff is in sight) and
// its diff. The diff loads, and its editor mounts, only once the box comes near the screen; until then the box keeps the
// height its rows will take. Lock files, built files, big diffs and long deleted files come folded with Show; binary
// files, files over 1 MB and renames without changes say what they are.
import { memo, useEffect, useRef, useState } from "react";
import { ArrowRight, Binary, ChevronDown, CircleAlert, FileMinus, FileX, FoldVertical, LoaderCircle, Lock, RefreshCw } from "lucide-react";
import { EditorView } from "@codemirror/view";
import { cardFileDiff } from "../../api";
import { estimatedRows, foldedNote, foldedWhy, patchRows, sameContent, splitPath, STATUS } from "../../lib/changes";
import { formatBytes } from "../../lib/format";
import { languageFor } from "../../lib/codeLanguages";
import { diffState } from "../../lib/diffView";
import type { FileChange, FileDiff } from "../../types";
import { Diffstat } from "./Diffstat";

/** Where a file's texts come from: `from` and `to` of the changes (to null: the worktree on disk; from "": the pull
 *  request's diff). */
export type DiffSource = { taskId: string; from: string; to: string | null };

/** A file's path in a header: its folder dimmer, its name in bold, and old → new for a rename. */
export function PathName({ f }: { f: FileChange }) {
  const now = splitPath(f.path);
  const title = f.oldPath ? `${f.oldPath} → ${f.path}` : f.path;
  if (!f.oldPath) return <span className="chg-path" title={title}><span className="dir">{now.dir && now.dir + "/"}</span><b>{now.name}</b></span>;
  const was = splitPath(f.oldPath);
  return (
    <span className="chg-path" title={title}>
      <span className="dir">{was.dir && was.dir + "/"}</span>{was.name}<ArrowRight className="arrow" aria-label="to" />
      {was.dir !== now.dir && <span className="dir">{now.dir && now.dir + "/"}</span>}<b>{now.name}</b>
    </span>
  );
}

/** The status letter, with its word for a tooltip and screen readers. */
export function StatusLetter({ f }: { f: FileChange }) {
  const s = STATUS[f.status];
  return <span className={`chg-st ${s.cls}`} title={s.word} aria-label={s.word}>{s.letter}</span>;
}

type Load = { state: "idle" } | { state: "loading" } | { state: "done"; diff: FileDiff } | { state: "failed"; error: string };

export const FileBox = memo(function FileBox({ f, source, near, onView }: {
  f: FileChange; source: DiffSource; near: boolean;
  /** The file's editor once it is mounted (null when it goes), for Alt+↓ and Alt+↑ across files. */
  onView: (path: string, view: EditorView | null) => void;
}) {
  const folds = foldedWhy(f);
  const [folded, setFolded] = useState(!!folds);
  const [load, setLoad] = useState<Load>({ state: "idle" });
  const [attempt, setAttempt] = useState(0);
  const renameOnly = sameContent(f);
  // a file's diff (a binary file's sizes) loads when its box comes near the screen, unfolded
  const wanted = near && !renameOnly && !folded;
  useEffect(() => {
    if (!wanted || load.state !== "idle") return;
    let alive = true;
    setLoad({ state: "loading" });
    cardFileDiff(source.taskId, f.path, f.oldPath, source.from, source.to)
      .then((diff) => alive && setLoad({ state: "done", diff }))
      .catch((e) => alive && setLoad({ state: "failed", error: String(e) }));
    return () => { alive = false; };
  }, [wanted, attempt]); // eslint-disable-line react-hooks/exhaustive-deps
  const retry = () => { setLoad({ state: "idle" }); setAttempt((n) => n + 1); };

  const diff = load.state === "done" ? load.diff : null;
  const tooLarge = !!diff?.tooLarge && !diff.patch;
  const why = renameOnly ? (f.status === "copied" ? "Copied, no changes" : "Renamed, no changes")
    : f.status === "typechange" ? "Changed type" : tooLarge ? "Too big to show" : folds?.why;
  const hasBody = !renameOnly;
  const stat = f.binary ? <span className="diffstat"><span className="bin">Binary</span></span>
    : renameOnly ? null : <Diffstat add={f.additions ?? 0} del={f.deletions ?? 0} />;
  return (
    <section className={`chg-box${folded ? " folded" : ""}`} data-path={f.path} aria-label={f.path}>
      <header className="chg-fh">
        {hasBody ? <button className="chg-caret" aria-expanded={!folded} title={`${folded ? "Show" : "Fold"} this file`} onClick={() => setFolded(!folded)}>
          <ChevronDown className="icon" /></button> : <span className="chg-caret-off" />}
        <StatusLetter f={f} /><PathName f={f} />{stat}
        {f.uncommitted && <span className="badge">Not committed</span>}
        {why && <span className="why">{why}</span>}
        <span className="grow" />
      </header>
      {hasBody && folds && !f.binary && (
        <div className="chg-note">
          {folds.kind === "lock" ? <Lock className="icon" /> : folds.kind === "deleted" ? <FileMinus className="icon" /> : <FoldVertical className="icon" />}
          <span>{foldedNote(f, folds.kind)}</span>
          <button className="btn sm" data-show onClick={() => setFolded(false)}>{folds.kind === "deleted" ? "Show" : "Show diff"}</button>
        </div>
      )}
      {hasBody && !folded && <Body f={f} load={load} retry={retry} near={near} onView={onView} />}
    </section>
  );
});

function Body({ f, load, retry, near, onView }: { f: FileChange; load: Load; retry: () => void; near: boolean; onView: (path: string, view: EditorView | null) => void }) {
  const name = splitPath(f.path).name;
  if (load.state === "failed") {
    return <div className="chg-note" role="alert"><CircleAlert className="icon" /><span>Couldn't load this file: {load.error}</span>
      <button className="btn ghost sm" onClick={retry}><RefreshCw className="icon" />Try again</button></div>;
  }
  if (load.state !== "done") {
    const rows = estimatedRows(f);
    return (
      <div className="chg-wait" style={{ "--rows-h": `${Math.min(rows, 4000) * 20 + 8}px` } as React.CSSProperties}>
        {near && <div className="chg-note faint"><LoaderCircle className="icon spin" />Loading {name}…</div>}
      </div>
    );
  }
  const d = load.diff;
  if (d.binary || f.binary) {
    const text = f.status === "added" ? <>Binary file added: {formatBytes(d.newSize)}.</>
      : f.status === "deleted" ? <>Binary file deleted: {formatBytes(d.oldSize)}.</>
      : <>Binary file changed: {formatBytes(d.oldSize)} <ArrowRight className="arrow" aria-label="to" /> {formatBytes(d.newSize)}.</>;
    return <div className="chg-note"><Binary className="icon" /><span>{text}</span></div>;
  }
  if (d.patch !== null) return <PatchCode patch={d.patch} />;
  if (d.tooLarge) {
    return <div className="chg-note"><FileX className="icon" /><span>Too big to show here: {formatBytes(Math.max(d.oldSize, d.newSize))}. Open it in your editor to read it.</span></div>;
  }
  return <DiffCode path={f.path} diff={d} onView={onView} />;
}

/** The diff in a read-only CodeMirror editor (lib/diffView.ts), in the file's language. It draws at once; a language
 *  that hasn't loaded yet colours the code when it arrives. */
function DiffCode({ path, diff, onView }: { path: string; diff: FileDiff; onView: (path: string, view: EditorView | null) => void }) {
  const host = useRef<HTMLDivElement>(null);
  const [digits, setDigits] = useState(3);
  useEffect(() => {
    if (!host.current) return;
    let alive = true;
    const lang = languageFor(path);
    const build = (language: Parameters<typeof diffState>[0]["language"]) =>
      diffState({ oldText: diff.oldText, newText: diff.newText, hunks: diff.hunks, language, label: `Changes in ${path}` });
    const built = build(lang?.support ?? null);
    setDigits(built.digits);
    const view = new EditorView({ state: built.state, parent: host.current });
    onView(path, view);
    if (lang && !lang.support) lang.load().then((l) => { if (alive) view.setState(build(l).state); }).catch(() => {});
    return () => { alive = false; onView(path, null); view.destroy(); };
  }, [path, diff]); // eslint-disable-line react-hooks/exhaustive-deps
  return <div className="chg-code cm-host" ref={host} style={{ "--digits": Math.max(2, digits) } as React.CSSProperties} />;
}

/** A pull request's diff (no file texts): its rows as D1 draws them, and the unchanged lines between hunks, which it
 *  leaves out, as folds that can't open. */
function PatchCode({ patch }: { patch: string }) {
  const rows = patchRows(patch);
  const digits = Math.max(2, ...rows.map((r) => (r.kind === "gap" ? 0 : String(r.no).length)));
  return (
    <div className="chg-code" style={{ "--digits": digits } as React.CSSProperties}>
      <div className="chg-lines">
        {rows.map((r, i) => r.kind === "gap"
          ? <div className="fold" key={i}><span className="ln" /><span className="sg" /><span className="tx"><span className="fold-off">{r.lines.toLocaleString("en-GB")} unchanged {r.lines === 1 ? "line isn't" : "lines aren't"} in the pull request's diff</span></span></div>
          : <div className={`r${r.kind === "same" ? "" : " " + r.kind}`} key={i}><span className="ln">{r.no}</span><span className="sg">{r.kind === "add" ? "+" : r.kind === "del" ? "−" : ""}</span><span className="tx">{r.text}</span></div>)}
      </div>
    </div>
  );
}
