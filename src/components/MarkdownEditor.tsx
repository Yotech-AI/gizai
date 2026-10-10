// CodeMirror 6 Markdown editor with a live preview: headings sized, bold/italic styled, syntax marks
// (# ** ` > and link URLs) dimmed except on the line you're editing, task-list boxes clickable,
// task refs (KADE-12) and @mentions shown as chips, and links to Gizai items (gizai:) as one chip each. A toolbar formats
// the selection (design system: MarkdownEditor). Ctrl+B / Ctrl+I / Ctrl+E / Ctrl+K format; Ctrl+Enter or Ctrl+S saves
// (Cmd on macOS), Escape cancels. Typing @ opens the item picker at the cursor, which links a task, project, client,
// agent, person or doc.
import { useEffect, useMemo, useRef, useState, type MutableRefObject } from "react";
import { Bold, Code, Heading, Italic, Link, List, ListChecks, ListOrdered, Minus, Table, TextQuote, type LucideIcon } from "lucide-react";
import { activeFormats, format, type FormatCmd } from "../lib/mdFormat";
import { findTrigger, ITEM_LINK_SOURCE, itemLink, KIND_NAME, parseItemUrl, pickRows, unescapeLinkText, type ItemKind, type PickItem, type PickRow } from "../lib/itemLinks";
import { loadPickItems } from "../lib/pickItems";
import { openItem } from "../lib/openItem";
import { modClick, modKey } from "../lib/keys";
import { ItemPicker, type PickerAt } from "./ItemPicker";
import { Compartment, EditorState, Prec, RangeSetBuilder } from "@codemirror/state";
import {
  Decoration, EditorView, MatchDecorator, ViewPlugin, WidgetType, drawSelection, keymap, placeholder as cmPlaceholder,
  type DecorationSet, type ViewUpdate,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, insertNewlineAndIndent } from "@codemirror/commands";
import { insertNewlineContinueMarkup, markdown, markdownLanguage } from "@codemirror/lang-markdown";
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

/** A link to a Gizai item, shown as one chip with its name. The cursor steps over it and Backspace takes it out whole;
 *  Ctrl+click (Cmd+click on macOS) opens the item, so a plain click never leaves what you are writing. */
class ItemChip extends WidgetType {
  constructor(readonly label: string, readonly kind: ItemKind, readonly key: string) { super(); }
  eq(o: ItemChip) { return o.label === this.label && o.kind === this.kind && o.key === this.key; }
  toDOM() {
    const s = document.createElement("span");
    s.className = `cm-item-chip item-chip kind-${this.kind}`;
    s.textContent = this.label;
    s.title = `${KIND_NAME[this.kind]}: ${this.label} (${modKey()}+click opens it)`;
    s.addEventListener("mousedown", (e) => {
      if (!modClick(e)) return;
      e.preventDefault();
      openItem(this.kind, this.key).catch(() => {});
    });
    return s;
  }
  // Ctrl+click (Cmd+click on macOS, where Ctrl+click is a right click) is the chip's own; any other click places the
  // cursor as usual.
  ignoreEvent(e: Event) { return e.type === "mousedown" && modClick(e as MouseEvent); }
}

const refMark = Decoration.mark({ class: "cm-chip cm-chip-ref" });
const mentionMark = Decoration.mark({ class: "cm-chip cm-chip-mention" });
// A gizai: link is matched first and whole, so the task ref inside it (GA-12) gets no chip of its own.
const chipMatcher = new MatchDecorator({
  regexp: new RegExp(`${ITEM_LINK_SOURCE}|\\b[A-Z][A-Z0-9]{1,5}-\\d+\\b|(?<=^|\\s)@[a-zA-Z0-9_-]{2,32}`, "g"),
  decorate: (add, from, to, m) => {
    if (m[2]) {
      const target = parseItemUrl(`gizai:${m[2]}/${m[3]}`);
      if (target) add(from, to, Decoration.replace({ widget: new ItemChip(unescapeLinkText(m[1] ?? ""), target.kind, target.key) }));
      return;
    }
    add(from, to, m[0].startsWith("@") ? mentionMark : refMark);
  },
});
/** Only the item chips of a set of decorations: they are atomic, the other chips aren't. */
function itemChipsOnly(set: DecorationSet): DecorationSet {
  const b = new RangeSetBuilder<Decoration>();
  for (const it = set.iter(); it.value; it.next()) if (it.value.spec.widget instanceof ItemChip) b.add(it.from, it.to, it.value);
  return b.finish();
}
const chips = ViewPlugin.fromClass(class {
  decorations: DecorationSet;
  items: DecorationSet;
  constructor(view: EditorView) { this.decorations = chipMatcher.createDeco(view); this.items = itemChipsOnly(this.decorations); }
  update(u: ViewUpdate) { this.decorations = chipMatcher.updateDeco(u, this.decorations); this.items = itemChipsOnly(this.decorations); }
}, {
  decorations: (v) => v.decorations,
  provide: (p) => EditorView.atomicRanges.of((view) => view.plugin(p)?.items ?? Decoration.none),
});

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

// `key` is the shortcut's letter, shown with the modifier: Ctrl+B, or Cmd+B on macOS.
type Tool = { cmd: FormatCmd; icon: LucideIcon; label: string; key?: string };
const TOOLS: (Tool | "sep")[] = [
  { cmd: "heading", icon: Heading, label: "Heading" }, { cmd: "bold", icon: Bold, label: "Bold", key: "B" },
  { cmd: "italic", icon: Italic, label: "Italic", key: "I" }, { cmd: "quote", icon: TextQuote, label: "Quote" }, "sep",
  { cmd: "code", icon: Code, label: "Code", key: "E" }, { cmd: "link", icon: Link, label: "Link", key: "K" }, "sep",
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

/** The `@…` before the cursor (`findTrigger`), only for a plain cursor in the editor that has the focus. Not `hasFocus`,
 *  which is also false while the window is in the background (and always in a headless test). */
function triggerAt(view: EditorView) {
  const sel = view.state.selection.main;
  if (!sel.empty || view.root.activeElement !== view.contentDOM) return null;
  const line = view.state.doc.lineAt(sel.head);
  const t = findTrigger(line.text.slice(0, sel.head - line.from), line.from);
  return t && { ...t, to: sel.head };
}

/** Replaces the `@…` before the cursor with a picked row: a kind types `@task.` (and so on), an item becomes its link and a
 *  space. False when the cursor is no longer in an `@…`. */
function insertRow(view: EditorView, row: PickRow): boolean {
  const t = triggerAt(view);
  if (!t) return false;
  let text: string;
  if (row.type === "kind") text = `@${row.kind}.`;
  else {
    const link = itemLink(row.item);
    text = /^[\s).,;:!?]/.test(view.state.sliceDoc(t.to, t.to + 1)) ? link : `${link} `;
  }
  view.dispatch({ changes: { from: t.from, to: t.to, insert: text }, selection: { anchor: t.from + text.length }, scrollIntoView: true, userEvent: "input.complete" });
  return true;
}

/** Enter in an editor that sends (the chat composer): Shift+Enter adds a line, continuing a list or quote. */
const newLine = (v: EditorView) => insertNewlineContinueMarkup(v) || insertNewlineAndIndent(v);

/** What a parent can do with the editor (the chat composer): focus it, empty it after sending, and start a link (+ → Link an
 *  item types @ at the cursor, which opens the picker). */
export type EditorHandle = { focus: () => void; clear: () => void; startLink: () => void };

type Pick = { from: number; query: string; at: PickerAt | null };

export function MarkdownEditor({ value, onChange, onSave, onBlur, onCancel, onEnter, placeholder, autoFocus, ariaLabel, minHeight, hint, toolbar = true,
  disabled = false, pickerUp, handle, className }: {
  value: string;
  onChange?: (md: string) => void;
  onSave?: (md: string) => void;
  onBlur?: (md: string) => void;
  onCancel?: () => void;
  /** Enter calls this instead of adding a line (Shift+Enter adds one): the chat composer sends. */
  onEnter?: (md: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
  ariaLabel?: string;
  minHeight?: number;
  /** Shown at the right of the toolbar, e.g. "Ctrl+Enter saves". */
  hint?: string;
  toolbar?: boolean;
  /** Read only and dimmed (the chat while the Team Lead is paused). */
  disabled?: boolean;
  /** The item picker opens upward (an editor at the bottom of the window); else below the cursor when there is room. */
  pickerUp?: boolean;
  handle?: MutableRefObject<EditorHandle | null>;
  className?: string;
}) {
  const [active, setActive] = useState<Set<FormatCmd>>(new Set());
  const activeKey = useRef("");
  const host = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const cb = useRef({ onChange, onSave, onBlur, onCancel, onEnter });
  cb.current = { onChange, onSave, onBlur, onCancel, onEnter };
  const conf = useRef({ editable: new Compartment(), placeholder: new Compartment() });

  // The item picker: open while the cursor is in an `@…` (unless Escape closed that one), with the items asked for when it opens.
  const [pick, setPick] = useState<Pick | null>(null);
  const [chosen, setSelected] = useState(0);
  const [items, setItems] = useState<PickItem[] | null>(null);
  const [itemsError, setItemsError] = useState<string | null>(null);
  const dismissed = useRef<number | null>(null);
  const rows = useMemo(() => (pick ? pickRows(pick.query, items ?? []) : []), [pick?.query, items]);
  // The list can get shorter under the selection (the items arrive): it stays on a row.
  const selected = Math.min(chosen, Math.max(0, rows.length - 1));
  // A search that finds nothing closes it, so Enter and Escape do what they did: `@name` stays a plain mention.
  const open = !!pick && (rows.length > 0 || items === null || !!itemsError);
  const pk = useRef({ open, rows, selected });
  pk.current = { open, rows, selected };
  const wanted = !!pick;
  useEffect(() => {
    if (!wanted) return;
    let alive = true;
    loadPickItems().then((list) => { if (alive) { setItems(list); setItemsError(null); } }).catch((e) => { if (alive) setItemsError(String(e)); });
    return () => { alive = false; };
  }, [wanted]);

  // The query the selection belongs to: a new one selects the first row again.
  const shownQuery = useRef<string | null>(null);
  const follow = (view: EditorView) => {
    const t = triggerAt(view);
    if (!t) dismissed.current = null;
    if (!t || dismissed.current === t.from) { shownQuery.current = null; setPick(null); return; }
    if (shownQuery.current !== t.query) { shownQuery.current = t.query; setSelected(0); }
    setPick((p) => ({ from: t.from, query: t.query, at: p?.from === t.from ? p.at : null }));
    view.requestMeasure({
      key: "item-picker",
      read: (v) => v.coordsAtPos(t.from),
      write: (c) => { if (c) setPick((p) => (p && p.from === t.from ? { ...p, at: { left: c.left, top: c.top, bottom: c.bottom } } : p)); },
    });
  };
  const pickRow = (view: EditorView, i: number) => {
    const row = pk.current.rows[i];
    if (row && insertRow(view, row)) view.focus();
  };

  useEffect(() => {
    const view = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          // While the picker is open it takes these keys first: Enter picks (it neither sends, saves nor adds a line) and
          // Escape closes only the picker.
          Prec.highest(keymap.of([
            { key: "ArrowDown", run: () => move(1) },
            { key: "ArrowUp", run: () => move(-1) },
            { key: "Enter", run: (v) => choose(v), shift: (v) => choose(v) },
            { key: "Mod-Enter", run: (v) => choose(v) },
            { key: "Tab", run: (v) => choose(v) },
            { key: "Escape", run: (v) => close(v) },
          ])),
          Prec.high(keymap.of([
            { key: "Mod-Enter", run: (v) => { cb.current.onSave?.(v.state.doc.toString()); return true; } },
            { key: "Mod-s", run: (v) => { cb.current.onSave?.(v.state.doc.toString()); return true; } },
            { key: "Escape", run: () => { if (!cb.current.onCancel) return false; cb.current.onCancel(); return true; } },
            {
              key: "Enter",
              run: (v) => { if (!cb.current.onEnter) return false; cb.current.onEnter(v.state.doc.toString()); return true; },
              shift: (v) => (cb.current.onEnter ? newLine(v) : false),
            },
            { key: "Mod-b", run: (v) => run(v, "bold") },
            { key: "Mod-i", run: (v) => run(v, "italic") },
            { key: "Mod-e", run: (v) => run(v, "code") },
            { key: "Mod-k", run: (v) => run(v, "link") },
          ])),
          history(), drawSelection(), EditorView.lineWrapping,
          markdown({ base: markdownLanguage }), syntaxHighlighting(highlight), livePreview, chips, theme,
          conf.current.placeholder.of(cmPlaceholder(placeholder ?? "")),
          conf.current.editable.of([EditorView.editable.of(!disabled), EditorState.readOnly.of(disabled)]),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.contentAttributes.of({ "aria-label": ariaLabel ?? "Markdown editor", spellcheck: "true" }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) cb.current.onChange?.(u.state.doc.toString());
            if (u.focusChanged && !u.view.hasFocus) cb.current.onBlur?.(u.state.doc.toString());
            if (u.docChanged || u.selectionSet || u.focusChanged) follow(u.view);
            if (u.docChanged || u.selectionSet) {
              const now = activeFormats(u.state.doc.toString(), u.state.selection.main.head);
              const key = [...now].sort().join(",");
              if (key !== activeKey.current) { activeKey.current = key; setActive(now); }
            }
          }),
        ],
      }),
    });
    function move(by: number) {
      const { open, rows, selected } = pk.current;
      if (!open) return false;
      if (rows.length) setSelected((selected + by + rows.length) % rows.length);
      return true;
    }
    function choose(v: EditorView) {
      if (!pk.current.open) return false;
      if (pk.current.rows.length) pickRow(v, pk.current.selected);
      return true;
    }
    function close(v: EditorView) {
      if (!pk.current.open) return false;
      dismissed.current = triggerAt(v)?.from ?? null;
      shownQuery.current = null;
      setPick(null);
      return true;
    }
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

  useEffect(() => {
    viewRef.current?.dispatch({ effects: conf.current.placeholder.reconfigure(cmPlaceholder(placeholder ?? "")) });
  }, [placeholder]);
  useEffect(() => {
    viewRef.current?.dispatch({ effects: conf.current.editable.reconfigure([EditorView.editable.of(!disabled), EditorState.readOnly.of(disabled)]) });
    if (disabled) setPick(null);
  }, [disabled]);

  useEffect(() => {
    if (!handle) return;
    handle.current = {
      focus: () => viewRef.current?.focus(),
      clear: () => {
        const v = viewRef.current;
        if (v && v.state.doc.length) v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: "" } });
      },
      startLink: () => {
        const v = viewRef.current;
        if (!v || disabled) return;
        v.focus();
        const s = v.state.selection.main;
        // An @ right after a word wouldn't open the picker (that is how an email address looks): put a space first.
        const before = v.state.sliceDoc(Math.max(0, s.from - 1), s.from);
        const text = before === "" || /[\s([]/.test(before) ? "@" : " @";
        v.dispatch({ changes: { from: s.from, to: s.to, insert: text }, selection: { anchor: s.from + text.length }, scrollIntoView: true, userEvent: "input.type" });
      },
    };
    return () => { handle.current = null; };
  }, [handle, disabled]);

  const picker = open && pick?.at ? (
    <ItemPicker at={pick.at} up={pickerUp} query={pick.query} rows={rows} loading={items === null} error={itemsError} selected={selected}
      onSelect={setSelected} onPick={(i) => { if (viewRef.current) pickRow(viewRef.current, i); }} />
  ) : null;
  const surface = <div className="md-surface" ref={host} style={{ minHeight }} />;
  const cls = `md-editor${className ? ` ${className}` : ""}${disabled ? " disabled" : ""}`;
  if (!toolbar) return <div className={cls}>{surface}{picker}</div>;
  return (
    <div className={cls}>
      {picker}
      <div className="md-toolbar" role="toolbar" aria-label="Formatting">
        {TOOLS.map((t, i) => t === "sep" ? <span key={i} className="sep" /> : (
          <button key={t.cmd} type="button" title={t.key ? `${t.label} (${modKey()}+${t.key})` : t.label} aria-label={t.label} aria-pressed={active.has(t.cmd)}
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
