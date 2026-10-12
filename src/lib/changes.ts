// A card's changes (Code Editor F1, GA-112): what the Changes view shows, worked out without the screen. Git's hunks
// become the merge view's chunks line for line (so the view, the file list's +/− and GitHub agree), with the changed
// words of each pair of lines found by @codemirror/merge's own diff; the folds of unchanged code and their steps; the
// file list's groups, letters and labels; which files come folded; and the list's heading.
import { Text } from "@codemirror/state";
import { Change, Chunk, presentableDiff } from "@codemirror/merge";
import type { CardChanges, DiffHunk, FileChange, FileStatus, Run } from "../types";

/** Unchanged lines kept in sight around a change. */
export const CONTEXT = 3;
/** The fewest unchanged lines a fold hides. */
export const MIN_FOLD = 4;
/** How many lines a fold's ↑ and ↓ show. */
export const STEP = 20;
/** A diff with more changed lines than this comes folded. */
export const BIG_DIFF = 3000;
/** A deleted file with more lines than this comes folded. */
export const BIG_DELETION = 40;
/** A pair of lines gets its changed words marked when at least this share of the longer one is unchanged. */
export const SAME_ENOUGH = 0.4;

// ---- git's hunks as the merge view's chunks ----

/** Where one side of a hunk is in `doc`: from the start of its first line to one past the end of its last (which may
 *  point past the end of the document, as in @codemirror/merge's chunks). A side without lines sits at the start of
 *  the line after `start` (git gives the line before the change, 0 at the top). */
function side(doc: Text, start: number, lines: number): [number, number] {
  if (lines === 0) {
    const at = start <= 0 ? 0 : Math.min(doc.length, doc.line(Math.min(start, doc.lines)).to + 1);
    return [at, at];
  }
  return [doc.line(start).from, doc.line(start + lines - 1).to + 1];
}

/** Whether every hunk lies inside both texts, in order. They don't when a file changed on disk between git's diff and
 *  reading it; the view then works the diff out itself. */
export function hunksFit(a: Text, b: Text, hunks: DiffHunk[]): boolean {
  let lastA = 0, lastB = 0;
  for (const h of hunks) {
    if (h.oldLines < 0 || h.newLines < 0 || h.oldStart < 0 || h.newStart < 0) return false;
    if (h.oldLines > 0 && (h.oldStart < 1 || h.oldStart + h.oldLines - 1 > a.lines)) return false;
    if (h.newLines > 0 && (h.newStart < 1 || h.newStart + h.newLines - 1 > b.lines)) return false;
    if (h.oldLines === 0 && h.oldStart > a.lines) return false;
    if (h.newLines === 0 && h.newStart > b.lines) return false;
    if (h.oldStart < lastA || h.newStart < lastB) return false;
    lastA = h.oldStart + h.oldLines;
    lastB = h.newStart + h.newLines;
  }
  return true;
}

/** The changed words between a removed and an added line, from @codemirror/merge's diff, relative to each line; none when
 *  the lines have too little in common (a line written anew is just removed and added). */
export function wordChanges(oldLine: string, newLine: string): readonly Change[] {
  if (oldLine === newLine) return [];
  const changes = presentableDiff(oldLine, newLine, { scanLimit: 500 });
  const changed = changes.reduce((n, c) => n + Math.max(c.toA - c.fromA, c.toB - c.fromB), 0);
  const same = Math.max(oldLine.length, newLine.length) - changed;
  return same >= SAME_ENOUGH * Math.max(oldLine.length, newLine.length) ? changes : [];
}

/** Git's hunks as the merge view's chunks: one chunk a hunk, over exactly its lines, so the view shows git's diff and
 *  not a rough one of its own. Inside a hunk the n-th removed line is compared with the n-th added line, and the words
 *  that changed are marked (`Chunk.changes`, relative to the chunk's start). */
export function gitChunks(a: Text, b: Text, hunks: DiffHunk[]): Chunk[] {
  return hunks.map((h) => {
    const [fromA, toA] = side(a, h.oldStart, h.oldLines);
    const [fromB, toB] = side(b, h.newStart, h.newLines);
    const changes: Change[] = [];
    for (let i = 0; i < Math.min(h.oldLines, h.newLines); i++) {
      const la = a.line(h.oldStart + i), lb = b.line(h.newStart + i);
      for (const c of wordChanges(la.text, lb.text)) {
        changes.push(new Change(la.from - fromA + c.fromA, la.from - fromA + c.toA, lb.from - fromB + c.fromB, lb.from - fromB + c.toB));
      }
    }
    return new Chunk(changes, fromA, toA, fromB, toB);
  });
}

/** For `diffConfig.override`: git's hunks as whole-line changes in the two texts, one per hunk, in place of the merge
 *  view's own diff (which runs on the main thread and turns rough on big files). */
export function hunkChanges(a: string, b: string, hunks: DiffHunk[]): Change[] {
  const ta = Text.of(a.split("\n")), tb = Text.of(b.split("\n"));
  if (!hunksFit(ta, tb, hunks)) return [];
  return hunks.map((h) => {
    const [fromA, toA] = side(ta, h.oldStart, h.oldLines);
    const [fromB, toB] = side(tb, h.newStart, h.newLines);
    return new Change(fromA, Math.min(toA, a.length), fromB, Math.min(toB, b.length));
  });
}

/** How many lines a chunk removes and adds. */
export function chunkLines(a: Text, b: Text, c: Chunk): { removed: number; added: number } {
  return {
    removed: c.toA > c.fromA ? a.lineAt(c.endA).number - a.lineAt(c.fromA).number + 1 : 0,
    added: c.toB > c.fromB ? b.lineAt(c.endB).number - b.lineAt(c.fromB).number + 1 : 0,
  };
}

/** The old line numbers of a chunk's removed lines, first and last (null when it removes none). */
export function removedNumbers(a: Text, c: Chunk): [number, number] | null {
  return c.toA > c.fromA ? [a.lineAt(c.fromA).number, a.lineAt(c.endA).number] : null;
}

/** A text as the view shows it: its lines, without the empty line after a last line break (git counts no such line). */
export function shownLines(doc: Text, text: string): number {
  return text.endsWith("\n") && doc.lines > 1 ? doc.lines - 1 : doc.lines;
}

// ---- folds ----

/** Unchanged lines hidden behind a fold bar, by their line numbers in the new text (inclusive). `where`: at the top of
 *  the file (only ↑ 20, the lines above the change below), at its end (only ↓ 20) or between two changes (both). */
export type Fold = { from: number; to: number; where: "top" | "mid" | "end" };

/** The folds of a file with `lines` lines whose changes cover these line ranges of the new text (in order; an empty
 *  range [n, n - 1] is a deletion before line n): every stretch of at least MIN_FOLD unchanged lines, keeping CONTEXT
 *  lines next to a change. A file without changes isn't folded. */
export function foldsOf(lines: number, changed: { from: number; to: number }[]): Fold[] {
  if (!changed.length || lines <= 0) return [];
  const folds: Fold[] = [];
  let next = 1; // the first line not yet looked at
  for (let i = 0; i <= changed.length; i++) {
    const top = i === 0, end = i === changed.length;
    const until = end ? lines : changed[i].from - 1; // the last unchanged line of this stretch
    const from = top ? next : next + CONTEXT;
    const to = end ? until : until - CONTEXT;
    if (to - from + 1 >= MIN_FOLD) folds.push({ from, to, where: top ? "top" : end ? "end" : "mid" });
    if (!end) next = Math.max(next, changed[i].to + 1);
  }
  return folds;
}

/** A fold after one of its buttons: "down" shows STEP more lines below the change above, "up" STEP more above the change
 *  below, "all" everything. Null when nothing is left to hide. */
export function openFold(f: Fold, how: "down" | "up" | "all"): Fold | null {
  if (how === "all") return null;
  const next = how === "down" ? { ...f, from: f.from + STEP } : { ...f, to: f.to - STEP };
  return next.to >= next.from ? next : null;
}

/** What a fold bar offers: its label and the steps it shows (none for STEP lines or fewer). */
export function foldBar(f: Fold): { count: number; label: string; down: boolean; up: boolean } {
  const count = f.to - f.from + 1;
  const steps = count > STEP;
  return { count, label: `${count.toLocaleString("en-GB")} unchanged ${count === 1 ? "line" : "lines"}`, down: steps && f.where !== "top", up: steps && f.where !== "end" };
}

// ---- the file list ----

export const STATUS: Record<FileStatus, { letter: string; word: string; cls: string }> = {
  added: { letter: "A", word: "Added", cls: "a" },
  modified: { letter: "M", word: "Modified", cls: "m" },
  deleted: { letter: "D", word: "Deleted", cls: "d" },
  renamed: { letter: "R", word: "Renamed", cls: "r" },
  copied: { letter: "C", word: "Copied", cls: "r" },
  typechange: { letter: "T", word: "Changed type", cls: "m" },
};

/** A count as the view writes it: thousands separated ("18,402"). */
export const count = (n: number) => n.toLocaleString("en-GB");

/** "src/components" and "Sidebar.tsx" from "src/components/Sidebar.tsx" (the folder is "" at the top level). */
export function splitPath(path: string): { dir: string; name: string } {
  const i = path.lastIndexOf("/");
  return { dir: path.slice(0, i + 1).replace(/\/$/, ""), name: path.slice(i + 1) };
}

/** The files under their folder (its full path): folders in path order, then the files at the top level; each folder's
 *  files keep the list's order. */
export function groupFiles(files: FileChange[]): { dir: string; files: FileChange[] }[] {
  const groups = new Map<string, FileChange[]>();
  for (const f of files) {
    const { dir } = splitPath(f.path);
    groups.set(dir, [...(groups.get(dir) ?? []), f]);
  }
  return [...groups.entries()].sort(([a], [b]) => (a === "" ? 1 : b === "" ? -1 : a < b ? -1 : a > b ? 1 : 0)).map(([dir, files]) => ({ dir, files }));
}

/** The files whose path (or old path) holds `query`, ignoring case. */
export function filterFiles(files: FileChange[], query: string): FileChange[] {
  const q = query.trim().toLowerCase();
  return q ? files.filter((f) => f.path.toLowerCase().includes(q) || !!f.oldPath?.toLowerCase().includes(q)) : files;
}

const LOCKS: Record<string, string> = {
  "package-lock.json": "npm", "composer.lock": "Composer", "cargo.lock": "Cargo", "yarn.lock": "Yarn", "pnpm-lock.yaml": "pnpm",
};
/** The package manager that writes this lock file, or null when it isn't one. */
export function lockOf(path: string): string | null {
  return LOCKS[splitPath(path).name.toLowerCase()] ?? null;
}
/** Whether a build made this file: under dist/ or build/, a minified script or a source map. */
export function isBuilt(path: string): boolean {
  return /(^|\/)(dist|build)\//.test(path) || /\.min\.(js|css)$/i.test(path) || /\.map$/i.test(path);
}
/** A renamed or copied file whose content stayed as it was. */
export const sameContent = (f: FileChange) => (f.status === "renamed" || f.status === "copied") && !f.binary && !f.additions && !f.deletions;

/** Why a file comes folded (its header's word and the note under it), or null when it shows at once. */
export function foldedWhy(f: FileChange): { why: string; kind: "lock" | "built" | "big" | "deleted" } | null {
  const lines = (f.additions ?? 0) + (f.deletions ?? 0);
  if (f.binary) return null;
  if (lockOf(f.path)) return { why: "Lock file, folded", kind: "lock" };
  if (isBuilt(f.path)) return { why: "Built file, folded", kind: "built" };
  if (lines > BIG_DIFF) return { why: "Big diff, folded", kind: "big" };
  if (f.status === "deleted" && (f.deletions ?? 0) > BIG_DELETION) return { why: "Deleted, folded", kind: "deleted" };
  return null;
}

/** The note under a folded file's header. */
export function foldedNote(f: FileChange, kind: "lock" | "built" | "big" | "deleted"): string {
  const add = f.additions ?? 0, del = f.deletions ?? 0;
  if (kind === "lock") return `A lock file: what ${lockOf(f.path)} wrote for the packages.`;
  if (kind === "built") return "A built file: a build wrote it, not a person.";
  if (kind === "deleted") return `Deleted: ${count(del)} lines.`;
  const what = !del ? `${count(add)} added lines` : !add ? `${count(del)} removed lines` : `${count(add + del)} changed lines`;
  return `A big diff: ${what}. It stays folded until you ask, so the page stays quick.`;
}

/** The rows a file's diff will take, to size its box before it loads: its changed lines, plus room for context. */
export function estimatedRows(f: FileChange): number {
  if (f.binary || sameContent(f)) return 1;
  const lines = (f.additions ?? 0) + (f.deletions ?? 0);
  return f.status === "added" || f.status === "deleted" ? lines : lines + 8;
}

// ---- the heading and the tab ----

/** The Changes tab's label: the counts, "No changes", or nothing (no branch yet, still loading, or it failed). */
export function tabLabel(c: CardChanges | null | undefined): { add: number; del: number } | "none" | null {
  if (!c || c.source === "none") return null;
  return c.files.length ? { add: c.additions, del: c.deletions } : "none";
}

/** The first line of the list's heading: where the changes come from. "Since origin/main" for the card's worktree;
 *  else what the backend says (its branch, the range saved at its merge, its merge commit, the pull request's diff). */
export function sourceLine(c: CardChanges, branch?: string | null): { since?: string; text?: string } {
  if (c.source === "worktree") return { since: c.baseLabel };
  if (c.source === "branch" && branch) return { text: `From branch ${branch} (its worktree was removed)` };
  if (c.source === "pull") return { text: (c.note ?? "From the pull request's diff").replace(/\.$/, "") + " (folds can't open)" };
  return c.note ? { text: c.note.replace(/\.$/, "") } : { since: c.baseLabel };
}

/** How many files aren't committed yet. */
export const uncommitted = (c: CardChanges) => c.files.filter((f) => f.uncommitted).length;

/** "Run by Backend Agent" and "10 Oct 14:02". */
export function runLine(r: Pick<Run, "agentName" | "startedAt" | "createdAt">): { who: string; when: string } {
  const at = new Date(r.startedAt ?? r.createdAt);
  const when = `${at.toLocaleDateString("en-GB", { day: "numeric", month: "short" })} ${at.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" })}`;
  return { who: `Run by ${r.agentName}`, when };
}

/** "1 file", "20 files". */
export const files = (n: number) => `${count(n)} ${n === 1 ? "file" : "files"}`;

// ---- a pull request's diff (source "pull"): the patch's rows ----

export type PatchRow =
  | { kind: "add" | "del" | "same"; no: number; text: string }
  | { kind: "gap"; lines: number };

/** One file's part of a pull request's diff as rows: removed lines with their old number, added and unchanged lines with
 *  their number now, and a gap for the unchanged lines before and between hunks that the diff leaves out. */
export function patchRows(patch: string): PatchRow[] {
  const rows: PatchRow[] = [];
  let oldNo = 0, newNo = 0, started = false;
  const lines = patch.replace(/\r\n/g, "\n").split("\n");
  if (lines[lines.length - 1] === "") lines.pop(); // after the last line break
  for (const line of lines) {
    const h = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(line);
    if (h) {
      const nextNew = Number(h[3]) - (h[4] === "0" ? 0 : 1);
      const gap = nextNew - newNo;
      if (gap > 0) rows.push({ kind: "gap", lines: gap });
      oldNo = Number(h[1]) - (h[2] === "0" ? 0 : 1);
      newNo = nextNew;
      started = true;
      continue;
    }
    if (!started || line.startsWith("\\")) continue;
    if (line.startsWith("+")) rows.push({ kind: "add", no: ++newNo, text: line.slice(1) });
    else if (line.startsWith("-")) rows.push({ kind: "del", no: ++oldNo, text: line.slice(1) });
    else if (line.startsWith(" ") || line === "") {
      oldNo++;
      rows.push({ kind: "same", no: ++newNo, text: line.slice(1) });
    }
  }
  return rows;
}
