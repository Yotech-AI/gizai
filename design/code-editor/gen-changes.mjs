// Builds the Code Editor's D1 design (GA-104): design/code-editor/changes.html (the Changes view and the colours for
// code and diffs, for review), new-tokens.json (the new diff-* and syn-* tokens in design/tokens.json's format, for F0)
// and the colour tables in README.md. The code in the page is parsed with Lezer's own grammars and coloured through the
// same tag-to-token map F0 builds in src/lib/codeTheme.ts, so the page shows what CodeMirror will show. The diffs, their
// folds and the changed words are computed here too.
//
// usage: node design/code-editor/gen-changes.mjs
// It needs @lezer/highlight, javascript, html, php and rust. Until F0 adds @codemirror/lang-php and lang-rust to the app,
// install them outside the repository and point LEZER_DIR at that node_modules:
//   npm install --prefix /some/scratch/folder @lezer/highlight @lezer/javascript @lezer/html @lezer/php @lezer/rust
//   LEZER_DIR=/some/scratch/folder/node_modules node design/code-editor/gen-changes.mjs
// All five must come from the same node_modules: Lezer's tags are compared by identity.
import fs from "node:fs";
import path from "node:path";

const HERE = path.dirname(new URL(import.meta.url).pathname);
const GZ = path.resolve(HERE, "../..");
const LZ = process.env.LEZER_DIR ? path.resolve(process.env.LEZER_DIR) : path.join(GZ, "node_modules");
const need = async (pkg) => {
  const file = path.join(LZ, pkg, "dist/index.js");
  if (!fs.existsSync(file)) { console.error(`${pkg} isn't in ${LZ}: see the usage at the top of this file.`); process.exit(1); }
  return import(file);
};
const { tags: t, tagHighlighter, highlightTree } = await need("@lezer/highlight");
const jsx = (await need("@lezer/javascript")).parser.configure({ dialect: "jsx ts" });
const html = (await need("@lezer/html")).parser;
const php = (await need("@lezer/php")).parser;
const phpPlain = php.configure({ top: "Program" });
const rust = (await need("@lezer/rust")).parser;

/* ================================================================================================================
   The colours. One source for the page's CSS, new-tokens.json and the README's tables. An alias ("{text}") follows
   another token in both themes, as st-review follows needs today.
   ================================================================================================================ */
const SURF = {
  dark: { bg: "#0f1013", raised: "#15171b", side: "#0b0c0f", selected: "#22252c", text: "#ededf0", "text-2": "#a5a8b1", "text-3": "#7f838d", danger: "#f2706b" },
  light: { bg: "#ffffff", raised: "#ffffff", side: "#f3f4f7", selected: "#e6e9f0", text: "#14161b", "text-2": "#4b4f59", "text-3": "#636772", danger: "#c93530" },
};
const DIFF = [
  { name: "diff-add", value: { dark: "#4cc38a", light: "#167347" }, text: true,
    usage: "Added: the + mark and the line number of an added line, + counts (+964), the A status letter. Text on bg, raised, selected and diff-add-soft." },
  { name: "diff-del", value: { dark: "#f2706b", light: "#b42d29" }, text: true,
    usage: "Removed: the − mark and the old line number of a removed line, − counts (−112), the D status letter. Text on bg, raised, selected and diff-del-soft." },
  { name: "diff-add-soft", value: { dark: "rgba(76, 195, 138, 0.10)", light: "rgba(40, 180, 90, 0.10)" },
    usage: "Behind an added line, its number and its + mark." },
  { name: "diff-del-soft", value: { dark: "rgba(242, 112, 107, 0.10)", light: "rgba(230, 60, 50, 0.08)" },
    usage: "Behind a removed line, its old number and its − mark." },
  { name: "diff-add-strong", value: { dark: "rgba(76, 195, 138, 0.20)", light: "rgba(40, 180, 90, 0.18)" },
    usage: "Behind the characters that changed inside an added line, drawn over diff-add-soft." },
  { name: "diff-del-strong", value: { dark: "rgba(242, 112, 107, 0.20)", light: "rgba(230, 60, 50, 0.13)" },
    usage: "Behind the characters that changed inside a removed line, drawn over diff-del-soft." },
  { name: "diff-fold", value: { dark: "#16181e", light: "#f1f3f8" },
    usage: "The fold bar between changes (\"37 unchanged lines\", ↑ 20, ↓ 20), with text-2 on it." },
];
const SYN = [
  { name: "syn-keyword", value: { dark: "#b4a0fb", light: "#6941c6" }, hue: "violet", from: "c-violet, lighter in dark and darker in light",
    usage: "Keywords (fn, function, return, if, pub, use, const), self and this, Blade's directives (@if, @foreach, @php)." },
  { name: "syn-meta", value: "{syn-keyword}", usage: "<?php and ?>, Blade's {{ }} and {!! !!}, attributes (#[derive], #[Route]), decorators, escapes." },
  { name: "syn-function", value: { dark: "#85adff", light: "#1e54c4" }, hue: "blue", from: "c-blue, lighter in dark and darker in light",
    usage: "Functions and methods, where they're defined and called; macros (println!)." },
  { name: "syn-tag", value: "{syn-function}", usage: "HTML, Blade and JSX tags (div, x-button, Sidebar)." },
  { name: "syn-type", value: { dark: "#e3bd3c", light: "#855600" }, hue: "gold", from: "c-yellow in dark; a dark gold in light",
    usage: "Types, classes, structs, enums, interfaces, namespaces; Rust's lifetimes." },
  { name: "syn-attribute", value: "{syn-type}", usage: "Attribute names in HTML, Blade and JSX (class, wire:model, onClick)." },
  { name: "syn-string", value: { dark: "#f29a4a", light: "#9c4507" }, hue: "orange", from: "c-orange in dark; a burnt orange in light",
    usage: "Strings, characters, template strings, regular expressions, attribute values." },
  { name: "syn-number", value: "{syn-string}", usage: "Numbers, true, false and null: literals, with the strings." },
  { name: "syn-comment", value: { dark: "#8a8e98", light: "#5f636e" }, hue: "grey", from: "c-grey in dark; a shade darker than text-3 in light",
    usage: "Comments, doc comments and Blade's {{-- --}}." },
  { name: "syn-variable", value: "{text}", usage: "Variables and other names; the text of HTML and JSX." },
  { name: "syn-property", value: "{text}", usage: "Properties and fields ($this->total, task.title, self.files), JSON and YAML keys." },
  { name: "syn-operator", value: "{text-2}", usage: "Operators (= + -> => && ?:)." },
  { name: "syn-punctuation", value: "{text-2}", usage: "Brackets, commas, semicolons, < > around tags." },
  { name: "syn-invalid", value: "{danger}", usage: "What the parser can't read (an unclosed tag): with a wavy underline, so it isn't colour alone." },
];
const ALL = [...DIFF, ...SYN];
const valOf = (tok, theme) => {
  const v = typeof tok.value === "string" ? tok.value : tok.value[theme];
  if (!v.startsWith("{")) return v;
  const ref = v.slice(1, -1);
  return SURF[theme][ref] ?? valOf(ALL.find((x) => x.name === ref), theme);
};

/* ---------- contrast (WCAG 2) ---------- */
const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const rgba = (c) => { const p = c.match(/\(([^)]+)\)/)[1].split(",").map(Number); return { rgb: p.slice(0, 3), a: p[3] }; };
const over = (top, under) => { const { rgb, a } = rgba(top); return rgb.map((v, i) => Math.round(v * a + under[i] * (1 - a))); };
const lum = (c) => { const f = (v) => { v /= 255; return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]); };
const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((m, n) => n - m); return (x + 0.05) / (y + 0.05); };
const toHex = (c) => "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("");
const hueOf = ([r, g, b]) => { r /= 255; g /= 255; b /= 255; const mx = Math.max(r, g, b), mn = Math.min(r, g, b), d = mx - mn;
  if (!d) return null; const h = mx === r ? (g - b) / d + (g < b ? 6 : 0) : mx === g ? (b - r) / d + 2 : (r - g) / d + 4; return Math.round(h * 60); };
const SURFACES = (theme) => {
  const bg = hex(SURF[theme].bg);
  const d = (n) => valOf(DIFF.find((x) => x.name === n), theme);
  const addSoft = over(d("diff-add-soft"), bg), delSoft = over(d("diff-del-soft"), bg);
  return { bg, raised: hex(SURF[theme].raised), selected: hex(SURF[theme].selected), addSoft, delSoft,
    addStrong: over(d("diff-add-strong"), addSoft), delStrong: over(d("diff-del-strong"), delSoft), fold: hex(d("diff-fold")) };
};
const ratios = (color, theme) => Object.fromEntries(Object.entries(SURFACES(theme)).map(([k, s]) => [k, ratio(hex(color), s)]));
// The checks the card asks for: 4.5:1 on bg and raised, and no teal (150-200°) or magenta (285-345°) hue.
const problems = [];
for (const tok of ALL.filter((x) => x.text || x.name.startsWith("syn-")))
  for (const th of ["dark", "light"]) {
    const c = valOf(tok, th), r = ratios(c, th), h = hueOf(hex(c));
    if (r.bg < 4.5 || r.raised < 4.5) problems.push(`${tok.name} ${th}: ${r.bg.toFixed(2)} on bg, ${r.raised.toFixed(2)} on raised`);
    if (h !== null && ((h >= 150 && h <= 200 && tok.name !== "diff-add") || (h >= 285 && h <= 345))) problems.push(`${tok.name} ${th}: hue ${h}°`);
  }
if (problems.length) { console.error("The colours miss the card's rules:\n" + problems.join("\n")); process.exit(1); }

/* ================================================================================================================
   Highlighting: Lezer's tags to the syn-* tokens (src/lib/codeTheme.ts does the same with a HighlightStyle).
   ================================================================================================================ */
const MAP = [
  { tags: [t.keyword, t.self, t.atom, t.modifier], token: "keyword", cls: "kw", lezer: "keyword (and controlKeyword, definitionKeyword, moduleKeyword, operatorKeyword), self, atom, modifier" },
  { tags: [t.meta, t.escape, t.special(t.brace)], token: "meta", cls: "meta", lezer: "meta (and processingInstruction, documentMeta, annotation), escape, special(brace)" },
  { tags: [t.function(t.variableName), t.function(t.propertyName), t.macroName], token: "function", cls: "fn", lezer: "function(variableName), function(propertyName), macroName" },
  { tags: [t.tagName], token: "tag", cls: "tag", lezer: "tagName, standard(tagName)" },
  { tags: [t.typeName, t.className, t.namespace, t.special(t.variableName)], token: "type", cls: "ty", lezer: "typeName, className, namespace, special(variableName) (Rust's lifetimes)" },
  { tags: [t.string, t.regexp, t.url, t.special(t.string)], token: "string", cls: "str", lezer: "string (and character, attributeValue, docString), special(string), regexp, url" },
  { tags: [t.number, t.bool, t.null], token: "number", cls: "num", lezer: "number (integer, float), bool, null" },
  { tags: [t.comment], token: "comment", cls: "com", lezer: "comment (lineComment, blockComment, docComment)" },
  { tags: [t.attributeName], token: "attribute", cls: "attr", lezer: "attributeName" },
  { tags: [t.propertyName], token: "property", cls: "prop", lezer: "propertyName, special(propertyName), definition(propertyName)" },
  { tags: [t.variableName, t.name, t.labelName, t.content], token: "variable", cls: "var", lezer: "variableName, definition(variableName), name, labelName, content" },
  { tags: [t.operator, t.function(t.punctuation)], token: "operator", cls: "op", lezer: "operator (and all its kinds), function(punctuation) (=>)" },
  { tags: [t.punctuation], token: "punctuation", cls: "pun", lezer: "punctuation (separator, paren, brace, squareBracket, angleBracket)" },
  { tags: [t.invalid], token: "invalid", cls: "inv", lezer: "invalid" },
];
const HL = tagHighlighter(MAP.map((m) => ({ tag: m.tags, class: "s-" + m.cls })));
const paint = (out, base, tree) => highlightTree(tree, HL, (f, to, c) => { for (let i = f; i < to; i++) out[base + i] = c; });
const fill = (out, from, to, c) => { for (let i = from; i < to; i++) out[i] = c; };

// Blade, as F0's src/lib/blade.ts will do it on the model of lang-liquid: HTML around, PHP inside {{ }}, {!! !!},
// a directive's arguments and @php … @endphp; directives as keywords, the delimiters as meta, {{-- --}} as a comment.
function bladeClasses(src) {
  const out = new Array(src.length).fill(""), regions = [];
  for (let i = 0; i < src.length;) {
    const at = (s) => src.startsWith(s, i);
    if (at("{{--")) { const e = src.indexOf("--}}", i) + 4; regions.push({ from: i, to: e, kind: "comment" }); i = e; continue; }
    if (at("{!!") || at("{{")) { const raw = at("{!!"), n = raw ? 3 : 2, e = src.indexOf(raw ? "!!}" : "}}", i) + n; regions.push({ from: i, to: e, kind: "echo", n }); i = e; continue; }
    if (src[i] === "@" && /[a-z]/i.test(src[i + 1] ?? "") && !/[\w.@]/.test(src[i - 1] ?? "")) {
      const name = /^@[a-zA-Z]+/.exec(src.slice(i))[0];
      let e = i + name.length;
      if (name === "@php" && src[e] !== "(") { const end = src.indexOf("@endphp", e) + 7; regions.push({ from: i, to: end, kind: "block" }); i = end; continue; }
      let args = null;
      const open = src[e] === "(" ? e : src[e] === " " && src[e + 1] === "(" ? e + 1 : -1;
      if (open >= 0) { let depth = 0, j = open; for (; j < src.length; j++) { if (src[j] === "(") depth++; else if (src[j] === ")" && --depth === 0) break; } args = [open, j]; e = j + 1; }
      regions.push({ from: i, to: e, kind: "directive", nameEnd: i + name.length, args }); i = e; continue;
    }
    i++;
  }
  const masked = [...src].map((ch, k) => (ch !== "\n" && regions.some((r) => k >= r.from && k < r.to) ? " " : ch)).join("");
  paint(out, 0, html.parse(masked));
  const code = (from, to) => paint(out, from, phpPlain.parse(src.slice(from, to)));
  for (const r of regions) {
    fill(out, r.from, r.to, "");
    if (r.kind === "comment") fill(out, r.from, r.to, "s-com");
    if (r.kind === "echo") { fill(out, r.from, r.from + r.n, "s-meta"); fill(out, r.to - r.n, r.to, "s-meta"); code(r.from + r.n, r.to - r.n); }
    if (r.kind === "block") { fill(out, r.from, r.from + 4, "s-kw"); fill(out, r.to - 7, r.to, "s-kw"); code(r.from + 4, r.to - 7); }
    if (r.kind === "directive") {
      fill(out, r.from, r.nameEnd, "s-kw");
      if (r.args) { fill(out, r.args[0], r.args[0] + 1, "s-pun"); fill(out, r.args[1], r.args[1] + 1, "s-pun"); code(r.args[0] + 1, r.args[1]); }
    }
  }
  return out;
}
function classesOf(src, lang) {
  if (lang === "blade") return bladeClasses(src);
  const out = new Array(src.length).fill("");
  if (lang !== "text") paint(out, 0, ({ tsx: jsx, php, rust, json: jsx, html })[lang].parse(src));
  return out;
}
const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
// One line's HTML: runs of the same class (and changed or not) become one span.
function spans(text, cls, changed) {
  let html = "", i = 0;
  while (i < text.length) {
    let j = i + 1;
    while (j < text.length && cls[j] === cls[i] && !!changed?.[j] === !!changed?.[i]) j++;
    const c = [cls[i], changed?.[i] ? "x" : ""].filter(Boolean).join(" ");
    html += c ? `<span class="${c}">${esc(text.slice(i, j))}</span>` : esc(text.slice(i, j));
    i = j;
  }
  return html;
}
function lines(src, lang) {
  const cls = classesOf(src, lang), out = [];
  let at = 0;
  for (const text of src.split("\n")) { out.push({ text, cls: cls.slice(at, at + text.length) }); at += text.length + 1; }
  return out;
}

/* ================================================================================================================
   Diffs: lines by LCS, changed words in paired lines, 3 lines of context, folds of 4 lines or more (as F1 will).
   ================================================================================================================ */
function lcs(a, b, eq = (x, y) => x === y) {
  const n = a.length, m = b.length, dp = Array.from({ length: n + 1 }, () => new Int32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) dp[i][j] = eq(a[i], b[j]) ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  const ops = [];
  let i = 0, j = 0;
  while (i < n && j < m) {
    if (eq(a[i], b[j])) ops.push(["=", i++, j++]);
    else if (dp[i + 1][j] >= dp[i][j + 1]) ops.push(["-", i++, -1]);
    else ops.push(["+", -1, j++]);
  }
  while (i < n) ops.push(["-", i++, -1]);
  while (j < m) ops.push(["+", -1, j++]);
  return ops;
}
// Which characters changed between two versions of a line; nothing when the lines have too little in common.
function wordChanges(oldLine, newLine) {
  const tok = (s) => s.match(/\w+|\s+|[^\w\s]/g) ?? [];
  const a = tok(oldLine), b = tok(newLine), ops = lcs(a, b);
  const same = ops.filter((o) => o[0] === "=").reduce((n, o) => n + a[o[1]].length, 0);
  if (same < 0.4 * Math.max(oldLine.length, newLine.length)) return [null, null];
  const ca = new Array(oldLine.length).fill(false), cb = new Array(newLine.length).fill(false);
  const posA = [], posB = [];
  a.reduce((p, x) => (posA.push(p), p + x.length), 0); b.reduce((p, x) => (posB.push(p), p + x.length), 0);
  for (const [k, i, j] of ops) {
    if (k === "-") fill(ca, posA[i], posA[i] + a[i].length, true);
    if (k === "+") fill(cb, posB[j], posB[j] + b[j].length, true);
  }
  return [ca, cb];
}
const SIGN = { add: "+", del: "−", "": "" };
const row = (kind, no, l, changed) =>
  `<div class="r${kind ? " " + kind : ""}"><span class="ln">${no}</span><span class="sg">${SIGN[kind]}</span><span class="tx">${spans(l.text, l.cls, changed)}</span></div>`;
// A fold bar and the lines it hides. `where`: "top", "mid" or "end"; ↑ 20 shows lines above the change under the bar,
// ↓ 20 lines below the change over it; the count itself shows them all. `pull`: no file text, so it can't open.
function foldBar(hidden, where, pull = false) {
  const n = hidden.length;
  const steps = !pull && n > 20
    ? (where !== "top" ? `<button class="fold-step" data-step="down" title="Show 20 more lines below the change above">↓ 20</button>` : "")
      + (where !== "end" ? `<button class="fold-step" data-step="up" title="Show 20 more lines above the change below">↑ 20</button>` : "")
    : "";
  const label = pull ? `<span class="fold-off">${n} unchanged lines aren't in the pull request's diff</span>`
    : `<button class="fold-all" title="Show all ${n} lines">${n} unchanged lines</button>`;
  return `<div class="fold" data-where="${where}"><span class="ln">${ic("unfold-vertical", "sm")}</span><span class="sg"></span><span class="tx">${label}${steps}</span></div>`
    + (pull ? "" : `<div class="fold-hidden" hidden>${hidden.join("")}</div>`);
}
function diffRows(oldSrc, newSrc, lang, { pull = false } = {}) {
  const A = lines(oldSrc, lang), B = lines(newSrc, lang);
  const ops = lcs(A.map((l) => l.text), B.map((l) => l.text));
  const items = [];
  let add = 0, del = 0;
  for (let k = 0; k < ops.length;) {
    if (ops[k][0] === "=") { const [, i, j] = ops[k++]; items.push({ eq: true, html: row("", j + 1, B[j]) }); continue; }
    const dels = [], adds = [];
    while (k < ops.length && ops[k][0] !== "=") (ops[k][0] === "-" ? dels : adds).push(ops[k++]);
    const marks = dels.map((d, n) => (adds[n] ? wordChanges(A[d[1]].text, B[adds[n][2]].text) : [null, null]));
    dels.forEach((d, n) => { del++; items.push({ html: row("del", d[1] + 1, A[d[1]], marks[n][0]) }); });
    adds.forEach((a, n) => { add++; items.push({ html: row("add", a[2] + 1, B[a[2]], marks[n]?.[1]) }); });
  }
  let html = "";
  for (let k = 0; k < items.length;) {
    if (!items[k].eq) { html += items[k++].html; continue; }
    let e = k; while (e < items.length && items[e].eq) e++;
    const top = k === 0, end = e === items.length;
    const keepA = top ? 0 : 3, keepB = end ? 0 : 3, run = items.slice(k, e).map((x) => x.html);
    if (run.length - keepA - keepB >= 4 && !(top && end)) {
      html += run.slice(0, keepA).join("") + foldBar(run.slice(keepA, run.length - keepB), top ? "top" : end ? "end" : "mid", pull) + run.slice(run.length - keepB).join("");
    } else html += run.join("");
    k = e;
  }
  return { html, add, del, digits: String(Math.max(A.length, B.length)).length };
}
const whole = (src, lang, kind) => {
  const L = lines(src, lang);
  return { html: L.map((l, i) => row(kind, i + 1, l)).join(""), add: kind === "add" ? L.length : 0, del: kind === "del" ? L.length : 0, digits: String(L.length).length };
};

/* ================================================================================================================
   Icons: Lucide's own data (lucide-react, ISC), drawn once in a sprite.
   ================================================================================================================ */
const LUCIDE = path.join(GZ, "node_modules/lucide-react/dist/esm/icons");
const used = new Set();
function ic(name, cls = "") { used.add(name); return `<svg class="icon${cls ? " " + cls : ""}" aria-hidden="true"><use href="#i-${name}"/></svg>`; }
async function sprite() {
  const attrs = (o) => Object.entries(o).filter(([k]) => k !== "key").map(([k, v]) => `${k.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase())}="${v}"`).join(" ");
  let s = "";
  for (const name of [...used].sort()) {
    const node = (await import(path.join(LUCIDE, `${name}.js`))).__iconNode;
    s += `<symbol id="i-${name}" viewBox="0 0 24 24">${node.map(([tag, a]) => `<${tag} ${attrs(a)}/>`).join("")}</symbol>`;
  }
  return `<svg class="sprite" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><defs>${s}</defs></svg>`;
}

/* ================================================================================================================
   The code. Nothing here has a backtick or a dollar-brace: they're template strings.
   ================================================================================================================ */
const SIDEBAR_OLD = String.raw`// The sidebar (design system: Sidebar): quick actions, work areas, projects, the team's agents and their memory.
// The company pages stay at the bottom, with the update notice above them when a newer release is out.
import { Building2, Crown, FolderKanban, Inbox, ListTodo, MessagesSquare, Search, SquarePen } from "lucide-react";
import { getTeam, listChatThreads, listProjects, listTasks, memoryNotes } from "../api";
import { go, href, type Route } from "../router";
import { leadOf, memoryHome, memoryScope, noteCount, sharersOf } from "../lib/memory";
import type { Member, MemoryNote } from "../types";
import { useData } from "../lib/useData";
import { useCurrentTeam } from "../lib/team";
import { useLiveRuns } from "../lib/useLiveRuns";
import { inboxCount } from "../lib/inbox";
import { useDrawer } from "../lib/drawers";
import { liveTotal, useFolded, type Fold } from "../lib/sidebar";
import { useChatLive } from "./chat/useChat";
import { UpdateNotice } from "./UpdateNotice";

type Props = { route: Route; youId: string; onSearch: () => void; onNewTask: () => void };

export function Sidebar({ route, youId, onSearch, onNewTask }: Props) {
  const projects = useData(() => listProjects());
  const allTasks = useData(() => listTasks());
  const [teamId] = useCurrentTeam();
  const team = useData(() => getTeam(teamId), [teamId]);
  const live = useLiveRuns();
  const agents = (team.data?.members ?? []).filter((m) => m.kind === "agent");
  const lead = leadOf(agents);
  const [folded, fold] = useFolded();
  const is = (...pages: Route["page"][]) => pages.includes(route.page);
  return (
    <aside className="side" aria-label="Navigation">
      <div className="side-main">
        <button className="nav-item" onClick={onNewTask}><SquarePen className="icon" /><span>New task</span></button>
        <button className="nav-item" onClick={onSearch}><Search className="icon" /><span>Search</span></button>
        <Item to={{ page: "inbox" }} icon={Inbox} label="Inbox" active={is("inbox")} />

        <div className="nav-section">
          <div className="nav-label">Work</div>
          <Item to={{ page: "chat" }} icon={MessagesSquare} label="Chat" active={is("chat", "chats")} />
          <Item to={{ page: "tasks" }} icon={ListTodo} label="Tasks" active={is("tasks", "board", "task")} />
          <Item to={{ page: "clients" }} icon={Building2} label="Clients" active={is("clients", "client")} />
          <Item to={{ page: "projects" }} icon={FolderKanban} label="Projects" active={is("projects")} />
        </div>

        <div className="nav-section">
          <div className="nav-label">Projects</div>
          {(projects.data ?? []).filter((p) => p.status === "active").slice(0, 12).map((p) => (
            <a key={p.id} className="nav-item" href={href({ page: "project", id: p.id })}>
              <span className="dot" style={{ background: p.color ?? "var(--text-3)" }} />
              <span>{p.name}</span>
            </a>
          ))}
        </div>

        <FoldSection section="agents" name="Agents" folded={folded("agents")} onFold={() => fold("agents")}>
          {agents.map((a) => <AgentRow key={a.actorId} agent={a} route={route} live={live} />)}
        </FoldSection>

        <MemorySection route={route} lead={lead} agents={agents} folded={folded("memory")} onFold={() => fold("memory")} />
      </div>
    </aside>
  );
}

/** A section that folds (Agents, Memory): the heading's text and its caret fold it, and the choice is kept. */
export function FoldSection({ section, name, folded, onFold, children }: {
  section: Fold; name: string; folded: boolean; onFold: () => void; children: React.ReactNode;
}) {
  const list = "nav-list-" + section;
  return (
    <div className="nav-section">
      <div className="nav-label">
        <span className="fold" onClick={onFold}>{name}</span>
        <button aria-expanded={!folded} aria-controls={list} title={folded ? "Show " + name : "Hide " + name} onClick={onFold}>
          {folded ? <ChevronDown className="icon sm" /> : <ChevronUp className="icon sm" />}
        </button>
      </div>
      <div className="nav-list" id={list} hidden={folded}>{children}</div>
    </div>
  );
}`;
const SIDEBAR_NEW = SIDEBAR_OLD
  .replace(`import { getTeam, listChatThreads`, `import { editorFolders, getTeam, listChatThreads`)
  .replace(`import { UpdateNotice } from "./UpdateNotice";`, `import { UpdateNotice } from "./UpdateNotice";\nimport { EditorSection } from "./editor/EditorSection";`)
  .replace(`  const live = useLiveRuns();\n`, `  const live = useLiveRuns();\n  const folders = useData(() => editorFolders());\n`)
  .replace(`        <FoldSection section="agents"`, `        <EditorSection folders={folders.data ?? []} folded={folded("editor")} onFold={() => fold("editor")}\n          current={route.page === "editor" ? route.id : undefined} />\n\n        <FoldSection section="agents"`)
  .replace(`/** A section that folds (Agents, Memory):`, `/** A section that folds (Editor, Agents, Memory):`);

const EDITOR_SECTION = String.raw`// The sidebar's Editor section (GA-118): the folders opened in the Editor, newest first, with how each one
// stands. It folds like Agents and Memory, and remembers it.
import { FolderCode } from "lucide-react";
import { href } from "../../router";
import type { EditorFolder } from "../../types";
import { FoldSection } from "../Sidebar";

type Props = { folders: EditorFolder[]; folded: boolean; onFold: () => void; current?: string };

/** At most 8 rows: "3 behind" or "2 changed" on the right, a card's identifier for a worktree. */
export function EditorSection({ folders, folded, onFold, current }: Props) {
  const rows = folders.slice(0, 8);
  return (
    <FoldSection section="editor" name="Editor" folded={folded} onFold={onFold}>
      {rows.length === 0 && <span className="nav-item faint">No folders open yet</span>}
      {rows.map((f) => (
        <a key={f.id} className={current === f.id ? "nav-item on" : "nav-item"} href={href({ page: "editor", id: f.id })}>
          <FolderCode className="icon" />
          <span>{f.name}</span>
          {f.taskId && <span className="meta faint">by hand</span>}
        </a>
      ))}
    </FoldSection>
  );
}`;

const FOLD_TS_OLD = String.raw`// The sidebar's folding sections (GA-90): which are folded, kept on this computer, and how many agents are at work.
import { useCallback, useState } from "react";
import type { LiveRun, Member } from "../types";

const KEY = "gizai-sidebar-folded";

function readFolded(): Set<string> {
  try {
    const raw = localStorage.getItem(KEY);
    return new Set(raw ? (JSON.parse(raw) as string[]) : []);
  } catch {
    return new Set();
  }
}

function writeFolded(folded: Set<string>) {
  try {
    localStorage.setItem(KEY, JSON.stringify([...folded]));
  } catch {
    /* private mode: the choice lasts until Gizai closes */
  }
}

/** How many of the team's agents have a run going now (shown on folded Agents). */
export function liveTotal(live: LiveRun[], agents: Member[]): number {
  const ids = new Set(agents.map((a) => a.actorId));
  return live.filter((r) => ids.has(r.agentId)).length;
}

/** The sections that fold. */
export type Fold = "agents" | "memory";

export function useFolded(): [(f: Fold) => boolean, (f: Fold) => void] {
  const [folded, setFolded] = useState(readFolded);
  const fold = useCallback((f: Fold) => {
    setFolded((old) => {
      const next = new Set(old);
      if (next.has(f)) next.delete(f); else next.add(f);
      writeFolded(next);
      return next;
    });
  }, []);
  return [(f) => folded.has(f), fold];
}`;
const FOLD_TS_NEW = FOLD_TS_OLD.replace(`export type Fold = "agents" | "memory";`, `export type Fold = "editor" | "agents" | "memory";`);

const EDITOR_RS = String.raw`//! The Editor's folders and files (GA-118): one folder level at a time, and the path gate every command goes through.
use std::path::{Component, Path, PathBuf};

use crate::os;

/// A file over this size opens read-only; over TOO_BIG it doesn't open at all.
pub const READ_ONLY_OVER: u64 = 1024 * 1024;
pub const TOO_BIG: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub ignored: bool,
}

/// path inside folder, or why not: no "..", no ".git", and no link that leads out of the folder.
pub fn inside<'a>(folder: &'a Path, path: &str) -> Result<PathBuf, String> {
    let rel = Path::new(path);
    for part in rel.components() {
        match part {
            Component::ParentDir => return Err(format!("{path}: .. isn't allowed here")),
            Component::Normal(p) if p == ".git" => return Err("Gizai never opens .git".into()),
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err(format!("{path}: give a path inside the folder")),
        }
    }
    let real = folder.join(rel).canonicalize().map_err(|e| format!("{path}: {e}"))?;
    if !real.starts_with(folder) {
        return Err(format!("{path} leads out of {}", os::display(folder)));
    }
    Ok(real)
}`;

const OLD_FOLDERS_RS = String.raw`//! Folders an agent may use. GA-118 moved these rules to gizai-core (folder_rules.rs), next to the Editor's.
use std::path::{Path, PathBuf};

/// Never an agent's folder: the root, the home folder itself, and the folders that hold keys or logins.
const REFUSED: &[&str] = &["/", "~", "~/.ssh", "~/.gnupg", "~/.aws", "~/.config/gh", "~/.local/share/gizai"];

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    Root,
    Home,
    Secrets(PathBuf),
    GizaiData,
}

impl Refusal {
    pub fn sentence(&self) -> String {
        match self {
            Refusal::Root => "An agent can't work in / : pick a project's folder.".into(),
            Refusal::Home => "An agent can't work in your home folder itself: pick a folder in it.".into(),
            Refusal::Secrets(p) => format!("{} holds keys or logins, so no agent may use it.", p.display()),
            Refusal::GizaiData => "That is Gizai's own data folder.".into(),
        }
    }
}

/// Why folder can't be an agent's folder, or None when it can.
pub fn refusal(folder: &Path, home: &Path) -> Option<Refusal> {
    let expand = |p: &str| match p.strip_prefix('~') {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(p),
    };
    for (i, raw) in REFUSED.iter().enumerate() {
        let refused = expand(raw);
        if folder != refused && !(i >= 2 && folder.starts_with(&refused)) {
            continue;
        }
        return Some(match i {
            0 => Refusal::Root,
            1 => Refusal::Home,
            6 => Refusal::GizaiData,
            _ => Refusal::Secrets(refused),
        });
    }
    None
}`;

const LOCK_OLD = String.raw`    "node_modules/@codemirror/lang-markdown": {
      "version": "6.3.4",
      "resolved": "https://registry.npmjs.org/@codemirror/lang-markdown/-/lang-markdown-6.3.4.tgz",
      "license": "MIT"
    },
    "node_modules/@codemirror/language": {
      "version": "6.11.3",
      "license": "MIT"
    },`;
const LOCK_NEW = String.raw`    "node_modules/@codemirror/lang-markdown": {
      "version": "6.3.4",
      "resolved": "https://registry.npmjs.org/@codemirror/lang-markdown/-/lang-markdown-6.3.4.tgz",
      "license": "MIT"
    },
    "node_modules/@codemirror/lang-php": {
      "version": "6.0.2",
      "resolved": "https://registry.npmjs.org/@codemirror/lang-php/-/lang-php-6.0.2.tgz",
      "license": "MIT"
    },
    "node_modules/@codemirror/language": {
      "version": "6.11.3",
      "license": "MIT"
    },`;

const CONTROLLER_PHP = String.raw`<?php

namespace App\Http\Controllers;

use App\Models\Invoice;
use App\Support\CsvWriter;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\StreamedResponse;

class InvoiceExportController extends Controller
{
    /** Streams the invoices the filter shows as a CSV file, newest first. */
    public function __invoke(Request $request): StreamedResponse
    {
        $this->authorize('export', Invoice::class);

        $invoices = Invoice::query()
            ->issuedBetween($request->query('from'), $request->query('to'))
            ->latest('issued_at');

        return response()->streamDownload(function () use ($invoices) {
            $csv = new CsvWriter(fopen('php://output', 'w'));
            $csv->row(['Number', 'Client', 'Issued', 'Total']);
            foreach ($invoices->lazy(500) as $invoice) {
                $csv->row([$invoice->number, $invoice->client->name, $invoice->issued_at->toDateString(),
                    $invoice->totalInEuros()]);
            }
        }, 'invoices-'.now()->format('Y-m-d').'.csv');
    }
}`;

const INDEX_OLD = String.raw`<x-app-layout :title="__('Invoices')">
    <div class="flex items-center justify-between">
        <h1 class="text-xl font-semibold">{{ __('Invoices') }}</h1>
        <x-button href="{{ route('invoices.create') }}">{{ __('New invoice') }}</x-button>
    </div>

    {{-- The filter keeps its values in the address, so a link shows the same list. --}}
    <form method="GET" class="mt-6 flex gap-3">
        <x-input type="date" name="from" :value="request('from')" />
        <x-input type="date" name="to" :value="request('to')" />
        <x-button type="submit" variant="secondary">{{ __('Filter') }}</x-button>
    </form>

    <div class="mt-6 divide-y divide-gray-200">
        @forelse ($invoices as $invoice)
            <x-invoice-row :invoice="$invoice" />
        @empty
            <p class="py-6 text-gray-500">{{ __('No invoices in this period.') }}</p>
        @endforelse
    </div>

    {{ $invoices->withQueryString()->links() }}
</x-app-layout>`;
const INDEX_NEW = INDEX_OLD
  .replace(`        <x-button href="{{ route('invoices.create') }}">{{ __('New invoice') }}</x-button>\n`,
    `        <div class="flex gap-2">\n            @can('export', App\\Models\\Invoice::class)\n                <x-button variant="secondary" href="{{ route('invoices.export', request()->query()) }}">{{ __('Export as CSV') }}</x-button>\n            @endcan\n            <x-button href="{{ route('invoices.create') }}">{{ __('New invoice') }}</x-button>\n        </div>\n`)
  .replace(`text-gray-500`, `text-gray-600`);

const WEB_OLD = String.raw`<?php

use App\Http\Controllers\ClientController;
use App\Http\Controllers\InvoiceController;
use Illuminate\Support\Facades\Route;

Route::middleware(['auth', 'verified'])->group(function () {
    Route::get('/', fn () => redirect()->route('invoices.index'));
    Route::resource('clients', ClientController::class)->only(['index', 'show']);
    Route::resource('invoices', InvoiceController::class);
});`;
const WEB_NEW = WEB_OLD
  .replace(`use App\\Http\\Controllers\\InvoiceController;\n`, `use App\\Http\\Controllers\\InvoiceController;\nuse App\\Http\\Controllers\\InvoiceExportController;\n`)
  .replace(`    Route::resource('invoices'`, `    Route::get('/invoices/export', InvoiceExportController::class)->name('invoices.export');\n    Route::resource('invoices'`);

const SAMPLES = [
  { lang: "php", label: "PHP", file: "app/Models/Invoice.php", src: String.raw`<?php

namespace App\Models;

use App\Enums\InvoiceStatus;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;

#[\Illuminate\Database\Eloquent\Attributes\ScopedBy(ClientScope::class)]
class Invoice extends Model
{
    protected $fillable = ['number', 'client_id', 'issued_at', 'total'];

    protected function casts(): array
    {
        return ['issued_at' => 'date', 'total' => 'integer', 'status' => InvoiceStatus::class];
    }

    /** The client the invoice is for; a deleted client keeps its invoices. */
    public function client(): BelongsTo
    {
        return $this->belongsTo(Client::class)->withTrashed();
    }

    // Invoices issued in a period, both days included.
    public function scopeIssuedBetween(Builder $query, ?string $from, ?string $to): void
    {
        $query->when($from, fn (Builder $q) => $q->whereDate('issued_at', '>=', $from))
            ->when($to, fn (Builder $q) => $q->whereDate('issued_at', '<=', $to));
    }

    public function totalInEuros(): float
    {
        return round($this->total / 100, 2);
    }
}` },
  { lang: "blade", label: "Blade", file: "resources/views/components/invoice-row.blade.php", src: String.raw`{{-- One invoice in the list: number, client, date and total, its status as a word. --}}
@props(['invoice'])

<div {{ $attributes->class(['flex items-center gap-4 py-3', 'opacity-60' => $invoice->status->isVoid()]) }}>
    <a href="{{ route('invoices.show', $invoice) }}" class="font-mono text-sm">{{ $invoice->number }}</a>
    <span class="flex-1 truncate">{{ $invoice->client->name }}</span>
    <time datetime="{{ $invoice->issued_at->toDateString() }}">{{ $invoice->issued_at->isoFormat('D MMM') }}</time>

    @if ($invoice->status->isPaid())
        <x-badge color="green">{{ __('Paid') }}</x-badge>
    @elseif ($invoice->isOverdue())
        <x-badge color="red">{{ __('Overdue by :days days', ['days' => $invoice->daysOverdue()]) }}</x-badge>
    @else
        <x-badge>{{ __('Open') }}</x-badge>
    @endif

    @php($total = number_format($invoice->totalInEuros(), 2, ',', '.'))
    <span class="w-28 text-right tabular-nums">€ {{ $total }}</span>
    {!! $invoice->note_html !!}
</div>` },
  { lang: "tsx", label: "TSX", file: "src/components/changes/ChangesTabs.tsx", src: String.raw`// The Card | Changes tabs at the top of the task page (F1). The counts load with the card.
import { FileDiff, SquareKanban } from "lucide-react";
import type { CardChanges } from "../../types";
import { DiffStat } from "./DiffStat";

type Tab = "card" | "changes";
type Props = { open: Tab; changes: CardChanges | null; onOpen: (tab: Tab) => void };

/** "+964 −112", or "No changes"; no counts while the card has no branch or they're loading. */
export function ChangesTabs({ open, changes, onOpen }: Props) {
  const counted = changes !== null && changes.source !== "none";
  const empty = counted && changes.files.length === 0;
  return (
    <div className="tabs task-tabs" role="tablist" aria-label="The card and its changes">
      <button className="tab" role="tab" aria-selected={open === "card"} onClick={() => onOpen("card")}>
        <SquareKanban className="icon" />Card
      </button>
      <button className="tab" role="tab" aria-selected={open === "changes"} onClick={() => onOpen("changes")}>
        <FileDiff className="icon" />Changes
        {counted && (empty ? <span className="n">No changes</span>
          : <DiffStat add={changes.additions} del={changes.deletions} />)}
      </button>
    </div>
  );
}` },
  { lang: "rust", label: "Rust", file: "crates/gizai-agents/src/changes.rs", src: String.raw`/// One changed file from git diff --numstat -z -M: its counts, or None for a binary file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

/// Parses numstat's NUL-separated records. A rename leaves its path empty and adds two fields: old, new.
pub fn parse_numstat(out: &[u8]) -> Vec<FileChange> {
    let mut fields = out.split(|b| *b == 0).map(|f| String::from_utf8_lossy(f).into_owned());
    let mut files = Vec::with_capacity(64);
    while let Some(head) = fields.next() {
        let mut parts = head.splitn(3, '\t');
        let (add, del) = (parts.next().unwrap_or("-"), parts.next().unwrap_or("-"));
        let (old_path, path) = match parts.next() {
            Some("") | None => (fields.next(), fields.next().unwrap_or_default()),
            Some(p) => (None, p.to_string()),
        };
        if !path.is_empty() {
            files.push(FileChange { path, old_path, additions: add.parse().ok(), deletions: del.parse().ok() });
        }
    }
    files
}` },
];

const GLYPH = {
  review: `<svg class="st st-review" viewBox="0 0 16 16" fill="none" role="img" aria-label="Review"><circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><circle cx="8" cy="8" r="2.4" fill="currentColor"/></svg>`,
};

/* ================================================================================================================
   The page's pieces.
   ================================================================================================================ */
const DS = (add, del) => `<span class="diffstat">${add != null ? `<span class="add">+${add.toLocaleString("en-GB")}</span>` : ""}${del ? `<span class="del">−${del.toLocaleString("en-GB")}</span>` : ""}</span>`;
const LETTER = { A: ["a", "Added"], M: ["m", "Modified"], D: ["d", "Deleted"], R: ["r", "Renamed"], T: ["m", "Changed type"] };
const st = (s) => `<span class="chg-st ${LETTER[s][0]}" title="${LETTER[s][1]}" aria-label="${LETTER[s][1]}">${s}</span>`;
const pathHtml = (p, old) => {
  const split = (x) => { const i = x.lastIndexOf("/"); return [x.slice(0, i + 1), x.slice(i + 1)]; };
  const [dir, name] = split(p);
  if (!old) return `<span class="chg-path" title="${p}"><span class="dir">${dir}</span><b>${name}</b></span>`;
  const [odir, oname] = split(old);
  return `<span class="chg-path" title="${old} → ${p}"><span class="dir">${odir}</span>${oname}${ic("arrow-right", "arrow")}${odir === dir ? "" : `<span class="dir">${dir}</span>`}<b>${name}</b></span>`;
};
// One file in the diff column. f.body: rows (a diff), or a note for binary, lock, big, too large and rename-only files.
function fileBox(f) {
  const id = f.id ? ` id="${f.id}"` : "";
  const badges = (f.uncommitted ? `<span class="badge">Not committed</span>` : "") + (f.why ? `<span class="why">${f.why}</span>` : "");
  const open = f.open !== false ? `<button class="btn ghost sm" title="Open ${f.path} in the Editor, at this change">${ic("square-code")}Open</button>` : "";
  const caret = f.body ? `<button class="chg-caret" aria-expanded="${!f.folded}" title="${f.folded ? "Show" : "Fold"} this file">${ic("chevron-down")}</button>` : `<span class="chg-caret-off"></span>`;
  return `<section class="chg-box${f.folded ? " folded" : ""}${f.cls ? " " + f.cls : ""}"${id} data-path="${f.path}">
  <header class="chg-fh">${caret}${st(f.status)}${pathHtml(f.path, f.oldPath)}${f.stat ?? DS(f.add, f.del)}${badges}<span class="grow"></span>${open}</header>
  ${f.body ?? ""}
</section>`;
}
const code = (d, extra = "") => `<div class="chg-code" style="--digits: ${Math.max(2, d.digits)}">${extra}<div class="chg-lines">${d.html}</div></div>`;
const note = (icon, text, action = "") => `<div class="chg-note">${ic(icon)}<span>${text}</span>${action}</div>`;

// Screen 2: GA-118, a Gizai card in Review.
const D = {
  sidebarTsx: diffRows(SIDEBAR_OLD, SIDEBAR_NEW, "tsx"),
  editorSection: whole(EDITOR_SECTION, "tsx", "add"),
  foldTs: diffRows(FOLD_TS_OLD, FOLD_TS_NEW, "tsx"),
  editorRs: whole(EDITOR_RS, "rust", "add"),
  oldFolders: whole(OLD_FOLDERS_RS, "rust", "del"),
  lock: diffRows(LOCK_OLD, LOCK_NEW, "json"),
};
const G = [
  { dir: "crates/gizai-agents/src", files: [
    { path: "crates/gizai-agents/src/editor.rs", status: "A", ...D.editorRs, body: code(D.editorRs) },
    { path: "crates/gizai-agents/src/old_folders.rs", status: "D", ...D.oldFolders, folded: true, open: false, why: "Deleted, folded",
      body: note("file-minus", `Deleted: ${D.oldFolders.del} lines.`, `<button class="btn sm" data-show>Show</button>`) + `<div class="chg-later" hidden>${code(D.oldFolders)}</div>` },
  ] },
  { dir: "crates/gizai-core/src", files: [
    { path: "crates/gizai-core/src/folder_rules.rs", oldPath: "crates/gizai-core/src/folders.rs", status: "R", add: null, del: 0, stat: "", why: "Renamed, no changes", open: true },
  ] },
  { dir: "docs/samples", files: [
    { path: "docs/samples/memory-graph.min.json", status: "M", add: 1, del: 1, open: true, why: "Too big to show",
      body: note("file-x", "Too big to show here: 1.2 MB. Open it in your editor to read it.") },
  ] },
  { dir: "src/assets", files: [
    { path: "src/assets/editor-empty.png", status: "M", add: null, del: 0, stat: `<span class="diffstat"><span class="bin">Binary</span></span>`,
      body: note("binary", `Binary file changed: 12 KB ${ic("arrow-right", "arrow")} 14 KB.`) },
    { path: "src/assets/lang-map.json", status: "A", add: 3214, del: 0, folded: true, why: "Big diff, folded",
      body: note("fold-vertical", "A big diff: 3,214 added lines. It stays folded until you ask, so the page stays quick.", `<button class="btn sm" data-show>Show diff</button>`) },
  ] },
  { dir: "src/components", files: [
    { path: "src/components/Sidebar.tsx", status: "M", ...D.sidebarTsx, body: code(D.sidebarTsx) },
  ] },
  { dir: "src/components/editor", files: [
    { path: "src/components/editor/EditorSection.tsx", status: "A", ...D.editorSection, uncommitted: true, body: code(D.editorSection) },
  ] },
  { dir: "src/lib", files: [
    { path: "src/lib/sidebar.ts", status: "M", ...D.foldTs, uncommitted: true, body: code(D.foldTs) },
  ] },
  { dir: "", files: [
    { path: "package-lock.json", status: "M", add: 412, del: 20, folded: true, why: "Lock file, folded",
      body: note("lock", "A lock file: what npm wrote for the new packages.", `<button class="btn sm" data-show>Show diff</button>`)
        + `<div class="chg-later" hidden>${code(D.lock)}<div class="chg-note faint">This design shows the first change only.</div></div>` },
  ] },
];
const FILES = G.flatMap((g) => g.files);
FILES.forEach((f, i) => { f.id = "f" + i; });
const TOTAL = { add: FILES.reduce((n, f) => n + (f.add ?? 0), 0), del: FILES.reduce((n, f) => n + (f.del ?? 0), 0), files: FILES.length };
const UNCOMMITTED = FILES.filter((f) => f.uncommitted).length;

function fileRow(f, on = false) {
  const name = f.path.slice(f.path.lastIndexOf("/") + 1);
  const was = f.oldPath ? `<span class="was">${f.oldPath.slice(f.oldPath.lastIndexOf("/") + 1)} </span>${ic("arrow-right", "arrow")} ` : "";
  const counts = f.add == null && f.status === "M" ? `<span class="bin">Binary</span>` : f.status === "R" && !f.body ? `<span class="bin">Renamed</span>` : DS(f.add, f.del);
  return `<a class="chg-file${on ? " on" : ""}" href="#${f.id}" data-target="${f.id}" title="${f.oldPath ? f.oldPath + " → " : ""}${f.path}">${st(f.status)}<span class="nm">${was}${name}</span><span class="meta">${counts}${f.uncommitted ? `<span class="chg-dot" title="Not committed yet" aria-label="Not committed yet"></span>` : ""}</span></a>`;
}
function fileList(groups, { head, on = "f5" } = {}) {
  const tree = groups.map((g) => (g.dir ? `<button class="chg-dir" aria-expanded="true" title="${g.dir}">${ic("chevron-down")}<span>${g.dir}</span></button>` : "")
    + `<div class="chg-group">${g.files.map((f) => fileRow(f, f.id === on)).join("")}</div>`).join("");
  return `<aside class="chg-files" aria-label="Changed files">
  <div class="chg-sum">${head}</div>
  <label class="chg-filter">${ic("search", "sm")}<input class="input" placeholder="Filter files…" aria-label="Filter files"></label>
  <div class="chg-tree">${tree}<div class="chg-nomatch" hidden>No file matches <b></b>. <button class="link" data-clear>Clear the filter</button></div></div>
  <div class="chg-keys"><span><span class="kbd">n</span> <span class="kbd">p</span> next, previous file</span><span><span class="kbd">Alt ↓</span> <span class="kbd">Alt ↑</span> next, previous change</span><span><span class="kbd">[</span> fold this list</span><span><span class="kbd">Esc</span> back to Card</span></div>
</aside>`;
}
const foldListBtn = `<button class="btn ghost sm icon-only" data-foldlist title="Fold the file list ([)" aria-label="Fold the file list">${ic("panel-left-close")}</button>`;
const sumHead = (src, { stat = DS(TOTAL.add, TOTAL.del), files = TOTAL.files, unc = UNCOMMITTED } = {}) =>
  `<div class="src"><span>${src}</span>${foldListBtn}</div><div class="tot">${stat} · ${files} files</div>`
  + (unc ? `<div class="unc"><span class="chg-dot"></span>${unc} files not committed yet</div>` : "");

/* ---------- the frame: the app's own sidebar and main column ---------- */
const sidebar = (on = "tasks") => `<aside class="side" aria-label="Navigation"><div class="side-main">
  <button class="nav-item">${ic("square-pen")}<span>New task</span><span class="meta"><span class="kbd">N</span></span></button>
  <button class="nav-item">${ic("search")}<span>Search</span><span class="meta"><span class="kbd">Ctrl K</span></span></button>
  <a class="nav-item">${ic("inbox")}<span>Inbox</span><span class="meta"><span class="count alert">2</span></span></a>
  <div class="nav-section"><div class="nav-label">Work</div>
    <a class="nav-item">${ic("messages-square")}<span>Chat</span></a>
    <a class="nav-item${on === "tasks" ? " on" : ""}">${ic("list-todo")}<span>Tasks</span></a>
    <a class="nav-item">${ic("building-2")}<span>Clients</span></a>
    <a class="nav-item">${ic("folder-kanban")}<span>Projects</span></a></div>
  <div class="nav-section"><div class="nav-label">Projects<button aria-label="New project">${ic("plus", "sm")}</button></div>
    <a class="nav-item"><span class="dot" style="background:var(--c-blue)"></span><span>Gizai</span></a>
    <a class="nav-item"><span class="dot" style="background:var(--c-orange)"></span><span>Kade portal</span></a></div>
  <div class="nav-section"><div class="nav-label"><span class="fold">Agents</span><button aria-label="Hide agents">${ic("chevron-up", "sm")}</button></div>
    <a class="nav-item">${ic("crown")}<span>Team Lead</span></a>
    <a class="nav-item">${ic("monitor")}<span>Frontend Agent</span></a>
    <a class="nav-item">${ic("server")}<span>Backend Agent</span><span class="meta"><span class="live-tag"><span class="pulse"></span>1</span></span></a>
    <a class="nav-item">${ic("palette")}<span>Design Agent</span></a>
    <a class="nav-item">${ic("flask-conical")}<span>QA Agent</span></a></div>
</div><div class="side-foot"><div class="nav-section"><div class="nav-label">Company</div>
  <a class="nav-item">${ic("chart-column")}<span>Usage</span></a><a class="nav-item">${ic("network")}<span>Team</span></a><a class="nav-item">${ic("settings")}<span>Settings</span></a></div></div></aside>`;
const topbar = (id, project, { props = false } = {}) => `<div class="topbar"><div class="crumbs"><a>Tasks</a><span class="sep">/</span><a>${project}</a><span class="sep">/</span><b class="mono">${id}</b></div>
  <div class="actions"><button class="btn ghost sm" title="Copy the task ID">${ic("copy")}Copy ID</button><button class="btn ghost sm" title="Open the card's worktree in VS Code, Zed or PhpStorm (D2, F2)">${ic("square-code")}Open in${ic("chevron-down", "sm")}</button>${props ? "" : `<button class="btn ghost sm icon-only" title="Show properties (])" aria-label="Show properties">${ic("panel-right")}</button>`}</div></div>`;
const tabs = (open, label) => `<div class="tabs task-tabs" role="tablist" aria-label="The card and its changes">
  <a class="tab" role="tab" aria-selected="${open === "card"}">${ic("square-kanban")}Card</a>
  <a class="tab" role="tab" aria-selected="${open === "changes"}">${ic("file-diff")}Changes${label}</a></div>`;
const frame = (title, body, { h = 900, w = 1440, side = true, note = "" } = {}) => `<figure class="frame-wrap themed" data-theme="dark">
  <figcaption class="frame-cap"><b>${title}</b><span class="theme-name">Dark</span>${note ? `<span>${note}</span>` : ""}</figcaption>
  <div class="frame-fit"><div class="frame" style="width:${w}px;height:${h}px">${side ? `<div class="app">${sidebar()}<main class="main">${body}</main></div>` : `<main class="main solo">${body}</main>`}</div></div>
</figure>`;
const spec = (title, body, cls = "") => `<figure class="spec themed${cls ? " " + cls : ""}" data-theme="dark"><figcaption class="frame-cap"><b>${title}</b><span class="theme-name">Dark</span></figcaption><div class="spec-body">${body}</div></figure>`;

/* ---------- Screen 1: the card ---------- */
const TAB_LABEL = ` ${DS(TOTAL.add, TOTAL.del)}`;
const props = `<aside class="props" aria-label="Properties"><div class="props-head">Properties<button class="btn ghost sm icon-only" aria-label="Hide properties" title="Hide properties (])">${ic("x")}</button></div><div class="props-body">
  <div class="prop-row"><span class="k">Status</span><span class="v">${GLYPH.review}Review</span></div>
  <div class="prop-row"><span class="k">Priority</span><span class="v"><span class="pri medium"><i></i><i></i><i></i></span>Medium</span></div>
  <div class="prop-row"><span class="k">Assignee</span><span class="v"><span class="avatar sm">JS</span>Jeffrey</span></div>
  <div class="prop-row"><span class="k">Labels</span><span class="v"><span class="label-pill"><span class="dot" style="background:var(--c-violet)"></span>code-editor</span></span></div>
  <div class="prop-row"><span class="k">Project</span><span class="v"><span class="dot" style="width:8px;height:8px;border-radius:50%;background:var(--c-blue)"></span>Gizai</span></div>
  <div class="props-sep"></div>
  <div class="prop-row"><span class="k">Branch</span><span class="v mono ellipsis" style="font-size:var(--fs-sm)">gizai/ga-118-editor-the-folding-section</span></div>
  <div class="prop-row"><span class="k">Changes</span><span class="v">${DS(TOTAL.add, TOTAL.del)}<span class="faint" style="font-size:var(--fs-sm)">${TOTAL.files} files</span></span></div>
</div></aside>`;
const runRow = (badge, id, who, rest, commits, when, show = true) => `<div class="run-row run-line"><button class="panel-row">${ic("chevron-right", "sm chev")}${badge}<span class="chip-id">${id}</span><span class="grow">${who} <span class="faint">· ${rest}${commits ? ` · ${commits}` : ""}</span></span>${show ? `<button class="btn ghost sm" title="Only what this run committed">${ic("file-diff")}Show changes</button>` : ""}<span class="faint">${when}</span></button></div>`;
const SCREEN1 = `${topbar("GA-118", "Gizai", { props: true })}${tabs("card", TAB_LABEL)}
<div class="split"><div class="doc"><div class="doc-in">
  <div class="task-head">${GLYPH.review}<span class="pri medium"><i></i><i></i><i></i></span><span class="id">GA-118</span><a class="proj"><span class="dot" style="width:8px;height:8px;border-radius:50%;background:var(--c-blue)"></span>Gizai</a></div>
  <input class="title-input" value="Editor: the folding section in the sidebar" aria-label="Title">
  <section class="block"><div class="block-head"><h3>Description</h3></div><div class="md-click"><div class="prose"><p>A folding <strong>Editor</strong> section between Projects and Agents: the folders opened in the Editor, newest first (at most 8), each with how it stands.</p></div></div></section>
  <section class="block"><div class="block-head"><h3>Acceptance criteria</h3><span class="faint" style="font-size:var(--fs-sm)">QA checks these one by one</span></div><div class="md-click"><div class="prose"><ul><li class="task-list-item"><input type="checkbox" checked disabled>It folds like Agents and Memory, and remembers it.</li><li class="task-list-item"><input type="checkbox" checked disabled>At most 8 folders, newest first.</li></ul></div></div></section>
  <section class="run-card" aria-label="Pull request"><div class="run-head">${ic("git-pull-request")}<span class="title">Pull request</span><a>#94</a><span class="badge info">Open</span><span class="right">${DS(TOTAL.add, TOTAL.del)}<button class="link" style="color:var(--accent)">Show changes</button></span></div>
    <div class="run-summary"><span class="muted">QA passed it. Merge it on GitHub; a merge moves the card to Deploy.</span></div></section>
  <div class="tabs" role="tablist"><button class="tab" role="tab">${ic("message-square")}Comments<span class="n">4</span></button><button class="tab" role="tab">${ic("activity")}Activity</button><button class="tab" role="tab" aria-selected="true">${ic("play")}Runs<span class="n">3</span></button></div>
  <div class="panel">
    ${runRow(`<span class="badge ok">Passed</span>`, "c81e4f2a", "QA Agent", "qa · Auto", "1 commit", "2 h ago")}
    ${runRow(`<span class="badge ok">Ready for testing</span>`, "9d03b7e1", "Frontend Agent", "frontend · Auto", "3 commits", "5 h ago")}
    ${runRow(`<span class="badge fail">Stopped</span>`, "47aa10c3", "Frontend Agent", "frontend · Run", "no commits", "6 h ago", false)}
  </div>
</div></div>${props}</div>`;

/* ---------- Screen 2: Changes ---------- */
const SCREEN2 = `${topbar("GA-118", "Gizai")}${tabs("changes", TAB_LABEL)}
<div class="chg" tabindex="-1">${fileList(G, { head: sumHead(`Since <span class="mono">origin/main</span>`) })}
<div class="chg-diffs">${FILES.map(fileBox).join("\n")}</div></div>`;

/* ---------- Screen 3: one run's changes (KADE-41, a Laravel card) ---------- */
const R = {
  controller: whole(CONTROLLER_PHP, "php", "add"),
  index: diffRows(INDEX_OLD, INDEX_NEW, "blade"),
  web: diffRows(WEB_OLD, WEB_NEW, "php"),
};
const RG = [
  { dir: "app/Http/Controllers", files: [{ id: "k0", path: "app/Http/Controllers/InvoiceExportController.php", status: "A", ...R.controller, body: code(R.controller) }] },
  { dir: "resources/views/invoices", files: [{ id: "k1", path: "resources/views/invoices/index.blade.php", status: "M", ...R.index, body: code(R.index) }] },
  { dir: "routes", files: [{ id: "k2", path: "routes/web.php", status: "M", ...R.web, body: code(R.web) }] },
];
const RFILES = RG.flatMap((g) => g.files);
const RTOTAL = { add: RFILES.reduce((n, f) => n + f.add, 0), del: RFILES.reduce((n, f) => n + f.del, 0) };
const runHead = `<div class="chg-run"><div class="src"><span>${ic("server", "sm")}Run by Backend Agent</span>${foldListBtn}</div>
  <div class="tot">10 Oct 14:02 · 2 commits · ${DS(RTOTAL.add, RTOTAL.del)} · 3 files</div>
  <ul class="chg-commits"><li><span class="mono">4e1c9a2</span>Export invoices as CSV from the invoices page</li><li><span class="mono">b73f0d8</span>Invoices: the empty text reads better</li></ul>
  <button class="btn sm chg-back">${ic("arrow-left")}All changes</button></div>`;
const SCREEN3 = `${topbar("KADE-41", "Kade portal")}${tabs("changes", ` ${DS(1186, 240)}`)}
<div class="chg" tabindex="-1">${fileList(RG, { head: runHead, on: "k1" })}<div class="chg-diffs">${RFILES.map(fileBox).join("\n")}</div></div>`;

/* ---------- states ---------- */
const emptyState = (icon, title, text, action = "") => `<div class="chg-empty"><div class="empty">${ic(icon, "lg")}<b>${title}</b><span>${text}</span>${action}</div></div>`;
const solo = (open, label, body) => `${topbar("GA-121", "Gizai")}${tabs(open, label)}${body}`;
const STATES = [
  ["No branch yet", "A card in Backlog or To do before its first run. The tab shows no counts.",
    solo("changes", "", emptyState("git-branch", "No changes yet", "The first run makes this card's branch.", `<button class="btn">${ic("square-kanban")}Back to Card</button>`))],
  ["No changes", "The branch exists but has nothing against main. The tab says so.",
    solo("changes", `<span class="n">No changes</span>`, emptyState("file-diff", "No changes", "This card's branch has no changes against origin/main."))],
  ["Nothing to find", "Merged before Gizai saved its range, the merge commit isn't here, and gh isn't logged in.",
    solo("changes", "", emptyState("circle-alert", "Gizai can't find what this card changed", "Fetch main, or log gh in (Settings → GitHub).", `<button class="btn">${ic("settings")}Open GitHub settings</button>`))],
  ["A failure", "What went wrong and how to fix it, in the error banner; the list and diffs stay empty.",
    solo("changes", "", `<div class="error-banner" role="alert">${ic("circle-alert")}<span>Couldn't read GA-121's changes: git diff failed (fatal: bad object 3f5c1a2). Fetch main in Gizai's folder for this project, then try again.</span><button class="btn ghost sm">${ic("refresh-cw")}Try again</button></div>`)],
  ["Loading", "The file list comes with the card; each file loads as it nears the screen.",
    solo("changes", "", `<div class="chg chg-small">${`<aside class="chg-files"><div class="chg-sum"><div class="src"><span>Since <span class="mono">origin/main</span></span></div><div class="tot faint">Loading the changed files…</div></div></aside>`}<div class="chg-diffs">${fileBox({ path: "src/components/Sidebar.tsx", status: "M", add: 9, del: 2, body: `<div class="chg-note faint">${ic("loader-circle", "spin")}Loading Sidebar.tsx…</div>` })}${fileBox({ path: "src/lib/sidebar.ts", status: "M", add: 1, del: 1, body: `<div class="chg-note" role="alert" style="color:var(--danger)">${ic("circle-alert")}<span>Couldn't load this file: it changed on disk while Gizai read it.</span><button class="btn ghost sm">${ic("refresh-cw")}Try again</button></div>` })}</div></div>`)],
];

/* ---------- where the changes come from: the list's heading ---------- */
const SOURCES = [
  ["The worktree", "In progress, Testing, Review, a person's card: commits and files not committed yet.", sumHead(`Since <span class="mono">origin/main</span>`, { stat: DS(512, 98), files: 10, unc: 2 })],
  ["No remote", "A project without a link to GitHub or Bitbucket.", sumHead(`Since <span class="mono">main</span>`, { stat: DS(40, 3), files: 3, unc: 0 })],
  ["Branch only", "Its worktree was removed (Settings → Data, or after a merge).", sumHead(`From branch <span class="mono">gizai/ga-12-export-invoices</span> (its worktree was removed)`, { stat: DS(512, 98), files: 10, unc: 0 })],
  ["Merged", "The range Gizai saved at the merge.", sumHead(`As merged into main on 10 Oct (pull request #81)`, { stat: DS(1923, 112), files: 34, unc: 0 })],
  ["Merged earlier", "Before Gizai saved ranges: the merge commit against its first parent.", sumHead(`From the merge commit <span class="mono">3f5c1a2</span> on main`, { stat: DS(964, 112), files: 20, unc: 0 })],
  ["Pull request's diff", "Nothing on this computer; GitHub's patch, without the files around it.", sumHead(`From GitHub's diff of pull request #12 (no file texts: folds can't open)`, { stat: DS(120, 44), files: 6, unc: 0 })],
];
const pullFold = foldBar(new Array(37).fill(""), "mid", true);

/* ---------- parts at their true size ---------- */
const PARTS_LIST = `<div class="chg-files part-list"><div class="chg-tree">
  <button class="chg-dir" aria-expanded="true">${ic("chevron-down")}<span>src/components</span></button>
  ${fileRow({ id: "p0", path: "src/components/Sidebar.tsx", status: "M", add: 9, del: 2 }, false)}
  ${fileRow({ id: "p1", path: "src/components/RunHistory.tsx", status: "M", add: 31, del: 4 }, true)}
  ${fileRow({ id: "p2", path: "src/components/editor/EditorSection.tsx", status: "A", add: 24, del: 0, uncommitted: true })}
  ${fileRow({ id: "p3", path: "src/components/OldPanel.tsx", status: "D", add: 0, del: 80 })}
  ${fileRow({ id: "p4", path: "src/lib/folder_rules.rs", oldPath: "src/lib/folders.rs", status: "R", add: null, del: 0 })}
  ${fileRow({ id: "p5", path: "src/assets/editor-empty.png", status: "M", add: null, del: 0 })}
  ${fileRow({ id: "p6", path: "src/components/AVeryLongComponentNameThatDoesNotFit.tsx", status: "M", add: 1200, del: 310 })}
  <button class="chg-dir" aria-expanded="false">${ic("chevron-right")}<span>crates/gizai-agents/src</span><span class="n">4 files</span></button>
</div></div>`;
const PARTS_HEADERS = [
  { path: "src/components/Sidebar.tsx", status: "M", add: 9, del: 2 },
  { path: "src/components/editor/EditorSection.tsx", status: "A", add: 24, del: 0, uncommitted: true },
  { path: "crates/gizai-agents/src/old_folders.rs", status: "D", add: 0, del: 45, folded: true, why: "Deleted, folded", open: false, body: "x" },
  { path: "crates/gizai-agents/src/changes.rs", oldPath: "crates/gizai-agents/src/diff.rs", status: "R", add: 12, del: 30 },
  { path: "crates/gizai-core/src/folder_rules.rs", oldPath: "crates/gizai-core/src/folders.rs", status: "R", add: null, del: 0, stat: "", why: "Renamed, no changes" },
  { path: "scripts/deploy.sh", status: "T", add: null, del: 0, stat: "", why: "Now a link to tools/deploy.sh" },
  { path: "package-lock.json", status: "M", add: 412, del: 20, folded: true, why: "Lock file, folded", body: "x" },
].map((f) => fileBox({ ...f, body: f.body === "x" ? " " : undefined })).join("");

/* ================================================================================================================
   Tables: the tokens with their values and contrast.
   ================================================================================================================ */
const fx = (n) => n.toFixed(2);
const cell = (tok, th) => {
  const v = valOf(tok, th), alias = typeof tok.value === "string" ? tok.value.slice(1, -1) : null;
  const raw = typeof tok.value === "string" ? tok.value : tok.value[th];
  const isText = tok.text || tok.name.startsWith("syn-");
  const sample = isText ? `<span class="sw-text" style="color:var(--${tok.name})">Aa</span>` : `<span class="sw-fill" style="--fill: var(--${tok.name})"></span>`;
  const r = isText ? ratios(v, th) : null;
  return `<td class="sw" data-theme="${th}">${sample}</td><td><code>${alias ? "= " + alias : raw}</code>${alias && !tok.text ? `<span class="faint"> ${v}</span>` : ""}${r ? `<div class="cr">${fx(r.bg)} bg · ${fx(r.raised)} raised</div><div class="cr faint">${fx(r.addSoft)} / ${fx(r.delSoft)} on changed lines · ${fx(r.addStrong)} / ${fx(r.delStrong)} on changed words</div>` : ""}</td>`;
};
const tokenTable = (list) => `<table class="tok-table"><thead><tr><th>Token</th><th>For</th><th colspan="2">Dark</th><th colspan="2">Light</th></tr></thead><tbody>
${list.map((tok) => `<tr><td><code>--${tok.name}</code></td><td>${esc(tok.usage)}${tok.from ? `<div class="faint">From ${tok.from}.</div>` : ""}</td>${cell(tok, "dark")}${cell(tok, "light")}</tr>`).join("\n")}
</tbody></table>`;
const mapTable = `<table class="tok-table map"><thead><tr><th>Token</th><th>Lezer tags (src/lib/codeTheme.ts)</th></tr></thead><tbody>${MAP.map((m) => `<tr><td><code class="s-${m.cls}">--syn-${m.token}</code></td><td><code>${m.lezer}</code></td></tr>`).join("")}</tbody></table>`;

/* ---------- the CSS: the new tokens (as F0 will add them) and the Changes view (as F1 will build it) ---------- */
const varLine = (tok, th) => {
  const v = typeof tok.value === "string" ? tok.value : tok.value[th];
  return `--${tok.name}: ${v.startsWith("{") ? `var(--${v.slice(1, -1)})` : v};`;
};
const TOKENS_CSS = `/* The new tokens, as F0 adds them to design/tokens.json (new-tokens.json) and src/styles/tokens.css. */
:root, [data-theme="dark"] { ${ALL.filter((x) => typeof x.value !== "string").map((x) => varLine(x, "dark")).join(" ")} }
[data-theme="light"] { ${ALL.filter((x) => typeof x.value !== "string").map((x) => varLine(x, "light")).join(" ")} }
/* Aliases. In the app they sit on :root with the rest; this page also themes frames inside the page, so every themed
   element works them out again (a var() alias inherits as the value it had where it was declared). */
:root, [data-theme] { ${ALL.filter((x) => typeof x.value === "string").map((x) => varLine(x, "dark")).join(" ")}
  --st-review: var(--needs); --st-done: var(--success); --st-blocked: var(--danger); --pr-urgent: var(--danger); }`;


const TEMPLATE = fs.readFileSync(path.join(HERE, "changes.template.html"), "utf8");

const sampleBlock = (s) => {
  const L = lines(s.src, s.lang);
  return `<figure class="spec themed sample" data-theme="dark"><figcaption class="frame-cap"><b>${s.label}</b><span class="mono faint">${s.file}</span><span class="theme-name">Dark</span></figcaption>
<div class="chg-box"><div class="chg-code" style="--digits: 2"><div class="chg-lines">${L.map((l, i) => row("", i + 1, l)).join("")}</div></div></div></figure>`;
};

const parts = {
  TOKENS_CSS,
  SCREEN1: frame("Screen 1 · The card, with Card | Changes at the top", SCREEN1, { h: 900, note: "GA-118 in Review: the card as it is today, under the new tabs." }),
  SCREEN2: frame("Screen 2 · Changes", SCREEN2, { h: 900, note: "Every file below each other. Scroll inside the frame; the carets, folds and Show buttons work." }),
  SCREEN3: frame("Screen 3 · One run's changes", SCREEN3, { h: 900, note: "From the Runs tab: Show changes on a run. KADE-41, a Laravel card." }),
  STATES: STATES.map(([title, sub, body]) => frame(title, body, { h: 420, w: 1192, side: false, note: sub })).join("\n"),
  TAB_VARIANTS: spec("The Changes tab's label", `<div class="tabs task-tabs">${[
    [true, TAB_LABEL, "Counts: added and removed lines"], [false, `<span class="n">No changes</span>`, "A branch with nothing against main"],
    [false, "", "No branch yet, or still loading"], [false, ` ${DS(18402, 0)}`, "Large counts keep their thousands separator"],
  ].map(([on, l, why]) => `<span class="tab-demo"><a class="tab" aria-selected="${on}">${ic("file-diff")}Changes${l}</a><span class="faint">${why}</span></span>`).join("")}</div>`),
  SOURCES: SOURCES.map(([title, sub, head]) => spec(title, `<div class="chg-files part-head"><div class="chg-sum">${head}</div></div><p class="faint spec-sub">${sub}</p>`, "small")).join(""),
  PULL_FOLD: spec("A fold that can't open (the pull request's diff)", `<div class="chg-box"><div class="chg-code" style="--digits: 2"><div class="chg-lines">${pullFold}</div></div></div>`),
  PARTS_LIST: spec("File list rows", PARTS_LIST + `<p class="faint spec-sub">Hover, the current file (selected), a file not committed yet (the dot), deleted, renamed, binary, a long name, a folded folder with its count.</p>`),
  PARTS_HEADERS: spec("File headers", `<div class="part-headers">${PARTS_HEADERS}</div>`),
  DIFF_TABLE: tokenTable(DIFF), SYN_TABLE: tokenTable(SYN), MAP_TABLE: mapTable,
  SAMPLES: SAMPLES.map(sampleBlock).join("\n"),
  TOTAL_ADD: TOTAL.add.toLocaleString("en-GB"), TOTAL_DEL: TOTAL.del.toLocaleString("en-GB"), TOTAL_FILES: String(TOTAL.files),
};
let page = TEMPLATE.replace(/\{\{(\w+)\}\}/g, (m, k) => (k in parts ? parts[k] : m));
page = page.replace("{{SPRITE}}", await sprite());
fs.writeFileSync(path.join(HERE, "changes.html"), page);

/* ---------- new-tokens.json: the entries for design/tokens.json's color.tokens, in its format ---------- */
fs.writeFileSync(path.join(HERE, "new-tokens.json"), JSON.stringify({
  note: "GA-104 (Code Editor D1): the new tokens for design/tokens.json's color.tokens, in its format. F0 adds them after c-grey and runs node scripts/tokens-css.ts.",
  tokens: ALL.map(({ name, value, usage }) => ({ name, value, usage })),
}, null, 2).replace(/\{\n\s+"dark": ("[^"]+"),\n\s+"light": ("[^"]+")\n\s+\}/g, "{ \"dark\": $1, \"light\": $2 }") + "\n");

/* ---------- the README's tables ---------- */
const mdRow = (tok) => {
  const c = (th) => { const v = valOf(tok, th); const alias = typeof tok.value === "string" ? `= ${tok.value.slice(1, -1)}` : `\`${v}\``;
    if (!(tok.text || tok.name.startsWith("syn-"))) return alias;
    const r = ratios(v, th); return `${alias} · ${fx(r.bg)} / ${fx(r.raised)} · ${fx(r.addSoft)} / ${fx(r.delSoft)} · ${fx(r.addStrong)} / ${fx(r.delStrong)}`; };
  return `| \`--${tok.name}\` | ${c("dark")} | ${c("light")} |`;
};
const MD_TABLES = `| Token | Dark | Light |\n|---|---|---|\n${ALL.map(mdRow).join("\n")}`;
const readme = fs.readFileSync(path.join(HERE, "README.md"), "utf8");
const start = "<!-- tables: generated by gen-changes.mjs -->", end = "<!-- /tables -->";
const a = readme.indexOf(start), b = readme.indexOf(end);
if (a >= 0 && b > a) fs.writeFileSync(path.join(HERE, "README.md"), readme.slice(0, a + start.length) + "\n" + MD_TABLES + "\n" + readme.slice(b));
console.log(`wrote changes.html, new-tokens.json and README.md's tables · Screen 2: +${TOTAL.add} −${TOTAL.del} in ${TOTAL.files} files · icons: ${used.size}`);
