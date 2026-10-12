// GA-112 QA: what the Changes view works out without the screen (src/lib/changes.ts): git's hunks as the merge view's
// chunks line for line, the folds of unchanged code (3 lines around a change, steps of 20, a click for all), removed
// lines' old numbers, the file list's groups, letters and folded files, the tab's label and the heading.
import { describe, expect, it } from "vitest";
import { Text } from "@codemirror/state";
import type { CardChanges, DiffHunk, FileChange } from "../types";
import {
  BIG_DELETION, BIG_DIFF, CONTEXT, STEP, chunkLines, count, estimatedRows, filterFiles, foldBar, foldedNote, foldedWhy, foldsOf,
  gitChunks, groupFiles, hunkChanges, hunksFit, isBuilt, lockOf, openFold, patchRows, removedNumbers, runLine, sameContent,
  shownLines, sourceLine, splitPath, tabLabel, uncommitted, wordChanges,
} from "./changes";

const doc = (s: string) => Text.of(s.split("\n"));
const lines = (n: number, f: (i: number) => string) => Array.from({ length: n }, (_, i) => f(i + 1)).join("\n") + "\n";
const file = (path: string, more: Partial<FileChange> = {}): FileChange => ({
  path, oldPath: null, status: "modified", additions: 1, deletions: 1, binary: false, uncommitted: false, ...more,
});
const card = (more: Partial<CardChanges> = {}): CardChanges => ({
  source: "worktree", from: "abc", to: null, baseLabel: "origin/main", additions: 0, deletions: 0, files: [], note: null, ...more,
});

/** A 1,500-line file with 40 scattered lines changed in place, and git's -U0 hunks for it (one per changed line). */
function scattered() {
  const at = Array.from({ length: 40 }, (_, i) => 10 + i * 37); // 10, 47, 84, … 1453
  const oldText = lines(1500, (i) => `  const value${i} = compute(${i}, "row ${i}");`);
  const newText = lines(1500, (i) => (at.includes(i) ? `  const value${i} = compute(${i} + 1, "row ${i}");` : `  const value${i} = compute(${i}, "row ${i}");`));
  const hunks: DiffHunk[] = at.map((n) => ({ oldStart: n, oldLines: 1, newStart: n, newLines: 1 }));
  return { at, oldText, newText, hunks };
}

describe("git's hunks as the merge view's chunks", () => {
  it("a 1,500-line file with 40 scattered changed lines: 40 one-line chunks, at git's lines, +40 −40", () => {
    const { at, oldText, newText, hunks } = scattered();
    const a = doc(oldText), b = doc(newText);
    expect(hunksFit(a, b, hunks)).toBe(true);
    const chunks = gitChunks(a, b, hunks);
    expect(chunks).toHaveLength(40);
    let add = 0, del = 0;
    chunks.forEach((c, i) => {
      const n = chunkLines(a, b, c);
      expect(n).toEqual({ removed: 1, added: 1 });
      expect(a.lineAt(c.fromA).number).toBe(at[i]);
      expect(b.lineAt(c.fromB).number).toBe(at[i]);
      expect(removedNumbers(a, c)).toEqual([at[i], at[i]]);
      add += n.added; del += n.removed;
    });
    // the file list's counts (git's numstat) are the hunks' lines: the view agrees with them
    expect([add, del]).toEqual([hunks.reduce((s, h) => s + h.newLines, 0), hunks.reduce((s, h) => s + h.oldLines, 0)]);
    expect([add, del]).toEqual([40, 40]);
  });

  it("marks the changed words of a pair of lines, relative to the chunk", () => {
    const a = doc("one\nlet x = 1;\nthree\n"), b = doc("one\nlet x = 2;\nthree\n");
    const [c] = gitChunks(a, b, [{ oldStart: 2, oldLines: 1, newStart: 2, newLines: 1 }]);
    expect(c.changes).toHaveLength(1);
    const ch = c.changes[0];
    expect(a.sliceString(c.fromA + ch.fromA, c.fromA + ch.toA)).toBe("1");
    expect(b.sliceString(c.fromB + ch.fromB, c.fromB + ch.toB)).toBe("2");
  });

  it("marks no words in a line written anew, and none in equal lines", () => {
    expect(wordChanges("fn main() {}", "fn main() {}")).toEqual([]);
    expect(wordChanges("const a = loadEverything();", "return <Spinner big />;")).toEqual([]);
    expect(wordChanges("let total = 10;", "let total = 12;").length).toBeGreaterThan(0);
  });

  it("an insertion at the top (git's oldStart 0) and a deletion at the end sit where git puts them", () => {
    const a = doc("b\nc\nd\n"), b = doc("a\nb\nc\n");
    const hunks: DiffHunk[] = [{ oldStart: 0, oldLines: 0, newStart: 1, newLines: 1 }, { oldStart: 3, oldLines: 1, newStart: 3, newLines: 0 }];
    expect(hunksFit(a, b, hunks)).toBe(true);
    const [top, end] = gitChunks(a, b, hunks);
    expect([top.fromA, top.toA]).toEqual([0, 0]);
    expect(b.sliceString(top.fromB, top.toB)).toBe("a\n");
    expect(a.sliceString(end.fromA, end.toA)).toBe("d\n");
    expect([end.fromB, end.toB]).toEqual([b.length, b.length]);
    expect(chunkLines(a, b, top)).toEqual({ removed: 0, added: 1 });
    expect(chunkLines(a, b, end)).toEqual({ removed: 1, added: 0 });
    expect(removedNumbers(a, top)).toBeNull();
    expect(removedNumbers(a, end)).toEqual([3, 3]);
  });

  it("a pure deletion in the middle sits at the start of the line after git's start", () => {
    const a = doc("1\n2\n3\n4\n5\n"), b = doc("1\n2\n5\n");
    const [c] = gitChunks(a, b, [{ oldStart: 3, oldLines: 2, newStart: 2, newLines: 0 }]);
    expect(c.fromB).toBe(b.line(3).from);
    expect(a.sliceString(c.fromA, c.toA)).toBe("3\n4\n");
    expect(removedNumbers(a, c)).toEqual([3, 4]);
  });

  it("refuses hunks that don't fit the texts (the file changed on disk after git's diff)", () => {
    const a = doc("1\n2\n3\n"), b = doc("1\n2\n3\n");
    expect(hunksFit(a, b, [{ oldStart: 3, oldLines: 5, newStart: 3, newLines: 1 }])).toBe(false);
    expect(hunksFit(a, b, [{ oldStart: 9, oldLines: 0, newStart: 1, newLines: 1 }])).toBe(false);
    expect(hunksFit(a, b, [{ oldStart: 2, oldLines: 1, newStart: 2, newLines: 1 }, { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1 }])).toBe(false);
    expect(hunksFit(a, b, [{ oldStart: -1, oldLines: 1, newStart: 1, newLines: 1 }])).toBe(false);
    expect(hunkChanges("1\n2\n3\n", "1\n2\n3\n", [{ oldStart: 3, oldLines: 5, newStart: 3, newLines: 1 }])).toEqual([]);
  });

  it("gives the merge view's override whole-line changes, one per hunk, inside the texts", () => {
    const { oldText, newText, hunks } = scattered();
    const changes = hunkChanges(oldText, newText, hunks);
    expect(changes).toHaveLength(40);
    for (const c of changes) {
      expect(c.toA).toBeLessThanOrEqual(oldText.length);
      expect(c.toB).toBeLessThanOrEqual(newText.length);
      expect(oldText.slice(c.fromA, c.toA).split("\n").filter(Boolean)).toHaveLength(1);
    }
  });

  it("counts a text's lines as git does: no line after the last line break", () => {
    expect(shownLines(doc("a\nb\n"), "a\nb\n")).toBe(2);
    expect(shownLines(doc("a\nb"), "a\nb")).toBe(2);
    expect(shownLines(doc("\n"), "\n")).toBe(1);
  });
});

describe("folds of unchanged code", () => {
  it("keeps 3 lines around a change and folds the rest, at the top, between changes and at the end", () => {
    expect(CONTEXT).toBe(3);
    // big.ts in the UI probe: changes at 10 and 47 of 1,500 lines
    const folds = foldsOf(1500, [{ from: 10, to: 10 }, { from: 47, to: 47 }]);
    expect(folds).toEqual([
      { from: 1, to: 6, where: "top" },
      { from: 14, to: 43, where: "mid" },
      { from: 51, to: 1500, where: "end" },
    ]);
    expect(folds.map((f) => foldBar(f).label)).toEqual(["6 unchanged lines", "30 unchanged lines", "1,450 unchanged lines"]);
  });

  it("keeps 3 lines on both sides of a deletion (an empty range before a line)", () => {
    expect(foldsOf(100, [{ from: 50, to: 49 }])).toEqual([{ from: 1, to: 46, where: "top" }, { from: 53, to: 100, where: "end" }]);
  });

  it("doesn't fold fewer than 4 lines, and doesn't fold a file without changes", () => {
    expect(foldsOf(30, [{ from: 10, to: 10 }, { from: 20, to: 20 }]).filter((f) => f.where === "mid")).toEqual([]);
    expect(foldsOf(30, [{ from: 10, to: 10 }, { from: 21, to: 21 }]).filter((f) => f.where === "mid")).toEqual([{ from: 14, to: 17, where: "mid" }]);
    expect(foldsOf(5, [{ from: 3, to: 3 }])).toEqual([]);
    expect(foldsOf(100, [])).toEqual([]);
  });

  it("↓ 20 shows 20 more lines under the change above, ↑ 20 above the change below, a click all", () => {
    expect(STEP).toBe(20);
    const mid = { from: 14, to: 43, where: "mid" as const };
    expect(foldBar(mid)).toEqual({ count: 30, label: "30 unchanged lines", down: true, up: true });
    const down = openFold(mid, "down")!;
    expect(down).toEqual({ from: 34, to: 43, where: "mid" });
    expect(foldBar(down)).toEqual({ count: 10, label: "10 unchanged lines", down: false, up: false });
    expect(openFold(mid, "up")).toEqual({ from: 14, to: 23, where: "mid" });
    expect(openFold(mid, "all")).toBeNull();
    expect(openFold(down, "down")).toBeNull();
  });

  it("offers only ↑ 20 at the top of a file and only ↓ 20 at its end", () => {
    expect(foldBar({ from: 1, to: 100, where: "top" })).toMatchObject({ down: false, up: true });
    expect(foldBar({ from: 1, to: 100, where: "end" })).toMatchObject({ down: true, up: false });
    expect(foldBar({ from: 5, to: 5, where: "mid" }).label).toBe("1 unchanged line");
  });
});

describe("the file list", () => {
  it("groups files under their folder, folders in path order and the top level last", () => {
    const groups = groupFiles([file("package-lock.json"), file("src/components/Sidebar.tsx"), file("crates/core/src/a.rs"), file("src/components/editor/E.tsx"), file("src/lib/sidebar.ts"), file("crates/core/src/b.rs")]);
    expect(groups.map((g) => g.dir)).toEqual(["crates/core/src", "src/components", "src/components/editor", "src/lib", ""]);
    expect(groups[0].files.map((f) => f.path)).toEqual(["crates/core/src/a.rs", "crates/core/src/b.rs"]);
    expect(splitPath("src/lib/sidebar.ts")).toEqual({ dir: "src/lib", name: "sidebar.ts" });
    expect(splitPath("README.md")).toEqual({ dir: "", name: "README.md" });
  });

  it("filters on the path and the old path, ignoring case", () => {
    const list = [file("src/Sidebar.tsx"), file("src/folder_rules.rs", { status: "renamed", oldPath: "src/folders.rs" }), file("README.md")];
    expect(filterFiles(list, "SIDE").map((f) => f.path)).toEqual(["src/Sidebar.tsx"]);
    expect(filterFiles(list, "folders.rs").map((f) => f.path)).toEqual(["src/folder_rules.rs"]);
    expect(filterFiles(list, "  ")).toBe(list);
    expect(filterFiles(list, "zzz")).toEqual([]);
  });

  it("folds lock files, built files, big diffs and long deleted files, and says why", () => {
    expect(lockOf("package-lock.json")).toBe("npm");
    expect(lockOf("api/composer.lock")).toBe("Composer");
    expect(lockOf("Cargo.lock")).toBe("Cargo");
    expect(lockOf("src/lock.ts")).toBeNull();
    expect(isBuilt("dist/app.js") && isBuilt("web/build/x.css") && isBuilt("a/b.min.js") && isBuilt("x.js.map")).toBe(true);
    expect(isBuilt("src/distance.ts")).toBe(false);
    expect(foldedWhy(file("package-lock.json", { additions: 412, deletions: 20 }))).toEqual({ why: "Lock file, folded", kind: "lock" });
    expect(foldedWhy(file("dist/app.js"))).toEqual({ why: "Built file, folded", kind: "built" });
    expect(foldedWhy(file("src/lang-map.json", { status: "added", additions: BIG_DIFF + 1, deletions: 0 }))).toEqual({ why: "Big diff, folded", kind: "big" });
    expect(foldedWhy(file("src/lang-map.json", { additions: BIG_DIFF, deletions: 0 }))).toBeNull();
    expect(foldedWhy(file("src/old.rs", { status: "deleted", additions: 0, deletions: BIG_DELETION + 5 }))).toEqual({ why: "Deleted, folded", kind: "deleted" });
    expect(foldedWhy(file("src/old.rs", { status: "deleted", additions: 0, deletions: BIG_DELETION }))).toBeNull();
    expect(foldedWhy(file("package-lock.json", { binary: true, additions: null, deletions: null }))).toBeNull();
    expect(foldedNote(file("package-lock.json"), "lock")).toBe("A lock file: what npm wrote for the packages.");
    expect(foldedNote(file("src/old.rs", { deletions: 1045 }), "deleted")).toBe("Deleted: 1,045 lines.");
    expect(foldedNote(file("a.json", { additions: 3214, deletions: 0 }), "big")).toBe("A big diff: 3,214 added lines. It stays folded until you ask, so the page stays quick.");
    expect(foldedNote(file("a.json", { additions: 3000, deletions: 214 }), "big")).toContain("3,214 changed lines");
  });

  it("knows a rename without changes, and sizes a box before its diff loads", () => {
    expect(sameContent(file("b.rs", { status: "renamed", oldPath: "a.rs", additions: 0, deletions: 0 }))).toBe(true);
    expect(sameContent(file("b.rs", { status: "renamed", oldPath: "a.rs", additions: 2, deletions: 1 }))).toBe(false);
    expect(estimatedRows(file("b.rs", { status: "renamed", oldPath: "a.rs", additions: 0, deletions: 0 }))).toBe(1);
    expect(estimatedRows(file("x.png", { binary: true, additions: null, deletions: null }))).toBe(1);
    expect(estimatedRows(file("new.ts", { status: "added", additions: 25, deletions: 0 }))).toBe(25);
    expect(estimatedRows(file("m.ts", { additions: 7, deletions: 2 }))).toBe(17);
  });
});

describe("the tab and the heading", () => {
  it("the Changes tab counts, says No changes, or nothing (no branch, loading, failed)", () => {
    expect(tabLabel(card({ additions: 6, deletions: 4, files: [file("a.ts")] }))).toEqual({ add: 6, del: 4 });
    expect(tabLabel(card({ files: [] }))).toBe("none");
    expect(tabLabel(card({ source: "none" }))).toBeNull();
    expect(tabLabel(null)).toBeNull();
    expect(tabLabel(undefined)).toBeNull();
  });

  it("says where the changes come from, as D1 writes it", () => {
    expect(sourceLine(card())).toEqual({ since: "origin/main" });
    expect(sourceLine(card({ baseLabel: "main" }))).toEqual({ since: "main" });
    expect(sourceLine(card({ source: "branch", note: "From the branch" }), "gizai/ga-12-export")).toEqual({ text: "From branch gizai/ga-12-export (its worktree was removed)" });
    expect(sourceLine(card({ source: "merge", note: "As merged into main (pull request #81)." }))).toEqual({ text: "As merged into main (pull request #81)" });
    expect(sourceLine(card({ source: "pull", note: "From GitHub's diff of pull request #12." }))).toEqual({ text: "From GitHub's diff of pull request #12 (folds can't open)" });
    expect(sourceLine(card({ source: "pull", note: null }))).toEqual({ text: "From the pull request's diff (folds can't open)" });
  });

  it("counts the files not committed yet, writes counts with thousands and a run's line", () => {
    expect(uncommitted(card({ files: [file("a"), file("b", { uncommitted: true }), file("c", { status: "added", uncommitted: true })] }))).toBe(2);
    expect(count(18402)).toBe("18,402");
    const r = runLine({ agentName: "Backend Agent", startedAt: Date.UTC(2026, 9, 10, 12, 2), createdAt: 0 });
    expect(r.who).toBe("Run by Backend Agent");
    expect(r.when).toMatch(/^10 Oct \d\d:\d\d$/);
  });
});

describe("a pull request's patch (source pull)", () => {
  it("gives removed lines their old number, added and unchanged lines their new one, and gaps for what it leaves out", () => {
    const patch = "@@ -10,3 +10,4 @@ fn main()\n keep\n-old line\n+new line\n+another\n same\n@@ -40,2 +41,2 @@\n-x\n+y\n z\n\\ No newline at end of file\n";
    expect(patchRows(patch)).toEqual([
      { kind: "gap", lines: 9 },
      { kind: "same", no: 10, text: "keep" },
      { kind: "del", no: 11, text: "old line" },
      { kind: "add", no: 11, text: "new line" },
      { kind: "add", no: 12, text: "another" },
      { kind: "same", no: 13, text: "same" },
      { kind: "gap", lines: 27 },
      { kind: "del", no: 40, text: "x" },
      { kind: "add", no: 41, text: "y" },
      { kind: "same", no: 42, text: "z" },
    ]);
  });

  it("reads a new file's patch from line 1, and CRLF line ends", () => {
    expect(patchRows("@@ -0,0 +1,2 @@\r\n+a\r\n+b\r\n")).toEqual([{ kind: "add", no: 1, text: "a" }, { kind: "add", no: 2, text: "b" }]);
  });
});
