// How code looks (Code Editor F0, GA-109), for a card's changes and the Editor: Lezer's highlighting tags on D1's syn-*
// tokens (the table in design/code-editor/README.md), and a CodeMirror theme in the mono font with D2's active line,
// brackets and search matches. Only CSS variables, so code follows the t key and Settings → Appearance (theme, font and
// interface size) at once, in dark and light. No italic or bold: the bundled fonts have no italic, and colour does it.
import { HighlightStyle, syntaxHighlighting, type Language } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { highlightTree, tagHighlighter, tags as t, type Tag } from "@lezer/highlight";

export type SynToken =
  | "keyword" | "meta" | "function" | "tag" | "type" | "string" | "number" | "comment" | "attribute" | "property"
  | "variable" | "operator" | "punctuation" | "invalid";

/** Which tags get which `--syn-*` token. A tag covers its kinds (keyword covers controlKeyword), and the most specific
 *  tag wins: function(variableName) over variableName, null over keyword, tagName over typeName. */
export const SYNTAX: { token: SynToken; tags: Tag[] }[] = [
  { token: "keyword", tags: [t.keyword, t.self, t.atom, t.modifier] },
  { token: "meta", tags: [t.meta, t.escape, t.special(t.brace)] },
  { token: "function", tags: [t.function(t.variableName), t.function(t.propertyName), t.macroName] },
  { token: "tag", tags: [t.tagName] },
  { token: "type", tags: [t.typeName, t.className, t.namespace, t.special(t.variableName)] },
  { token: "string", tags: [t.string, t.regexp, t.url, t.special(t.string)] },
  { token: "number", tags: [t.number, t.bool, t.null] },
  { token: "comment", tags: [t.comment] },
  { token: "attribute", tags: [t.attributeName] },
  { token: "property", tags: [t.propertyName] },
  { token: "variable", tags: [t.variableName, t.name, t.labelName, t.content] },
  { token: "operator", tags: [t.operator, t.function(t.punctuation)] },
  { token: "punctuation", tags: [t.punctuation] },
  { token: "invalid", tags: [t.invalid] },
];

/** The colours, for an editor. syn-invalid also gets a wavy underline, so it isn't colour alone. */
export const codeHighlightStyle = HighlightStyle.define(SYNTAX.map(({ token, tags }) => ({
  tag: tags,
  color: `var(--syn-${token})`,
  ...(token === "invalid" ? { textDecoration: "underline wavy", textUnderlineOffset: "3px" } : {}),
})));

/** The same map with plain class names (`syn-keyword`), to colour code outside an editor and to test it. */
export const codeClasses = tagHighlighter(SYNTAX.map(({ token, tags }) => ({ tag: tags, class: `syn-${token}` })));

export type CodeRun = { from: number; to: number; token: SynToken };

/** Parses `code` in `language` and lists the coloured stretches with their token, nested languages (PHP in Blade, CSS
 *  in HTML) included. What has no token (spaces, plain HTML text) isn't listed. */
export function highlightCode(code: string, language: Language): CodeRun[] {
  const runs: CodeRun[] = [];
  highlightTree(language.parser.parse(code), codeClasses, (from, to, classes) => {
    const token = classes.split(" ").reverse().find((c) => c.startsWith("syn-"))?.slice(4) as SynToken | undefined;
    if (!token) return;
    const last = runs[runs.length - 1];
    if (last && last.to === from && last.token === token) last.to = to;
    else runs.push({ from, to, token });
  });
  return runs;
}

/** The editor around the code: t-code in --font-mono (it grows with Settings → Appearance's interface size), the app's
 *  surfaces and text colours, and D2's active line, matching bracket and search matches. */
export const codeEditorTheme = EditorView.theme({
  "&": { color: "var(--text)", backgroundColor: "var(--bg)", fontSize: "calc(12.5px + var(--ui-grow, 0px))" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.6" },
  ".cm-content": { padding: "4px 0", caretColor: "var(--text)" },
  ".cm-line": { padding: "0 24px 0 6px" },
  ".cm-cursor, .cm-dropCursor": { borderLeft: "2px solid var(--text)" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
    backgroundColor: "var(--accent-soft)",
  },
  ".cm-activeLine": { backgroundColor: "var(--hover)" },
  ".cm-gutters": { backgroundColor: "var(--bg)", color: "var(--text-3)", border: "none" },
  ".cm-lineNumbers .cm-gutterElement": { minWidth: "calc(3ch + 24px)", padding: "0 8px 0 16px" },
  ".cm-activeLineGutter": { backgroundColor: "var(--hover)", color: "var(--text)" },
  ".cm-foldGutter .cm-gutterElement": { width: "22px", textAlign: "center" },
  ".cm-foldPlaceholder": { margin: "0 2px", padding: "0 4px", border: "none", borderRadius: "var(--radius-s)", backgroundColor: "var(--selected)", color: "var(--text-2)" },
  "&.cm-focused .cm-matchingBracket": { borderRadius: "2px", backgroundColor: "var(--selected)", boxShadow: "inset 0 0 0 1px var(--line-2)" },
  "&.cm-focused .cm-nonmatchingBracket": { borderRadius: "2px", backgroundColor: "var(--danger-soft)" },
  ".cm-searchMatch": { borderRadius: "2px", backgroundColor: "var(--accent-soft)" },
  ".cm-searchMatch.cm-searchMatch-selected": { boxShadow: "inset 0 0 0 1px var(--accent)" },
  ".cm-selectionMatch": { backgroundColor: "var(--hover)" },
  ".cm-tooltip": { border: "1px solid var(--line-2)", borderRadius: "var(--radius)", backgroundColor: "var(--overlay)", color: "var(--text)", boxShadow: "var(--shadow-pop)" },
  ".cm-panels": { backgroundColor: "var(--raised)", color: "var(--text)" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--line)" },
  ".cm-panels.cm-panels-bottom": { borderTop: "1px solid var(--line)" },
});

/** The code theme and its colours: give these to an editor, with the file's language from codeLanguages.ts. */
export const codeTheme: Extension = [codeEditorTheme, syntaxHighlighting(codeHighlightStyle)];
