// The Editor's code (GA-113): CodeMirror with F0's code theme and languages (src/lib/codeTheme.ts, codeLanguages.ts) and
// @codemirror/search. The page keeps one EditorState per tab and shows the open tab's in one EditorView, so each tab
// keeps its own undo history, cursor and scroll. Its keys: Ctrl+S saves, the search keymap (Ctrl+F find and replace,
// F3 or Ctrl+G next, Ctrl+Alt+G go to line, Ctrl+D the next occurrence), Tab indents as the file already does.
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, foldGutter, foldKeymap, indentOnInput, indentUnit } from "@codemirror/language";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, StateEffect, StateField, Transaction, type Extension } from "@codemirror/state";
import {
  Decoration, EditorView, crosshairCursor, drawSelection, dropCursor, highlightActiveLine, highlightActiveLineGutter, highlightSpecialChars,
  keymap, lineNumbers, rectangularSelection, type DecorationSet, type ViewUpdate,
} from "@codemirror/view";
import { codeTheme } from "../../lib/codeTheme";
import { loadLanguage } from "../../lib/codeLanguages";
import { indentOf } from "../../lib/editor";
import { findPanel } from "./FindPanel";

/** What the code tells the page; the page swaps them as it renders, every state shares the one object. */
export type CodeHandlers = { save: () => void; update: (u: ViewUpdate) => void };

const language = new Compartment();
const locked = new Compartment();

/** A line marked for a moment, when search in the folder or Go to line opens a file there. */
const flash = StateEffect.define<number | null>();
const flashField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    for (const e of tr.effects) {
      if (!e.is(flash)) continue;
      if (e.value === null || e.value > tr.state.doc.lines) return Decoration.none;
      const line = tr.state.doc.line(e.value);
      return Decoration.set([Decoration.line({ class: "cm-flash" }).range(line.from)]);
    }
    return tr.docChanged ? deco.map(tr.changes) : deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const pageTheme = EditorView.theme({
  "&": { flex: "1 1 auto", minHeight: "0", height: "100%" },
  ".cm-scroller": { overflow: "auto" },
  ".cm-flash": { backgroundColor: "var(--accent-soft)", transition: "background-color .6s" },
  ".cm-foldGutter .cm-gutterElement": { opacity: "0", cursor: "pointer" },
  ".cm-gutters:hover .cm-foldGutter .cm-gutterElement": { opacity: "1" },
  // Go to line's small panel (@codemirror/search's own), with the app's inputs and buttons; find and replace is FindPanel
  ".cm-panel.cm-gotoLine input": {
    height: "calc(28px + var(--ui-box, 0px))", margin: "0 8px", padding: "0 8px", border: "1px solid var(--line-2)", borderRadius: "var(--radius)",
    background: "var(--bg)", color: "var(--text)", fontFamily: "var(--font-mono)", fontSize: "var(--fs-sm)", outline: "none",
  },
  ".cm-panel.cm-gotoLine button": {
    height: "26px", padding: "0 9px", border: "1px solid var(--line-2)", borderRadius: "var(--radius)", backgroundImage: "none",
    background: "var(--raised)", color: "var(--text)", fontFamily: "var(--font-sans)", fontSize: "var(--fs-sm)", cursor: "pointer",
  },
  ".cm-panel.cm-gotoLine": { padding: "8px 12px", fontSize: "var(--fs-sm)" },
});

/** Whether the file may change here: read-only shows no cursor and takes no typing. */
const lockOf = (readOnly: boolean): Extension => [EditorState.readOnly.of(readOnly), EditorView.editable.of(!readOnly)];

/** A tab's state: its text, the code theme, its language once loaded (`setLanguage`), read-only or not. */
export function codeState(text: string, readOnly: boolean, on: { current: CodeHandlers }): EditorState {
  return EditorState.create({
    doc: text,
    extensions: [
      lineNumbers(), highlightActiveLineGutter(), highlightSpecialChars(), history(), foldGutter(), drawSelection(), dropCursor(),
      EditorState.allowMultipleSelections.of(true), indentOnInput(), bracketMatching(), rectangularSelection(), crosshairCursor(),
      highlightActiveLine(), highlightSelectionMatches(), search({ top: true, createPanel: findPanel }), flashField,
      indentUnit.of(indentOf(text)), EditorState.tabSize.of(4),
      keymap.of([
        { key: "Mod-s", run: () => { on.current.save(); return true; }, preventDefault: true },
        ...defaultKeymap, ...searchKeymap, ...historyKeymap, ...foldKeymap, indentWithTab,
      ]),
      codeTheme, pageTheme,
      language.of([]),
      locked.of(lockOf(readOnly)),
      EditorView.updateListener.of((u) => on.current.update(u)),
    ],
  });
}

/** Loads the file's language (once for all files of that language) and gives it to its state: through the view when it
 *  shows that state, else to the kept state by `put`. */
export async function setLanguage(path: string, view: EditorView | null, get: () => EditorState | undefined, put: (s: EditorState) => void) {
  const lang = await loadLanguage(path).catch(() => null);
  if (!lang) return;
  const effects = language.reconfigure(lang);
  const s = get();
  if (!s) return;
  if (view && view.state === s) view.dispatch({ effects });
  else put(s.update({ effects }).state);
}

/** Read-only or not, as a transaction's effects (Edit anyway, or an agent starting on the card). */
export const lockEffect = (readOnly: boolean) => locked.reconfigure(lockOf(readOnly));

/** The whole text replaced by the one on disk (Reload, or an untouched tab reloading by itself), the cursor kept where it
 *  can be. Not something to undo: the file on disk changed, not the text here. */
export function replaceText(s: EditorState, text: string): Transaction {
  const head = Math.min(s.selection.main.head, text.length);
  return s.update({
    changes: { from: 0, to: s.doc.length, insert: text },
    selection: { anchor: head },
    annotations: [Transaction.addToHistory.of(false), Transaction.remote.of(true)],
  });
}

/** Puts the cursor at `line` (from 1) and `column` (from 1, at most the line's end), scrolls it to the middle and marks
 *  the line for a moment. */
export function goToLine(view: EditorView, line: number, column = 1) {
  const doc = view.state.doc;
  const l = doc.line(Math.max(1, Math.min(line, doc.lines)));
  const at = Math.min(l.from + Math.max(0, column - 1), l.to);
  view.dispatch({ selection: { anchor: at }, effects: [EditorView.scrollIntoView(at, { y: "center" }), flash.of(l.number)] });
  view.focus();
  setTimeout(() => { try { view.dispatch({ effects: flash.of(null) }); } catch { /* the view is gone */ } }, 1500);
}
