// One file's diff in the Changes view (Code Editor F1, GA-112): a read-only CodeMirror editor on the file as it is now,
// with @codemirror/merge's unified view showing the removed lines above the added ones. The chunks are git's hunks
// (diffConfig.override, then exactly as git has them, see exactChunks), so the view shows git's diff and not a rough one
// of its own; removed lines get their old numbers in the number column (lineNumberWidgetMarker), every changed line its
// + or −, and unchanged code folds behind our own bar (↑ 20, ↓ 20, all) instead of collapseUnchanged. Alt+↓ and Alt+↑
// go to the next and previous change (goToNextChunk, goToPreviousChunk). An added or deleted file shows whole, every
// line added or removed.
import { EditorState, Prec, RangeSet, RangeSetBuilder, StateEffect, StateField, Text, type Extension, type Range } from "@codemirror/state";
import {
  Decoration, EditorView, GutterMarker, WidgetType, gutter, gutterLineClass, gutterWidgetClass, keymap, lineNumbers,
  lineNumberWidgetMarker, type BlockInfo, type DecorationSet,
} from "@codemirror/view";
import type { LanguageSupport } from "@codemirror/language";
import { Chunk, getChunks, getOriginalDoc, goToNextChunk, goToPreviousChunk, unifiedMergeView } from "@codemirror/merge";
import { foldBar, foldsOf, gitChunks, hunkChanges, hunksFit, openFold, removedNumbers, shownLines, type Fold } from "./changes";
import { codeTheme } from "./codeTheme";
import type { DiffHunk } from "../types";

/** A file's texts split into lines the way an editor splits them (\n, \r\n or \r). */
export const textOf = (s: string) => Text.of(s.split(/\r\n?|\n/));

// ---- the chunks, exactly as git has them ----

/** The merge view's own chunk field. unifiedMergeView builds its chunks from the diff (here git's hunks, through
 *  diffConfig.override) line by line, and joins two changes that only an empty line keeps apart; git keeps them apart.
 *  So the field is found once (the one holding the view's chunks) and given git's chunks as they are. Without it (a
 *  later version of the package), the override's chunks stay. */
let chunkField: StateField<readonly Chunk[]> | null | undefined;
function exactChunks(unified: Extension, doc: Text, chunks: readonly Chunk[]): Extension {
  if (chunkField === undefined) {
    chunkField = null;
    try {
      const probe = EditorState.create({ doc, extensions: unified });
      const built = getChunks(probe)?.chunks;
      const fields: StateField<unknown>[] = [];
      const walk = (e: unknown) => { if (Array.isArray(e)) e.forEach(walk); else if (e instanceof StateField) fields.push(e); };
      walk(unified);
      chunkField = (built && (fields.find((f) => probe.field(f, false) === built) as StateField<readonly Chunk[]> | undefined)) || null;
    } catch { /* keep the override's chunks */ }
  }
  return chunkField ? Prec.highest(chunkField.init(() => chunks)) : [];
}

// ---- the gutters: numbers, + and − ----

/** A stack of numbers or marks beside a block of removed lines, one a line. */
class Stack extends GutterMarker {
  constructor(readonly first: number, readonly last: number, readonly mark: string | null) { super(); }
  eq(o: Stack) { return o.first === this.first && o.last === this.last && o.mark === this.mark; }
  toDOM() {
    const dom = document.createElement("div");
    dom.className = "cm-diffStack";
    for (let n = this.first; n <= this.last; n++) dom.appendChild(document.createElement("div")).textContent = this.mark ?? String(n);
    return dom;
  }
}
class Mark extends GutterMarker {
  constructor(readonly text: string) { super(); }
  eq(o: Mark) { return o.text === this.text; }
  toDOM() { return document.createTextNode(this.text); }
}
class LineClass extends GutterMarker {
  constructor(readonly elementClass: string) { super(); }
  eq(o: LineClass) { return o.elementClass === this.elementClass; }
}
/** Lucide's unfold-vertical (ISC), for the fold bar's place in the number column. */
const UNFOLD = ["M12 22v-6", "M12 8V2", "M4 12H2", "M10 12H8", "M16 12h-2", "M22 12h-2", "m15 19-3 3-3-3", "m15 5-3-3-3 3"];
class Icon extends GutterMarker {
  eq() { return true; }
  toDOM() {
    const ns = "http://www.w3.org/2000/svg";
    const svg = document.createElementNS(ns, "svg");
    for (const [k, v] of Object.entries({ class: "icon sm", viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "2", "stroke-linecap": "round", "stroke-linejoin": "round", "aria-hidden": "true" })) svg.setAttribute(k, v);
    for (const d of UNFOLD) svg.appendChild(document.createElementNS(ns, "path")).setAttribute("d", d);
    return svg;
  }
}
const plus = new Mark("+"), minus = new Mark("−"), icon = new Icon();
const added = new LineClass("cm-diffAdded"), removed = new LineClass("cm-diffRemoved"), folded = new LineClass("cm-diffFolded");

/** The old line numbers of the removed lines shown at `pos` (a deletion widget), or null. */
function removedAt(state: EditorState, pos: number): [number, number] | null {
  const chunk = getChunks(state)?.chunks.find((c) => c.fromB === pos && c.toA > c.fromA);
  return chunk ? removedNumbers(getOriginalDoc(state), chunk) : null;
}

// ---- the fold bar ----

const openFoldEffect = StateEffect.define<{ from: number; how: "down" | "up" | "all" }>();

class FoldWidget extends WidgetType {
  constructor(readonly fold: Fold) { super(); }
  eq(o: FoldWidget) { return o.fold.from === this.fold.from && o.fold.to === this.fold.to && o.fold.where === this.fold.where; }
  get estimatedHeight() { return 28; }
  toDOM(view: EditorView) {
    const bar = foldBar(this.fold);
    const dom = document.createElement("div");
    dom.className = "cm-diffFold";
    const button = (cls: string, text: string, title: string, how: "down" | "up" | "all") => {
      const b = dom.appendChild(document.createElement("button"));
      b.type = "button";
      b.className = cls;
      b.textContent = text;
      b.title = title;
      b.dataset.how = how;
      b.addEventListener("click", (e) => { e.preventDefault(); view.dispatch({ effects: openFoldEffect.of({ from: this.fold.from, how }) }); });
    };
    button("fold-all", bar.label, `Show all ${bar.count.toLocaleString("en-GB")} lines`, "all");
    if (bar.down) button("fold-step", "↓ 20", "Show 20 more lines below the change above", "down");
    if (bar.up) button("fold-step", "↑ 20", "Show 20 more lines above the change below", "up");
    return dom;
  }
}

/** The folds of unchanged code, and the empty line after a last line break hidden (git has no such line). */
function foldField(initial: Fold[], hideLast: boolean, lines: number) {
  return StateField.define<Fold[]>({
    create: () => initial,
    update(folds, tr) {
      for (const e of tr.effects) {
        if (e.is(openFoldEffect)) folds = folds.flatMap((f) => (f.from === e.value.from ? openFold(f, e.value.how) ?? [] : [f]));
      }
      return folds;
    },
    provide: (field) => EditorView.decorations.compute([field], (state) => {
      const doc = state.doc, out: Range<Decoration>[] = [];
      let lastCovered = false;
      for (const f of state.field(field)) {
        // the fold at the end also covers the empty line after the last line break
        const end = f.where === "end" && f.to >= lines ? (lastCovered = true, doc.length) : doc.line(f.to).to;
        out.push(Decoration.replace({ widget: new FoldWidget(f), block: true }).range(doc.line(f.from).from, end));
      }
      if (hideLast && !lastCovered) out.push(Decoration.replace({}).range(doc.line(lines).to, doc.length));
      return Decoration.set(out, true);
    }),
  });
}

// ---- keys ----

/** Alt+↓ and Alt+↑: the next or previous change in this file; at the last (or first) one, nothing, so the Changes view
 *  goes on to the next (or previous) file. */
function chunkStep(dir: 1 | -1) {
  return (view: EditorView) => {
    const chunks = getChunks(view.state)?.chunks ?? [];
    const head = view.state.selection.main.head;
    const more = dir > 0 ? chunks.some((c) => c.fromB > head) : chunks.some((c) => c.endB < head);
    return more && (dir > 0 ? goToNextChunk : goToPreviousChunk)(view);
  };
}
const chunkKeys = keymap.of([
  { key: "Alt-ArrowDown", run: chunkStep(1) },
  { key: "Alt-ArrowUp", run: chunkStep(-1) },
]);

// ---- the look ----

/** The diff's own look on top of the code theme: D1's diff-* tokens, the number column as wide as the longest number
 *  plus 20px, then 18px for the mark (design/code-editor/changes.css). */
const diffTheme = EditorView.theme({
  ".cm-gutters": { backgroundColor: "var(--bg)" },
  ".cm-lineNumbers .cm-gutterElement": {
    boxSizing: "border-box", minWidth: "calc(var(--digits, 3) * 1ch + 20px)", padding: "0 10px 0 0", textAlign: "right",
  },
  ".cm-gutter.cm-diffSign": { width: "18px" },
  ".cm-diffSign .cm-gutterElement": { padding: "0", textAlign: "center" },
  ".cm-gutterElement.cm-diffAdded": { backgroundColor: "var(--diff-add-soft)", color: "var(--diff-add)" },
  ".cm-gutterElement.cm-diffRemoved": { backgroundColor: "var(--diff-del-soft)", color: "var(--diff-del)" },
  ".cm-gutterElement.cm-diffFolded": { backgroundColor: "var(--diff-fold)", display: "flex", alignItems: "center", justifyContent: "flex-end" },
  ".cm-diffSign .cm-gutterElement.cm-diffFolded": { justifyContent: "center" },
  ".cm-lineNumbers .cm-gutterElement.cm-diffFolded": { paddingRight: "8px" },
  ".cm-line": { padding: "0 24px 0 6px" },
  "&.cm-merge-b .cm-line.cm-changedLine, .cm-line.cm-diffWholeAdd": { backgroundColor: "var(--diff-add-soft)" },
  ".cm-line.cm-diffWholeDel": { backgroundColor: "var(--diff-del-soft)" },
  "&.cm-merge-b .cm-changedLine .cm-changedText": { background: "var(--diff-add-strong)", borderRadius: "2px" },
  "&.cm-merge-b .cm-deletedChunk": { backgroundColor: "var(--diff-del-soft)", paddingLeft: "6px" },
  "&.cm-merge-b .cm-deletedChunk .cm-deletedLine": { paddingRight: "24px" },
  "&.cm-merge-b .cm-deletedChunk .cm-deletedText": { background: "var(--diff-del-strong)", borderRadius: "2px" },
  ".cm-diffFold": {
    display: "flex", alignItems: "center", gap: "6px", minHeight: "28px", padding: "3px 12px 3px 6px", boxSizing: "border-box",
    backgroundColor: "var(--diff-fold)", color: "var(--text-2)", fontFamily: "var(--font-sans)", fontSize: "var(--fs-sm)", lineHeight: "1.3",
    whiteSpace: "nowrap",
  },
});

// ---- the editor ----

export type DiffInput = {
  oldText: string | null;
  newText: string | null;
  hunks: DiffHunk[];
  language: LanguageSupport | null;
  /** What screen readers hear for the code: "Changes in src/lib/sidebar.ts". */
  label: string;
};

/** The editor state of one file's diff, and how many digits its longest line number has (for `--digits`). */
export function diffState(input: DiffInput): { state: EditorState; digits: number } {
  const common: Extension[] = [
    EditorState.readOnly.of(true),
    EditorView.contentAttributes.of({ "aria-label": input.label }),
    // the diff's look over the code theme's (the number column, line padding)
    codeTheme, Prec.high(diffTheme),
    input.language ?? [],
  ];
  if (input.oldText === null || input.newText === null) return wholeState(input, common);
  const a = textOf(input.oldText), b = textOf(input.newText);
  const lines = shownLines(b, input.newText);
  const fits = hunksFit(a, b, input.hunks);
  const chunks = fits ? gitChunks(a, b, input.hunks) : Chunk.build(a, b);
  const unified = unifiedMergeView({
    original: a,
    mergeControls: false,
    gutter: false,
    ...(fits ? { diffConfig: { override: (x: string, y: string) => hunkChanges(x, y, input.hunks) } } : {}),
  });
  // + and the added lines' colour beside each added line
  const addedLines = new RangeSetBuilder<GutterMarker>(), plusses = new RangeSetBuilder<GutterMarker>();
  const changed: { from: number; to: number }[] = [];
  let deletionAtEnd = false;
  for (const c of chunks) {
    if (c.toB > c.fromB) {
      const first = b.lineAt(c.fromB).number, last = b.lineAt(c.endB).number;
      for (let n = first; n <= last; n++) {
        const at = b.line(n).from;
        addedLines.add(at, at, added);
        plusses.add(at, at, plus);
      }
      changed.push({ from: first, to: last });
    } else {
      const n = b.lineAt(c.fromB).number;
      changed.push({ from: n, to: n - 1 });
      if (c.fromB === b.length && n > lines) deletionAtEnd = true;
    }
  }
  const signs = plusses.finish();
  const field = foldField(foldsOf(lines, changed), lines < b.lines && !deletionAtEnd, lines);
  const widgetIs = (state: EditorState, widget: WidgetType, block: BlockInfo) =>
    widget instanceof FoldWidget ? "fold" : removedAt(state, block.from) ? "removed" : null;
  const state = EditorState.create({
    doc: b,
    extensions: [
      common,
      lineNumbers(),
      lineNumberWidgetMarker.of((view, widget, block) => {
        if (widget instanceof FoldWidget) return icon;
        const r = removedAt(view.state, block.from);
        return r ? new Stack(r[0], r[1], null) : null;
      }),
      gutter({
        class: "cm-diffSign",
        markers: () => signs,
        widgetMarker: (view, widget, block) => {
          const r = widget instanceof FoldWidget ? null : removedAt(view.state, block.from);
          return r ? new Stack(r[0], r[1], "−") : null;
        },
      }),
      gutterLineClass.of(addedLines.finish()),
      gutterWidgetClass.of((view, widget, block) => {
        const kind = widgetIs(view.state, widget, block);
        return kind === "fold" ? folded : kind === "removed" ? removed : null;
      }),
      field,
      chunkKeys,
      fits ? exactChunks(unified, b, chunks) : [],
      unified,
    ],
  });
  const digits = String(Math.max(shownLines(a, input.oldText), lines)).length;
  return { state, digits };
}

/** An added or deleted file: the whole text, every line added (+, its number) or removed (−, its old number). */
function wholeState(input: DiffInput, common: Extension[]): { state: EditorState; digits: number } {
  const removedFile = input.newText === null;
  const text = (removedFile ? input.oldText : input.newText) ?? "";
  const doc = textOf(text);
  const lines = shownLines(doc, text);
  const lineDeco = Decoration.line({ class: removedFile ? "cm-diffWholeDel" : "cm-diffWholeAdd" });
  const decos = new RangeSetBuilder<Decoration>(), classes = new RangeSetBuilder<GutterMarker>(), signs = new RangeSetBuilder<GutterMarker>();
  for (let n = 1; n <= lines; n++) {
    const at = doc.line(n).from;
    decos.add(at, at, lineDeco);
    classes.add(at, at, removedFile ? removed : added);
    signs.add(at, at, removedFile ? minus : plus);
  }
  const deco: DecorationSet = decos.finish();
  const hide = lines < doc.lines ? Decoration.set([Decoration.replace({}).range(doc.line(lines).to, doc.length)]) : Decoration.none;
  const signSet: RangeSet<GutterMarker> = signs.finish();
  const state = EditorState.create({
    doc,
    extensions: [
      common,
      lineNumbers(),
      gutter({ class: "cm-diffSign", markers: () => signSet }),
      gutterLineClass.of(classes.finish()),
      EditorView.decorations.of(deco),
      EditorView.decorations.of(hide),
    ],
  });
  return { state, digits: String(lines).length };
}
