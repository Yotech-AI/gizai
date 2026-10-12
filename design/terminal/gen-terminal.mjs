// Builds design/terminal/terminal.html: the design of the terminal (GA-132, TD of GA-124's research, docs/research/terminal.md
// §5.1 screens, §5.2 keys, §5.4 colours and the font) as one page that opens from disk in a browser. It follows D2's
// design/code-editor/gen-editor.mjs: the screens are drawn with the app's own stylesheets (src/styles), the new components
// in terminal.css and the new tokens in terminal-tokens.css, every screen in dark and then light. Icons are Lucide's own
// data (lucide-react, ISC). The code above the Editor's panel is coloured by @lezer/javascript with the app's syn-* tokens.
//
// The terminal's colours live in COLORS below, once: this file checks them (contrast on term-bg, cyan and magenta apart
// from --live and --needs) before it writes anything, then writes new-tokens.json (design/tokens.json's format, for TF0),
// terminal-tokens.css (the same tokens for this page) and the tables in README.md. Every text the screens show is in X,
// in the page's order: the frames use it and the table "Every text" lists it.
// usage: node design/terminal/gen-terminal.mjs
import fs from "node:fs";
import path from "node:path";
import { parser as jsParser } from "@lezer/javascript";
import { highlightTree, tagHighlighter, tags as t } from "@lezer/highlight";

const HERE = path.dirname(new URL(import.meta.url).pathname);
const GZ = path.resolve(HERE, "../..");
const LUCIDE = `${GZ}/node_modules/lucide-react/dist/esm/icons`;

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const q = (s) => esc(s).replace(/"/g, "&quot;");
const plain = (html) => String(html).replace(/<[^>]+>/g, "");

// ---------- icons: Lucide's own data, as one sprite of the icons the page uses ----------
const used = new Set();
const ic = (name, cls = "icon") => { used.add(name); return `<svg class="${cls}" aria-hidden="true"><use href="#i-${name}"/></svg>`; };
const sm = (name, more = "") => ic(name, `icon sm${more ? ` ${more}` : ""}`);
async function iconNode(name) {
  const file = `${LUCIDE}/${name}.js`;
  if (!fs.existsSync(file)) throw new Error(`no Lucide icon ${name}`);
  const m = await import(file);
  if (m.__iconNode) return m.__iconNode;
  const alias = fs.readFileSync(file, "utf8").match(/from '\.\/(.+?)\.js'/);
  if (alias) return iconNode(alias[1]);
  throw new Error(`no icon data in ${name}`);
}
const attrs = (o) => Object.entries(o).filter(([k]) => k !== "key").map(([k, v]) => `${k.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase())}="${v}"`).join(" ");
async function sprite() {
  const out = [];
  for (const name of [...used].sort()) out.push(`<symbol id="i-${name}" viewBox="0 0 24 24">${(await iconNode(name)).map(([tag, a]) => `<${tag} ${attrs(a)}/>`).join("")}</symbol>`);
  return out.join("\n");
}

// Status glyphs: Gizai's own, the paths of src/components/StatusIcon.tsx (the picker's Cards group shows a card's column).
const GLYPH = {
  in_progress: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><path d="M8 4.25a3.75 3.75 0 0 1 0 7.5z" fill="currentColor"/>',
  testing: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><path d="M8 8V4.25a3.75 3.75 0 1 1-3.75 3.75z" fill="currentColor"/>',
};
const ST_NAME = { in_progress: "In progress", testing: "Testing" };
const st = (cat) => `<span class="st"><svg class="st st-${cat}" viewBox="0 0 16 16" fill="none" role="img" aria-label="${ST_NAME[cat]}" style="width:16px;height:16px">${GLYPH[cat]}</svg></span>`;
const kbd = (k) => `<span class="kbd">${k}</span>`;

// ---------- every text: where, when, the text and its buttons. The frames use these; the last table lists them. ----------
const TEXTS = [];
const say = (where, when, text, buttons = "") => { TEXTS.push({ where, when, text, buttons }); return text; };
const X = {
  // 1. the sidebar
  sideHead: say("Sidebar", "Always, between Projects and Editor", "Terminal", "+ (tooltip: New terminal) · caret (Fold Terminal, Show Terminal)"),
  sideFolded: say("Sidebar", "Folded, while terminals are open", "3 open", "Tooltip: 3 terminals are open"),
  sideEmpty: say("Sidebar", "No terminal open", "New terminal", "Opens the folder picker, as + does"),
  sideBusy: say("Sidebar row", "A program runs in it", "npm", "The program in the foreground, in mono: npm, cargo, php, vim. Tooltip: ~/Herd/gizai · zsh · running npm"),
  sideIdle: say("Sidebar row", "The shell waits for a command", "", "Nothing on the right. Tooltip: ~/Herd/gizai · zsh · waiting for a command"),
  sideEnded: say("Sidebar row", "The shell ended with another code than 0", "ended", "Tooltip: KADE-41's worktree · zsh ended with exit code 2"),
  sideMenu: say("Sidebar row, right click", "Always", "Rename · Show in Editor · Copy folder path · Close", "Rename shows F2; Close asks first when a program runs"),
  sideAsk: say("Sidebar row, Close or a middle click", "A program runs in it", "npm is still running in gizai 2. Close it and end npm?", "Close · Cancel"),
  sideRename: say("Sidebar row, F2", "Renaming", "gizai 2", "The name, selected, in a box in its place. Enter keeps it, Escape cancels; an empty name gives gizai 2 back; at most 40 characters"),
  // 2. New terminal
  pickTitle: say("Folder picker", "From + on the Terminal heading, or New terminal", "New terminal in", "Its title, above the search box"),
  pickFind: say("Folder picker", "Always", "Find a folder", "Placeholder; Enter starts a terminal in the chosen folder and opens its page"),
  pickGroups: say("Folder picker", "Group heads", "Projects · In ~/Herd · Cards", "The Editor's groups; the folder the page shows is on top, chosen"),
  pickMine: say("Folder picker, Cards", "A card pinned to you", "by hand"),
  pickLive: say("Folder picker, Cards", "An agent works on the card", "QA Agent is working", "With the pulse; off. Tooltip: QA Agent is working on GA-118: a terminal there waits until its run ends."),
  pickNotMine: say("Folder picker, Cards", "Another's card, chosen", "GA-112 is Frontend Agent's card: a shell there can change its files, as an edit can.", "Edit anyway · Cancel"),
  pickMany: say("Folder picker", "20 terminals or more are open", "20 terminals are open: each keeps its shell running until you close it.", "The number is how many are open: 21, 22, …"),
  pickNone: say("Folder picker", "The search finds nothing", "No folder found"),
  pickFoot: say("Folder picker", "Always, at the bottom", "Projects' folders, the folders in ~/Herd and card worktrees. More places: Settings → General → Editor folders.", "The Editor's own footer"),
  // 3. the Terminal page
  pageCrumbs: say("Terminal page, top bar", "Always", "Terminal / gizai 2", "Double-click or F2 on the name renames it in place"),
  pageWhere: say("Terminal page, top bar", "Always", "~/Herd/gizai · zsh · 132 × 38", "The folder in mono; the shell and its size. Tooltip of the size: 132 columns, 38 rows"),
  showInEditor: say("Terminal page, top bar", "Always", "Show in Editor", "Tooltip: Show gizai 2 in gizai's Editor, in its terminal panel"),
  pageMore: say("Terminal page, top bar → ⋯", "Always", "Rename · Find · Clear · Copy folder path · Close", "Tooltip of ⋯: More. Rename F2, Find Ctrl+Shift+F (Cmd+F), Clear empties the scrollback"),
  tabTip: say("Terminal tab", "Always", "gizai 2", "Tooltip: gizai 2 · running npm. × Close (a middle click closes too). Double-click renames"),
  tabPlus: say("Terminal tabs", "After the last tab", "+", "Tooltip: New terminal in gizai (Ctrl+Shift+`)"),
  tabAsk: say("Terminal tab, × or a middle click", "A program runs in it", "npm is still running in gizai 2. Close it and end npm?", "Cancel · Close"),
  findBox: say("Find (Ctrl+Shift+F, Cmd+F)", "Its box", "Find", "Placeholder. Toggles: Match case · Words · Regular expression"),
  findCount: say("Find", "After the box", "3 of 12", "No results · Not a valid regular expression"),
  findButtons: say("Find", "Its buttons", "Previous match · Next match · Close", "Tooltips: Previous match (Shift+Enter) · Next match (Enter) · Close (Escape)"),
  ended: say("Terminal bar, under the tabs", "The shell ended with another code than 0", "The shell ended with exit code 2.", "Start again · Close"),
  endedSignal: say("Terminal bar", "A signal ended the shell", "The shell was ended by a signal.", "Start again · Close"),
  endedTab: say("Terminal tab and sidebar row", "The shell ended", "ended"),
  failed: say("Terminal bar", "The shell couldn't start", "Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found.", "Try again · Close"),
  emptyTitle: say("Terminal page", "#/terminal, no terminal open", "No terminal open", "The empty state's heading"),
  empty: say("Terminal page", "#/terminal, no terminal open", "Start a shell in a project's folder, a folder in ~/Herd or a card's worktree.", "New terminal"),
  gone: say("Terminal page", "#/terminal/<id> of a terminal that was closed", "This terminal isn't open any more.", "New terminal"),
  // 4. the Editor's panel
  btn: say("Editor, top bar", "Between Fetch and Open in; the panel hidden", "Terminal", "Tooltip: Show the terminal (Ctrl+`). With no terminal in the folder yet, it starts one in its root"),
  btnOn: say("Editor, top bar", "The panel shows (pressed)", "Terminal", "Tooltip: Hide the terminal (Ctrl+`)"),
  btnLive: say("Editor, top bar", "An agent works on the card; no terminal there yet", "Terminal", "Off. Tooltip: QA Agent is working on GA-118: a terminal there waits until its run ends."),
  btnNotMine: say("Editor, top bar", "Another's card, before Edit anyway", "Terminal", "Off. Tooltip: GA-112 is Frontend Agent's card: press Edit anyway first, since a shell can change its files."),
  ownPage: say("Panel, right of its tabs", "Always", "Own page", "Tooltip: Open gizai on its own page"),
  hide: say("Panel, right of its tabs", "Always", "Hide", "Tooltip: Hide the terminal (Ctrl+`): its shells keep running"),
  handle: say("Panel, its top border", "The drag handle", "Terminal panel's height", "Its name for screen readers, with the share (30%). Drag; double-click: 30% again; ↑ and ↓ move it 5%"),
  plusOff: say("Panel, + after its tabs", "An agent works on the card", "+", "Off. Tooltip: QA Agent is working on GA-118: a terminal there waits until its run ends."),
  // 5. one terminal, two places
  awayPage: say("Panel or page", "The other place shows the terminal", "gizai 2 is on its own page.", "Show here"),
  awayPanel: say("Terminal page", "The Editor's panel shows the terminal", "gizai 2 is in gizai's Editor.", "Show here"),
  awayTab: say("Panel tab", "Its terminal is on its own page", "gizai 2", "Its icon is the one of Own page. Tooltip: gizai 2 is on its own page."),
};

// ---------- the terminal's colours: one list, checked, then written to new-tokens.json, terminal-tokens.css and README.md ----
// A value is a hex colour, or "{token}" for another token (the app's, or one of these), as in design/tokens.json.
const LIVE_WHY = "QA Agent is working on GA-118: a terminal there waits until its run ends.";
const NOTMINE_WHY = "GA-112 is Frontend Agent's card: press Edit anyway first, since a shell can change its files.";
const COLORS = [
  ["term-fg", "{text}", "{text}", "Text a program prints without a colour, and the prompt; as the code's text."],
  ["term-bg", "{bg}", "{bg}", "The terminal's background: the code's, so the Editor's panel and the code above it are one surface."],
  ["term-cursor", "{text}", "{text}", "The cursor: a block while the terminal has the focus, its outline while it hasn't; it doesn't blink."],
  ["term-cursor-text", "{bg}", "{bg}", "The character under the block cursor."],
  ["term-selection", "#283350", "#dae1f9", "The selection, and find's matches (the current one with an accent border): accent at 26% (dark) and 18% (light) on bg, opaque so xterm.js's search can use it."],
  ["term-selection-inactive", "{selected}", "{selected}", "The selection while the terminal hasn't the focus."],
  ["term-black", "#30333a", "#24272e", "ANSI 0. Dark: a block a little above the background (ls's fifos, htop's bars), and black text on coloured blocks; as text on the background xterm.js lifts it to 4.5:1."],
  ["term-red", "{diff-del}", "{diff-del}", "ANSI 1: errors, failed tests, git's removed lines and unstaged files. diff-del's value, so git diff reads as the Changes view."],
  ["term-green", "{diff-add}", "{diff-add}", "ANSI 2: success, passed tests, git's added lines and staged files. diff-add's value."],
  ["term-yellow", "#e3bd3c", "#855600", "ANSI 3: warnings, git's commit hashes. The gold of syn-type."],
  ["term-blue", "#85adff", "#1e54c4", "ANSI 4: folders in ls, rustc's arrows and line numbers. The blue of syn-function."],
  ["term-magenta", "#c58af9", "#8a3cc2", "ANSI 5: images in ls. A violet: at least 35° of hue from needs (magenta, “needs you”)."],
  ["term-cyan", "#56b8f0", "#0a6c9c", "ANSI 6: symbolic links in ls, git's @@ lines, URLs. A sky blue: at least 35° of hue from live (teal, an agent at work)."],
  ["term-white", "#c4c7ce", "#d0d3da", "ANSI 7: light grey. In dark, text a little dimmer than term-fg; in light, white text on coloured blocks, and lifted to 4.5:1 when it is text on the background."],
  ["term-bright-black", "#7f838d", "#636772", "ANSI 8: grey text, such as hints, a shell's suggestions and dim lines. text-3's value."],
  ["term-bright-red", "#ff9590", "#962420", "ANSI 9: bold red (xterm.js draws bold text in the bright colours)."],
  ["term-bright-green", "#72dca6", "#0b5a36", "ANSI 10: bold green, such as executables in ls and cargo's Compiling."],
  ["term-bright-yellow", "#f5d56b", "#6a4400", "ANSI 11: bold yellow, such as rustc's warning."],
  ["term-bright-blue", "#a9c5ff", "#1440a0", "ANSI 12: bold blue, such as folders in ls."],
  ["term-bright-magenta", "#d6a7ff", "#6f22a6", "ANSI 13: bold magenta. As apart from needs as term-magenta."],
  ["term-bright-cyan", "#86d1f7", "#06577f", "ANSI 14: bold cyan, such as symbolic links in ls. As apart from live as term-cyan."],
  ["term-bright-white", "#fafafb", "#f3f4f7", "ANSI 15: the brightest white in dark; in light a pale block, lifted to 4.5:1 as text."],
];
// Bright colours are the stronger ones on each background: lighter in dark, darker in light (as GitHub's light terminal
// theme does), so that every colour a program prints as text keeps 4.5:1. The greys keep their usual order.
const BACKGROUND_ONLY = { dark: ["term-black"], light: ["term-white", "term-bright-white"] };
const APART = ["term-cyan", "term-bright-cyan", "term-magenta", "term-bright-magenta"];
const MIN_HUE = 35, MIN_DE = 15;
const SIZES = [
  ["size-term-tab", "32px", "The terminal tabs' strip, on the Terminal page and in the Editor's panel (28px compact)."],
  ["size-term-panel", "30%", "The Editor's terminal panel the first time: a share of the code column (.ed-main), kept as a share."],
  ["size-term-panel-min", "120px", "The panel's least height: its tabs (32px) and 4 lines (4 × 20px), with 4px above and below."],
  ["size-term-code-min", "120px", "The code's least height while the panel shows (6 lines)."],
  ["size-handle", "6px", "The drag handle's grab area, centred on the panel's 1px top border."],
];
const SIZE_CSS = {
  "size-term-tab": "calc(32px + var(--ui-box, 0px))",
  "size-term-panel": "30%",
  "size-term-panel-min": "calc(120px + var(--ui-box, 0px) + 4 * 1.6 * var(--ui-grow, 0px))",
  "size-term-code-min": "120px",
  "size-handle": "6px",
};

function themeVars(file) {
  const css = fs.readFileSync(file, "utf8");
  const block = (sel) => { const i = css.indexOf(sel); return css.slice(css.indexOf("{", i) + 1, css.indexOf("}", i)); };
  const vars = (text) => Object.fromEntries([...text.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
  return { dark: vars(block(':root, [data-theme="dark"] {')), light: vars(block('[data-theme="light"] {')) };
}
const APP = themeVars(`${GZ}/src/styles/tokens.css`);
const OURS = { dark: Object.fromEntries(COLORS.map(([n, d]) => [n, d])), light: Object.fromEntries(COLORS.map(([n, , l]) => [n, l])) };
/** A token's colour in a theme, following "{token}" and var(--token) to the hex. */
function hexOf(theme, v) {
  for (let i = 0; i < 10; i++) {
    const m = v.match(/^\{([\w-]+)\}$/) ?? v.match(/^var\(--([\w-]+)\)$/);
    if (!m) return v;
    v = OURS[theme][m[1]] ?? APP[theme][m[1]] ?? APP.dark[m[1]];
    if (!v) throw new Error(`no token ${m[1]}`);
  }
  throw new Error("an alias loops");
}
const hex2rgb = (h) => { const n = parseInt(h.slice(1), 16); return [n >> 16, (n >> 8) & 255, n & 255]; };
const lin = (v) => { v /= 255; return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
const lum = (h) => { const [r, g, b] = hex2rgb(h); return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b); };
const contrast = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, s) => s - p); return (x + 0.05) / (y + 0.05); };
/** OKLCH hue in degrees (Björn Ottosson's OKLab). */
function hue(h) {
  const [r, g, b] = hex2rgb(h).map(lin);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return ((Math.atan2(B, A) * 180) / Math.PI + 360) % 360;
}
const hueGap = (a, b) => { const d = Math.abs(hue(a) - hue(b)) % 360; return d > 180 ? 360 - d : d; };
function lab(h) {
  const [r, g, b] = hex2rgb(h).map(lin);
  const f = (x) => (x > 216 / 24389 ? Math.cbrt(x) : (24389 / 27 * x + 16) / 116);
  const [X, Y, Z] = [f((0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047), f(0.2126 * r + 0.7152 * g + 0.0722 * b), f((0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883)];
  return [116 * Y - 16, 500 * (X - Y), 200 * (Y - Z)];
}
/** CIEDE2000 colour difference. */
function deltaE(h1, h2) {
  const [L1, a1, b1] = lab(h1), [L2, a2, b2] = lab(h2), rad = Math.PI / 180, deg = 180 / Math.PI;
  const Cb = (Math.hypot(a1, b1) + Math.hypot(a2, b2)) / 2, G = 0.5 * (1 - Math.sqrt(Cb ** 7 / (Cb ** 7 + 25 ** 7)));
  const a1p = (1 + G) * a1, a2p = (1 + G) * a2, C1 = Math.hypot(a1p, b1), C2 = Math.hypot(a2p, b2);
  const hp1 = (Math.atan2(b1, a1p) * deg + 360) % 360, hp2 = (Math.atan2(b2, a2p) * deg + 360) % 360;
  let dh = hp2 - hp1; if (C1 * C2 === 0) dh = 0; else if (dh > 180) dh -= 360; else if (dh < -180) dh += 360;
  const dH = 2 * Math.sqrt(C1 * C2) * Math.sin((dh / 2) * rad), Lb = (L1 + L2) / 2, Cbp = (C1 + C2) / 2;
  let hb = hp1 + hp2; if (C1 * C2 !== 0) { if (Math.abs(hp1 - hp2) > 180) hb += hp1 + hp2 < 360 ? 360 : -360; hb /= 2; }
  const T = 1 - 0.17 * Math.cos((hb - 30) * rad) + 0.24 * Math.cos(2 * hb * rad) + 0.32 * Math.cos((3 * hb + 6) * rad) - 0.2 * Math.cos((4 * hb - 63) * rad);
  const Rc = 2 * Math.sqrt(Cbp ** 7 / (Cbp ** 7 + 25 ** 7)), dTh = 30 * Math.exp(-(((hb - 275) / 25) ** 2));
  const Sl = 1 + (0.015 * (Lb - 50) ** 2) / Math.sqrt(20 + (Lb - 50) ** 2), Sc = 1 + 0.045 * Cbp, Sh = 1 + 0.015 * Cbp * T;
  const dC = C2 - C1, dL = L2 - L1;
  return Math.sqrt((dL / Sl) ** 2 + (dC / Sc) ** 2 + (dH / Sh) ** 2 - Math.sin(2 * dTh * rad) * Rc * (dC / Sc) * (dH / Sh));
}

// The checks: the page isn't written when one fails.
const problems = [];
for (const theme of ["dark", "light"]) {
  const bg = hexOf(theme, "{term-bg}");
  if (bg !== hexOf(theme, "{bg}")) problems.push(`${theme}: term-bg isn't the code's background (bg)`);
  for (const [name] of COLORS) {
    if (["term-bg", "term-cursor-text", "term-selection", "term-selection-inactive"].includes(name) || BACKGROUND_ONLY[theme].includes(name)) continue;
    const c = contrast(hexOf(theme, `{${name}}`), bg);
    if (c < 4.5) problems.push(`${theme}: ${name} is ${c.toFixed(2)}:1 on term-bg, under 4.5:1`);
  }
  for (const name of APART) for (const state of ["live", "needs"]) {
    const a = hexOf(theme, `{${name}}`), b = hexOf(theme, `{${state}}`);
    if (hueGap(a, b) < MIN_HUE || deltaE(a, b) < MIN_DE) problems.push(`${theme}: ${name} is too near ${state} (${hueGap(a, b).toFixed(0)}°, ΔE ${deltaE(a, b).toFixed(1)})`);
  }
}
if (problems.length) { console.error(`The colours break the card's rules:\n- ${problems.join("\n- ")}`); process.exit(1); }

// new-tokens.json: tokens.json's format, for TF0 to add after git-deleted (colours) and size-statusbar (sizes)
const tokenValue = (d, l) => (d === l && d.startsWith("{") ? d : { dark: d, light: l });
fs.writeFileSync(`${HERE}/new-tokens.json`, JSON.stringify({
  note: "GA-132 (the terminal's design, TD): the new tokens for design/tokens.json, in its format. TF0 adds the colours to color.tokens after git-deleted and the sizes to size.tokens after size-statusbar, runs node scripts/tokens-css.ts, and gives size-term-tab and size-term-panel-min their growth in src/styles/appearance.css (terminal-tokens.css shows how). Made by gen-terminal.mjs.",
  color: { tokens: COLORS.map(([name, d, l, usage]) => ({ name, value: tokenValue(d, l), usage })) },
  size: { tokens: SIZES.map(([name, value, usage]) => ({ name, value, usage })) },
}, null, 2) + "\n");
// terminal-tokens.css: the same tokens for this page, each alias declared in both themes so a frame resolves its own
const cssValue = (v) => v.replace(/^\{([\w-]+)\}$/, "var(--$1)");
fs.writeFileSync(`${HERE}/terminal-tokens.css`, `/* The terminal's tokens (GA-132, TD), until TF0 puts them in design/tokens.json and src/styles/tokens.css. Made by
   gen-terminal.mjs from the same list as new-tokens.json: change them there. An alias is declared in each theme, so a frame
   of this page in the other theme resolves it to its own colour. This file comes before tokens.css, so the app's own
   tokens win once they have these names. */

:root, [data-theme="dark"] {
${COLORS.map(([n, d]) => `  --${n}: ${cssValue(d)};`).join("\n")}
}

[data-theme="light"] {
${COLORS.map(([n, , l]) => `  --${n}: ${cssValue(l)};`).join("\n")}
}

:root {
  /* the tab strip grows with the density (--ui-box) like size-tab; the panel's least height keeps its 4 lines as the
     Interface text grows */
${SIZES.map(([n]) => `  --${n}: ${SIZE_CSS[n]};`).join("\n")}
}
`);

// ---------- the code above the Editor's panel: Sidebar.tsx with the Terminal section added, coloured as CodeMirror does ----------
const HL = tagHighlighter([
  { tag: [t.keyword, t.controlKeyword, t.definitionKeyword, t.moduleKeyword, t.operatorKeyword, t.modifier, t.self], class: "syn-keyword" },
  { tag: [t.string, t.special(t.string), t.regexp, t.escape, t.docString, t.attributeValue], class: "syn-string" },
  { tag: [t.number, t.integer, t.float, t.bool, t.null, t.atom, t.unit], class: "syn-number" },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], class: "syn-comment" },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.function(t.definition(t.variableName))], class: "syn-function" },
  { tag: [t.typeName, t.className, t.namespace], class: "syn-type" },
  { tag: [t.variableName, t.definition(t.variableName), t.labelName], class: "syn-variable" },
  { tag: [t.propertyName, t.definition(t.propertyName)], class: "syn-property" },
  { tag: [t.tagName], class: "syn-tag" },
  { tag: [t.attributeName], class: "syn-attribute" },
  { tag: [t.operator, t.arithmeticOperator, t.logicOperator, t.compareOperator, t.updateOperator, t.definitionOperator, t.typeOperator, t.controlOperator], class: "syn-operator" },
  { tag: [t.punctuation, t.separator, t.bracket, t.angleBracket, t.squareBracket, t.paren, t.brace, t.derefOperator], class: "syn-punctuation" },
  { tag: [t.meta, t.documentMeta, t.annotation, t.processingInstruction, t.macroName], class: "syn-meta" },
]);
const SIDEBAR_SRC = (() => {
  const lines = fs.readFileSync(`${GZ}/src/components/Sidebar.tsx`, "utf8").split("\n");
  const at = lines.findIndex((l) => l.includes("<EditorSection route={route}"));
  if (at < 0) throw new Error("Sidebar.tsx has no EditorSection line");
  lines.splice(at, 0, '      <TerminalSection route={route} folded={folded("terminal")} onFold={() => fold("terminal")} />', "");
  return lines.join("\n");
})();
const SIDEBAR_CLS = (() => {
  const cls = new Array(SIDEBAR_SRC.length).fill(null);
  highlightTree(jsParser.configure({ dialect: "jsx ts" }).parse(SIDEBAR_SRC), HL, (from, to, c) => { for (let i = from; i < to; i++) cls[i] = c; });
  return cls;
})();
const SIDEBAR_LINE = SIDEBAR_SRC.split("\n").findIndex((l) => l.includes("<TerminalSection")) + 1;
function codeLines(from, to) {
  const out = [];
  let start = 0;
  SIDEBAR_SRC.split("\n").forEach((text, n) => {
    if (n + 1 >= from && n + 1 <= to) out.push({ n: n + 1, text, cls: SIDEBAR_CLS.slice(start, start + text.length) });
    start += text.length + 1;
  });
  return out;
}
const runs = (text, cls) => {
  let html = "";
  for (let i = 0; i < text.length;) { let j = i; while (j < text.length && cls[j] === cls[i]) j++; html += cls[i] ? `<span class="${cls[i]}">${esc(text.slice(i, j))}</span>` : esc(text.slice(i, j)); i = j; }
  return html;
};
/** The code: a still picture of CodeMirror with the code theme, `lines` of it from `first`, the cursor on `active`. */
const code = ({ first = SIDEBAR_LINE - 14, lines = 40, active = SIDEBAR_LINE } = {}) => `<div class="ed-code" role="textbox" aria-multiline="true" aria-label="Code">${codeLines(first, first + lines - 1).map((l) =>
  `<div class="ln${l.n === active ? " active" : ""}"><span class="no">${l.n}</span><span class="fd"></span><span class="tx">${runs(l.text, l.cls)}${l.n === active ? '<span class="cursor"></span>' : ""}</span></div>`).join("")}</div>`;

// ---------- the terminal's screen: a still picture of xterm.js. A line is text with ⟦classes|text⟧ runs in it. ----------
function screen(lines) {
  return `<div class="term-screen" role="img">${lines.map((l) => `<div class="tl">${esc(l).replace(/⟦([^|⟧]+)\|([^⟧]*)⟧/g, (_, c, s) => `<span class="${c.split(" ").map((x) => `x-${x}`).join(" ")}">${s}</span>`)}</div>`).join("")}</div>`;
}
const P = (dir = "~/Herd/gizai", branch = "main") => `⟦green|jefsev@arch⟧ ⟦blue|${dir}⟧ ⟦bright-black|${branch}⟧ $ `;
const CUR = "⟦cursor| ⟧";
const VITE = [
  `${P()}npm run dev`, "", "> gizai@0.9.1 dev", "> vite", "",
  "  ⟦bright-green bold|VITE v8.0.2⟧  ⟦dim|ready in⟧ ⟦bold|312⟧ ⟦dim|ms⟧", "",
  "  ⟦bold|Local⟧:   ⟦cyan|http://localhost:1420/⟧",
  "  ⟦dim|Network: use --host to expose⟧", "  ⟦dim|press⟧ ⟦bold|h + enter⟧ ⟦dim|to show help⟧",
  "⟦dim|14:02:31⟧ ⟦bright-cyan bold|[vite]⟧ ⟦yellow|(client)⟧ ⟦green|hmr update⟧ ⟦dim|/src/components/Sidebar.tsx⟧",
  "⟦dim|14:03:12⟧ ⟦bright-cyan bold|[vite]⟧ ⟦yellow|(client)⟧ ⟦green|hmr update⟧ ⟦dim|/src/components/terminal/TerminalTabs.tsx, /src/styles/terminal.css⟧",
  "⟦dim|14:05:47⟧ ⟦bright-cyan bold|[vite]⟧ ⟦yellow|(client)⟧ ⟦green|hmr update⟧ ⟦dim|/src/pages/TerminalPage.tsx⟧",
  CUR,
];
const GIT_STATUS = [
  `${P()}git status`,
  "On branch gizai/ga-132-terminal-design",
  "Your branch is ahead of 'origin/gizai/ga-132-terminal-design' by 1 commit.",
  '  (use "git push" to publish your local commits)', "",
  "Changes to be committed:", '  (use "git restore --staged <file>..." to unstage)',
  "        ⟦green|new file:   design/terminal/gen-terminal.mjs⟧", "        ⟦green|modified:   design/README.md⟧", "",
  "Changes not staged for commit:", '  (use "git add <file>..." to update what will be committed)',
  '  (use "git restore <file>..." to discard changes in working directory)',
  "        ⟦red|modified:   design/terminal/terminal.css⟧", "",
  "Untracked files:", '  (use "git add <file>..." to include in what will be committed)',
  "        ⟦red|design/terminal/new-tokens.json⟧", "",
  `${P()}${CUR}`,
];
const GIT_DIFF = [
  `${P()}git diff design/README.md`,
  "⟦bold|diff --git a/design/README.md b/design/README.md⟧", "⟦bold|index 9c2f1e4..3a7b0d2 100644⟧",
  "⟦bold|--- a/design/README.md⟧", "⟦bold|+++ b/design/README.md⟧",
  "⟦cyan|@@ -36,6 +36,9 @@⟧ dense, dark, calm, and honest about what agents are doing.",
  "   the working strip on a card, the border of a live run card. Never use teal for anything else.",
  ' - `needs` (magenta) means "a person is needed": Review, holds, the Inbox count.',
  "⟦green|+- The terminal's own colours (`term-*`) are what programs print, not Gizai's states:⟧",
  "⟦green|+  `term-cyan` and `term-magenta` stay clearly apart from `live` and `needs`.⟧",
  "⟦red|-- `success`, `warning`, `danger` are run and form outcomes.⟧",
  "⟦green|+- `success`, `warning`, `danger` are run and form outcomes. Each badge pairs its colour⟧",
  "⟦green|+  with a word.⟧",
  " - Status is shown by glyph shape *and* hue (see StatusIcon), so it reads without colour:",
  `${P()}${CUR}`,
];
const LS_COLS = [
  [["Cargo.lock", ""], ["Cargo.toml", ""], ["CLAUDE.md", ""]],
  [["crates", "bright-blue bold"], ["design", "bright-blue bold"], ["docs", "bright-blue bold"]],
  [["gizai.tar.gz", "bright-red bold"], ["index.html", ""], ["install.ps1", ""]],
  [["install.sh", "bright-green bold"], ["latest", "bright-cyan bold"], ["LICENSE", ""]],
  [["node_modules", "bright-blue bold"], ["package.json", ""], ["package-lock.json", ""]],
  [["README.md", ""], ["screenshot.png", "bright-magenta bold"], ["scripts", "bright-blue bold"]],
  [["src", "bright-blue bold"], ["src-tauri", "bright-blue bold"], ["target", "bright-blue bold"]],
  [["tmp", "black bg-green"], ["tsconfig.json", ""], ["vite.config.ts", ""]],
];
const LS = [`${P()}ls --color`, ...[0, 1, 2].map((r) => LS_COLS.map((col, i) => {
  const [name, cls] = col[r];
  const pad = i === LS_COLS.length - 1 ? "" : " ".repeat(Math.max(...col.map(([n]) => n.length)) + 2 - name.length);
  return (cls ? `⟦${cls}|${name}⟧` : name) + pad;
}).join("")), `${P()}${CUR}`];
const CARGO = [
  `${P()}cargo test -p gizai-agents --test terminal_test`,
  "⟦bright-yellow bold|warning⟧⟦bold|: unused variable: `rows`⟧",
  "   ⟦bright-blue bold|-->⟧ crates/gizai-agents/src/terminal.rs:212:9",
  "    ⟦bright-blue bold||⟧",
  "⟦bright-blue bold|212⟧ ⟦bright-blue bold||⟧     let rows = size.rows;",
  "    ⟦bright-blue bold||⟧         ⟦bright-yellow bold|^^^^⟧ ⟦bright-yellow bold|help: prefix it with an underscore: `_rows`⟧",
  "",
  "   ⟦bright-green bold|Compiling⟧ gizai-agents v0.9.1 (/home/jefsev/Herd/gizai/crates/gizai-agents)",
  "    ⟦bright-green bold|Finished⟧ `test` profile [unoptimized + debuginfo] target(s) in 4.82s",
  "     ⟦bright-green bold|Running⟧ tests/terminal_test.rs (target/debug/deps/terminal_test-1f2e3d4c)",
  "", "running 6 tests",
  "test echoes_what_it_is_sent ... ⟦green|ok⟧", "test follows_resize ... ⟦green|ok⟧", "test sets_term_and_hides_gizai_vars ... ⟦green|ok⟧",
  "test names_the_busy_program ... ⟦green|ok⟧", "test close_ends_background_jobs ... ⟦red|FAILED⟧", "test keeps_the_exit_code ... ⟦green|ok⟧",
  "", "failures:", "", "---- close_ends_background_jobs stdout ----",
  "thread 'close_ends_background_jobs' panicked at tests/terminal_test.rs:88:5:",
  "sleep 1000 & is still running 3 s after close (pid 48211)",
  "", "test result: ⟦red|FAILED⟧. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.21s", "",
  "⟦bright-red bold|error⟧: test failed, to rerun pass `-p gizai-agents --test terminal_test`",
  `${P()}${CUR}`,
];
const ARTISAN = [
  `${P("~/.local/share/gizai/worktrees/KADE/KADE-41", "gizai/kade-41-fix-the-invoice-total")}php artisan serve`, "",
  "   ⟦bg-blue fg-bg bold| INFO ⟧ Server running on [⟦bold|http://127.0.0.1:8000⟧].", "",
  "  ⟦bright-black|Press Ctrl+C to stop the server⟧", "",
  "  2026-10-12 14:20:03 / ............................................... ⟦bright-black|~ 0.12ms⟧",
  "  2026-10-12 14:20:04 /invoices/41 .................................... ⟦bright-black|~ 3.40ms⟧",
  "^C", `${P("~/.local/share/gizai/worktrees/KADE/KADE-41", "gizai/kade-41-fix-the-invoice-total")}exit 2`,
];
const term = (lines, { cls = "", find = "", n = null } = {}) => `<div class="term-place${cls ? ` ${cls}` : ""}" aria-label="Terminal">${find}${screen(n ? lines.slice(-n) : lines)}</div>`;

// ---------- the sidebar, with the Terminal section in the state asked for ----------
const nav = (icon, label, { meta = "", on = false, cls = "", tag = "a", title = "" } = {}) =>
  `<${tag} class="nav-item${on ? " on" : ""}${cls ? ` ${cls}` : ""}"${title ? ` title="${q(title)}"` : ""}>${ic(icon)}<span>${label}</span>${meta ? `<span class="meta">${meta}</span>` : ""}</${tag}>`;
const liveTag = (n) => `<span class="live-tag"><span class="pulse"></span>${n} live</span>`;
const TERMS = [
  { key: "gizai", name: "gizai", tip: "~/Herd/gizai · zsh · waiting for a command" },
  { key: "gizai 2", name: "gizai 2", busy: X.sideBusy, tip: "~/Herd/gizai · zsh · running npm" },
  { key: "KADE-41", card: "Fix the invoice total", ended: true, tip: "KADE-41's worktree · zsh ended with exit code 2" },
];
const termRow = (x, { on, hover, focus, rename, ask } = {}) => {
  const cls = ["nav-item", on === x.key && "on", hover === x.key && "is-hover", focus === x.key && "is-focus"].filter(Boolean).join(" ");
  const name = rename === x.key ? `<input class="input name-in" value="${q(x.name)}" aria-label="Terminal's name">`
    : x.card ? `<span class="card-id">${x.key}</span><span class="faint ellipsis">${x.card}</span>` : `<span class="ellipsis">${x.name}</span>`;
  const right = x.busy ? `<span class="stand"><span class="prog">${x.busy}</span></span>` : x.ended ? `<span class="stand">${X.endedTab}</span>` : "";
  return `<${rename === x.key ? "div" : "a"} class="${cls}" title="${q(x.tip)}">${ic("terminal")}${name}${right ? `<span class="meta">${right}</span>` : ""}</${rename === x.key ? "div" : "a"}>`
    + (ask === x.key ? `<div class="ed-confirm" role="alertdialog" aria-label="Close ${q(x.name)}"><span class="grow">${X.sideAsk}</span><button class="btn danger sm">Close</button><button class="btn ghost sm">Cancel</button></div>` : "");
};
function terminalSection(state = "open", o = {}) {
  const head = (folded, tag = "") => `<div class="nav-label"><span class="fold">${X.sideHead}${tag}</span>`
    + `<button aria-label="New terminal" title="New terminal"${o.plus ? ' class="is-hover"' : ""}>${sm("plus")}</button>`
    + `<button aria-expanded="${!folded}" aria-label="${folded ? "Show" : "Fold"} Terminal" title="${folded ? "Show" : "Fold"} Terminal">${sm(folded ? "chevron-up" : "chevron-down")}</button></div>`;
  if (state === "folded") return `<div class="nav-section" aria-label="Terminal">${head(true, `<span class="fold-tag" title="3 terminals are open">${X.sideFolded}</span>`)}</div>`;
  if (state === "empty") return `<div class="nav-section" aria-label="Terminal">${head(false)}<div class="nav-list">${nav("terminal", X.sideEmpty, { tag: "button", cls: o.hover === "empty" ? "is-hover" : "" })}</div></div>`;
  return `<div class="nav-section" aria-label="Terminal">${head(false)}<div class="nav-list">${(o.terms ?? TERMS).map((x) => termRow(x, o)).join("")}</div></div>`;
}
const EDITOR_ROWS = `<a class="nav-item" title="~/Herd/gizai · main, up to date with origin/main · 4 files changed, not committed">${ic("folder-code")}<span>gizai</span><span class="meta"><span class="stand">4 changed</span></span></a>`
  + `<a class="nav-item" title="~/Herd/kade-portal">${ic("folder-code")}<span>kade-portal</span></a>`
  + `<a class="nav-item" title="KADE-41 Fix the invoice total. Pinned to you: agents leave it alone">${ic("folder-code")}<span class="card-id">KADE-41</span><span class="faint ellipsis">Fix the invoice total</span><span class="meta"><span class="stand">${sm("keyboard")}by hand</span></span></a>`;
const editorSection = (on = false) => `<div class="nav-section" aria-label="Editor"><div class="nav-label"><span class="fold">Editor</span><button aria-label="Open a folder" title="Open a folder">${sm("plus")}</button>`
  + `<button aria-expanded="true" aria-label="Fold Editor" title="Fold Editor">${sm("chevron-down")}</button></div><div class="nav-list">${on ? EDITOR_ROWS.replace('<a class="nav-item"', '<a class="nav-item on"') : EDITOR_ROWS}</div></div>`;
/** The whole sidebar; `crop` keeps the Terminal section with its neighbours, Projects and Editor. */
function sidebar({ terminal = "open", crop = false, editorOn = false, ...o } = {}) {
  const top = nav("square-pen", "New task", { tag: "button", meta: kbd("N") }) + nav("search", "Search", { tag: "button", meta: kbd("Ctrl K") })
    + nav("inbox", "Inbox", { meta: '<span class="count alert">2</span>' })
    + `<div class="nav-section"><div class="nav-label">Work</div>${nav("messages-square", "Chat")}${nav("list-todo", "Tasks")}${nav("building-2", "Clients")}${nav("folder-kanban", "Projects")}</div>`;
  const projects = `<div class="nav-section"><div class="nav-label">Projects<button aria-label="New project" title="New project">${sm("plus")}</button></div>`
    + `<a class="nav-item"><span class="dot" style="background:var(--c-blue)"></span><span>Gizai</span></a><a class="nav-item"><span class="dot" style="background:var(--c-orange)"></span><span>Kade portal</span></a></div>`;
  const agents = `<div class="nav-section" aria-label="Agents"><div class="nav-label"><span class="fold">Agents</span><button aria-expanded="true" aria-label="Fold Agents" title="Fold Agents">${sm("chevron-down")}</button></div><div class="nav-list">`
    + nav("crown", "Team Lead") + nav("monitor", "Frontend Agent") + nav("server", "Backend Agent", { meta: liveTag(1) }) + nav("flask-conical", "QA Agent", { meta: liveTag(1) }) + `</div></div>`;
  const memory = `<div class="nav-section" aria-label="Memory"><div class="nav-label"><span class="fold">Memory</span><button aria-expanded="false" aria-label="Show Memory" title="Show Memory">${sm("chevron-up")}</button></div></div>`;
  const foot = `<div class="side-foot"><div class="nav-section"><div class="nav-label">Company</div>${nav("chart-column", "Usage")}${nav("network", "Team")}${nav("users", "Users")}${nav("settings", "Settings")}</div></div>`;
  const main = `${projects}${terminalSection(terminal, o)}${editorSection(editorOn)}`;
  return crop ? `<aside class="side" aria-label="Navigation"><div class="side-main">${main}</div></aside>`
    : `<aside class="side" aria-label="Navigation"><div class="side-main">${top}${main}${agents}${memory}</div>${foot}</aside>`;
}

// ---------- the folder picker, titled New terminal in ----------
const where = (s) => `<span class="where">${s}</span>`;
const dot = (c) => `<span class="dot" style="width:9px;height:9px;border-radius:50%;flex:none;background:var(--${c})"></span>`;
const opt = (lead, label, right = "", { active = false, off = "", card = "", title = "" } = {}) => `<button class="opt${active ? " active" : ""}" role="option" aria-selected="${active}"${off ? ` aria-disabled="true" title="${q(off)}"` : title ? ` title="${q(title)}"` : ""}>`
  + `${lead}${card ? `<span class="card-id">${card}</span>` : ""}<span class="label ellipsis">${label}</span>${right}</button>`;
const PICK_CARDS = (ask = false) => `<div role="group" aria-label="Cards"><div class="pop-label">Cards</div>`
  + (ask ? "" : opt(st("in_progress"), "Fix the invoice total", `<span class="stand">${sm("keyboard")}${X.pickMine}</span>`, { card: "KADE-41" }))
  + opt(st("testing"), "Show a run's changes", `<span class="stand"><span class="pulse"></span>${X.pickLive}</span>`, { card: "GA-118", off: LIVE_WHY })
  + opt(st("in_progress"), "Remember the open tabs per folder", "", { card: "GA-112", active: ask, title: "GA-112 is Frontend Agent's card" })
  + (ask ? `<div class="ed-confirm" role="alertdialog" aria-label="GA-112"><span class="grow">${X.pickNotMine}</span><button class="btn sm">Edit anyway</button><button class="btn ghost sm">Cancel</button></div>` : "")
  + `</div>`;
const PICK_ALL = (o = {}) => `<div role="group" aria-label="Projects"><div class="pop-label">Projects</div>`
  + opt(dot("c-blue"), "Gizai", where("~/Herd/gizai"), { active: !o.ask }) + opt(dot("c-orange"), "Kade portal", where("~/Herd/kade-portal")) + `</div>`
  + `<div role="group" aria-label="In ~/Herd"><div class="pop-label">In ~/Herd</div>`
  + opt(sm("folder-code"), "gizai-site", where("~/Herd/gizai-site")) + opt(sm("folder-code"), "invoice-api", where("~/Herd/invoice-api")) + opt(sm("folder-code"), "scratch", where("~/Herd/scratch")) + `</div>`
  + PICK_CARDS(o.ask);
const picker = ({ query = "", body, left = 256, top = 160, note = "" }) => `<div class="pop ed-picker" role="dialog" aria-label="${X.pickTitle}" style="left:${left}px;top:${top}px">`
  + `<div class="ed-picker-title">${X.pickTitle}</div>`
  + `<div class="ed-picker-in">${sm("search")}<input class="input" value="${q(query)}" placeholder="${q(X.pickFind)}" aria-label="${q(X.pickFind)}"></div>`
  + (note ? `<div class="ed-picker-note">${ic("info")}<span>${note}</span></div>` : "")
  + `<div class="ed-picker-list" role="listbox" aria-label="Folders">${body}</div><div class="ed-picker-foot">${X.pickFoot.replace("Settings → General → Editor folders", "<a>Settings → General → Editor folders</a>")}</div></div>`;

// ---------- the Terminal page ----------
const topbar = (crumbs, mid = "", actions = "", cls = "") => `<div class="topbar"><div class="crumbs${cls ? ` ${cls}` : ""}">${crumbs}</div>${mid}<div class="actions">${actions}</div></div>`;
const termCrumbs = (name, rename = false) => `<a>Terminal</a><span class="sep">/</span>${rename ? `<input class="input name-in" value="${q(name)}" aria-label="Terminal's name">` : `<b title="Double-click to rename (F2)">${name}</b>`}`;
const termWhere = (p = "~/Herd/gizai", shell = "zsh", size = "132 × 38") => `<div class="term-where"><span class="path" title="${q(p)}">${p}</span><span class="sep">·</span><span>${shell}</span><span class="sep">·</span><span class="size" title="${size.replace(" × ", " columns, ")} rows">${size}</span></div>`;
const moreBtn = (open = false) => `<button class="btn ghost sm icon-only${open ? " is-hover" : ""}" aria-label="More" title="More" aria-haspopup="menu" aria-expanded="${open}">${ic("ellipsis")}</button>`;
const termActions = ({ more = false, name = "gizai 2", folder = "gizai" } = {}) => `<button class="btn sm" title="Show ${q(name)} in ${q(folder)}'s Editor, in its terminal panel">${ic("folder-code")}${X.showInEditor}</button>${moreBtn(more)}`;
const termTop = (o = {}) => topbar(termCrumbs(o.name ?? "gizai 2", o.rename), termWhere(o.path, o.shell, o.size), termActions(o), "ed-crumbs");
/** A terminal tab: { name, busy, ended, on, is, rename, away, card } */
const ttab = (x) => `<div class="${["ed-tab", "term-tab", x.is].filter(Boolean).join(" ")}" role="tab" tabindex="${x.on ? 0 : -1}" aria-selected="${!!x.on}" title="${q(x.away ? `${x.name} is on its own page.` : `${x.name} · ${x.busy ? `running ${x.busy}` : x.ended ? "ended" : "waiting for a command"}`)}">`
  + `${ic(x.away ? "maximize-2" : "terminal")}${x.rename ? `<input class="input name-in" value="${q(x.name)}" aria-label="Terminal's name">` : `<span class="name">${x.card ? `<span class="card-id">${x.name}</span>` : x.name}</span>`}`
  + `${x.busy ? `<span class="prog">${x.busy}</span>` : x.ended ? `<span class="state">${X.endedTab}</span>` : ""}`
  + `<button class="x" aria-label="Close ${q(x.name)}" title="Close (middle click)">${ic("x")}</button></div>`;
const plusTab = (folder = "gizai", off = "") => `<button class="btn ghost sm icon-only${off ? " term-btn" : ""}" aria-label="New terminal in ${folder}" title="${q(off || `New terminal in ${folder} (Ctrl+Shift+\`)`)}"${off ? ' aria-disabled="true"' : ""}>${ic("plus")}</button>`;
const ttabs = (list, { folder = "gizai", acts = "", plusOff = "" } = {}) => `<div class="ed-tabs term-tabs" role="tablist" aria-label="Terminals in ${folder}">${list.map(ttab).join("")}${plusTab(folder, plusOff)}${acts ? `<span class="acts">${acts}</span>` : ""}</div>`;
const GIZAI_TABS = (o = {}) => [{ name: "gizai", on: o.on === "gizai", is: o.is?.gizai }, { name: "gizai 2", busy: "npm", on: o.on !== "gizai", is: o.is?.["gizai 2"], rename: o.rename, away: o.away }];
const termPage = ({ top = termTop(), bar = "", tabs = ttabs(GIZAI_TABS()), body = term(VITE), overlay = "" } = {}) =>
  `<div class="main"><div class="term-page">${top}${tabs}${bar}${body}</div>${overlay}</div>`;
const bar = (kind, lead, text, acts = "") => `<div class="ed-bar${kind ? ` ${kind}` : ""}" role="${/failed|refused/.test(kind) ? "alert" : "status"}">${lead}<span class="say">${text}</span>${acts ? `<span class="acts">${acts}</span>` : ""}</div>`;
const btn = (label, { kind = "", icon = "", off = false, title = "" } = {}) => `<button class="btn sm${kind ? ` ${kind}` : ""}"${off ? " disabled" : ""}${title ? ` title="${q(title)}"` : ""}>${icon ? ic(icon) : ""}${label}</button>`;

// ---------- the Editor page with its panel ----------
const standing = (text) => { const [branch, ...rest] = text.split(" · "); return `<div class="ed-standing">${sm("git-branch")}<span class="branch">${branch}</span>${rest.map((r) => `<span class="sep">·</span><span>${r}</span>`).join("")}</div>`; };
const fetchBtn = `<button class="btn ghost sm" title="Fetch origin/main now (last fetched 2m ago)">${ic("refresh-cw")}Fetch</button>`;
const openInBtn = `<button class="btn sm" aria-haspopup="menu">${ic("external-link")}Open in${ic("chevron-down")}</button>`;
const termBtn = ({ on = false, off = "", is = "" } = {}) => `<button class="btn ghost sm term-btn${is ? ` ${is}` : ""}" aria-pressed="${on}"${off ? ' aria-disabled="true"' : ""} title="${q(off || (on ? "Hide the terminal (Ctrl+`)" : "Show the terminal (Ctrl+`)"))}">${ic("terminal")}${X.btn}</button>`;
const edActions = (o = {}) => `${o.fetch === false ? "" : fetchBtn}${termBtn(o)}${openInBtn}`;
const MARK = { modified: ["M", "Modified"], untracked: ["U", "Untracked: new, not in git yet"] };
const mark = (m) => `<span class="ed-mark ${m}" title="${q(MARK[m][1])}">${MARK[m][0]}</span>`;
const treeRow = ([depth, kind, name, o = {}]) => `<div class="ed-row${o.ignored ? " ignored" : ""}" style="--depth:${depth}" role="treeitem"${kind === "dir" ? ` aria-expanded="${!!o.open}"` : ""}${o.selected ? ' aria-selected="true"' : ""}>`
  + `${kind === "dir" ? ic(o.open ? "chevron-down" : "chevron-right") : '<span class="room"></span>'}${kind === "dir" ? ic(o.open ? "folder-open" : "folder") : ic(/\.(tsx?|rs|css|mjs)$/.test(name) ? "file-code" : /\.json$/.test(name) ? "file-json" : /\.md$/.test(name) ? "file-text" : "file")}`
  + `<span class="name">${name}</span>${o.mark ? mark(o.mark) : ""}${o.dot ? '<span class="ed-dot" title="Changes inside"></span>' : ""}</div>`;
const GIZAI_TREE = [
  [0, "dir", "crates"], [0, "dir", "design"], [0, "dir", "docs"], [0, "dir", "node_modules", { ignored: true }], [0, "dir", "scripts"],
  [0, "dir", "src", { open: true, dot: true }], [1, "dir", "assets"], [1, "dir", "components", { open: true, dot: true }],
  [2, "dir", "chat"], [2, "dir", "editor"], [2, "dir", "terminal", { dot: true }], [2, "file", "Avatar.tsx"], [2, "file", "Board.tsx"],
  [2, "file", "Sidebar.tsx", { mark: "modified", selected: true }], [2, "file", "StatusIcon.tsx"], [1, "dir", "lib", { dot: true }], [1, "dir", "pages"], [1, "dir", "styles"],
  [1, "file", "api.ts"], [1, "file", "App.tsx"], [1, "file", "router.ts"], [0, "dir", "src-tauri"], [0, "dir", "target", { ignored: true }],
  [0, "file", "Cargo.toml"], [0, "file", "package.json"], [0, "file", "README.md"],
];
const sidePanel = (title = "gizai") => `<aside class="ed-side" aria-label="Files and search"><div class="ed-side-top"><div class="seg" role="group" aria-label="Files or search"><button aria-pressed="true">Files</button><button aria-pressed="false" title="Search in the folder (Ctrl+Shift+F)">${sm("search")}Search</button></div>`
  + `<button class="btn ghost sm icon-only" aria-label="Fold the file tree (Ctrl+B)" title="Fold the file tree (Ctrl+B)">${ic("panel-left-close")}</button></div>`
  + `<button class="ed-goto">${sm("search")}Go to file${kbd("Ctrl P")}</button>`
  + `<div class="ed-tree-head"><span>${title}</span><button class="btn ghost sm icon-only" aria-label="New file" title="New file">${ic("file-plus")}</button><button class="btn ghost sm icon-only" aria-label="New folder" title="New folder">${ic("folder-plus")}</button><button class="btn ghost sm icon-only" aria-label="Fold every folder" title="Fold every folder">${ic("chevrons-down-up")}</button></div>`
  + `<div class="ed-tree" role="tree" aria-label="Files of ${title}">${GIZAI_TREE.map(treeRow).join("")}</div></aside>`;
const fileTabs = `<div class="ed-tabs" role="tablist" aria-label="Open files"><div class="ed-tab" role="tab" tabindex="0" aria-selected="true" title="src/components/Sidebar.tsx"><span class="name">Sidebar.tsx</span>${mark("modified")}<button class="x" aria-label="Close Sidebar.tsx" title="Close (Ctrl+W)">${ic("x")}</button></div>`
  + `<div class="ed-tab" role="tab" tabindex="-1" aria-selected="false" title="src/lib/sidebar.ts"><span class="name">sidebar.ts</span>${mark("modified")}<button class="x" aria-label="Close sidebar.ts" title="Close (Ctrl+W)">${ic("x")}</button></div>`
  + `<div class="ed-tab" role="tab" tabindex="-1" aria-selected="false" title="src/components/terminal/TerminalTabs.tsx"><span class="name">TerminalTabs.tsx</span>${mark("untracked")}<button class="x" aria-label="Close TerminalTabs.tsx" title="Close (Ctrl+W)">${ic("x")}</button></div></div>`;
const status = (cursor = `Ln ${SIDEBAR_LINE}, Col 7`) => `<div class="ed-status" role="status" aria-label="Status bar"><span class="path" title="src/components/Sidebar.tsx">src/components/Sidebar.tsx</span><span class="item git modified">Modified</span><span class="grow"></span>`
  + `<button class="item" title="Go to line (Ctrl+Alt+G)">${cursor}</button><span class="item" title="Line endings, kept as the file has them">LF</span><span class="item">TypeScript JSX</span><span class="item">Saved</span></div>`;
const panelActs = `<button class="btn ghost sm" title="Open gizai on its own page">${ic("maximize-2")}${X.ownPage}</button><button class="btn ghost sm" title="Hide the terminal (Ctrl+\`): its shells keep running">${ic("chevron-down")}${X.hide}</button>`;
const handle = (share = 30, is = "") => `<div class="term-handle${is ? ` ${is}` : ""}" role="separator" aria-orientation="horizontal" aria-label="${X.handle}" aria-valuenow="${share}" aria-valuemin="0" aria-valuemax="100" tabindex="0"></div>`;
/** The panel: its handle, its tabs (this folder's terminals) with Own page and Hide, and the terminal. */
const panel = ({ share = "30%", tabsList = GIZAI_TABS({ on: "gizai" }), body = term(CARGO, { cls: "scrolled" }), handleIs = "", plusOff = "" } = {}) =>
  `<div class="term-panel" style="--term-share:${share}">${handle(parseInt(share, 10) || 30, handleIs)}${ttabs(tabsList, { acts: panelActs, plusOff })}${body}</div>`;
/** The code column (.ed-main): file tabs, the code, the panel when it shows, the status bar. */
const edMain = ({ pnl = "", first, lines = 44 } = {}) => `<div class="ed-main${pnl ? " has-term" : ""}">${fileTabs}${code({ first, lines })}${pnl}${status()}</div>`;
const editorPage = ({ actions = edActions({ on: true }), bar: b = "", pnl = panel(), crumbsName = "gizai", stand = "main · up to date with origin/main · 4 changed", mono = false } = {}) =>
  `<div class="main"><div class="ed-page">${topbar(`<a>Editor</a><span class="sep">/</span><b${mono ? ' class="mono"' : ""} title="~/Herd/gizai">${crumbsName}</b>`, standing(stand), actions, "ed-crumbs")}${b}<div class="ed">${sidePanel(crumbsName)}${edMain({ pnl })}</div></div></div>`;
/** Only the Editor's top bar (and its bar), for the button's states. */
const edTopStrip = (o, b = "", name = "gizai", stand = "main · up to date with origin/main · 4 changed") => `<div class="main">${topbar(`<a>Editor</a><span class="sep">/</span><b${name !== "gizai" ? ' class="mono"' : ""}>${name}</b>`, standing(stand), edActions(o), "ed-crumbs")}${b}</div>`;

// ---------- frames: each screen in dark and in light ----------
const frame = (theme, inner, { h = null, w = null, cls = "" } = {}) =>
  `<div class="win${cls ? ` ${cls}` : ""}" data-theme="${theme}" style="${h ? `height:${h}px;` : ""}${w ? `width:${w}px;` : ""}">${inner}</div>`;
/** A whole window: dark, then its light twin (made in the browser, so the file holds each screen once). */
const both = (inner, opts = {}) => `<figure class="shot dark"><figcaption class="shot-label"><span class="theme-name">Dark</span>${opts.label ? ` · ${opts.label}` : ""}</figcaption>${frame("dark", inner, { h: opts.h ?? 900, cls: opts.cls })}</figure>`;
/** Small frames: a row of them in dark, then the same row in light. items: [{ label, inner, w, h, cls, tip }] */
const rows = (items, opts = {}) => `<div class="variants"><div class="vrow dark"><div class="vhead theme-name">Dark</div><div class="vitems">${items.map((it) =>
  `<figure class="vitem"${it.w ? ` style="width:${it.w}px"` : ""}>${it.label ? `<figcaption>${it.label}</figcaption>` : ""}${frame("dark", it.inner, { h: it.h ?? opts.h, cls: `strip ${it.cls ?? ""}` })}${it.tip ? `<p class="tip">${it.tip}</p>` : ""}</figure>`).join("")}</div></div></div>`;
/** Strips the window's width (bars and the like), one per row. */
const strips = (items) => `<div class="variants"><div class="vrow dark stack"><div class="vhead theme-name">Dark</div><div class="vitems">${items.map((it) =>
  `<figure class="vitem wide">${it.label ? `<figcaption>${it.label}</figcaption>` : ""}${frame("dark", it.inner, { cls: `strip ${it.cls ?? ""}`, h: it.h })}${it.tip ? `<p class="tip">${it.tip}</p>` : ""}</figure>`).join("")}</div></div></div>`;

// ---------- the page's sections ----------
const sections = [];
const section = (id, title, html) => sections.push({ id, title, html: `<section class="dz-sec" id="${id}"><h2>${title}</h2>${html}</section>` });
const notes = (...items) => `<ul class="dz-notes">${items.map((i) => `<li>${i}</li>`).join("")}</ul>`;
const p = (html) => `<p class="dz-p">${html}</p>`;
const h3 = (s) => `<h3>${s}</h3>`;
const c = (s) => `<code>${s}</code>`;
const table = (head, body) => `<table class="dz-table"><thead><tr>${head.map((x) => `<th>${x}</th>`).join("")}</tr></thead><tbody>${body.map((r) => `<tr>${r.map((x) => `<td>${x}</td>`).join("")}</tr>`).join("")}</tbody></table>`;

// 1. The sidebar's Terminal section ------------------------------------------------------------------------------------------
const side = (o) => sidebar({ ...o, crop: true });
const rowMenu = `<div class="pop" role="menu" aria-label="Terminal" style="left:140px;top:150px;min-width:210px">`
  + `<button class="opt is-hover" role="menuitem">Rename${kbd("F2")}</button><button class="opt" role="menuitem">Show in Editor</button><button class="opt" role="menuitem">Copy folder path</button>`
  + `<div class="sep"></div><button class="opt" role="menuitem">Close</button></div>`;
section("sidebar", "1. The sidebar's Terminal section",
  p("Between Projects and Editor, a section that folds like Editor, Agents and Memory: the same <code>FoldSection</code>, with <code>\"terminal\"</code> added to <code>Fold</code> and kept in <code>gizai.sidebar.folded</code> with the others. Its rows are the open terminals, oldest first, so a new one comes at the bottom and rows don't move under the pointer. There is no limit on how many.")
  + notes(
    "<b>Heading.</b> “Terminal”, exactly as Editor's: <b>+</b> (New terminal) opens the folder picker (§2), the caret folds the section, and a click on the word folds it too. Folded, the heading says how many terminals are open, “3 open”, where Editor says “2 behind” and Agents “1 live”.",
    "<b>A row.</b> Lucide <code>terminal</code>, the name, and on the right in <code>text-3</code> what runs in it now: the program in mono (“npm”, “cargo”, “vim”), nothing while the shell waits for a command, or “ended” when the shell ended with a code other than 0. The folder, the shell and its state are in the row's tooltip. A click opens the terminal on its page (<code>#/terminal/&lt;id&gt;</code>); the row is selected while that page shows.",
    "<b>Names.</b> The folder's name, then “gizai 2”, “gizai 3” for more in the same folder (the lowest free number). A card's worktree shows the card's identifier in mono and its title faint, as the Editor's card rows do. Rename (F2) sets any name up to 40 characters; an empty one gives the default back.",
    "<b>Never teal, never a pulse</b> on a terminal's row: those mean an agent at work. What a terminal runs is your own work, so it is grey text.",
    "<b>No × on the row</b>: a button inside a row that is a link isn't allowed (D2's rule), and the row stays quiet. Closing is in the row's menu, a middle click, and the × on the terminal's tab.",
    "<b>Right click</b> (or the context menu key, or Shift+F10): Rename (F2), Show in Editor (opens the folder in the Editor with this terminal in its panel), Copy folder path, Close. The menu is the app's <code>ContextMenu</code>, as the Editor's rows have.",
    "<b>Close</b> on a terminal that runs a program asks once, under the row, in one sentence: “npm is still running in gizai 2. Close it and end npm?”, with Close and Cancel; Enter closes, Escape cancels. A terminal that waits for a command, or has ended, closes at once.",
    "<b>None open:</b> one row, “New terminal”, which opens the picker, as the Editor's “Open a folder” row does.")
  + rows([
    { label: "Open: gizai 2's page shows, gizai hovered", inner: side({ on: "gizai 2", hover: "gizai" }), w: 250 },
    { label: "Folded: 3 open", inner: side({ terminal: "folded" }), w: 250, tip: "Tooltip on “3 open”: 3 terminals are open." },
    { label: "None open", inner: side({ terminal: "empty", hover: "empty" }), w: 250 },
    { label: "Right click on gizai 2", inner: side({ on: "gizai 2", hover: "gizai 2" }) + rowMenu, w: 380, h: 470 },
    { label: "Close gizai 2 while npm runs", inner: side({ on: "gizai 2", ask: "gizai 2" }), w: 250 },
    { label: "Rename (F2)", inner: side({ on: "gizai 2", rename: "gizai 2" }), w: 250 },
  ]));

// 2. New terminal: the folder picker ---------------------------------------------------------------------------------------------
section("new", "2. New terminal: the Editor's folder picker",
  p("<b>+</b> on the Terminal heading (and New terminal when none is open) opens the Editor's folder picker, titled “New terminal in”, beside the sidebar: the same groups (Projects, the folders in ~/Herd, Cards), search box and footer. The folder the page shows is on top and chosen, so <b>+</b> then Enter starts another terminal there. Enter or a click starts a terminal in the chosen folder and opens its page; Escape closes the picker. Ctrl+Shift+` starts one at once in the folder you are in, without the picker.")
  + notes(
    "Only the folders the Editor may open: projects' folders, the folders in ~/Herd and card worktrees, with the Editor's refusals; never your home folder. From there <code>cd</code> goes anywhere, as you: the rule decides where Gizai starts a shell, it doesn't fence the shell in.",
    "<b>A card an agent works on</b> is in the list, off, with the pulse and who works (as in the Editor's picker); its tooltip says “QA Agent is working on GA-118: a terminal there waits until its run ends.”",
    "<b>A card that isn't pinned to you</b> (another's, an agent's between runs, or nobody's): choosing it asks first, under its row, with the Editor's own button: “GA-112 is Frontend Agent's card: a shell there can change its files, as an edit can.” Edit anyway starts the terminal; Cancel keeps the picker open. A card pinned to you (by hand) starts at once.",
    "<b>From 20 open</b>, a note on top of the list: “20 terminals are open: each keeps its shell running until you close it.” It never stops you: there is no limit.")
  + both(sidebar({ on: "gizai 2", plus: true }) + termPage() + picker({ body: PICK_ALL(), top: 400 }), { label: "+ on Terminal, over gizai 2's page" })
  + rows([
    { label: "20 terminals open", inner: picker({ left: 0, top: 0, body: PICK_ALL(), note: X.pickMany }), w: 382, h: 520, cls: "pick" },
    { label: "GA-112 isn't yours: asked first", inner: picker({ left: 0, top: 0, query: "ga-", body: PICK_CARDS(true) }), w: 382, h: 360, cls: "pick" },
    { label: "Nothing found", inner: picker({ left: 0, top: 0, query: "invoices-v2", body: `<div class="pop-empty">${X.pickNone}</div>` }), w: 382, h: 220, cls: "pick" },
  ]));

// 3. The Terminal page -------------------------------------------------------------------------------------------------------------
const moreMenu = `<div class="pop right" role="menu" aria-label="More" style="right:20px;top:46px;left:auto;width:240px">`
  + `<button class="opt" role="menuitem">Rename${kbd("F2")}</button><button class="opt is-hover" role="menuitem">Find${kbd("Ctrl Shift F")}</button><button class="opt" role="menuitem">Clear</button>`
  + `<button class="opt" role="menuitem">Copy folder path</button><div class="sep"></div><button class="opt" role="menuitem">Close</button></div>`;
const findBox = (query = "hmr", count = "11 of 12", none = false) => `<div class="term-find" role="search" aria-label="Find in the terminal"><div class="in"><input class="input" value="${q(query)}" placeholder="${X.findBox}" aria-label="Find">`
  + `<span class="ed-toggles"><button class="ed-toggle" aria-pressed="false" title="Match case">${ic("case-sensitive")}</button><button class="ed-toggle" aria-pressed="false" title="Words">${ic("whole-word")}</button><button class="ed-toggle" aria-pressed="false" title="Regular expression">${ic("regex")}</button></span></div>`
  + `<span class="count${none ? " none" : ""}">${count}</span><button class="btn ghost sm icon-only" aria-label="Previous match" title="Previous match (Shift+Enter)">${ic("arrow-up")}</button>`
  + `<button class="btn ghost sm icon-only" aria-label="Next match" title="Next match (Enter)">${ic("arrow-down")}</button><button class="btn ghost sm icon-only" aria-label="Close" title="Close (Escape)">${ic("x")}</button></div>`;
const VITE_FIND = VITE.map((l, i) => l.replace(/⟦green\|hmr update⟧/, i === VITE.length - 3 ? "⟦now|hmr⟧⟦green| update⟧" : "⟦hit|hmr⟧⟦green| update⟧"));
const tabAsk = `<div class="pop ed-ask" role="alertdialog" aria-label="Close gizai 2" style="left:86px;top:${52 + 33}px"><p>${X.tabAsk}</p><div class="acts"><button class="btn ghost sm">Cancel</button><button class="btn danger sm">Close</button></div></div>`;
const endedPage = (b, tabsList) => termPage({ tabs: ttabs(tabsList, { folder: "KADE-41" }), top: termTop({ name: "KADE-41", path: "~/.local/share/gizai/worktrees/KADE/KADE-41", size: "132 × 38" }), bar: b, body: term(ARTISAN, { cls: "ended" }) });
const KADE_TABS = (o = {}) => [{ name: "KADE-41", card: true, on: true, ended: !o.failed }];
section("page", "3. The Terminal page",
  p("<code>#/terminal/&lt;id&gt;</code>. The top bar says which terminal and where it runs; under it the terminals of <b>this terminal's folder</b> as tabs (the same set as that folder's Editor panel, §4), then the terminal, which fills the rest. <code>#/terminal</code> without one is an empty page with New terminal.")
  + notes(
    "<b>Top bar.</b> Terminal / <b>gizai 2</b>: a double-click (or F2) renames it in place. Then the folder in mono, the shell and its size in columns × rows, all in <code>text-3</code>; the size follows the place it shows in. Actions: <b>Show in Editor</b> (the folder in the Editor, with this terminal in its panel) and <b>⋯</b>: Rename (F2), Find (Ctrl+Shift+F), Clear (empties the scrollback), Copy folder path, Close.",
    "<b>Tabs.</b> The folder's terminals, drawn as the Editor's file tabs, <code>size-term-tab</code> high: the <code>terminal</code> icon, the name, the program that runs in it in mono, and ×. <b>+</b> starts another terminal in the same folder, without a picker. × shows on hover and on the open tab, as on file tabs. A double-click renames a tab in place; a middle click or × closes it, asking under the tab when a program runs. Ctrl+PgDn and Ctrl+PgUp step through them; ← and → move between tabs that have the focus.",
    "<b>The terminal</b> fills the rest: <code>term-bg</code> (the code's background), 8px above and below and 12px at the sides, the mono font at the code size, the <code>term-*</code> colours (§6). The cursor is a block while the terminal has the focus and an outline while it hasn't, and it doesn't blink: only the live pulse moves on its own.",
    "<b>Find</b> (Ctrl+Shift+F; Cmd+F on macOS): a small box at the top right, over the terminal, so the terminal keeps its size and full-screen programs don't redraw. The query in mono, Match case, Words and Regular expression inside the box, “3 of 12”, previous and next, close. Matches are marked as the Editor marks them: <code>term-selection</code>, the current one with an <code>accent</code> border.",
    "<b>Ended.</b> <code>exit</code> with code 0 closes the terminal and its tab. Any other end keeps it, read-only, with a bar under the tabs: “The shell ended with exit code 2.” with Start again (a new shell in the same folder, the same name and tab) and Close; the screen stays as it was, to read, select and find in. The tab and the sidebar row say “ended”. A shell that never started says why: “Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found.”, with Try again and Close.")
  + both(sidebar({ on: "gizai 2" }) + termPage(), { label: "gizai 2 running npm run dev" })
  + h3("Its menu, renaming, closing a tab")
  + rows([
    { label: "⋯", inner: termPage({ top: termTop({ more: true }), body: term(VITE.slice(0, 8)) }) + moreMenu, w: 760, h: 300 },
    { label: "Renaming in the top bar (a double-click on the name)", inner: termPage({ top: termTop({ rename: true }), body: term(VITE.slice(0, 8)) }), w: 760, h: 300 },
    { label: "Renaming a tab (a double-click)", inner: termPage({ tabs: ttabs(GIZAI_TABS({ rename: true })), body: term(VITE.slice(0, 8)) }), w: 760, h: 300 },
    { label: "A middle click on gizai 2's tab while npm runs", inner: termPage({ tabs: ttabs(GIZAI_TABS({ is: { "gizai 2": "is-hover" } })), body: term(VITE.slice(0, 8)) }) + tabAsk, w: 760, h: 300 },
  ])
  + h3("Find")
  + rows([
    { label: "Find (Ctrl+Shift+F): “hmr”, the eleventh of twelve (nine more are in the scrollback above)", inner: termPage({ body: term(VITE_FIND, { find: findBox() }) }), w: 1100, h: 460 },
    { label: "No results", inner: termPage({ body: term(VITE.slice(0, 8), { find: findBox("vitest", "No results", true) }) }), w: 760, h: 300 },
  ])
  + h3("Ended, couldn't start, none open")
  + strips([
    { label: "The shell ended with exit code 2 (KADE-41, after php artisan serve)", inner: endedPage(bar("file", ic("info"), X.ended, btn("Start again", { icon: "rotate-ccw" }) + btn("Close", { kind: "ghost" })), KADE_TABS()), h: 330 },
    { label: "The shell couldn't start", inner: termPage({ top: termTop({ name: "gizai 3", size: "132 × 38" }), tabs: ttabs([{ name: "gizai" }, { name: "gizai 2", busy: "npm" }, { name: "gizai 3", on: true, ended: true }]), bar: bar("file failed", ic("circle-alert"), X.failed, btn("Try again", { icon: "rotate-ccw" }) + btn("Close", { kind: "ghost" })), body: term([""], { cls: "ended" }) }), h: 200 },
  ])
  + rows([
    { label: "#/terminal: none open", inner: `<div class="main"><div class="term-page">${topbar("<b>Terminal</b>")}<div class="ed-empty"><div class="empty">${ic("terminal")}<b>${X.emptyTitle}</b><span>${X.empty}</span><button class="btn primary">${ic("plus")}New terminal</button></div></div></div></div>`, w: 620, h: 300 },
    { label: "The address of a terminal that was closed", inner: `<div class="main"><div class="term-page">${topbar("<a>Terminal</a>")}<div class="ed-empty"><div class="empty">${ic("terminal")}<b>${X.gone}</b><button class="btn">${ic("plus")}New terminal</button></div></div></div></div>`, w: 620, h: 300 },
  ]));

// 4. The Editor with its terminal panel ------------------------------------------------------------------------------------------------
const LIVE_BAR = bar("", '<span class="pulse" aria-hidden="true"></span>', "QA Agent is working on GA-118: its worktree is read-only until the run ends.", btn("Open card"));
const NOTMINE_BAR = bar("", ic("lock"), "GA-112 is Frontend Agent's card: its worktree is read-only here. Edit anyway puts your changes in it, uncommitted, for the agent's next run.", btn("Edit anyway") + btn("Open card"));
const EDITING_BAR = bar("", ic("triangle-alert"), "You're editing GA-112's worktree. Frontend Agent's next run gets your changes, uncommitted.", btn("Done editing"));
const colFrame = (pnl, { first = SIDEBAR_LINE - 14, label, share } = {}) => `<div class="main">${edMain({ pnl, first, lines: 60 })}</div>`;
const pointer = (top) => `<svg class="dz-pointer" style="left:48%;top:${top}px" aria-hidden="true"><use href="#i-move-vertical"/></svg>`;
used.add("move-vertical");
section("editor", "4. The Editor with its terminal panel",
  p("The Editor's top bar gets a <b>Terminal</b> button between Fetch and Open in. It shows a panel in the code column, between the code and the status bar, with this folder's terminals as tabs; the file tree keeps its full height, as in VS Code's default layout. The status bar stays at the Editor's bottom edge, so it doesn't jump when the panel opens.")
  + notes(
    "<b>The button</b> is a ghost button like Fetch (Lucide <code>terminal</code>, “Terminal”); without Fetch (no origin remote) it comes first. Pressed (<code>aria-pressed</code>, the <code>selected</code> fill) while the panel shows. Its tooltip says “Show the terminal (Ctrl+`)” or “Hide the terminal (Ctrl+`)”. When the folder has no terminal yet it starts one in the folder's root, which gets the focus.",
    "<b>Off</b>, with the reason as its tooltip, while a terminal can't start there: an agent works on the card (“QA Agent is working on GA-118: a terminal there waits until its run ends.”), or the card isn't pinned to you and you haven't pressed Edit anyway (a shell can change files just as an edit can). Off is <code>aria-disabled</code>, not <code>disabled</code>, so the pointer and the keyboard still reach it and the tooltip shows.",
    "<b>A terminal opened before an agent started</b> stays, with the Editor's bar over it. Then the button still shows and hides the panel (it starts nothing), and the panel's <b>+</b> is off with the same reason.",
    "<b>The panel</b> is 30% of the column (<code>.ed-main</code>: the file tabs, the code, the panel and the status bar) the first time. Its top border is the <b>drag handle</b>: a 1px <code>line</code> with a 6px grab area, the <code>row-resize</code> pointer, 2px of <code>accent</code> on hover and while dragging. A double-click gives 30% back. Limits: at least 120px of code, and at least 120px of panel (its tabs and about 4 lines). The height is kept as a share, once for all folders (<code>gizai.editor.panel</code>), so it stays 30% when the window changes size; whether the panel shows is kept per folder, with its file tabs.",
    "<b>Its tabs</b>: this folder's terminals, as on the Terminal page, with <b>+</b>; on the right <b>Own page</b> (the terminal on its page, <code>#/terminal/&lt;id&gt;</code>) and <b>Hide</b> (Ctrl+`). Hiding keeps every shell running; closing the last tab hides the panel.",
    "<b>Keys in the panel</b> go to the shell: Ctrl+W, Ctrl+P and Ctrl+B don't reach the Editor while the terminal has the focus. Ctrl+` (hide), Ctrl+Shift+` (a new tab) and find still work.")
  + both(sidebar({ editorOn: true, on: null }) + editorPage(), { label: "gizai, the panel at 30%: cargo test ran in it" })
  + h3("The Terminal button: its four states")
  + strips([
    { label: "Closed: the panel hidden", inner: edTopStrip({ on: false }), tip: "Tooltip: Show the terminal (Ctrl+`)." },
    { label: "Open: pressed while the panel shows", inner: edTopStrip({ on: true }), tip: "Tooltip: Hide the terminal (Ctrl+`)." },
    { label: "Off: an agent works on the card (no terminal there yet)", inner: edTopStrip({ on: false, fetch: false, off: LIVE_WHY, is: "is-hover" }, LIVE_BAR, "GA-118", "gizai/ga-118-show-a-run-s-changes · 3 changed"), tip: `Tooltip: ${LIVE_WHY}` },
    { label: "Off: another's card, until Edit anyway", inner: edTopStrip({ on: false, fetch: false, off: NOTMINE_WHY }, NOTMINE_BAR, "GA-112", "gizai/ga-112-remember-the-open-tabs · 1 changed"), tip: `Tooltip: ${NOTMINE_WHY}` },
    { label: "After Edit anyway: it works", inner: edTopStrip({ on: false, fetch: false }, EDITING_BAR, "GA-112", "gizai/ga-112-remember-the-open-tabs · 2 changed") },
  ])
  + h3("The panel's height")
  + p("Each frame is the code column alone, 600px high. The share counts the whole column, so it holds when the window grows or shrinks; dragging stops at the limits, and the pointer keeps <code>row-resize</code> until the button is let go.")
  + rows([
    { label: "Hidden", inner: colFrame(""), w: 470, h: 600 },
    { label: "30%, the first time (and after a double-click)", inner: colFrame(panel({ share: "30%" })), w: 470, h: 600 },
    { label: "Dragged higher: 62%, the handle under the pointer", inner: colFrame(panel({ share: "62%", handleIs: "is-drag" })) + pointer(600 - 24 - 600 * 0.62 - 9), w: 470, h: 600 },
    { label: "Dragged lower: at its least, 120px (tabs and 4 lines)", inner: colFrame(panel({ share: "0%" })), w: 470, h: 600 },
    { label: "Dragged as high as it goes: 120px of code left", inner: colFrame(panel({ share: "100%" })), w: 470, h: 600 },
  ])
  + h3("An agent started after you opened a terminal there")
  + strips([
    { label: "GA-118: the bar says who works; the terminal stays; + is off", inner: `<div class="main"><div class="ed-page">${topbar(`<a>Editor</a><span class="sep">/</span><b class="mono">GA-118</b>`, standing("gizai/ga-118-show-a-run-s-changes · 3 changed"), edActions({ on: true, fetch: false }), "ed-crumbs")}${LIVE_BAR}<div class="ed"><div class="ed-main has-term">${panel({ share: "100%", tabsList: [{ name: "GA-118", card: true, on: true, busy: "less" }], body: term([`${P("~/.local/share/gizai/worktrees/GA/GA-118", "gizai/ga-118-show-a-run-s-changes")}git log --oneline -3`, "⟦yellow|5d0e2b7⟧ Show a run's changes in the run panel", "⟦yellow|a41c9f3⟧ Keep a run's range for its Changes", "⟦yellow|72d0fcb⟧ Merge pull request #110 from Yotech-AI/gizai/ga-124-research", `${P("~/.local/share/gizai/worktrees/GA/GA-118", "gizai/ga-118-show-a-run-s-changes")}less src/lib/changes.ts`]), plusOff: LIVE_WHY }).replace("--term-share:100%", "--term-share:100%;flex:1")}</div></div></div></div>`, h: 330 },
  ]));

// 5. One terminal, two places ------------------------------------------------------------------------------------------------------------
const away = (text) => `<div class="term-place"><div class="term-away">${ic("maximize-2")}<span>${text}</span><button class="btn sm">Show here</button></div></div>`;
section("two", "5. One terminal, two places",
  p("A terminal is one shell with one screen, which moves to the place that shows it: the Terminal page or its folder's panel in the Editor. Open gizai 2 from the sidebar and its page has it; go back to the Editor and the panel has it again, with everything on it: its scrollback, what runs in it, its selection. It takes the size of the place, 120 × 12 in the panel and 132 × 38 on the page, and a full-screen program (vim, htop) redraws at the new size.")
  + notes(
    "<b>The place that asked last shows the terminal.</b> Today one page shows at a time, so the place on screen always has it. When two places want it at once (a second window or a split, later), the other one shows a quiet line in the terminal's place, “gizai 2 is on its own page.”, with Show here, which moves it back.",
    "<b>The panel's tab says the same</b>: its icon becomes Own page's (Lucide <code>maximize-2</code>) and its tooltip reads “gizai 2 is on its own page.”. On the page it reads “gizai 2 is in gizai's Editor.”.",
    "<b>Own page</b> and <b>Show in Editor</b> are the two ways across; the sidebar's row stays the same row wherever the terminal shows.")
  + rows([
    { label: "The Editor's panel, while gizai 2 is on its own page", inner: colFrame(panel({ share: "40%", tabsList: GIZAI_TABS({ on: "gizai 2", away: true }), body: away(X.awayPage) })), w: 700, h: 520 },
    { label: "The Terminal page, while gizai 2 is in the Editor's panel", inner: termPage({ body: away(X.awayPanel) }), w: 700, h: 300 },
  ]));

// 6. The terminal's colours -------------------------------------------------------------------------------------------------------------
const ANSI = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
// text on a block: the background's colour, which reads on every block but black (dark) and white (light)
const blockText = (n) => (n === "black" ? "x-bright-white" : n === "white" ? "x-black" : "x-fg-bg");
const ansiGrid = `<div class="dz-ansi"><span class="head"></span>${ANSI.map((n) => `<span class="head">${n}</span>`).join("")}`
  + `<span class="head">Text</span>${ANSI.map((n) => `<span class="x-${n}">${n}</span>`).join("")}`
  + `<span class="head">Bright text</span>${ANSI.map((n) => `<span class="x-bright-${n}">bright ${n}</span>`).join("")}`
  + `<span class="head">Background</span>${ANSI.map((n) => `<span class="x-bg-${n} ${blockText(n)}">${n}</span>`).join("")}`
  + `<span class="head">Bright background</span>${ANSI.map((n) => `<span class="x-bg-bright-${n} ${blockText(n === "black" ? "bright-black" : n)}">bright ${n}</span>`).join("")}`
  + `<span class="head">Selected, find</span><span class="x-sel">selected</span><span class="x-sel-off">not focused</span><span class="x-hit">a match</span><span class="x-now">the current</span><span><span class="x-cursor">c</span>ursor</span><span><span class="x-cursor-off">c</span>ursor, no focus</span><span class="x-dim">dim</span><span class="x-bold">bold</span></div>`;
const sample = (lines, n) => `<div class="main"><div class="term-place" style="padding:8px 12px">${screen(lines)}</div></div>`;
const swatch = (hex) => `<span class="dz-sw" style="background:${hex}"></span><code>${hex}</code>`;
const TERM_ROWS = COLORS.map(([name, d, l, usage]) => {
  const cell = (theme, v) => {
    const hex = hexOf(theme, v);
    const bg = hexOf(theme, "{term-bg}"), sel = hexOf(theme, "{term-selection}");
    const k = (x) => `<span class="num${x < 4.5 ? " low" : ""}">${x.toFixed(2)}</span>`;
    const skip = ["term-bg", "term-selection", "term-selection-inactive", "term-cursor-text"].includes(name);
    return [`${v.startsWith("{") ? `= ${v.slice(1, -1)} · ` : ""}${swatch(hex)}`, skip ? (name === "term-cursor-text" ? `${k(contrast(hex, hexOf(theme, "{term-cursor}")))} on the cursor` : name === "term-bg" ? "" : `${k(contrast(hexOf(theme, "{term-fg}"), hex))} under term-fg`) : `${k(contrast(hex, bg))} · ${k(contrast(hex, sel))}`];
  };
  return [c(`--${name}`), ...cell("dark", d), ...cell("light", l), usage];
});
const APART_ROWS = APART.map((name) => [c(`--${name}`), ...["dark", "light"].flatMap((theme) => ["live", "needs"].map((s) => {
  const a = hexOf(theme, `{${name}}`), b = hexOf(theme, `{${s}}`);
  return `<span class="num">${hueGap(a, b).toFixed(0)}° · ΔE ${deltaE(a, b).toFixed(1)}</span>`;
}))]);
const README_LINE = "- The terminal's own colours (`term-*`) are what programs print, not Gizai's states: `term-cyan` is a sky blue and `term-magenta` a violet, at least 35° of hue (OKLCH) from `live` and `needs`, so a program's cyan never reads as an agent at work, nor its magenta as \"needs you\".";
section("colours", "6. The terminal's colours",
  p("Twenty-two new colour tokens, <code>term-*</code>, for dark and light, in <code>design/terminal/new-tokens.json</code> (<code>design/tokens.json</code>'s format; TF0 moves them there). xterm.js takes colours as strings, so TF0 reads each token with <code>getComputedStyle</code> when a terminal starts and after each theme change (<code>t</code>, Settings → Appearance), and an alias such as <code>{bg}</code> arrives resolved.")
  + notes(
    "<b>One surface with the code.</b> <code>term-bg</code> is <code>bg</code>, the code's background, and <code>term-fg</code> is <code>text</code>; the cursor is <code>text</code> as in the code, the selection the accent's blue, stronger than the code's, so a selection in output stands out.",
    "<b>Red and green are the Changes view's.</b> <code>term-red</code> is <code>diff-del</code> and <code>term-green</code> <code>diff-add</code>, so <code>git diff</code> in a terminal reads as a card's Changes. Yellow is the gold of <code>syn-type</code> and blue the blue of <code>syn-function</code>.",
    "<b>Cyan and magenta keep away from Gizai's two signals.</b> Teal <code>live</code> means an agent at work and magenta <code>needs</code> means “needs you”; a program's cyan and magenta are its own output. So <code>term-cyan</code> is a sky blue and <code>term-magenta</code> a violet: at least 35° of hue (OKLCH) and a ΔE2000 of at least 15 from both, in both themes. The generator refuses values that break this.",
    "<b>Contrast.</b> Every colour a program prints as text is at least 4.5:1 on <code>term-bg</code>. Bright colours are the stronger ones on each background: lighter in dark, darker in light. Three are block colours, under 4.5:1 by nature (black in dark; white and bright white in light); xterm.js's <code>minimumContrastRatio: 4.5</code> lifts any text that lands under 4.5:1, so a program's black text on the dark background still reads.",
    "<b>Never colour alone</b> holds in the terminal too: programs print words with their colours (“ok”, “FAILED”, “modified:”), and Gizai adds no colour of its own to their output. Where a program's own pair lands under 4.5:1, such as ls's black on green in light, xterm.js draws the text lighter or darker until it reads.")
  + h3("The tokens")
  + p("Contrast (WCAG 2) on <code>term-bg</code> · on <code>term-selection</code>; grey numbers are under 4.5:1. <code>=</code> follows another token. For the selection, the number is <code>term-fg</code> on it.")
  + table(["Token", "Dark", "Contrast", "Light", "Contrast", "Used for"], TERM_ROWS)
  + p("How far cyan and magenta are from Gizai's signals: the hue gap in OKLCH and the colour difference ΔE2000. The rule is at least 35° and 15.")
  + table(["Token", "Dark: from live", "Dark: from needs", "Light: from live", "Light: from needs"], APART_ROWS)
  + h3("The sixteen colours, and the selection, find and the cursor")
  + rows([{ label: "On term-bg", inner: `<div class="main">${ansiGrid}</div>`, w: 1100 }])
  + h3("Samples")
  + rows([
    { label: "ls --color (folders bold blue, executables bold green, an archive bold red, an image bold magenta, a symbolic link bold cyan, a shared folder black on green)", inner: sample(LS), w: 900 },
    { label: "git status", inner: sample(GIT_STATUS), w: 700 },
    { label: "git diff", inner: sample(GIT_DIFF), w: 760 },
    { label: "A test run: cargo test, a warning and a failure", inner: sample(CARGO), w: 820 },
    { label: "Coloured blocks: php artisan serve", inner: sample(ARTISAN), w: 640 },
  ])
  + h3("The rule for design/README.md")
  + p("Added under “Colour carries meaning, never decoration”, after the line on <code>needs</code>:")
  + `<blockquote class="dz-quote">${esc(README_LINE.slice(2)).replace(/`([^`]+)`/g, "<code>$1</code>")}</blockquote>`);

// 7. Specs ------------------------------------------------------------------------------------------------------------------------------------
const gallery = (items, width = 262) => `<div class="variants"><div class="vrow dark"><div class="vhead theme-name">Dark</div><div class="vitems">`
  + frame("dark", `<table class="dz-gallery"><tbody>${items.map(([name, html, what]) => `<tr><th>${name}</th><td style="width:${width}px">${html}</td><td>${what}</td></tr>`).join("")}</tbody></table>`, { cls: "strip gal" })
  + `</div></div></div>`;
const inSide = (html) => `<div class="side gal-cell"><div class="nav-list">${html}</div></div>`;
const inTabs = (list) => `<div class="gal-cell col">${ttabs(list, { acts: "" })}</div>`;
const inBar = (html) => `<div class="gal-cell bar">${html}</div>`;
const inHandle = (is) => `<div class="gal-cell col" style="height:96px"><div class="ed-code" style="flex:1;padding:0"></div>${handle(30, is)}<div class="ed-tabs term-tabs" style="flex:none"></div><div class="term-place" style="flex:1"></div></div>`;
const NEW = '<span class="badge info">New</span>';
const sizeTable = (list) => table(["Part", "Size", "Token", ""], list.map(([a, b, tk, n]) => [a, b, tk, n ? NEW : ""]));
section("specs", "7. Specs: sizes, states, the font, keys",
  p("Sizes are tokens: the five new ones are in <code>new-tokens.json</code> (and <code>terminal-tokens.css</code>, which shows how they grow with the density and the Interface text size, as <code>size-tab</code> does); the rest are the app's spacing, text and radius tokens. States follow the design system: hover the <code>hover</code> fill, selected the <code>selected</code> fill, focus a 2px <code>focus</code> ring, off at 45%.")
  + h3("The sidebar's rows")
  + sizeTable([
    ["Row", "The app's <code>.nav-item</code>: 32px high (grows with the density), 8px sides, 10px between icon and name", c(".nav-item"), ""],
    ["Icon", "16px Lucide <code>terminal</code>, <code>text-3</code>; <code>text</code> on hover and selected", c(".icon"), ""],
    ["Name", "13.5px, 500, <code>text-2</code>; <code>text</code> on hover and selected. A card: its identifier mono 12.5px, its title <code>text-3</code>", `${c("--fs")}, ${c("--fs-sm")}`, ""],
    ["What runs", "11.5px <code>text-3</code>: the program in mono, “ended” in the interface font", c("--fs-xs"), ""],
    ["Close question", "The tree's <code>.ed-confirm</code>, 24px in, 12.5px", `${c("--space-6")}, ${c("--fs-sm")}`, ""],
  ])
  + gallery([
    ["Default", inSide(termRow(TERMS[0])), "Name text-2, icon text-3"],
    ["Hover", inSide(termRow(TERMS[0], { hover: "gizai" })), "hover fill, name and icon text"],
    ["Selected", inSide(termRow(TERMS[1], { on: "gizai 2" })), "Its page shows: selected fill"],
    ["Focus", inSide(termRow(TERMS[0], { focus: "gizai" })), "A 2px focus ring, 1px outside; F2 renames, the context menu key opens the menu"],
    ["A program runs", inSide(termRow(TERMS[1])), "The program, mono, text-3"],
    ["Ended", inSide(termRow(TERMS[2])), "“ended”; the card's identifier mono, its title faint"],
    ["Off", inSide(termRow(TERMS[0])), "A row is never off: a terminal you can see you can open"],
  ])
  + h3("The tabs (the Terminal page and the panel)")
  + sizeTable([
    ["Strip", "32px high (28px compact), on <code>side</code>, a <code>line</code> under it", c("--size-term-tab"), true],
    ["Tab", "10px left, 4px right, 6px between its parts; at most 240px, the name cut off", `${c("--space-1")}, ${c("--space-3")}`, ""],
    ["Icon, name, what runs", "14px icon <code>text-3</code>; 13.5px name; the program mono 11.5px <code>text-3</code>", `${c("--fs")}, ${c("--fs-xs")}`, ""],
    ["×", "20px button, 14px icon; shows on hover, focus and on the open tab", c("--radius-s"), ""],
    ["Open tab", "<code>bg</code>, joining the terminal, a 2px <code>accent</code> line on top", "", ""],
    ["+, Own page, Hide", "24px icon button; the app's ghost <code>btn sm</code> (26px)", c("--radius"), ""],
  ])
  + gallery([
    ["Default", inTabs([{ name: "gizai" }]), "Name text-2; × hidden"],
    ["Hover", inTabs([{ name: "gizai", is: "is-hover" }]), "hover fill, name text, × shows"],
    ["Selected", inTabs([{ name: "gizai 2", busy: "npm", on: true }]), "bg (joins the terminal), a 2px accent line on top, × shows"],
    ["Focus", inTabs([{ name: "gizai", is: "is-focus" }]), "A 2px focus ring inside the tab; ← and → move between tabs"],
    ["Ended", inTabs([{ name: "KADE-41", card: true, ended: true }]), "“ended” in place of the program"],
    ["On its own page", inTabs([{ name: "gizai 2", away: true }]), "Own page's icon; the tooltip says where it is"],
    ["Off", `<div class="gal-cell col">${ttabs([{ name: "GA-118", card: true, on: true }], { plusOff: LIVE_WHY })}</div>`, "Tabs are never off; + is, while an agent works on the card (45%, the reason as its tooltip)"],
  ], 360)
  + h3("The panel and its drag handle")
  + sizeTable([
    ["First height", "30% of the code column (<code>.ed-main</code>)", c("--size-term-panel"), true],
    ["Least height", "120px: its tabs and 4 lines, 4px above and below them", c("--size-term-panel-min"), true],
    ["Code's least height", "120px (6 lines) while the panel shows", c("--size-term-code-min"), true],
    ["The terminal", "<code>term-bg</code>; 4px 12px in the panel, 8px 12px on the page", `${c("--space-1")}, ${c("--space-2")}, ${c("--space-3")}`, ""],
    ["Handle: line", "1px <code>line</code> (the panel's top border); 2px <code>accent</code> on hover and while dragging, 2px <code>focus</code> with the keyboard", "", ""],
    ["Handle: grab area", "6px, centred on the line; <code>row-resize</code> pointer", c("--size-handle"), true],
    ["Handle: keys", "↑ and ↓ move it 5% when it has the focus; a double-click gives 30% back; <code>role=\"separator\"</code>, the share in <code>aria-valuenow</code>", "", ""],
  ])
  + gallery([
    ["Default", inHandle(""), "1px line"],
    ["Hover", inHandle("is-hover"), "2px accent, row-resize pointer"],
    ["Dragging", inHandle("is-drag"), "2px accent while the button is down; the panel follows, within the limits"],
    ["Focus", inHandle("is-focus"), "2px focus (Tab reaches it after the code); ↑ ↓ 5%"],
    ["Off", inHandle(""), "Never off: an agent at work doesn't hide or fix the panel"],
  ], 360)
  + h3("The Terminal button")
  + sizeTable([
    ["Button", "The app's ghost <code>btn sm</code>, like Fetch: 26px high, 9px sides, 6px between icon and word", `${c(".btn.ghost.sm")}, ${c("--radius")}`, ""],
    ["Icon, word", "15px Lucide <code>terminal</code>; “Terminal” 12.5px", c("--fs-sm"), ""],
    ["Pressed", "<code>selected</code> fill, <code>text</code>; <code>aria-pressed=\"true\"</code>", "", ""],
    ["Off", "45%; <code>aria-disabled</code>, so the tooltip with the reason still shows", "", ""],
  ])
  + gallery([
    ["Default", inBar(termBtn()), "text-2, no fill"],
    ["Hover", inBar(termBtn({ is: "is-hover" })), "hover fill, text"],
    ["Selected (pressed)", inBar(termBtn({ on: true })), "The panel shows: selected fill, text"],
    ["Focus", inBar(termBtn({ is: "is-focus" })), "A 2px focus ring, 1px outside"],
    ["Off", inBar(termBtn({ off: LIVE_WHY })), "45%; the tooltip says why (an agent at work, or Edit anyway first)"],
  ], 200)
  + h3("The font")
  + notes(
    "<b>The app's mono font at the code size</b>: <code>--font-mono</code>, 12.5px on 20px lines (<code>t-code</code>), growing with the Interface text size as the code does (<code>calc(12.5px + var(--ui-grow))</code>). It follows Settings → Appearance (Atkinson Hyperlegible Mono, JetBrains Mono, Geist Mono or Hack); TF0 waits for the font to load before xterm.js measures its cells, and fits again when it changes.",
    "<b>xterm.js's options</b>: <code>fontFamily</code> the computed <code>--font-mono</code>, <code>fontSize</code> 12.5 (+ the Interface step), <code>lineHeight</code> 1.6, <code>fontWeight</code> 400, <code>fontWeightBold</code> 700, <code>drawBoldTextInBrightColors</code> on, <code>minimumContrastRatio</code> 4.5, <code>cursorStyle</code> block, <code>cursorInactiveStyle</code> outline, <code>cursorBlink</code> off, <code>scrollback</code> 5000, <code>customGlyphs</code> and <code>rescaleOverlappingGlyphs</code> on.",
    "<b>Box drawing</b> (<code>tree</code>, htop, <code>git log --graph</code>): the bundled fonts are cut to Latin and have none. With WebGL xterm.js draws them itself (<code>customGlyphs</code>). For the DOM renderer, TF0 adds a box-drawing subset (U+2500–259F) of JetBrains Mono, which the app ships already under the OFL, as a fallback after <code>--font-mono</code> for the terminal only, and checks that <code>tree</code> lines up.",
    "<b>No italic or bold of Gizai's own</b>: a program that asks for bold gets the bright colour and the bold weight; one that asks for italic gets the browser's slant, since the fonts have no italic.")
  + table(["xterm.js's theme", "Token"], [
    ["foreground, background", `${c("term-fg")}, ${c("term-bg")}`], ["cursor, cursorAccent", `${c("term-cursor")}, ${c("term-cursor-text")}`],
    ["selectionBackground, selectionInactiveBackground", `${c("term-selection")}, ${c("term-selection-inactive")} (no selectionForeground: text keeps its colours)`],
    ["black … white, brightBlack … brightWhite", `${c("term-black")} … ${c("term-white")}, ${c("term-bright-black")} … ${c("term-bright-white")}`],
    ["scrollbarSliderBackground, …HoverBackground, …ActiveBackground", `${c("line-2")}, ${c("text-3")}, ${c("text-3")}`],
    ["Find's decorations: matchBackground, activeMatchBackground, activeMatchBorder", `${c("term-selection")}, ${c("term-selection")}, ${c("accent")}`],
  ])
  + h3("Keys (research §5.2)")
  + p("Cmd for Ctrl on macOS where the table says so. A key the terminal keeps for Gizai is let through with <code>attachCustomKeyEventHandler</code>; every other key goes to the shell.")
  + table(["Key", "Where", "What it does"], [
    ["Ctrl+` (all three systems)", "the Editor, also from inside its terminal", "shows or hides the terminal panel, as VS Code"],
    ["Ctrl+Shift+`", "the Editor; the Terminal page", "a new terminal in the folder: a new tab in the panel, or on the page"],
    ["Ctrl+Shift+C, Ctrl+Shift+V (Linux); Ctrl+C with a selection, Ctrl+V (Windows); Cmd+C, Cmd+V (macOS)", "in a terminal", "copy and paste, as VS Code on each system; without a selection Ctrl+C is the program's interrupt"],
    ["Shift+Insert (Linux, Windows)", "in a terminal", "paste"],
    ["Ctrl+Shift+F (Cmd+F on macOS)", "in a terminal", "find in its scrollback"],
    ["Ctrl+PgDn, Ctrl+PgUp", "in a terminal", "the next and previous terminal tab"],
    ["Ctrl+click (Cmd+click on macOS)", "on a link", "opens an http or https link in the browser"],
    ["every other key", "in a terminal", "goes to the shell: Ctrl+W, Ctrl+P, Ctrl+B, Ctrl+K, Ctrl+R and Escape do what the shell or the program wants, not what the Editor or Gizai would"],
  ].map(([k, w, d]) => [`<span class="kbd-run">${k}</span>`, w, d]))
  + p("And the screens' own keys:")
  + table(["Key", "Where", "What it does"], [
    ["Enter, Shift+Enter, Escape", "find", "the next match, the previous match, close find (the focus goes back to the terminal)"],
    ["F2; the context menu key or Shift+F10", "a sidebar row", "renames it in place; opens the row's menu"],
    ["Double-click", "a tab, the page's name", "renames it in place: Enter keeps, Escape cancels"],
    ["Middle click", "a row, a tab", "closes the terminal, asking first when a program runs"],
    ["← →", "a tab with the focus", "the next and previous tab"],
    ["↑ ↓; double-click", "the drag handle", "5% higher or lower; 30% again"],
    ["Enter, Escape", "the close question", "Close, Cancel"],
  ].map(([k, w, d]) => [`<span class="kbd-run">${k}</span>`, w, d]))
  + h3("On a narrow window")
  + notes(
    "At the window's smallest, 900 × 600, 652px stay beside the sidebar. The Terminal page's top bar keeps its name and buttons; the folder's path gives way first (cut off, the whole of it in its tooltip), then the shell and size.",
    "The Editor's top bar keeps Fetch, Terminal and Open in with their words; D2's standing gives way first, as it does now.",
    "The panel's tab strip scrolls sideways when its tabs don't fit, with the open tab scrolled into view; Own page and Hide stay at its right end. At 600px high the column is 548px: the panel's 30% is 164px, within its limits.")
  + h3("For screen readers and the keyboard")
  + notes(
    "The sidebar's rows are links (“gizai 2, running npm”); the tabs a <code>tablist</code>, each × a button named “Close gizai 2”. The question under a row or tab is an <code>alertdialog</code> that takes the focus, Close first.",
    "The terminal is xterm.js's own: TF0 turns on <code>screenReaderMode</code> when the system asks for it, so its rows are read as text.",
    "The handle is a <code>separator</code> with the share in <code>aria-valuenow</code> and its name “Terminal panel's height”; the Terminal button says pressed or not; off, it stays focusable and its reason is its description.",
    "Ended, couldn't start and the 20-open note are <code>status</code> (couldn't start is an <code>alert</code>), so they are read out when they appear."));

// 8. Every text -----------------------------------------------------------------------------------------------------------------------------
section("texts", "8. Every text",
  p("Every heading, row, button, tooltip and sentence on these screens, in sentence case, in the order of the page. Names, paths and numbers are the examples'; TF0, TF1 and TF2 put in the real ones. The words on a sidebar row (“npm”, “ended”) stay lowercase, like Agents' “paused” and the Editor's “4 changed”. Group heads in the picker are set in capitals by CSS, as the Editor's are.")
  + table(["Where", "When", "Text", "Buttons, tooltips"], TEXTS.map((x) => [esc(x.where), esc(x.when), x.text ? esc(x.text) : "<span class=\"faint\">(nothing)</span>", esc(x.buttons)])));

// ---------- the page ----------
const toc = sections.map((s) => `<a href="#${s.id}">${s.title}</a>`).join("");
const body = `<main class="dz">
<header class="dz-head">
  <div class="dz-kicker"><span class="badge outline">GA-132</span><span>Terminal · TD</span><span class="faint">for Jefsev's review</span></div>
  <h1>The terminal</h1>
  <p class="dz-lead">The design of GA-124's research §5.1, §5.2 and §5.4 (<code>docs/research/terminal.md</code>): the sidebar's Terminal section, New terminal, the Terminal page, the Editor's terminal panel, one terminal in two places, the terminal's colours and the specs. TF0 builds the terminal's view and moves the tokens into the app, TF1 the sidebar and the page, TF2 the Editor's panel.</p>
  <p class="dz-p">Every screen is drawn with the app's own stylesheets (<code>src/styles</code>), so it looks as the app will; the new components are in <code>terminal.css</code> and the new tokens in <code>terminal-tokens.css</code> (from <code>new-tokens.json</code>). The terminal's screen is a still picture of xterm.js in the <code>term-*</code> colours. Each screen shows dark, then light; the switch shows one theme. Hover a control for its tooltip. Built by <code>node design/terminal/gen-terminal.mjs</code>.</p>
  <div class="dz-show" role="group" aria-label="Show"><span class="faint">Show</span><div class="seg"><button data-v="both">Dark and light</button><button data-v="dark">Dark</button><button data-v="light">Light</button></div></div>
  <nav class="dz-toc" aria-label="Contents">${toc}</nav>
</header>
${sections.map((s) => s.html).join("\n")}
</main>`;
const SCRIPT = `// Each screen is written once, in dark; its light twin is made here, so the file holds every screen once.
document.querySelectorAll(".shot.dark, .vrow.dark").forEach((dark) => {
  const light = dark.cloneNode(true);
  light.classList.replace("dark", "light");
  light.querySelectorAll('[data-theme="dark"]').forEach((w) => { w.dataset.theme = "light"; });
  light.querySelectorAll(".theme-name").forEach((n) => { n.textContent = "Light"; });
  dark.after(light);
});
const KEY = "gizai.design.show";
function show(v) {
  const root = document.documentElement;
  root.dataset.show = v;
  root.dataset.theme = v === "light" ? "light" : "dark";
  document.querySelectorAll(".dz-show button").forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.v === v)));
  try { localStorage.setItem(KEY, v); } catch {}
}
document.querySelectorAll(".dz-show button").forEach((b) => b.addEventListener("click", () => show(b.dataset.v)));
let kept = null;
try { kept = localStorage.getItem(KEY); } catch {}
show(kept || "both");`;
const html = `<!doctype html>
<html lang="en" data-theme="dark" data-show="both">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Gizai · Terminal TD: terminals in the sidebar, on their own page and in the Editor</title>
<!-- Generated by design/terminal/gen-terminal.mjs: change that file, then run it. -->
<!-- The app's stylesheets in main.tsx's order. terminal-tokens.css comes first, so the app's own tokens win once they have its names. -->
<link rel="stylesheet" href="terminal-tokens.css">
<link rel="stylesheet" href="../../src/styles/fonts.css">
<link rel="stylesheet" href="../../src/styles/tokens.css">
<link rel="stylesheet" href="../../src/styles/components.css">
<link rel="stylesheet" href="../../src/styles/app.css">
<link rel="stylesheet" href="../../src/styles/changes.css">
<link rel="stylesheet" href="../../src/styles/appearance.css">
<link rel="stylesheet" href="terminal.css">
<!-- This page only: its layout around the frames, and stand-ins for what CodeMirror and xterm.js draw -->
<link rel="stylesheet" href="terminal-page.css">
</head>
<body>
<svg width="0" height="0" style="position:absolute" aria-hidden="true">
${await sprite()}
</svg>
${body}
<script>
${SCRIPT}
</script>
</body>
</html>
`;
fs.writeFileSync(`${HERE}/terminal.html`, html);

// README.md's tables, between its markers
const README = `${HERE}/README.md`;
if (fs.existsSync(README)) {
  const md = fs.readFileSync(README, "utf8");
  const val = (theme, v) => v.startsWith("{") ? `= ${v.slice(1, -1)} (\`${hexOf(theme, v)}\`)` : `\`${v}\``;
  const ratio = (theme, name) => ["term-bg", "term-selection", "term-selection-inactive", "term-cursor-text"].includes(name) ? "" : ` · ${contrast(hexOf(theme, `{${name}}`), hexOf(theme, "{term-bg}")).toFixed(2)}`;
  const tables = "| Token | Dark | Light |\n|---|---|---|\n" + COLORS.map(([n, d, l]) => `| \`--${n}\` | ${val("dark", d)}${ratio("dark", n)} | ${val("light", l)}${ratio("light", n)} |`).join("\n")
    + "\n\n| Size | Value | For |\n|---|---|---|\n" + SIZES.map(([n, v, u]) => `| \`--${n}\` | ${v} | ${u} |`).join("\n");
  const out = md.replace(/(<!-- tables: generated by gen-terminal\.mjs -->)[\s\S]*?(<!-- \/tables -->)/, (_, open, close) => `${open}\n${tables}\n${close}`);
  if (out !== md) fs.writeFileSync(README, out);
}
console.log(`wrote design/terminal/terminal.html: ${sections.length} sections, ${TEXTS.length} texts, ${used.size} icons, ${(html.length / 1024).toFixed(0)} KB; new-tokens.json (${COLORS.length} colours, ${SIZES.length} sizes); terminal-tokens.css`);
