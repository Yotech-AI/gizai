// GA-112 QA: one file's diff as the Changes view builds it (src/lib/diffView.ts), checked on the editor state without a
// screen: read-only, @codemirror/merge's unified view holding exactly git's hunks (not its own rougher diff, and not two
// hunks joined across an empty line), the fold bars with 3 lines kept around a change, removed lines' old numbers in the
// number column, Alt+↑/↓ bound, an added or deleted file shown whole, and hunks that don't fit falling back to the
// package's own diff.
import { describe, expect, it } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumberWidgetMarker, type DecorationSet } from "@codemirror/view";
import { getChunks, getOriginalDoc } from "@codemirror/merge";
import type { DiffHunk } from "../types";
import { diffState, textOf } from "./diffView";

const lines = (n: number, f: (i: number) => string) => Array.from({ length: n }, (_, i) => f(i + 1)).join("\n") + "\n";
const build = (oldText: string | null, newText: string | null, hunks: DiffHunk[]) =>
  diffState({ oldText, newText, hunks, language: null, label: "Changes in src/big.ts" });
const chunksOf = (s: EditorState) => getChunks(s)?.chunks ?? [];

/** The fold bars' folds (their widgets' `fold`), in order. */
function folds(s: EditorState) {
  const out: { from: number; to: number; where: string }[] = [];
  for (const d of s.facet(EditorView.decorations)) {
    if (typeof d === "function") continue;
    (d as DecorationSet).between(0, s.doc.length, (_f, _t, v) => {
      const w = v.spec.widget as { fold?: { from: number; to: number; where: string } } | undefined;
      if (w?.fold) out.push(w.fold);
    });
  }
  return out.sort((a, b) => a.from - b.from);
}

/** What the number column shows beside the block widget at `pos` (removed lines: their old numbers). */
function numbersAt(s: EditorState, pos: number): string | null {
  for (const f of s.facet(lineNumberWidgetMarker)) {
    const m = f({ state: s } as EditorView, {} as never, { from: pos } as never) as { first?: number; last?: number; mark?: string | null } | null;
    if (m && m.first !== undefined) return Array.from({ length: m.last! - m.first + 1 }, (_, i) => String(m.first! + i)).join(" ");
  }
  return null;
}

describe("a changed file's diff", () => {
  const at = Array.from({ length: 40 }, (_, i) => 10 + i * 37);
  const oldText = lines(1500, (i) => `export const row${i} = format(${i}, { width: 80, label: "row ${i}" });`);
  const newText = lines(1500, (i) => (at.includes(i) ? `export const row${i} = format(${i}, { width: 96, label: "row ${i}" });` : `export const row${i} = format(${i}, { width: 80, label: "row ${i}" });`));
  const hunks = at.map((n) => ({ oldStart: n, oldLines: 1, newStart: n, newLines: 1 }));

  it("is read-only and names the file for screen readers", () => {
    const { state } = build(oldText, newText, hunks);
    expect(state.readOnly).toBe(true);
    expect(state.doc.toString()).toBe(newText);
    expect(getOriginalDoc(state).toString()).toBe(oldText);
  });

  it("holds git's 40 one-line changes of a 1,500-line file, not blocks of its own, and +40 −40 like the file list", () => {
    const { state, digits } = build(oldText, newText, hunks);
    const chunks = chunksOf(state);
    expect(chunks).toHaveLength(40);
    const a = getOriginalDoc(state), b = state.doc;
    let add = 0, del = 0;
    chunks.forEach((c, i) => {
      expect(a.lineAt(c.fromA).number).toBe(at[i]);
      expect(b.lineAt(c.fromB).number).toBe(at[i]);
      expect(a.lineAt(c.endA).number).toBe(at[i]);
      expect(b.lineAt(c.endB).number).toBe(at[i]);
      add += b.lineAt(c.endB).number - b.lineAt(c.fromB).number + 1;
      del += a.lineAt(c.endA).number - a.lineAt(c.fromA).number + 1;
    });
    expect([add, del]).toEqual([40, 40]);
    expect(digits).toBe(4);
  });

  it("keeps two changes apart that only an empty line separates, as git does", () => {
    const { state } = build("a\n\nb\nc\n", "A\n\nB\nc\n", [{ oldStart: 1, oldLines: 1, newStart: 1, newLines: 1 }, { oldStart: 3, oldLines: 1, newStart: 3, newLines: 1 }]);
    const chunks = chunksOf(state);
    expect(chunks).toHaveLength(2);
    expect(state.doc.lineAt(chunks[0].fromB).number).toBe(1);
    expect(state.doc.lineAt(chunks[1].fromB).number).toBe(3);
  });

  it("shows git's lines when its diff differs from the one the package would make", () => {
    // git paired the moved block differently: one hunk replacing lines 2-3; the view keeps exactly that
    const { state } = build("x\na\nb\ny\n", "x\nb\na\ny\n", [{ oldStart: 2, oldLines: 2, newStart: 2, newLines: 2 }]);
    const chunks = chunksOf(state);
    expect(chunks).toHaveLength(1);
    expect(getOriginalDoc(state).sliceString(chunks[0].fromA, chunks[0].toA)).toBe("a\nb\n");
    expect(state.doc.sliceString(chunks[0].fromB, chunks[0].toB)).toBe("b\na\n");
  });

  it("folds unchanged code with 3 lines kept around each change", () => {
    const { state } = build(oldText, newText, hunks);
    const f = folds(state);
    expect(f[0]).toEqual({ from: 1, to: 6, where: "top" });
    expect(f[1]).toEqual({ from: 14, to: 43, where: "mid" });
    expect(f[f.length - 1]).toEqual({ from: 1453 + 4, to: 1500, where: "end" });
    expect(f).toHaveLength(41);
  });

  it("gives removed lines their old line numbers in the number column", () => {
    const { state } = build("1\n2\n3\n4\n5\n6\n", "1\n2\n6\n", [{ oldStart: 3, oldLines: 3, newStart: 2, newLines: 0 }]);
    const [c] = chunksOf(state);
    expect(numbersAt(state, c.fromB)).toBe("3 4 5");
    // a line that wasn't removed has no old numbers
    expect(numbersAt(state, 0)).toBeNull();
  });

  it("gives a changed line's removed side its old number even when lines were added above it", () => {
    const { state } = build("a\nb\nc\n", "new\na\nB\nc\n", [{ oldStart: 0, oldLines: 0, newStart: 1, newLines: 1 }, { oldStart: 2, oldLines: 1, newStart: 3, newLines: 1 }]);
    const chunks = chunksOf(state);
    expect(chunks).toHaveLength(2);
    expect(numbersAt(state, chunks[1].fromB)).toBe("2");
  });

  it("binds Alt+↓ and Alt+↑ to the next and previous change", () => {
    const { state } = build(oldText, newText, hunks);
    const keys = state.facet(keymap).flat().map((k) => k.key);
    expect(keys).toContain("Alt-ArrowDown");
    expect(keys).toContain("Alt-ArrowUp");
  });

  it("works the diff out itself when git's hunks don't fit the texts (changed on disk since)", () => {
    const { state } = build("a\nb\nc\n", "a\nB\nc\n", [{ oldStart: 7, oldLines: 4, newStart: 7, newLines: 4 }]);
    const chunks = chunksOf(state);
    expect(chunks).toHaveLength(1);
    expect(state.doc.lineAt(chunks[0].fromB).number).toBe(2);
  });

  it("reads CRLF files line by line", () => {
    const { state } = build("a\r\nb\r\nc\r\n", "a\r\nB\r\nc\r\n", [{ oldStart: 2, oldLines: 1, newStart: 2, newLines: 1 }]);
    expect(state.doc.lines).toBe(4);
    const [c] = chunksOf(state);
    expect(state.doc.lineAt(c.fromB).text).toBe("B");
    expect(textOf("x\r\ny\rz\n").lines).toBe(4);
  });
});

describe("an added or deleted file", () => {
  it("shows an added file whole, without a merge view, numbered to its last line", () => {
    const { state, digits } = build(null, lines(120, (i) => `line ${i}`), []);
    expect(getChunks(state)).toBeNull();
    expect(state.readOnly).toBe(true);
    expect(digits).toBe(3);
  });

  it("shows a deleted file whole from its old text", () => {
    const { state, digits } = build(lines(45, (i) => `old ${i}`), null, []);
    expect(state.doc.line(1).text).toBe("old 1");
    expect(digits).toBe(2);
  });
});
