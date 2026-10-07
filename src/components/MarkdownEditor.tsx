// CodeMirror 6 Markdown editor with a live preview: headings sized, bold/italic styled, syntax marks
// (# ** ` > and link URLs) dimmed except on the line you're editing, task-list boxes clickable,
// task refs (KADE-12) and @mentions shown as chips. A toolbar formats the selection (design system:
// MarkdownEditor). Ctrl+B / Ctrl+I / Ctrl+E / Ctrl+K format; Ctrl+Enter or Ctrl+S saves, Escape cancels.
import { useEffect, useRef, useState } from "react";
import { Bold, Code, Heading, Italic, Link, List, ListChecks, ListOrdered, Minus, Table, TextQuote, type LucideIcon } from "lucide-react";
import { activeFormats, format, type FormatCmd } from "../lib/mdFormat";
import { EditorState, Prec, RangeSetBuilder } from "@codemirror/state";
import {
  Decoration, EditorView, MatchDecorator, ViewPlugin, WidgetType, drawSelection, keymap, placeholder as cmPlaceholder,
  type DecorationSet, type ViewUpdate,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

const highlight = HighlightStyle.define([
  { tag: t.heading1, fontSize: "1.45em", fontWeight: "700" },
  { tag: t.heading2, fontSize: "1.25em", fontWeight: "650" },
  { tag: t.heading3, fontSize: "1.1em", fontWeight: "600" },
  { tag: [t.heading4, t.heading5, t.heading6], fontWeight: "600" },
  { tag: t.strong, fontWeight: "700" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.link, color: "var(--accent)" },
  { tag: t.monospace, fontFamily: "var(--mono)", fontSize: "0.92em" },
  { tag: t.quote, color: "var(--text-2)" },
]);

const MARKS = new Set(["HeaderMark", "EmphasisMark", "CodeMark", "LinkMark", "QuoteMark", "StrikethroughMark"]);
const dim = Decoration.mark({ class: "cm-md-mark" });

class CheckboxWidget extends WidgetType {
  constructor(readonly checked: boolean) { super(); }
  eq(o: CheckboxWidget) { return o.checked === this.checked; }
  toDOM(view: EditorView) {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = this.checked;
    box.className = "cm-task-box";
    box.setAttribute("aria-label", this.checked ? "Done; click to reopen" : "Click to tick off");
    box.addEventListener("mousedown", (e) => {
      e.preventDefault();
      const pos = view.posAtDOM(box);
      view.dispatch({ changes: { from: pos, to: pos + 3, insert: this.checked ? "[ ]" : "[x]" } });
    });
    return box;
  }
}

class BulletWidget extends WidgetType {
  eq() { return true; }
  toDOM() { const s = document.createElement("span"); s.className = "cm-bullet"; s.textContent = "•"; return s; }
}
const bullet = Decoration.replace({ widget: new BulletWidget() });
const hide = Decoration.replace({});

function buildPreview(view: EditorView): DecorationSet {
  const b = new RangeSetBuilder<Decoration>();
  const { state } = view;
  const active = new Set<number>();
  if (view.hasFocus) {
    for (const r of state.selection.ranges) {
      for (let l = state.doc.lineAt(r.from).number; l <= state.doc.lineAt(r.to).number; l++) active.add(l);
    }
  }
  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from, to,
      enter: (node) => {
        if (active.has(state.doc.lineAt(node.from).number)) return;
        if (node.name === "ListMark" && /^[-*+]$/.test(state.sliceDoc(node.from, node.to))) {
          // "- [ ] item" shows only the checkbox; "- item" shows a bullet
          b.add(node.from, node.to, node.node.nextSibling?.name === "Task" ? hide : bullet);
        } else if (node.name === "TaskMarker") {
          b.add(node.from, node.to, Decoration.replace({ widget: new CheckboxWidget(/x/i.test(state.sliceDoc(node.from, node.to))) }));
        } else if (MARKS.has(node.name) || (node.name === "URL" && node.node.parent?.name === "Link")) {
          b.add(node.from, node.to, dim);
        }
      },
    });
  }
  return b.finish();
}

const livePreview = ViewPlugin.fromClass(class {
  decorations: DecorationSet;
  constructor(view: EditorView) { this.decorations = buildPreview(view); }
  update(u: ViewUpdate) {
    if (u.docChanged || u.viewportChanged || u.selectionSet || u.focusChanged) this.decorations = buildPreview(u.view);
  }
}, { decorations: (v) => v.decorations });

const refMark = Decoration.mark({ class: "cm-chip cm-chip-ref" });
const mentionMark = Decoration.mark({ class: "cm-chip cm-chip-mention" });
const chipMatcher = new MatchDecorator({
  regexp: /\b[A-Z][A-Z0-9]{1,5}-\d+\b|(?<=^|\s)@[a-zA-Z0-9_-]{2,32}/g,
  decoration: (m) => (m[0].startsWith("@") ? mentionMark : refMark),
});
const chips = ViewPlugin.fromClass(class {
  decorations: DecorationSet;
  constructor(view: EditorView) { this.decorations = chipMatcher.createDeco(view); }
  update(u: ViewUpdate) { this.decorations = chipMatcher.updateDeco(u, this.decorations); }
}, { decorations: (v) => v.decorations });

const theme = EditorView.theme({
  "&": { fontSize: "var(--fs-md)", color: "var(--text)", backgroundColor: "transparent", minHeight: "inherit" },
  ".cm-scroller": { minHeight: "inherit" },
  "&.cm-focused": { outline: "none" },
  ".cm-content": { fontFamily: "var(--font-sans)", lineHeight: "1.6", padding: "0", caretColor: "var(--text)" },
  ".cm-line": { padding: "0 2px" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--text)" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection": { backgroundColor: "var(--accent-soft)" },
  ".cm-placeholder": { color: "var(--text-3)" },
  ".cm-md-mark": { color: "var(--text-3)", opacity: "0.6" },
  ".cm-bullet": { color: "var(--text-3)", padding: "0 2px" },
  ".cm-task-box": { margin: "0 4px 0 0", verticalAlign: "-1px", cursor: "pointer", accentColor: "var(--accent)" },
  ".cm-chip": { borderRadius: "4px", padding: "0 3px", fontSize: "0.92em" },
  ".cm-chip-ref": { fontFamily: "var(--font-mono)", background: "var(--hover)", color: "var(--accent)" },
  ".cm-chip-mention": { background: "var(--accent-soft)", color: "var(--accent)" },
});

type Tool = { cmd: FormatCmd; icon: LucideIcon; label: string; keys?: string };
const TOOLS: (Tool | "sep")[] = [
  { cmd: "heading", icon: Heading, label: "Heading" }, { cmd: "bold", icon: Bold, label: "Bold", keys: "Ctrl+B" },
  { cmd: "italic", icon: Italic, label: "Italic", keys: "Ctrl+I" }, { cmd: "quote", icon: TextQuote, label: "Quote" }, "sep",
  { cmd: "code", icon: Code, label: "Code", keys: "Ctrl+E" }, { cmd: "link", icon: Link, label: "Link", keys: "Ctrl+K" }, "sep",
  { cmd: "ordered", icon: ListOrdered, label: "Numbered list" }, { cmd: "bullet", icon: List, label: "Bullet list" },
  { cmd: "check", icon: ListChecks, label: "Checklist" }, "sep",
  { cmd: "rule", icon: Minus, label: "Divider" }, { cmd: "table", icon: Table, label: "Table" },
];

function run(view: EditorView, cmd: FormatCmd) {
  const sel = view.state.selection.main;
  const edit = format(view.state.doc.toString(), sel.from, sel.to, cmd);
  view.dispatch({ changes: edit.changes, selection: { anchor: edit.anchor, head: edit.head }, scrollIntoView: true, userEvent: "input.format" });
  view.focus();
  return true;
}

export function MarkdownEditor({ value, onChange, onSave, onBlur, onCancel, placeholder, autoFocus, ariaLabel, minHeight, hint, toolbar = true }: {
  value: string;
  onChange?: (md: string) => void;
  onSave?: (md: string) => void;
  onBlur?: (md: string) => void;
  onCancel?: () => void;
  placeholder?: string;
  autoFocus?: boolean;
  ariaLabel?: string;
  minHeight?: number;
  /** Shown at the right of the toolbar, e.g. "Ctrl+Enter saves". */
  hint?: string;
  toolbar?: boolean;
}) {
  const [active, setActive] = useState<Set<FormatCmd>>(new Set());
  const activeKey = useRef("");
  const host = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const cb = useRef({ onChange, onSave, onBlur, onCancel });
  cb.current = { onChange, onSave, onBlur, onCancel };

  useEffect(() => {
    const view = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          Prec.high(keymap.of([
            { key: "Mod-Enter", run: (v) => { cb.current.onSave?.(v.state.doc.toString()); return true; } },
            { key: "Mod-s", run: (v) => { cb.current.onSave?.(v.state.doc.toString()); return true; } },
            { key: "Escape", run: () => { if (!cb.current.onCancel) return false; cb.current.onCancel(); return true; } },
            { key: "Mod-b", run: (v) => run(v, "bold") },
            { key: "Mod-i", run: (v) => run(v, "italic") },
            { key: "Mod-e", run: (v) => run(v, "code") },
            { key: "Mod-k", run: (v) => run(v, "link") },
          ])),
          history(), drawSelection(), EditorView.lineWrapping,
          markdown({ base: markdownLanguage }), syntaxHighlighting(highlight), livePreview, chips, theme,
          cmPlaceholder(placeholder ?? ""),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.contentAttributes.of({ "aria-label": ariaLabel ?? "Markdown editor", spellcheck: "true" }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) cb.current.onChange?.(u.state.doc.toString());
            if (u.focusChanged && !u.view.hasFocus) cb.current.onBlur?.(u.state.doc.toString());
            if (u.docChanged || u.selectionSet) {
              const now = activeFormats(u.state.doc.toString(), u.state.selection.main.head);
              const key = [...now].sort().join(",");
              if (key !== activeKey.current) { activeKey.current = key; setActive(now); }
            }
          }),
        ],
      }),
    });
    viewRef.current = view;
    if (autoFocus) { view.focus(); view.dispatch({ selection: { anchor: view.state.doc.length } }); }
    return () => { view.destroy(); viewRef.current = null; };
    // The editor owns its text after mount; outside changes arrive through the effect below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const v = viewRef.current;
    if (v && !v.hasFocus && v.state.doc.toString() !== value) v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: value } });
  }, [value]);

  const surface = <div className="md-surface" ref={host} style={{ minHeight }} />;
  if (!toolbar) return <div className="md-editor">{surface}</div>;
  return (
    <div className="md-editor">
      <div className="md-toolbar" role="toolbar" aria-label="Formatting">
        {TOOLS.map((t, i) => t === "sep" ? <span key={i} className="sep" /> : (
          <button key={t.cmd} type="button" title={t.keys ? `${t.label} (${t.keys})` : t.label} aria-label={t.label} aria-pressed={active.has(t.cmd)}
            onMouseDown={(e) => e.preventDefault() /* keep the editor focused: leaving it saves */}
            onClick={() => viewRef.current && run(viewRef.current, t.cmd)}>
            <t.icon className="icon" />
          </button>
        ))}
        {hint && <span className="right">{hint}</span>}
      </div>
      {surface}
    </div>
  );
}
