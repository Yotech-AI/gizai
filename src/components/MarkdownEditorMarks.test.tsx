// GA-118: MarkdownEditor's `marks` prop. "dim" (the default, every editor but the doc page's) is the live preview: marks
// dimmed except on the lines you're editing. "hide" (the doc page's Editor) formats the text: headings, bold, italic,
// inline code, quotes (a bar on every line, also an empty `>` line), links (only their text) and dividers (a line), the
// marks showing again only on the lines with the cursor or the selection while the editor has the focus. "show" (the doc
// page's Markdown) shows every character as stored, in the mono font: nothing hidden, dimmed or replaced, headings not
// enlarged. Changing the prop reconfigures the open editor (its marks compartment): the text, the cursor and undo stay.
// There is no DOM here: a small hook runtime runs the component, and a stand-in EditorView (a real EditorState, no DOM)
// takes its extensions. The decorations come from the real view plugins of the marks compartment, built on that view.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Compartment, EditorSelection, EditorState, type Extension, type TransactionSpec } from "@codemirror/state";
import { EditorView, ViewPlugin, type DecorationSet } from "@codemirror/view";
import { defaultHighlightStyle, HighlightStyle, highlightingFor, syntaxHighlighting } from "@codemirror/language";
import { tags, type Tag } from "@lezer/highlight";
import { undo } from "@codemirror/commands";

type Inst = { slots: unknown[]; i: number };
type EffectSlot = { effect: true; deps?: unknown[]; first: boolean; cleanup?: () => void };
type View = {
  state: EditorState; hasFocus: boolean; visibleRanges: { from: number; to: number }[];
  dispatched: TransactionSpec[]; dispatch: (spec: TransactionSpec) => void;
};
const h = vi.hoisted(() => ({
  cur: null as Inst | null,
  dirty: false,
  effects: [] as (() => void)[],
  views: [] as View[],
  styles: [] as HighlightStyle[],
}));

vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const slot = <T,>(make: () => T): T => {
    const c = h.cur!;
    if (c.i === c.slots.length) c.slots.push(make());
    return c.slots[c.i++] as T;
  };
  const changed = (deps?: unknown[], prev?: unknown[]) => !deps || !prev || deps.length !== prev.length || deps.some((d, i) => !Object.is(d, prev[i]));
  const useState = ((init: unknown) => {
    if (!h.cur) return R.useState(init);
    const box = slot(() => {
      const b = { v: typeof init === "function" ? (init as () => unknown)() : init, set: (x: unknown) => {
        const v = typeof x === "function" ? (x as (p: unknown) => unknown)(b.v) : x;
        if (!Object.is(v, b.v)) { b.v = v; h.dirty = true; }
      } };
      return b;
    });
    return [box.v, box.set];
  }) as typeof R.useState;
  const useRef = ((init: unknown) => (h.cur ? slot(() => ({ current: init })) : R.useRef(init))) as typeof R.useRef;
  const useMemo = ((f: () => unknown, deps: unknown[]) => {
    if (!h.cur) return R.useMemo(f, deps);
    const s = slot(() => ({ deps: undefined as unknown[] | undefined, v: undefined as unknown }));
    if (changed(deps, s.deps)) { s.v = f(); s.deps = deps; }
    return s.v;
  }) as typeof R.useMemo;
  const useEffect = ((f: () => void | (() => void), deps?: unknown[]) => {
    if (!h.cur) return R.useEffect(f, deps);
    const s = slot<EffectSlot>(() => ({ effect: true, first: true }));
    if (s.first || changed(deps, s.deps)) {
      s.first = false;
      s.deps = deps;
      h.effects.push(() => { s.cleanup?.(); const c = f(); s.cleanup = typeof c === "function" ? c : undefined; });
    }
  }) as typeof R.useEffect;
  const hooks = { useState, useRef, useMemo, useEffect, useLayoutEffect: useEffect };
  return { ...R, default: { ...R, ...hooks }, ...hooks };
});

// The EditorView stand-in: the component's state, a focus flag, the whole text visible, and dispatch applied to the state.
vi.mock("@codemirror/view", async (orig) => {
  const real = await orig<typeof import("@codemirror/view")>();
  class FakeView implements View {
    state: EditorState;
    hasFocus = false;
    dispatched: TransactionSpec[] = [];
    constructor(config: { state: EditorState }) { this.state = config.state; h.views.push(this); }
    get visibleRanges() { return [{ from: 0, to: this.state.doc.length }]; }
    get viewport() { return { from: 0, to: this.state.doc.length }; }
    dispatch(spec: TransactionSpec) { this.dispatched.push(spec); this.state = this.state.update(spec).state; }
    focus() { this.hasFocus = true; }
    destroy() {}
    requestMeasure() {}
  }
  Object.setPrototypeOf(FakeView, real.EditorView); // its statics: theme, editorAttributes, updateListener, …
  return { ...real, EditorView: FakeView };
});
// The highlight styles the editor defines, to read what a style does to a heading.
vi.mock("@codemirror/language", async (orig) => {
  const real = await orig<typeof import("@codemirror/language")>();
  const define = real.HighlightStyle.define.bind(real.HighlightStyle);
  real.HighlightStyle.define = ((specs, options) => { const s = define(specs, options); h.styles.push(s); return s; }) as typeof real.HighlightStyle.define;
  return real;
});
vi.stubGlobal("navigator", { userAgent: "Linux" });

const { MarkdownEditor } = await import("./MarkdownEditor");
type Marks = "dim" | "hide" | "show";

// ---- the hook runtime: one MarkdownEditor ----
let inst: Inst = { slots: [], i: 0 };
function render(props: Parameters<typeof MarkdownEditor>[0]) {
  for (let n = 0; n < 30; n++) {
    h.dirty = false;
    inst.i = 0;
    h.cur = inst;
    try { MarkdownEditor(props); } finally { h.cur = null; }
    for (const f of h.effects.splice(0)) f();
    if (!h.dirty) return;
  }
  throw new Error("the render never settled");
}

beforeEach(() => { inst = { slots: [], i: 0 }; h.views.length = 0; h.effects.length = 0; });

// ---- reading the editor ----
/** Every extension object inside an extension (arrays, and a compartment's content). */
function flat(ext: Extension, out: unknown[] = []): unknown[] {
  if (Array.isArray(ext)) ext.forEach((e) => flat(e, out));
  else { out.push(ext); const inner = (ext as { inner?: Extension }).inner; if (inner) flat(inner, out); }
  return out;
}
/** The compartments a dispatch reconfigures. */
const reconfigured = (spec: TransactionSpec) => [spec.effects ?? []].flat()
  .map((e) => e.value as { compartment?: Compartment })
  .filter((v) => v?.compartment instanceof Compartment).map((v) => v.compartment!);
/** The marks compartment of a mounted editor and what is in it now: the compartment the last switch of `marks`
 *  reconfigured (the dispatches of the mount, the placeholder's and the editable flag's, are cleared before). */
function marksCompartment(view: View): { compartment: Compartment; content: Extension } {
  const last = view.dispatched.flatMap(reconfigured).at(-1);
  if (!last) throw new Error("no switch of marks seen");
  return { compartment: last, content: last.get(view.state)! };
}
// The syntax highlighter's own view plugin (the same one in every syntaxHighlighting): not one of the editor's.
const TREE_HIGHLIGHTER = new Set(flat(syntaxHighlighting(defaultHighlightStyle)).filter((x) => x instanceof ViewPlugin));
const pluginsIn = (ext: Extension) => flat(ext).filter((x): x is ViewPlugin<{ decorations: DecorationSet }> => x instanceof ViewPlugin && !TREE_HIGHLIGHTER.has(x));

type Deco = { from: number; to: number; spec: Record<string, unknown> };
/** The decorations the marks compartment's plugins give for the view's state. */
function decos(view: View, content: Extension): Deco[] {
  const out: Deco[] = [];
  for (const p of pluginsIn(content)) {
    const set = (p as unknown as { create: (v: View, a: unknown) => { decorations?: DecorationSet } }).create(view, undefined).decorations;
    if (!set) continue;
    for (const it = set.iter(); it.value; it.next()) out.push({ from: it.from, to: it.to, spec: it.value.spec as Record<string, unknown> });
  }
  return out;
}
const isLine = (d: Deco) => d.from === d.to && typeof d.spec.class === "string" && !d.spec.widget;
const isMark = (d: Deco) => d.from < d.to && typeof d.spec.class === "string";
const isReplace = (d: Deco) => !isLine(d) && !isMark(d);
/** A replacing widget as text: [•] a bullet, [ ] / [x] a task box, [hr] a divider, [chip: name] an item chip. */
function widgetText(w: unknown): string {
  const name = (w as object).constructor.name;
  if (name === "BulletWidget") return "[•]";
  if (name === "CheckboxWidget") return (w as { checked: boolean }).checked ? "[x]" : "[ ]";
  if (name === "RuleWidget") return "[hr]";
  if (name === "ItemChip") return `[chip: ${(w as { label: string }).label}]`;
  return `[${name}]`;
}
/** The text as it shows, line by line: hidden ranges left out, replacing widgets as `widgetText`. */
function shown(view: View, content: Extension): string[] {
  const doc = view.state.doc.toString();
  const reps = decos(view, content).filter(isReplace).sort((a, b) => a.from - b.from || b.to - a.to);
  let out = "", at = 0;
  for (const r of reps) {
    if (r.from < at) continue; // inside one already left out
    out += doc.slice(at, r.from) + (r.spec.widget ? widgetText(r.spec.widget) : "");
    at = r.to;
  }
  return (out + doc.slice(at)).split("\n");
}
const lineNo = (view: View, pos: number) => view.state.doc.lineAt(pos).number;
/** The lines with a line decoration of class `cls`. */
const linesWith = (view: View, content: Extension, cls: string) =>
  [...new Set(decos(view, content).filter((d) => isLine(d) && d.spec.class === cls).map((d) => lineNo(view, d.from)))].sort((a, b) => a - b);
/** The text under each mark decoration of class `cls`. */
const marked = (view: View, content: Extension, cls: string) =>
  decos(view, content).filter((d) => isMark(d) && String(d.spec.class).split(" ").includes(cls)).map((d) => view.state.sliceDoc(d.from, d.to));

/** The style a highlighter in the state gives a tag: the spec of the editor's highlight style that the state uses. */
function styleOf(state: EditorState, tag: Tag): Record<string, unknown> | null {
  const cls = highlightingFor(state, [tag]);
  if (!cls) return null;
  const style = h.styles.find((s) => s.style([tag]) === cls);
  return (style?.specs.find((s) => [s.tag].flat().includes(tag)) as Record<string, unknown> | undefined) ?? null;
}

/** Mounts an editor showing `text` with `marks`, and returns its view and marks compartment content (each Marks's,
 *  reached by switching to the others and back, which is also how the open editor changes). */
function mount(text: string, marks?: Marks) {
  const props = { value: text, marks };
  const view = first(props);
  const initialState = view.state;
  const content = {} as Record<Marks, Extension>;
  const initial = marks ?? "dim";
  for (const m of (["dim", "hide", "show"] as Marks[]).filter((x) => x !== initial).concat(initial)) {
    render({ ...props, marks: m });
    content[m] = marksCompartment(view).content;
  }
  // The editor was made with the content of its `marks` (or "dim").
  expect(marksCompartment(view).compartment.get(initialState)).toBe(content[initial]);
  view.dispatched.length = 0;
  return { view, content, initialState };
}
/** The first render: the editor is made; what its mount dispatched is cleared. */
function first(props: Parameters<typeof MarkdownEditor>[0]): View {
  render(props);
  expect(h.views).toHaveLength(1);
  h.views[0].dispatched.length = 0;
  return h.views[0];
}
/** Puts the cursor (or a selection) on these positions and gives the editor the focus. */
const focusAt = (view: View, anchor: number, head = anchor) => { view.hasFocus = true; view.state = view.state.update({ selection: EditorSelection.single(anchor, head) }).state; };

// The card's test doc: a heading, bold, italic, code, two quote lines with an empty `>` line between them, a link and a divider.
const DOC = [
  "# Title", // 1
  "", // 2
  "**bold** *italic* `code`", // 3
  "", // 4
  "> first", // 5
  ">", // 6
  "> second", // 7
  "", // 8
  "[link](https://example.com)", // 9
  "", // 10
  "---", // 11
].join("\n");
const FORMATTED = ["Title", "", "bold italic code", "", "first", "", "second", "", "link", "", "[hr]"];
const lineStart = (n: number) => DOC.split("\n").slice(0, n - 1).join("\n").length + (n > 1 ? 1 : 0);

describe("MarkdownEditor marks: hide (the doc page's Editor)", () => {
  it("hides every mark of the test doc while the editor doesn't have the focus", () => {
    const { view, content } = mount(DOC, "hide");
    expect(view.hasFocus).toBe(false);
    expect(shown(view, content.hide)).toEqual(FORMATTED);
    // Nothing is left dimmed: the marks are gone, not greyed.
    expect(marked(view, content.hide, "cm-md-mark")).toEqual([]);
  });

  it("gives every line of the quote its bar, the empty `>` line too, and no other line", () => {
    const { view, content } = mount(DOC, "hide");
    expect(linesWith(view, content.hide, "cm-md-quote")).toEqual([5, 6, 7]);
  });

  it("shows the link's text in the link colour, and inline code on its own background", () => {
    const { view, content } = mount(DOC, "hide");
    expect(marked(view, content.hide, "cm-md-link")).toEqual(["link"]);
    expect(marked(view, content.hide, "cm-md-code")).toEqual(["code"]);
  });

  it("keeps the heading's size and the bold and italic styles of the live preview", () => {
    const { view, content } = mount(DOC, "hide");
    const hide = view.state;
    render({ value: DOC, marks: "dim" });
    expect(styleOf(view.state, tags.heading1)).toMatchObject({ fontSize: "1.45em" });
    expect(styleOf(hide, tags.heading1)).toEqual(styleOf(view.state, tags.heading1));
    expect(styleOf(hide, tags.strong)).toMatchObject({ fontWeight: "700" });
    expect(styleOf(hide, tags.emphasis)).toMatchObject({ fontStyle: "italic" });
    expect(content.hide).not.toBe(content.dim);
  });

  it("shows only the line with the cursor with its marks, and no other line's, once the editor has the focus", () => {
    const { view, content } = mount(DOC, "hide");
    const lines = DOC.split("\n");
    for (const n of [1, 3, 5, 6, 7, 9, 11]) {
      focusAt(view, lineStart(n) + Math.min(1, lines[n - 1].length));
      const expected = FORMATTED.map((l, i) => (i === n - 1 ? lines[i] : l));
      expect(shown(view, content.hide), `cursor on line ${n}`).toEqual(expected);
    }
  });

  it("shows the marks of the cursor's line as written, not dimmed, so they can be edited", () => {
    const { view, content } = mount(DOC, "hide");
    focusAt(view, lineStart(3) + 3);
    const onLine3 = decos(view, content.hide).filter((d) => lineNo(view, d.from) === 3);
    // Only the code's background stays on the line: no hidden, replaced or dimmed mark.
    expect(onLine3.map((d) => d.spec.class)).toEqual(["cm-md-code"]);
    // Typing in a mark works on the text itself: the line shows what was typed.
    view.dispatch({ changes: { from: lineStart(3), insert: "*" } });
    expect(shown(view, content.hide)[2]).toBe("***bold** *italic* `code`");
  });

  it("keeps the bar on the quote line with the cursor, whose `>` shows", () => {
    const { view, content } = mount(DOC, "hide");
    focusAt(view, lineStart(7) + 3);
    expect(shown(view, content.hide).slice(4, 7)).toEqual(["first", "", "> second"]);
    expect(linesWith(view, content.hide, "cm-md-quote")).toEqual([5, 6, 7]);
  });

  it("shows every line of a selection with its marks", () => {
    const { view, content } = mount(DOC, "hide");
    focusAt(view, lineStart(5) + 2, lineStart(7) + 4);
    expect(shown(view, content.hide)).toEqual(["Title", "", "bold italic code", "", "> first", ">", "> second", "", "link", "", "[hr]"]);
  });

  it("hides the marks again when the editor loses the focus, the cursor where it was", () => {
    const { view, content } = mount(DOC, "hide");
    focusAt(view, lineStart(1) + 2);
    expect(shown(view, content.hide)[0]).toBe("# Title");
    view.hasFocus = false;
    expect(shown(view, content.hide)).toEqual(FORMATTED);
  });

  it("hides ~~ and _, the closing #s of a heading and a setext heading's underline, and draws *** and ___ as lines", () => {
    const text = "## Two ##\n\n~~gone~~ and _it_\n\nSetext\n======\n\n***\n\n___";
    const { view, content } = mount(text, "hide");
    expect(shown(view, content.hide)).toEqual(["Two", "", "gone and it", "", "Setext", "", "", "[hr]", "", "[hr]"]);
  });

  it("keeps a nested quote's bar and hides both of its marks", () => {
    const { view, content } = mount("> > deep\n> top", "hide");
    expect(shown(view, content.hide)).toEqual(["deep", "top"]);
    expect(linesWith(view, content.hide, "cm-md-quote")).toEqual([1, 2]);
  });

  it("keeps links to Gizai items as chips, and bullets and task boxes as in the live preview", () => {
    const text = "- item\n- [ ] todo\n- [x] done\n\nSee [GA-12 - Fix the login](gizai:task/GA-12) now";
    const { view, content } = mount(text, "hide");
    expect(shown(view, content.hide)).toEqual(["[•] item", " [ ] todo", " [x] done", "", "See [chip: GA-12 - Fix the login] now"]);
    render({ value: text, marks: "dim" });
    expect(shown(view, content.dim).slice(0, 3)).toEqual(["[•] item", " [ ] todo", " [x] done"]);
  });

  it("leaves a code block's fences visible and dimmed, and a table as text", () => {
    const text = "```\ncode **here**\n```\n\n| a | b |\n| - | - |\n| 1 | 2 |";
    const { view, content } = mount(text, "hide");
    expect(shown(view, content.hide)).toEqual(text.split("\n"));
    expect(marked(view, content.hide, "cm-md-mark")).toEqual(["```", "```"]);
  });

  it("hides only `[` and `](url)` of a link inside other text, and leaves `[text]` with no URL as written", () => {
    const { view, content } = mount("Read [the spec](https://x.test/a) first, and [this] too", "hide");
    expect(shown(view, content.hide)).toEqual(["Read the spec first, and [this] too"]);
    expect(marked(view, content.hide, "cm-md-link")).toEqual(["the spec"]);
  });
});

describe("MarkdownEditor marks: show (the doc page's Markdown)", () => {
  const TEXT = `${DOC}\n\n- [ ] todo\n- item\n\nSee [GA-12 - Fix the login](gizai:task/GA-12), KADE-3 and @jefsev`;

  it("shows every character as stored: nothing hidden, replaced or dimmed, no bar on the quote", () => {
    const { view, content } = mount(TEXT, "show");
    expect(shown(view, content.show)).toEqual(TEXT.split("\n"));
    expect(decos(view, content.show).filter(isReplace)).toEqual([]);
    expect(decos(view, content.show).filter(isLine)).toEqual([]);
    expect(marked(view, content.show, "cm-md-mark")).toEqual([]);
  });

  it("shows every character also on the line with the cursor and with the editor focused", () => {
    const { view, content } = mount(TEXT, "show");
    focusAt(view, lineStart(3));
    expect(shown(view, content.show)).toEqual(TEXT.split("\n"));
  });

  it("keeps a Gizai link as its Markdown, and gives task refs and @mentions only their colour", () => {
    const { view, content } = mount(TEXT, "show");
    expect(shown(view, content.show).at(-1)).toBe("See [GA-12 - Fix the login](gizai:task/GA-12), KADE-3 and @jefsev");
    expect(marked(view, content.show, "cm-chip-ref")).toEqual(["KADE-3"]);
    expect(marked(view, content.show, "cm-chip-mention")).toEqual(["@jefsev"]);
  });

  it("uses the mono font (.cm-md-source) and doesn't enlarge headings", () => {
    const { view } = mount(TEXT, "show");
    expect(view.state.facet(EditorView.editorAttributes)).toContainEqual({ class: "cm-md-source" });
    for (const tag of [tags.heading1, tags.heading2, tags.heading3]) expect(styleOf(view.state, tag)?.fontSize).toBeUndefined();
  });

  it("is the only mode with the mono font class", () => {
    const { view } = mount(TEXT, "hide");
    expect(view.state.facet(EditorView.editorAttributes)).not.toContainEqual({ class: "cm-md-source" });
    render({ value: TEXT, marks: "dim" });
    expect(view.state.facet(EditorView.editorAttributes)).not.toContainEqual({ class: "cm-md-source" });
  });
});

describe("MarkdownEditor marks: dim (every other editor)", () => {
  it("is the default, and dims the marks instead of hiding them, as before", () => {
    const { view, content, initialState } = mount(DOC);
    expect(shown(view, content.dim)).toEqual(DOC.split("\n"));
    expect(marked(view, content.dim, "cm-md-mark")).toEqual(["#", "**", "**", "*", "*", "`", "`", ">", ">", ">", "[", "]", "(", "https://example.com", ")"]);
    expect(linesWith(view, content.dim, "cm-md-quote")).toEqual([]);
    expect(marked(view, content.dim, "cm-md-link")).toEqual([]);
    expect(marked(view, content.dim, "cm-md-code")).toEqual([]);
    expect(styleOf(initialState, tags.heading1)).toMatchObject({ fontSize: "1.45em" });
    expect(initialState.facet(EditorView.editorAttributes)).not.toContainEqual({ class: "cm-md-source" });
  });

  it("shows the marks of the cursor's line undimmed, as before", () => {
    const { view, content } = mount(DOC);
    focusAt(view, lineStart(3) + 3);
    expect(decos(view, content.dim).filter((d) => lineNo(view, d.from) === 3)).toEqual([]);
    expect(marked(view, content.dim, "cm-md-mark")).toEqual(["#", ">", ">", ">", "[", "]", "(", "https://example.com", ")"]);
  });

  it("is what every editor but the doc page's uses: no other MarkdownEditor passes `marks`", () => {
    // Every component of the app, as text (Vite's ?raw).
    const sources = import.meta.glob<string>(["../**/*.tsx", "!../**/*.test.tsx"], { query: "?raw", import: "default", eager: true });
    const passes = (src: string) => src.split("<MarkdownEditor").slice(1).some((tag) => /\bmarks=/.test(tag.slice(0, tag.indexOf("/>"))));
    expect(Object.keys(sources).length).toBeGreaterThan(20);
    expect(Object.keys(sources).filter((f) => passes(sources[f]))).toEqual(["../pages/DocPage.tsx"]);
  });
});

describe("MarkdownEditor marks: switching", () => {
  it("reconfigures the open editor, keeping its text, cursor and undo, without a new view or a text change", () => {
    const props = { value: "# Title\n\nSome text", marks: "hide" as Marks };
    const view = first(props);
    view.hasFocus = true;
    view.dispatch({ changes: { from: view.state.doc.length, insert: " more" }, selection: { anchor: 3 }, userEvent: "input.type" });
    const before = view.state;
    view.dispatched.length = 0;

    render({ ...props, marks: "show" });
    expect(h.views).toHaveLength(1);
    expect(view.dispatched).toHaveLength(1);
    const spec = view.dispatched[0];
    expect(spec.changes).toBeUndefined();
    expect(spec.selection).toBeUndefined();
    expect(view.state.doc.toString()).toBe("# Title\n\nSome text more");
    expect(view.state.selection.main.head).toBe(3);
    expect(view.state.selection.eq(before.selection)).toBe(true);
    // Ctrl+Z still undoes the last edit.
    const target = { state: view.state, dispatch: (tr: { state: EditorState }) => { target.state = tr.state; } };
    expect(undo(target as never)).toBe(true);
    expect(target.state.doc.toString()).toBe("# Title\n\nSome text");
  });

  it("does nothing when `marks` doesn't change, and switches back the same way", () => {
    const props = { value: "text", marks: "hide" as Marks };
    const view = first(props);
    render({ ...props });
    render({ ...props, placeholder: undefined });
    expect(view.dispatched).toEqual([]);
    render({ ...props, marks: "show" });
    const toShow = marksCompartment(view).content;
    render({ ...props, marks: "hide" });
    const back = marksCompartment(view);
    expect(back.content).not.toBe(toShow);
    expect(h.views).toHaveLength(1);
  });
});
