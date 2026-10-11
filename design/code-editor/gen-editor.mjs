// Builds design/code-editor/editor.html: the design of the Editor (GA-105, card D2 of the Code Editor; research §5.2 on the
// branch gizai/ga-91-research-for-the-code-editor-a-card-s-ch) as one page that opens from disk in a browser. It is drawn
// with the app's own stylesheets (src/styles), the new components in editor.css and the tokens in editor-tokens.css, every
// screen in dark and then light. Icons are Lucide's own data (lucide-react, ISC), as in design/gen-design-system.mjs. TSX
// is coloured by @lezer/javascript, the parser CodeMirror uses, through the tag → token map TAG_TOKENS (for F0's code
// theme); PHP by a small highlighter of this file's own, since @lezer/php comes with F0. Every text the screens show is in
// TEXT, in the page's order: the frames use it and the table "Every text" lists it.
// usage: node design/code-editor/gen-editor.mjs
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

// Status glyphs: Gizai's own, the paths of src/components/StatusIcon.tsx and design/gen-design-system.mjs.
const GLYPH = {
  ready: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/>',
  in_progress: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><path d="M8 4.25a3.75 3.75 0 0 1 0 7.5z" fill="currentColor"/>',
  testing: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><path d="M8 8V4.25a3.75 3.75 0 1 1-3.75 3.75z" fill="currentColor"/>',
  review: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><circle cx="8" cy="8" r="2.4" fill="currentColor"/>',
};
const ST_NAME = { ready: "To do", in_progress: "In progress", testing: "Testing", review: "Review" };
const st = (cat) => `<svg class="st st-${cat}" viewBox="0 0 16 16" fill="none" role="img" aria-label="${ST_NAME[cat]}">${GLYPH[cat]}</svg>`;
const pri = (p) => `<span class="pri ${p}" title="${p[0].toUpperCase() + p.slice(1)}"><i></i><i></i><i></i></span>`;
const kbd = (k) => `<span class="kbd">${k}</span>`;

// ---------- every text: where, when, the text and its buttons. The frames use these; the last table lists them. ----------
const TEXTS = [];
const say = (where, when, text, buttons = "") => { TEXTS.push({ where, when, text, buttons }); return text; };
const B3 = "<b>3 commits behind origin/main</b>, fetched 2m ago.";
const X = {
  // 1. sidebar
  sideHead: say("Sidebar", "Always, between Projects and Agents", "Editor", "+ (Open a folder) · caret (Fold Editor, Show Editor)"),
  sideFolded: say("Sidebar", "Folded, while folders are behind", "2 behind", "Tooltip: 2 folders are behind origin/main"),
  sideEmpty: say("Sidebar", "No folder opened yet", "Open a folder"),
  sideBehind: say("Sidebar row", "The folder is behind main", "3 behind", "Tooltip: main is 3 commits behind origin/main, fetched 2m ago"),
  sideChanged: say("Sidebar row", "Not behind; files changed", "4 changed", "Tooltip: 4 files changed, not committed"),
  sideAhead: say("Sidebar row", "Neither; main has its own commits", "1 ahead", "Tooltip: main has 1 commit that origin/main doesn't"),
  sideByHand: say("Sidebar row", "A person's card (its worktree)", "by hand", "Tooltip: KADE-41 Fix the invoice total. Pinned to you: agents leave it alone"),
  sideLive: say("Sidebar row", "A card an agent works on now", "read-only", "With the pulse. Tooltip: QA Agent is working on GA-118: read-only until its run ends"),
  sideUpdating: say("Sidebar row", "Update runs", "updating", "Tooltip: Updating: move main 2 commits to 9e1d4c0 and run composer install"),
  sideMenu: say("Sidebar row, right click", "Always", "Open in VS Code · Open in Zed · Open in PhpStorm · Copy path · Remove from the list", "A card's row adds Open card"),
  // 2. folder picker
  pickFind: say("Folder picker", "Always", "Find a folder", "Placeholder of its search box"),
  pickGroups: say("Folder picker", "Group heads", "Projects · In ~/Herd · Cards", "One “In …” group per folder in Settings → General → Editor folders"),
  pickMine: say("Folder picker, Cards", "A card pinned to you", "by hand"),
  pickLive: say("Folder picker, Cards", "An agent works on the card", "QA Agent is working", "With the pulse"),
  pickNone: say("Folder picker", "The search finds nothing", "No folder found"),
  pickNoRoot: say("Folder picker", "A folder of Editor folders isn't there", "~/Herd isn't there. Change it in Settings → General → Editor folders."),
  pickNothing: say("Folder picker", "Nothing to open at all", "No folders yet. Link a project to its folder (project page → Edit), or say where your projects are in Settings → General → Editor folders."),
  pickFoot: say("Folder picker", "Always, at the bottom", "Projects' folders, the folders in ~/Herd and card worktrees. More places: Settings → General → Editor folders."),
  // 3. the Editor page
  pageEmpty: say("Editor page", "#/editor, no folder open", "Open a folder to work on its files: a project's folder, a folder in ~/Herd, or a card's worktree.", "Open a folder"),
  pageGone: say("Editor page", "The folder can't open", "Gizai can't open ~/Herd/old-site: it isn't there any more.", "Remove from the list"),
  standOk: say("Top bar", "Up to date", "main · up to date with origin/main · 4 changed"),
  standAll: say("Top bar", "Behind, ahead and changed", "main · 3 behind origin/main · 1 ahead · 2 changed"),
  standNever: say("Top bar", "Never fetched", "main · not fetched yet"),
  standNoRemote: say("Top bar", "A folder without an origin remote", "main · no remote", "Tooltip: kade-portal has no origin remote: Gizai can't tell whether it's behind"),
  standDetached: say("Top bar", "No branch checked out", "no branch (3f5c1a2)"),
  standNotGit: say("Top bar", "Not a git repository", "Not a git repository", "No git marks, Fetch or Update"),
  standCard: say("Top bar", "A person's card", "gizai/kade-41-fix-the-invoice-total · 2 changed · 1 not pushed"),
  fetch: say("Top bar", "Always (not for a folder without a remote)", "Fetch", "Tooltip: Fetch origin/main now (last fetched 2m ago). While it runs: Fetching…"),
  openIn: say("Top bar → Open in", "The editors found on this computer", "Open gizai in · VS Code · Zed · PhpStorm", "Each with the command found (code, zeditor, phpstorm). Note: At Sidebar.tsx, line 75"),
  openInNone: say("Top bar → Open in", "None found", "No other editor found. Gizai looks for VS Code (code), Zed (zed, zeditor, zedit) and PhpStorm (phpstorm, phpstorm.sh)."),
  goto: say("File tree", "Always", "Go to file", "Ctrl P"),
  treeHead: say("File tree", "Over the tree", "New file · New folder · Fold every folder", "Icon buttons, these are their tooltips"),
  treeFold: say("File tree", "Its fold button", "Fold the file tree (Ctrl+B)", "Folded: Show the file tree (Ctrl+B), on the left of the tabs"),
  treeMenu: say("File tree, right click", "On a file or folder", "New file · New folder · Rename F2 · Delete Delete · Copy path · Open in VS Code · Open in Zed · Open in PhpStorm", "In an ignored folder New, Rename and Delete are off: Ignored by git: read-only"),
  treeNew: say("File tree", "New file, as you type", "Name, or a path like app/Exports/InvoicesCsv.php", "Placeholder; Enter makes it, Escape cancels"),
  treeExists: say("File tree", "New file or Rename", "InvoicesCsv.php is already there."),
  treeGit: say("File tree", "New file or Rename", "Gizai never writes in .git."),
  treeOutside: say("File tree", "New file or Rename", "That path leads outside kade-portal."),
  treeDelete: say("File tree", "Delete, asked under the row", "Delete Invoice.php from disk?", "Delete · Cancel"),
  treeDeleteDir: say("File tree", "Delete a folder", "Delete the folder Exports and the 3 files in it?", "Delete · Cancel"),
  treeLoading: say("File tree", "A folder opens slowly", "Loading…"),
  treeEmptyDir: say("File tree", "An empty folder opens", "Empty folder"),
  treeCantRead: say("File tree", "A folder can't be read", "Can't read this folder: permission denied."),
  tipIgnoredDir: say("File tree tooltip", "An ignored folder", "Ignored by git: read-only, and search skips it"),
  tipIgnoredFile: say("File tree tooltip", "An ignored file elsewhere (.env)", "Ignored by git: it opens and saves as usual, and isn't committed"),
  tipDot: say("File tree tooltip", "A folder that holds changes", "Changes inside"),
  tipMarks: say("File tree, tab tooltips", "git's mark", "Modified · Added · Renamed from app/Models/Bill.php · Untracked: new, not in git yet · Conflicted: finish the merge in a terminal"),
  tabClose: say("Tab", "Its ×", "Close (Ctrl+W)", "Middle click closes too"),
  tabAsk: say("Tab", "Closing a tab that isn't saved", "Sidebar.tsx has changes that aren't saved.", "Save and close · Close without saving · Cancel"),
  barLive: say("Folder bar", "An agent works on the card now", "QA Agent is working on GA-118: its worktree is read-only until the run ends.", "Open card"),
  barIdle: say("Folder bar", "Another's card, no agent at work", "GA-112 is Frontend Agent's card: its worktree is read-only here. Edit anyway puts your changes in it, uncommitted, for the agent's next run.", "Edit anyway · Open card"),
  barEditing: say("Folder bar", "After Edit anyway", "You're editing GA-112's worktree. Frontend Agent's next run gets your changes, uncommitted.", "Done editing"),
  barTaken: say("Folder bar", "An agent starts while you edit", "Frontend Agent started working on GA-112: its worktree is read-only again. 1 tab has changes that weren't saved: copy what you need, then close it.", "Open card"),
  fileBig: say("File bar", "A file over 1 MB", "Read-only: this file is over 1 MB (3.4 MB). Change it in your own editor.", "Open in"),
  fileUtf8: say("File bar", "Not UTF-8 text", "Read-only: this file isn't UTF-8 text, and Gizai keeps files as they are.", "Open in"),
  fileIgnored: say("File bar", "In an ignored folder", "Read-only: it's in vendor/, which git ignores."),
  fileTooBig: say("Editor area", "A file over 5 MB", "Too big to show here (7.2 MB).", "Open in VS Code"),
  fileBinary: say("Editor area", "A binary file", "A binary file (48 KB): Gizai shows text only.", "Open in VS Code"),
  fileGone: say("Editor area", "The open file was deleted, no changes", "app/Exports/OldCsv.php isn't there any more.", "Close tab"),
  editorEmpty: say("Editor area", "No tab open", "Open a file from the tree, or press Ctrl+P to go to one."),
  saveFailed: say("File bar", "Save failed", "Couldn't save Invoice.php: you have no permission to write it. Your changes are still here.", "Try again"),
  stSaved: say("Status bar", "Saved", "Saved"),
  stUnsaved: say("Status bar", "Not saved", "Not saved", "A button: Save (Ctrl+S)"),
  stSaving: say("Status bar", "Saving", "Saving…"),
  stRO: say("Status bar", "Read-only", "Read-only: over 1 MB", "Or: ignored by git · not UTF-8 · QA Agent is working on GA-118"),
  stIgnored: say("Status bar", "An ignored file (.env)", "Ignored by git", "Where git's mark goes"),
  stReloaded: say("Status bar", "An untouched tab reloaded", "Reloaded: changed on disk", "For 5 seconds"),
  stFailed: say("Status bar", "Save failed", "Couldn't save"),
  stCursor: say("Status bar", "Always", "Ln 75, Col 79", "With a selection: Ln 75, Col 79 (12 selected). A button: Go to line (Ctrl+Alt+G)"),
  stEol: say("Status bar", "Always", "LF", "Or CRLF; UTF-8 with BOM when the file has one. Tooltip: Line endings, kept as the file has them"),
  stLang: say("Status bar", "Always", "TypeScript JSX", "PHP · Blade · Rust · Plain text …, from F0's language map"),
  stGit: say("Status bar", "The file's git status", "Modified", "Added · Renamed · Untracked · Conflicted; nothing when unchanged"),
  find: say("Find (Ctrl+F)", "Its two boxes", "Find · Replace", "Placeholders"),
  findButtons: say("Find (Ctrl+F)", "Its buttons", "Replace · Replace all", "Toggles: Match case · Words · Regular expression. Previous match (Shift+F3) · Next match (F3) · Close (Escape)"),
  findCount: say("Find (Ctrl+F)", "After the box", "3 of 12", "No results · Not a valid regular expression"),
  gotoBox: say("Go to file (Ctrl+P)", "Its box", "Go to file in gizai", "Nothing found: No file found"),
  // 4. search in the folder
  searchBox: say("Search in the folder", "Its box", "Search in kade-portal", "Toggles: Match case · Words · Regular expression"),
  searchHint: say("Search in the folder", "Before a search", "Searches the files git tracks and new files it doesn't ignore. Ignored files (node_modules/, vendor/, .env) are left out."),
  searchSum: say("Search in the folder", "Results", "8 results in 4 files"),
  searchBusy: say("Search in the folder", "While it searches", "Searching…"),
  searchNone: say("Search in the folder", "Nothing found", "No results. Ignored files (node_modules/, vendor/, .env) aren't searched."),
  searchCap: say("Search in the folder", "2,000 results", "2,000 results shown: narrow the search for more."),
  searchTime: say("Search in the folder", "After 10 seconds", "Search stopped after 10 seconds, at 1,204 results: narrow it for the rest."),
  searchBad: say("Search in the folder", "A bad regular expression", "Not a valid regular expression: missing )."),
  searchFailed: say("Search in the folder", "git grep failed", "Search failed: git grep couldn't read app/Legacy (permission denied)."),
  // 5. behind main and Update
  behindCan: say("Behind bar", "Behind; Update can run", `${B3} Update will move main 3 commits to 319bf0b and run composer install and npm ci.`, "Update"),
  behindOne: say("Behind bar", "1 behind, no install needed", "<b>1 commit behind origin/main</b>, fetched just now. Update will move main 1 commit to 7c2e0aa.", "Update"),
  refDirty: say("Behind bar", "Refused: uncommitted changes", `${B3} Update can't run: kade-portal has uncommitted changes in app/Models/Invoice.php and routes/web.php. Commit or stash them first.`, "Update (off, the same sentence as its tooltip)"),
  refMerge: say("Behind bar", "Refused: a merge, rebase, cherry-pick or revert", `${B3} Update can't run: a merge is in progress in kade-portal. Finish or abort it first.`, "Update (off)"),
  refBranch: say("Behind bar", "Refused: another branch", `${B3} Update can't run: kade-portal is on branch feature/pdf-export, not main. Switch to main first, in a terminal or your own editor.`, "Update (off)"),
  refDetached: say("Behind bar", "Refused: no branch", `${B3} Update can't run: kade-portal has no branch checked out. Switch to main first, in a terminal or your own editor.`, "Update (off)"),
  refAhead: say("Behind bar", "Refused: main has its own commits", `${B3} Update can't run: main in kade-portal has 1 commit that origin/main doesn't, so a fast-forward can't bring it up to date. Push or merge it first, in a terminal or your own editor.`, "Update (off)"),
  refRunning: say("Behind bar", "Refused: already updating", "An update of kade-portal is already running."),
  updating: say("Behind bar", "While Update runs", "Updating kade-portal: move main 3 commits to 319bf0b and run composer install and npm ci…", "Updating… (off); Fetch is off too"),
  updated: say("Behind bar", "Update ended", "Updated kade-portal: moved main 3 commits to 319bf0b and ran composer install and npm ci.", "Dismiss"),
  stopped: say("Behind bar", "Update stopped part-way", "The update of kade-portal stopped: npm ci failed (exit code 1). Before that it moved main 3 commits to 319bf0b and ran composer install.", "Show output · Dismiss"),
  didntStart: say("Behind bar", "Refused when its turn came", "The update of kade-portal didn't start: kade-portal has uncommitted changes in routes/web.php. Commit or stash them first.", "Dismiss"),
  fetchFailed: say("Behind bar", "Fetch failed", "Couldn't fetch main from origin: could not resolve host github.com. The counts are from the last fetch, 3h ago.", "Try again"),
  fetchFailedNever: say("Behind bar", "Fetch failed, never fetched", "Couldn't fetch main from origin: could not resolve host github.com. Gizai can't tell yet whether kade-portal is behind.", "Try again"),
  // 6. a person's card
  openInEditor: say("Task page top bar", "A card assigned to you", "Open in editor", "Tooltip: Work on KADE-41 by hand: Gizai makes its worktree, pins the card to you so agents leave it alone, and moves it to In progress. While it prepares: Preparing the worktree…"),
  openInEditorOff: say("Task page top bar", "An agent works on your card", "Backend Agent is working on KADE-41: wait for its run to end, or stop it.", "Open in editor (off), this is its tooltip"),
  openFailed: say("Task page, error banner", "The worktree couldn't be made", "Couldn't open KADE-41 in the editor: npm ci failed in its new worktree (exit code 1).", "Dismiss"),
  byHand: say("Board card, task page", "Pinned to a person", "By hand", "Tooltip: Pinned to Jeffrey Sevinga: agents leave it alone until it's handed over"),
  cardNone: say("Card bar", "Nothing changed yet", "No changes yet", "Commit… (off: Nothing to commit) · Push (off: Nothing to push) · Hand over to Testing (off: Commit a change first)"),
  cardChanged: say("Card bar", "Uncommitted changes", "2 files changed", "Commit… (primary) · Push (off) · Hand over to Testing (off: Commit your changes first: 2 files aren't committed.)"),
  cardUnpushed: say("Card bar", "Committed, not pushed", "1 commit not pushed", "Commit… (off) · Push · Hand over to Testing (primary: it pushes too)"),
  cardPushed: say("Card bar", "All pushed", "Everything committed and pushed", "Hand over to Testing (primary)"),
  handOverReview: say("Card bar", "The card's Testing switch is off", "Hand over to Review", "In Hand over to Testing's place"),
  cardHanded: say("Card bar", "After Hand over", "Handed over to Testing: QA Agent takes it from here. Its worktree is read-only while an agent works on it.", "Open card"),
  cardBack: say("Card bar", "QA sent it back", "QA Agent sent KADE-41 back to you with 2 issues: they're on the card.", "Open card, then the card's buttons"),
  commitBox: say("Commit…", "The box", "Commit message · KADE-41: Fix the invoice total · Commits every change, as git sees it:", "Cancel · Commit (Ctrl+Enter)"),
  commitEmpty: say("Commit…", "No message", "Write a commit message first."),
  commitHook: say("Commit…", "A git hook refused", "The repository's pre-commit hook stopped the commit: 2 files need formatting (Pint).", "Show output"),
  committed: say("Card bar", "After Commit", "Committed 2 files as 3f5c1a2."),
  pushed: say("Card bar", "After Push", "Pushed gizai/kade-41-fix-the-invoice-total (1 commit)."),
  pushFailed: say("Card bar", "Push failed", "Couldn't push gizai/kade-41-fix-the-invoice-total: the remote refused it (permission denied). Check Settings → GitHub and Bitbucket.", "Try again"),
  handedComment: say("Card comment", "After Hand over", "Handed over by Jeffrey Sevinga: 2 commits."),
  // 7. changed on disk
  diskChanged: say("File bar", "The file changed on disk; unsaved changes here", "app/Models/Invoice.php changed on disk while you were editing it.", "Reload (tooltip: Show the version on disk: your unsaved changes here are lost) · Keep mine (tooltip: Keep your version: saving it replaces the one on disk)"),
  diskKeeping: say("File bar", "After Keep mine", "Keeping your version of Invoice.php. The one on disk is newer: Ctrl+S asks once more before saving over it.", "Reload"),
  diskConfirm: say("File bar", "The first Ctrl+S after Keep mine", "Save over the newer Invoice.php on disk? Press Ctrl+S again.", "Save over it · Cancel"),
  diskDeleted: say("File bar", "The file was deleted on disk", "app/Models/Invoice.php was deleted on disk.", "Save it again · Close tab"),
  // 8. Settings
  setTitle: say("Settings → General", "Section", "Editor folders", "Its intro: Where your projects are. Besides projects' linked folders and card worktrees, the Editor opens every folder directly inside these."),
  setHint: say("Settings → General", "Under Folders", "Never /, your home folder, Gizai's data folder or folders with keys (~/.ssh, ~/.gnupg, ~/.config, …). Saved as soon as you add, change or remove one.", "Add folder · Browse… · × (Remove)"),
  setOk: say("Settings → General", "Under a folder", "14 folders: gizai-site, invoice-api, kade-portal and 11 more."),
  setHome: say("Settings → General", "Refused", "That's your home folder: pick the folder your projects are in, like ~/Herd."),
  setKeys: say("Settings → General", "Refused", "It holds keys or logins (~/.ssh)."),
  setData: say("Settings → General", "Refused", "That's Gizai's data folder (~/.local/share/gizai), or inside it."),
  setDisk: say("Settings → General", "Refused", "That's the whole disk (/): pick the folder your projects are in."),
  setFull: say("Settings → General", "Refused", "Give the full path, like ~/Herd or /srv/projects."),
  setTwice: say("Settings → General", "Refused", "It's in the list twice."),
  setFile: say("Settings → General", "Refused", "That's a file: pick a folder."),
  setMissing: say("Settings → General", "Warning", "It isn't there now: the picker lists its folders once it is."),
  setEmpty: say("Settings → General", "No folders", "No folders: the Editor opens projects' linked folders and card worktrees only."),
};

// ---------- code colours: Lezer's tags → D1's syn-* tokens (F0's HighlightStyle in src/lib/codeTheme.ts) ----------
const TAG_TOKENS = [
  ["keyword", [t.keyword, t.controlKeyword, t.definitionKeyword, t.moduleKeyword, t.operatorKeyword, t.modifier, t.self],
    "keyword, controlKeyword, definitionKeyword, moduleKeyword, operatorKeyword, modifier, self"],
  ["string", [t.string, t.special(t.string), t.regexp, t.escape, t.docString, t.attributeValue], "string, special(string), regexp, escape, docString, attributeValue"],
  ["number", [t.number, t.integer, t.float, t.bool, t.null, t.atom, t.unit], "number, integer, float, bool, null, atom, unit"],
  ["comment", [t.comment, t.lineComment, t.blockComment, t.docComment], "comment, lineComment, blockComment, docComment"],
  ["function", [t.function(t.variableName), t.function(t.propertyName), t.function(t.definition(t.variableName))], "function(variableName), function(propertyName), function(definition(variableName))"],
  ["type", [t.typeName, t.className, t.namespace], "typeName, className, namespace"],
  ["variable", [t.variableName, t.definition(t.variableName), t.labelName], "variableName, definition(variableName), labelName"],
  ["property", [t.propertyName, t.definition(t.propertyName)], "propertyName, definition(propertyName)"],
  ["tag", [t.tagName], "tagName"],
  ["attribute", [t.attributeName], "attributeName"],
  ["operator", [t.operator, t.arithmeticOperator, t.logicOperator, t.bitwiseOperator, t.compareOperator, t.updateOperator, t.definitionOperator, t.typeOperator, t.controlOperator],
    "operator and its kinds (arithmetic, logic, bitwise, compare, update, definition, type, control)"],
  ["punctuation", [t.punctuation, t.separator, t.bracket, t.angleBracket, t.squareBracket, t.paren, t.brace, t.derefOperator], "punctuation, separator, bracket and its kinds, derefOperator"],
  ["meta", [t.meta, t.documentMeta, t.annotation, t.processingInstruction, t.macroName], "meta, documentMeta, annotation, processingInstruction (<?php), macroName; Blade's directives"],
  ["invalid", [t.invalid], "invalid"],
];
const HL = tagHighlighter(TAG_TOKENS.map(([token, tags]) => ({ tag: tags, class: `syn-${token}` })));

/** One class (or null) per character of `src`, from the parser CodeMirror uses for TSX. */
function tsxClasses(src) {
  const cls = new Array(src.length).fill(null);
  highlightTree(jsParser.configure({ dialect: "jsx ts" }).parse(src), HL, (from, to, c) => { for (let i = from; i < to; i++) cls[i] = c; });
  return cls;
}

/** The same for PHP, by rules: the first that matches at a place wins. */
function phpClasses(src) {
  const cls = new Array(src.length).fill(null);
  const rules = [
    [/\/\*[\s\S]*?\*\//y, "comment"], [/\/\/[^\n]*/y, "comment"], [/#(?!\[)[^\n]*/y, "comment"],
    [/<\?php/y, "meta"], [/#\[[^\]\n]*\]/y, "meta"],
    [/'(?:[^'\\\n]|\\.)*'|"(?:[^"\\\n]|\\.)*"/y, "string"],
    [/\$[A-Za-z_]\w*/y, "variable"],
    [/(->)([A-Za-z_]\w*)(\s*\()?/y, (m) => [["operator", m[1].length], [m[3] ? "function" : "property", m[2].length]]],
    [/(::)(class)\b/y, (m) => [["operator", 2], ["keyword", 5]]],
    [/::/y, "operator"],
    [/\b(?:true|false|null)\b/y, "number"],
    [/\b(?:int|float|string|bool|array|void|mixed|iterable|object|self|static)\b(?!\s*\()/y, "type"],
    [/\b(?:namespace|use|class|extends|implements|trait|function|fn|return|public|protected|private|new|if|else|elseif|foreach|for|while|as|match|throw|try|catch|readonly|abstract|final|const|echo)\b/y, "keyword"],
    [/\d+(?:\.\d+)?/y, "number"],
    [/[A-Za-z_]\w*(?=\s*\()/y, "function"],
    [/[A-Z]\w*/y, "type"],
    [/[a-z_]\w*/y, null],
    [/=>|===|!==|==|<=|>=|&&|\|\||\?\?|[=+\-*/<>!?:.%]/y, "operator"],
    [/[{}()[\];,\\]/y, "punctuation"],
  ];
  let i = 0;
  outer: while (i < src.length) {
    for (const [re, what] of rules) {
      re.lastIndex = i;
      const m = re.exec(src);
      if (!m || m[0].length === 0) continue;
      if (typeof what === "function") {
        let at = i;
        for (const [c, n] of what(m)) { for (let k = at; k < at + n; k++) cls[k] = `syn-${c}`; at += n; }
      } else if (what) {
        for (let k = i; k < i + m[0].length; k++) cls[k] = `syn-${what}`;
      }
      i += m[0].length;
      continue outer;
    }
    i++;
  }
  return cls;
}

/** Lines `from`..`to` (from 1) of `src` with their characters' classes. */
function linesOf(src, cls, from, to) {
  const out = [];
  let start = 0;
  src.split("\n").forEach((text, n) => {
    if (n + 1 >= from && n + 1 <= to) out.push({ text, cls: cls.slice(start, start + text.length) });
    start += text.length + 1;
  });
  return out;
}

/** A line's HTML: runs of one syntax class, inside the marks `decos` puts over ranges, and the cursor at `cursor`. */
function renderLine(text, cls, decos = [], cursor = null) {
  const cuts = new Set([0, text.length]);
  for (const d of decos) { cuts.add(d.from); cuts.add(d.to); }
  if (cursor != null) cuts.add(cursor);
  const at = [...cuts].sort((a, b) => a - b);
  let html = "";
  for (let k = 0; k < at.length - 1; k++) {
    const [a, b] = [at[k], at[k + 1]];
    if (cursor === a) html += '<span class="cursor"></span>';
    let inner = "";
    for (let i = a; i < b;) {
      let j = i;
      while (j < b && cls[j] === cls[i]) j++;
      inner += cls[i] ? `<span class="${cls[i]}">${esc(text.slice(i, j))}</span>` : esc(text.slice(i, j));
      i = j;
    }
    const on = decos.filter((d) => d.from <= a && d.to >= b).map((d) => d.cls);
    html += on.length ? `<span class="${on.join(" ")}">${inner}</span>` : inner;
  }
  if (cursor === text.length) html += '<span class="cursor"></span>';
  return html;
}

/** The code: a still picture of CodeMirror with the code theme. `active` is the cursor's line; `folds` the lines whose fold
 *  caret shows (the gutter is hovered); `folded` a line folded after its text. */
function code(lines, { first = 1, active = null, cursor = null, decos = {}, folds = [], folded = {}, flash = null } = {}) {
  return `<div class="ed-code" role="textbox" aria-multiline="true" aria-readonly="false" aria-label="Code">${lines.map((l, i) => {
    const n = first + i;
    const cl = ["ln", n === active && "active", n === flash && "flash"].filter(Boolean).join(" ");
    const fold = folds.includes(n) ? sm("chevron-down") : folded[n] ? sm("chevron-right") : "";
    return `<div class="${cl}"><span class="no">${n}</span><span class="fd">${fold}</span><span class="tx">${renderLine(l.text, l.cls, decos[n] ?? [], cursor?.line === n ? cursor.col : null)}${folded[n] ? `<span class="folded" title="Unfold">…</span>${esc(folded[n])}` : ""}</span></div>`;
  }).join("")}</div>`;
}

// ---------- the samples ----------
// Sidebar.tsx as it is, with the Editor section being added above Agents (that line is the unsaved change).
const SIDEBAR_SRC = (() => {
  const lines = fs.readFileSync(`${GZ}/src/components/Sidebar.tsx`, "utf8").split("\n");
  const at = lines.findIndex((l) => l.includes('<FoldSection section="agents"'));
  lines.splice(at, 0, '      <EditorSection route={route} folded={folded("editor")} onFold={() => fold("editor")} />', "");
  return lines.join("\n");
})();
const SIDEBAR_CLS = tsxClasses(SIDEBAR_SRC);
const SIDEBAR_LINE = SIDEBAR_SRC.split("\n").findIndex((l) => l.includes("<EditorSection")) + 1;

const INVOICE_SRC = `<?php

namespace App\\Models;

use Illuminate\\Database\\Eloquent\\Factories\\HasFactory;
use Illuminate\\Database\\Eloquent\\Model;
use Illuminate\\Database\\Eloquent\\Relations\\BelongsTo;
use Illuminate\\Database\\Eloquent\\Relations\\HasMany;

class Invoice extends Model
{
    use HasFactory;

    protected $fillable = ['client_id', 'number', 'issued_at', 'due_at', 'vat_rate'];

    protected $casts = [
        'issued_at' => 'date',
        'due_at' => 'date',
        'vat_rate' => 'decimal:2',
    ];

    public function client(): BelongsTo
    {
        return $this->belongsTo(Client::class);
    }

    public function lines(): HasMany
    {
        return $this->hasMany(InvoiceLine::class);
    }

    /** The total in cents, VAT included, rounded once at the end. */
    public function total(): int
    {
        $net = $this->lines->sum(fn (InvoiceLine $line) => $line->amount * $line->quantity);

        return (int) round($net * (1 + $this->vat_rate / 100));
    }
}
`;
const INVOICE_CLS = phpClasses(INVOICE_SRC);
const invoice = (from = 1, to = 42) => linesOf(INVOICE_SRC, INVOICE_CLS, from, to);
const LN_TOTAL = INVOICE_SRC.split("\n").findIndex((l) => l.includes("public function total()")) + 1;
const LN_RETURN = INVOICE_SRC.split("\n").findIndex((l) => l.includes("return (int) round")) + 1;

// ---------- the app's sidebar, with the Editor section in the state asked for ----------
const nav = (icon, label, { meta = "", on = false, cls = "", tag = "a", title = "" } = {}) =>
  `<${tag} class="nav-item${on ? " on" : ""}${cls ? ` ${cls}` : ""}"${title ? ` title="${q(title)}"` : ""}>${ic(icon)}<span>${label}</span>${meta ? `<span class="meta">${meta}</span>` : ""}</${tag}>`;
const liveTag = (n) => `<span class="live-tag"><span class="pulse"></span>${n} live</span>`;

const FOLDERS = [
  { key: "gizai", name: "gizai", stand: X.sideChanged, title: "~/Code/gizai · main, up to date with origin/main · 4 files changed, not committed" },
  { key: "kade-portal", name: "kade-portal", stand: X.sideBehind, title: "~/Herd/kade-portal · main is 3 commits behind origin/main, fetched 2m ago" },
  { key: "KADE-41", card: "Fix the invoice total", stand: `${sm("keyboard")}${X.sideByHand}`, title: "KADE-41 Fix the invoice total · its worktree, pinned to you: agents leave it alone" },
  { key: "GA-118", card: "Show a run's changes", stand: `<span class="pulse"></span>${X.sideLive}`, title: "GA-118 Show a run's changes · QA Agent is working on GA-118: read-only until its run ends" },
  { key: "invoice-api", name: "invoice-api", stand: `${sm("loader-circle", "spin")}${X.sideUpdating}`, title: "~/Herd/invoice-api · Updating: move main 2 commits to 9e1d4c0 and run composer install" },
];

function editorSection(state = "open", { on = null, hover = null } = {}) {
  const head = (folded, tag = "") => `<div class="nav-label"><span class="fold">${X.sideHead}${tag}</span>`
    + `<button aria-label="Open a folder" title="Open a folder"${state === "picking" ? ' class="is-hover"' : ""}>${sm("plus")}</button>`
    + `<button aria-expanded="${!folded}" aria-label="${folded ? "Show" : "Fold"} Editor" title="${folded ? "Show" : "Fold"} Editor">${sm(folded ? "chevron-up" : "chevron-down")}</button></div>`;
  if (state === "folded") return `<div class="nav-section" aria-label="Editor">${head(true, `<span class="fold-tag" title="2 folders are behind origin/main">${X.sideFolded}</span>`)}</div>`;
  if (state === "empty") return `<div class="nav-section" aria-label="Editor">${head(false)}<div class="nav-list">${nav("folder-plus", X.sideEmpty, { tag: "button", cls: hover === "empty" ? "is-hover" : "" })}</div></div>`;
  const rows = FOLDERS.map((f) => `<a class="nav-item${on === f.key ? " on" : ""}${hover === f.key ? " is-hover" : ""}" title="${q(f.title)}">${ic("folder-code")}`
    + (f.card ? `<span class="ellipsis"><span class="card-id">${f.key}</span> <span class="faint">${f.card}</span></span>` : `<span class="ellipsis">${f.name}</span>`)
    + `<span class="meta"><span class="stand">${f.stand}</span></span></a>`).join("");
  return `<div class="nav-section" aria-label="Editor">${head(false)}<div class="nav-list">${rows}</div></div>`;
}

/** The whole sidebar; `crop` keeps the Editor section with its neighbours, Projects and Agents. */
function sidebar({ editor = "open", on = null, hover = null, page = null, crop = false } = {}) {
  const top = nav("square-pen", "New task", { tag: "button", meta: kbd("N") }) + nav("search", "Search", { tag: "button", meta: kbd("Ctrl K") })
    + nav("inbox", "Inbox", { meta: '<span class="count alert">2</span>' })
    + `<div class="nav-section"><div class="nav-label">Work</div>${nav("messages-square", "Chat")}${nav("list-todo", "Tasks", { on: page === "tasks" })}${nav("building-2", "Clients")}${nav("folder-kanban", "Projects")}</div>`;
  const projects = `<div class="nav-section"><div class="nav-label">Projects<button aria-label="New project" title="New project">${sm("plus")}</button></div>`
    + `<a class="nav-item"><span class="dot" style="background:var(--c-blue)"></span><span>Gizai</span></a><a class="nav-item"><span class="dot" style="background:var(--c-orange)"></span><span>Kade portal</span></a></div>`;
  const agents = `<div class="nav-section" aria-label="Agents"><div class="nav-label"><span class="fold">Agents</span><button aria-label="Fold Agents" title="Fold Agents">${sm("chevron-down")}</button></div><div class="nav-list">`
    + nav("crown", "Team Lead") + nav("monitor", "Frontend Agent") + nav("server", "Backend Agent", { meta: liveTag(1) })
    + (crop ? "" : nav("flask-conical", "QA Agent", { meta: liveTag(1) }) + nav("palette", "Design Agent", { meta: '<span class="faint">paused</span>' })) + `</div></div>`;
  const memory = `<div class="nav-section" aria-label="Memory"><div class="nav-label"><span class="fold">Memory</span><button aria-label="Show Memory" title="Show Memory">${sm("chevron-up")}</button></div></div>`;
  const foot = `<div class="side-foot"><div class="nav-section"><div class="nav-label">Company</div>${nav("chart-column", "Usage")}${nav("network", "Team")}${nav("users", "Users")}${nav("settings", "Settings", { on: page === "settings" })}</div></div>`;
  return crop
    ? `<aside class="side" aria-label="Navigation"><div class="side-main">${projects}${editorSection(editor, { on, hover })}${agents}</div></aside>`
    : `<aside class="side" aria-label="Navigation"><div class="side-main">${top}${projects}${editorSection(editor, { on, hover })}${agents}${memory}</div>${foot}</aside>`;
}

// ---------- the Editor page's parts ----------
const topbar = (crumbs, mid = "", actions = "") => `<div class="topbar"><div class="crumbs${mid ? " ed-crumbs" : ""}">${crumbs}</div>${mid}<div class="actions">${actions}</div></div>`;
const crumbs = (name, { mono = false, title = "" } = {}) => `<a>Editor</a><span class="sep">/</span><b${mono ? ' class="mono"' : ""}${title ? ` title="${q(title)}"` : ""}>${name}</b>`;
const standing = (text, title = "") => {
  const [branch, ...rest] = text.split(" · ");
  return `<div class="ed-standing"${title ? ` title="${q(title)}"` : ""}>${sm("git-branch")}<span class="branch">${branch}</span>${rest.map((r) => `<span class="sep">·</span><span>${r}</span>`).join("")}</div>`;
};
const fetchBtn = (busy = false) => `<button class="btn ghost sm" title="Fetch origin/main now (last fetched 2m ago)"${busy ? ' disabled aria-busy="true"' : ""}>${ic("refresh-cw", busy ? "icon spin" : "icon")}${busy ? "Fetching…" : X.fetch}</button>`;
const openInBtn = (open = false) => `<button class="btn ghost sm${open ? " is-pressed" : ""}" aria-haspopup="menu" aria-expanded="${open}">${ic("square-arrow-out-up-right")}Open in${sm("chevron-down")}</button>`;
const edActions = ({ fetch = true, busy = false, open = false } = {}) => `${fetch ? fetchBtn(busy) : ""}${openInBtn(open)}`;

const MARK = { modified: ["M", "Modified"], added: ["A", "Added"], renamed: ["R", "Renamed from app/Models/Bill.php"], untracked: ["U", "Untracked: new, not in git yet"],
  conflicted: ["C", "Conflicted: finish the merge in a terminal"], deleted: ["D", "Deleted"] };
const mark = (m) => `<span class="ed-mark ${m}" title="${q(MARK[m][1])}">${MARK[m][0]}</span>`;
const fileIcon = (n) => /^\.env/.test(n) ? "file-key" : /\.json$/.test(n) ? "file-json" : /\.(md|txt)$/.test(n) ? "file-text"
  : /\.(png|jpe?g|gif|svg|webp|ico)$/.test(n) ? "file-image" : /\.(tsx?|jsx?|mjs|rs|php|css|scss|html|sql|toml|ya?ml|sh|xml)$/.test(n) ? "file-code" : "file";

/** A tree row: [depth, "dir" | "file", name, options]. */
function row([depth, kind, name, o = {}]) {
  const cls = ["ed-row", o.ignored && "ignored", o.is].filter(Boolean).join(" ");
  const caret = kind === "dir" ? ic(o.open ? "chevron-down" : "chevron-right") : '<span class="room"></span>';
  const icon = kind === "dir" ? ic(o.open ? "folder-open" : "folder") : ic(o.icon ?? fileIcon(name));
  const title = o.title ?? (o.ignored ? (kind === "dir" ? X.tipIgnoredDir : X.tipIgnoredFile) : o.ro ?? "");
  const nameHtml = o.input != null ? `<input class="input name-in" value="${q(o.input)}" placeholder="${q(X.treeNew)}" aria-label="New file's name">` : `<span class="name">${name}</span>`;
  const right = (o.ro ? ic("lock", "icon ro") : "") + (o.mark ? mark(o.mark) : "") + (o.dot ? `<span class="ed-dot" title="${X.tipDot}"></span>` : "") + (o.count ? `<span class="count">${o.count}</span>` : "");
  return `<div class="${cls}" style="--depth:${depth}" role="treeitem"${kind === "dir" ? ` aria-expanded="${!!o.open}"` : ""}${o.selected ? ' aria-selected="true"' : ""} tabindex="${o.selected ? 0 : -1}"${title ? ` title="${q(title)}"` : ""}>${caret}${icon}${nameHtml}${right}</div>`
    + (o.note ? `<div class="ed-row-note${o.noteError ? " error" : ""}" style="--depth:${depth}">${o.note}</div>` : "")
    + (o.confirm ? `<div class="ed-confirm" style="--depth:${depth}" role="alertdialog"><span class="grow">${o.confirm}</span><button class="btn danger sm">Delete</button><button class="btn ghost sm">Cancel</button></div>` : "");
}
const tree = (rows) => rows.map(row).join("");

const GIZAI_TREE = [
  [0, "dir", ".github"], [0, "dir", "crates"], [0, "dir", "design"], [0, "dir", "docs"],
  [0, "dir", "node_modules", { ignored: true }], [0, "dir", "scripts"],
  [0, "dir", "src", { open: true, dot: true }],
  [1, "dir", "assets"], [1, "dir", "components", { open: true, dot: true }],
  [2, "dir", "chat"], [2, "dir", "editor", { dot: true, title: "Changes inside: 3 untracked files" }], [2, "dir", "memory"],
  [2, "file", "AgentForm.tsx"], [2, "file", "Avatar.tsx"], [2, "file", "Board.tsx"], [2, "file", "MarkdownEditor.tsx"],
  [2, "file", "Sidebar.tsx", { mark: "modified", selected: true }], [2, "file", "StatusIcon.tsx"], [2, "file", "Tabs.tsx"],
  [1, "dir", "lib", { dot: true }], [1, "dir", "pages"], [1, "dir", "styles"],
  [1, "file", "api.ts"], [1, "file", "App.tsx"], [1, "file", "main.tsx"], [1, "file", "router.ts"],
  [0, "dir", "src-tauri"], [0, "dir", "target", { ignored: true }],
  [0, "file", ".gitignore"], [0, "file", "Cargo.toml"], [0, "file", "index.html"], [0, "file", "package-lock.json"], [0, "file", "package.json"], [0, "file", "README.md"],
];
const KADE_TREE = ({ selected = "Invoice.php", invoiceMark = null, extra = [] } = {}) => [
  [0, "dir", "app", { open: true, dot: !!invoiceMark }],
  [1, "dir", "Http"], [1, "dir", "Models", { open: true, dot: !!invoiceMark }],
  [2, "file", "Client.php"], [2, "file", "Invoice.php", { selected: selected === "Invoice.php", mark: invoiceMark }], [2, "file", "InvoiceLine.php"], [2, "file", "User.php"],
  [1, "dir", "Providers"],
  [0, "dir", "bootstrap"], [0, "dir", "config"], [0, "dir", "database"], [0, "dir", "node_modules", { ignored: true }], [0, "dir", "public"],
  [0, "dir", "resources"], [0, "dir", "routes"], [0, "dir", "storage", { ignored: true }], [0, "dir", "tests", { dot: extra.includes("test") }],
  ...(extra.includes("test") ? [[1, "dir", "Feature", { open: true, dot: true }], [2, "file", "InvoiceTotalTest.php", { mark: "untracked" }]] : []),
  [0, "dir", "vendor", { ignored: true }],
  [0, "file", ".env", { ignored: true }], [0, "file", ".env.example"], [0, "file", ".gitignore"], [0, "file", "artisan"],
  [0, "file", "composer.json"], [0, "file", "composer.lock"], [0, "file", "package.json"], [0, "file", "phpunit.xml"], [0, "file", "vite.config.js"],
];

function sidePanel({ title, path: where, rows = "", mode = "files", search = "" }) {
  const tab = (k, icon, label, tip) => `<button role="tab" aria-selected="${mode === k}" class="${mode === k ? "on" : ""}" title="${q(tip)}">${sm(icon)}${label}</button>`;
  return `<div class="ed-side"><div class="ed-side-top"><div class="seg" role="tablist" aria-label="Files or search">${tab("files", "folder", "Files", "The folder's files")}${tab("search", "search", "Search", "Search in the folder (Ctrl+Shift+F)")}</div>`
    + `<button class="btn ghost sm icon-only" aria-label="Fold the file tree" title="${X.treeFold}">${ic("panel-left-close")}</button></div>`
    + (mode === "files"
      ? `<button class="ed-goto" title="Go to file (Ctrl+P)">${sm("search")}${X.goto}${kbd("Ctrl P")}</button>`
        + `<div class="ed-tree-head"><span title="${q(where)}">${title}</span>`
        + `<button class="btn ghost sm icon-only" aria-label="New file" title="New file">${ic("file-plus")}</button><button class="btn ghost sm icon-only" aria-label="New folder" title="New folder">${ic("folder-plus")}</button>`
        + `<button class="btn ghost sm icon-only" aria-label="Fold every folder" title="Fold every folder">${ic("chevrons-down-up")}</button></div>`
        + `<div class="ed-tree" role="tree" aria-label="Files of ${q(title)}">${rows}</div>`
      : search)
    + `</div>`;
}

/** A tab: { name, path, on, unsaved, mark, dir, ro, ignored, is } */
const tab = (f) => `<div class="${["ed-tab", f.unsaved && "unsaved", f.ignored && "ignored", f.is].filter(Boolean).join(" ")}" role="tab" tabindex="${f.on ? 0 : -1}" aria-selected="${!!f.on}" title="${q(f.path + (f.unsaved ? " · not saved" : ""))}">`
  + `${f.ro ? `<svg class="icon ro" aria-label="Read-only"><use href="#i-lock"/></svg>` : ""}<span class="name">${f.name}</span>${f.dir ? `<span class="dir">${f.dir}</span>` : ""}${f.mark ? mark(f.mark) : ""}`
  + `<button class="x" aria-label="Close ${q(f.name)}" title="${X.tabClose}">${ic("x")}</button></div>`;
if (true) used.add("lock");
const tabs = (list, { folded = false } = {}) => `<div class="ed-tabs" role="tablist" aria-label="Open files">${folded ? `<button class="btn ghost sm icon-only" aria-label="Show the file tree" title="Show the file tree (Ctrl+B)">${ic("panel-left-open")}</button>` : ""}${list.map(tab).join("")}</div>`;

const status = ({ path: p, git = "", cursor = X.stCursor, eol = X.stEol, lang = X.stLang, save }) =>
  `<div class="ed-status" role="status"><span class="path" title="${q(p)}">${p}</span>${git}<span class="grow"></span>`
  + `<button class="item" title="Go to line (Ctrl+Alt+G)">${cursor}</button><span class="item" title="Line endings, kept as the file has them">${eol}</span><span class="item">${lang}</span>${save}</div>`;
const gitWord = (m, text) => `<span class="item git ${m}">${text}</span>`;
const SAVE = {
  saved: `<span class="item">${sm("check")}${X.stSaved}</span>`,
  unsaved: `<button class="item unsaved" title="Save (Ctrl+S)">${X.stUnsaved}</button>`,
  saving: `<span class="item">${sm("loader-circle", "spin")}${X.stSaving}</span>`,
  ro: `<span class="item" title="Over 1 MB: change it in your own editor">${sm("lock")}${X.stRO}</span>`,
  reloaded: `<span class="item">${sm("rotate-ccw")}${X.stReloaded}</span>`,
  failed: `<span class="item bad">${sm("circle-alert")}${X.stFailed}</span>`,
};

/** A bar: kind ("behind", "refused", "done", "failed", "needs", "file", "card", "") and its parts. */
const bar = (kind, lead, text, acts = "", more = "") => `<div class="ed-bar${kind ? ` ${kind}` : ""}" role="${kind === "failed" || kind === "refused" ? "alert" : "status"}">${lead}<span class="say">${text}</span>${acts ? `<span class="acts">${acts}</span>` : ""}${more}</div>`;
const btn = (label, { kind = "", icon = "", off = false, title = "", busy = false, cls = "" } = {}) =>
  `<button class="btn sm${kind ? ` ${kind}` : ""}${cls ? ` ${cls}` : ""}"${off ? " disabled" : ""}${busy ? ' aria-busy="true"' : ""}${title ? ` title="${q(title)}"` : ""}>${icon ? ic(icon, busy ? "icon spin" : "icon") : ""}${label}</button>`;
const dismiss = `<button class="btn ghost sm icon-only" aria-label="Dismiss" title="Dismiss">${ic("x")}</button>`;

/** The main column of the Editor page. */
function editorMain({ crumbs: cr, stand = "", actions = edActions(), bar: b = "", side = "", tabs: tb = "", filebar = "", find = "", body = "", status: s = "", overlay = "" }) {
  return `<div class="main">${topbar(cr, stand, actions)}${b}<div class="ed-page"><div class="ed">${side}<div class="ed-main">${tb}${filebar}${find}${body}</div></div>${s}</div>${overlay}</div>`;
}
const win = (side, main) => `${side}${main}`;

// ---------- frames: each screen in dark and in light ----------
const frame = (theme, inner, { h = null, w = null, cls = "" } = {}) =>
  `<div class="win${cls ? ` ${cls}` : ""}" data-theme="${theme}" style="${h ? `height:${h}px;` : ""}${w ? `width:${w}px;` : ""}">${inner}</div>`;
/** A whole window: dark above light. */
const both = (inner, opts = {}) => `<figure class="shot dark"><figcaption class="shot-label"><span class="theme-name">Dark</span>${opts.label ? ` · ${opts.label}` : ""}</figcaption>${frame("dark", inner, { h: opts.h ?? 900, cls: opts.cls })}</figure>`;
/** Small frames: a row of them in dark, then the same row in light. items: [{ label, inner, w, h, cls }] */
const rows = (items, opts = {}) => `<div class="variants"><div class="vrow dark"><div class="vhead theme-name">Dark</div><div class="vitems">${items.map((it) =>
  `<figure class="vitem"${it.w ? ` style="width:${it.w}px"` : ""}>${it.label ? `<figcaption>${it.label}</figcaption>` : ""}${frame("dark", it.inner, { h: it.h ?? opts.h, cls: `strip ${it.cls ?? ""}` })}</figure>`).join("")}</div></div></div>`;
/** Strips the window's width (bars and the like), one per row: dark ones, then light ones. */
const strips = (items) => `<div class="variants"><div class="vrow dark stack"><div class="vhead theme-name">Dark</div><div class="vitems">${items.map((it) =>
  `<figure class="vitem wide">${it.label ? `<figcaption>${it.label}</figcaption>` : ""}${frame("dark", it.inner, { cls: `strip ${it.cls ?? ""}`, h: it.h })}</figure>`).join("")}</div></div></div>`;

// ---------- the page's sections ----------
const sections = [];
const section = (id, title, lead, body) => sections.push({ id, title, html: `<section class="dz-sec" id="${id}"><h2>${title}</h2>${lead}${body}</section>` });
const notes = (...items) => `<ul class="dz-notes">${items.map((i) => `<li>${i}</li>`).join("")}</ul>`;
const p = (html) => `<p class="dz-p">${html}</p>`;
const h3 = (s) => `<h3>${s}</h3>`;

// 1. The sidebar's Editor section ------------------------------------------------------------------------------------------
const sideOnly = (opts) => sidebar({ ...opts, crop: true });
const sideMenu = `<div class="pop" role="menu" aria-label="kade-portal" style="left:150px;top:214px;min-width:200px">`
  + `<button class="opt" role="menuitem">${sm("square-arrow-out-up-right")}Open in VS Code</button><button class="opt active" role="menuitem">${sm("square-arrow-out-up-right")}Open in Zed</button>`
  + `<button class="opt" role="menuitem">${sm("square-arrow-out-up-right")}Open in PhpStorm</button><div class="sep"></div>`
  + `<button class="opt" role="menuitem">${sm("copy")}Copy path</button><button class="opt" role="menuitem">${sm("x")}Remove from the list</button></div>`;
section("sidebar", "1. The sidebar's Editor section",
  p("Between Projects and Agents, a section that folds like Agents and Memory (GA-90's FoldSection, with <code>\"editor\"</code> added to <code>Fold</code> and kept in localStorage with the others). Its rows are the folders you opened, newest first, at most 8.")
  + notes(
    "<b>Heading.</b> “Editor”; <b>+</b> opens the folder picker (§2), the caret folds the section. Folded, the heading says how many folders are behind main, as folded Agents says how many agents are live.",
    "<b>A row.</b> Lucide <code>folder-code</code>, the folder's name, and on the right <i>one</i> word on how it stands, in <code>text-3</code>: behind first (“3 behind”), else changed (“4 changed”), else ahead (“1 ahead”), else nothing. The full standing and the folder's path are in the row's tooltip. Words, not arrows: the arrows were a short form, and a word is as short here.",
    "<b>A card's worktree</b> shows the card's identifier (mono) and title. Pinned to you: “by hand” with Lucide <code>keyboard</code>. An agent working on it now: the pulse (teal: an agent at work, its one meaning) and “read-only”. Lowercase, like Agents' “paused”.",
    "<b>Update running</b>: a turning <code>loader-circle</code> and “updating”. <b>Selected</b>: the folder the Editor page shows, with the <code>selected</code> fill like every page's row. Hover: the <code>hover</code> fill.",
    "<b>Right click</b> (or the context menu key) on a row: Open in VS Code, Zed or PhpStorm (only those found), Copy path, Remove from the list; a card's row adds Open card. No ⋯ button on the row: a button inside a row that is a link isn't allowed, and the row stays quiet.",
    "<b>Empty</b>: one row, “Open a folder”, which opens the picker.")
  + rows([
    { label: "Open: a folder shown, one hovered", inner: sideOnly({ editor: "open", on: "gizai", hover: "kade-portal" }), w: 250 },
    { label: "Folded, 2 folders behind", inner: sideOnly({ editor: "folded" }), w: 250 },
    { label: "Nothing opened yet", inner: sideOnly({ editor: "empty" }), w: 250 },
    { label: "Right click on kade-portal", inner: sideOnly({ editor: "open", on: "gizai", hover: "kade-portal" }) + sideMenu, w: 380, h: 500 },
  ]));

// 2. The folder picker ---------------------------------------------------------------------------------------------------------
const pickerRow = (icon, name, where, { active = false, mono = false } = {}) => `<button class="opt${active ? " active" : ""}" role="option" aria-selected="${active}">${icon}<span class="ellipsis${mono ? " mono" : ""}">${name}</span>${where}</button>`;
const where = (s) => `<span class="where">${s}</span>`;
const dot = (c) => `<span class="dot" style="width:9px;height:9px;border-radius:50%;flex:none;background:var(--${c})"></span>`;
const picker = ({ query = "", body, left = 256, top = 352, foot = true }) => `<div class="pop ed-picker" role="dialog" aria-label="Open a folder" style="left:${left}px;top:${top}px">`
  + `<div class="ed-picker-in">${ic("search")}<input class="input" value="${q(query)}" placeholder="${q(X.pickFind)}" aria-label="Find a folder"></div>`
  + `<div class="ed-picker-list" role="listbox">${body}</div>${foot ? `<div class="ed-picker-foot">${X.pickFoot.replace("Settings → General → Editor folders", "<a>Settings → General → Editor folders</a>")}</div>` : ""}</div>`;
const PICK_ALL = `<div class="pop-label">Projects</div>`
  + pickerRow(dot("c-blue"), "Gizai", where("~/Code/gizai"), { active: true }) + pickerRow(dot("c-orange"), "Kade portal", where("~/Herd/kade-portal"))
  + `<div class="pop-label">In ~/Herd</div>`
  + pickerRow(sm("folder"), "gizai-site", where("~/Herd/gizai-site")) + pickerRow(sm("folder"), "invoice-api", where("~/Herd/invoice-api")) + pickerRow(sm("folder"), "scratch", where("~/Herd/scratch"))
  + `<div class="pop-label">Cards</div>`
  + `<button class="opt" role="option">${st("in_progress")}<span class="card-id">KADE-41</span><span class="ellipsis">Fix the invoice total</span><span class="stand">${sm("keyboard")}${X.pickMine}</span></button>`
  + `<button class="opt" role="option">${st("testing")}<span class="card-id">GA-118</span><span class="ellipsis">Show a run's changes</span><span class="stand"><span class="pulse"></span>${X.pickLive}</span></button>`
  + `<button class="opt" role="option">${st("in_progress")}<span class="card-id">GA-112</span><span class="ellipsis">Remember the open tabs per folder</span></button>`;
const editorEmptyPage = editorMain({ crumbs: "<b>Editor</b>", actions: "", body: "" }).replace('<div class="ed-page"><div class="ed"><div class="ed-main"></div></div></div>',
  `<div class="ed-empty"><div class="empty">${ic("folder-code")}<b>No folder open</b><span>${X.pageEmpty}</span><button class="btn primary">${ic("folder-plus")}Open a folder</button></div></div>`);
section("picker", "2. Opening a folder: the folder picker",
  p("<b>+</b> on the Editor heading opens it, and so does Open a folder on the Editor page when no folder is open (<code>#/editor</code>, behind it here). A popover like the @ picker, 380px wide, beside the sidebar: its top at the heading, moved up as far as it must to fit the window (fixed, through a portal, since the sidebar clips what overflows it).")
  + notes(
    "One box filters all groups as you type. ↑ and ↓ choose, Enter opens, Escape closes. A folder already in the sidebar opens there; a new one goes to the top of the list.",
    "<b>Projects</b>: each active project's linked folder, with the project's colour dot, as in the sidebar. <b>In ~/Herd</b>: the folders directly inside it that aren't a project's; one group per folder in Settings → General → Editor folders. <b>Cards</b>: worktrees that exist, yours first: the column's glyph, the identifier, the title, and how it stands (“by hand”, or the pulse and who is working).",
    "On the right, where the folder is (mono, <code>text-3</code>, cut off at the start of the line when long). A folder outside these places opens only through a project: the footer says where to add one.")
  + both(win(sidebar({ editor: "picking", on: null }), editorEmptyPage) + picker({ body: PICK_ALL, top: 418 }), { label: "the picker over the Editor page with no folder open" })
  + rows([
    { label: "Filtered: “inv”", inner: picker({ query: "inv", left: 0, top: 0, body: `<div class="pop-label">Projects</div>${pickerRow(dot("c-orange"), "Kade portal", where("~/Herd/kade-portal"), { active: false })}<div class="pop-label">In ~/Herd</div>${pickerRow(sm("folder"), "<b>inv</b>oice-api", where("~/Herd/invoice-api"), { active: true })}<div class="pop-label">Cards</div><button class="opt" role="option">${st("in_progress")}<span class="card-id">KADE-41</span><span class="ellipsis">Fix the <b>inv</b>oice total</span><span class="stand">${sm("keyboard")}${X.pickMine}</span></button>` }), w: 382, h: 300, cls: "pick" },
    { label: "Nothing found", inner: picker({ query: "invoices-v2", left: 0, top: 0, body: `<div class="pop-empty">${X.pickNone}</div>` }), w: 382, h: 300, cls: "pick" },
    { label: "A folder of Editor folders is missing", inner: picker({ left: 0, top: 0, body: `<div class="pop-label">Projects</div>${pickerRow(dot("c-blue"), "Gizai", where("~/Code/gizai"), { active: true })}<div class="pop-label">In ~/Herd</div><div class="pop-empty">${X.pickNoRoot}</div>` }), w: 382, h: 300, cls: "pick" },
    { label: "Nothing to open at all", inner: picker({ left: 0, top: 0, foot: false, body: `<div class="pop-empty">${X.pickNothing}</div>` }), w: 382, h: 300, cls: "pick" },
  ]));

// 3. The Editor page ----------------------------------------------------------------------------------------------------------
const SIDEBAR_FIRST = SIDEBAR_LINE - 18;
const sidebarCode = (opts = {}) => {
  const line = SIDEBAR_SRC.split("\n")[SIDEBAR_LINE - 1];
  const open = line.indexOf("onFold={") + "onFold=".length;
  const close = line.lastIndexOf("}");
  return code(linesOf(SIDEBAR_SRC, SIDEBAR_CLS, SIDEBAR_FIRST, SIDEBAR_FIRST + 40), {
    first: SIDEBAR_FIRST, active: SIDEBAR_LINE, cursor: { line: SIDEBAR_LINE, col: close + 1 },
    decos: { [SIDEBAR_LINE]: [{ from: open, to: open + 1, cls: "bracket" }, { from: close, to: close + 1, cls: "bracket" }] },
    folds: [SIDEBAR_LINE + 2, SIDEBAR_LINE + 5], ...opts,
  });
};
const GIZAI_TABS = [
  { name: "Sidebar.tsx", path: "src/components/Sidebar.tsx", on: true, unsaved: true, mark: "modified" },
  { name: "sidebar.ts", path: "src/lib/sidebar.ts", mark: "modified" },
  { name: "README.md", path: "README.md" },
  { name: "README.md", dir: "design", path: "design/README.md" },
];
const gizaiStatus = (save = SAVE.unsaved, more = {}) => status({ path: "src/components/Sidebar.tsx", git: gitWord("modified", X.stGit), cursor: `Ln ${SIDEBAR_LINE}, Col 79`, lang: X.stLang, save, ...more });
const gizaiPage = ({ bar: b = "", overlay = "", tabsOpts = {}, sideRows = GIZAI_TREE, body = null, find = "", actions = edActions(), status: s = null } = {}) => editorMain({
  crumbs: crumbs("gizai", { title: "~/Code/gizai" }), stand: standing(X.standOk), actions, bar: b,
  side: sidePanel({ title: "gizai", path: "~/Code/gizai", rows: tree(sideRows) }),
  tabs: tabs(GIZAI_TABS, tabsOpts), find, body: body ?? sidebarCode(), status: s ?? gizaiStatus(), overlay,
});

const OPEN_IN_MENU = `<div class="pop right" role="menu" aria-label="Open in" style="right:20px;top:46px;left:auto;width:260px"><div class="pop-label">Open gizai in</div>`
  + `<button class="opt active" role="menuitem">VS Code<span class="faint">code</span></button><button class="opt" role="menuitem">Zed<span class="faint">zeditor</span></button>`
  + `<button class="opt" role="menuitem">PhpStorm<span class="faint">phpstorm</span></button><div class="pop-note">At Sidebar.tsx, line ${SIDEBAR_LINE}</div></div>`;
const OPEN_IN_NONE = `<div class="pop right" role="menu" aria-label="Open in" style="right:20px;top:46px;left:auto;width:300px"><div class="pop-label">Open gizai in</div><div class="pop-note">${X.openInNone}</div></div>`;
const gotoPalette = `<div class="ed-scrim"></div><div class="pal" role="dialog" aria-label="Go to file" style="position:absolute;top:70px">`
  + `<div class="pal-in">${ic("search")}<input value="sidebar" placeholder="${q(X.gotoBox)}" aria-label="Go to file"></div><div class="pal-list">`
  + [["Sidebar.tsx", "src/components"], ["sidebar.ts", "src/lib"], ["sidebar.test.ts", "src/lib"], ["SidebarFold.test.tsx", "src/components"]].map(([n, d], i) =>
    `<button class="pal-item${i === 0 ? " active" : ""}">${ic(fileIcon(n))}<span><b>${n.slice(0, 7)}</b>${n.slice(7)}</span><span class="where">${d}</span></button>`).join("")
  + `</div></div>`;
const findPanel = `<div class="ed-find" role="search" aria-label="Find and replace">`
  + `<div class="in"><input class="input" value="liveTag" aria-label="Find" placeholder="Find"><span class="ed-toggles"><button class="ed-toggle" aria-pressed="true" title="Match case">${ic("case-sensitive")}</button><button class="ed-toggle" aria-pressed="false" title="Words">${ic("whole-word")}</button><button class="ed-toggle" aria-pressed="false" title="Regular expression">${ic("regex")}</button></span></div>`
  + `<div class="row"><span class="count">3 of 4</span><button class="btn ghost sm icon-only" aria-label="Previous match" title="Previous match (Shift+F3)">${ic("arrow-up")}</button><button class="btn ghost sm icon-only" aria-label="Next match" title="Next match (F3)">${ic("arrow-down")}</button></div>`
  + `<button class="btn ghost sm icon-only close" aria-label="Close" title="Close (Escape)">${ic("x")}</button>`
  + `<div class="in plain"><input class="input" value="liveCount" aria-label="Replace" placeholder="Replace"></div>`
  + `<div class="row"><button class="btn sm">Replace</button><button class="btn sm">Replace all</button></div></div>`;
const findCode = () => {
  const lines = SIDEBAR_SRC.split("\n");
  const decos = {};
  lines.forEach((l, i) => { let k = -1; while ((k = l.indexOf("liveTag", k + 1)) >= 0) (decos[i + 1] ??= []).push({ from: k, to: k + 7, cls: "match" }); });
  const now = lines.findIndex((l) => l.includes("liveTag(n)")) + 1;
  decos[now] = decos[now].map((d) => ({ ...d, cls: "match now" }));
  const first = now - 9;
  return code(linesOf(SIDEBAR_SRC, SIDEBAR_CLS, first, first + 34), { first, active: now, decos });
};
const ctxMenu = (style) => `<div class="pop" role="menu" aria-label="Invoice.php" style="${style}">`
  + `<button class="opt" role="menuitem">${sm("file-plus")}New file</button><button class="opt" role="menuitem">${sm("folder-plus")}New folder</button>`
  + `<button class="opt active" role="menuitem">${sm("pencil")}Rename${kbd("F2")}</button><button class="opt" role="menuitem">${sm("trash-2")}Delete${kbd("Del")}</button><div class="sep"></div>`
  + `<button class="opt" role="menuitem">${sm("copy")}Copy path</button><div class="sep"></div>`
  + `<button class="opt" role="menuitem">${sm("square-arrow-out-up-right")}Open in VS Code</button><button class="opt" role="menuitem">${sm("square-arrow-out-up-right")}Open in Zed</button><button class="opt" role="menuitem">${sm("square-arrow-out-up-right")}Open in PhpStorm</button></div>`;
const treeOnly = (rowsHtml, extra = "", h = 420) => `<div class="ed-side" style="height:${h}px">${`<div class="ed-tree-head"><span>kade-portal</span></div>`}<div class="ed-tree" role="tree">${rowsHtml}</div></div>${extra}`;
const KADE_SMALL = (o = {}) => tree([
  [0, "dir", "app", { open: true }], [1, "dir", "Exports", { open: true, ...(o.exports ?? {}) }],
  ...(o.newFile ? [[2, "file", "", { input: o.newFile, note: o.note, noteError: o.noteError }]] : []),
  [2, "file", "InvoicesCsv.php", o.csv ?? {}], [1, "dir", "Models", { open: true }],
  [2, "file", "Client.php"], [2, "file", "Invoice.php", o.invoice ?? { selected: true }], [2, "file", "InvoiceLine.php"],
  [0, "dir", "vendor", { ignored: true }],
]);
const askPop = `<div class="pop ed-ask" role="alertdialog" aria-label="Close Sidebar.tsx" style="left:8px;top:40px"><p>${X.tabAsk}</p><div class="acts"><button class="btn ghost sm">Cancel</button><button class="btn danger sm">Close without saving</button><button class="btn primary sm">Save and close</button></div></div>`;
const gaTabs = (ro) => tabs([{ name: "RunHistory.tsx", path: "src/components/RunHistory.tsx", on: true, ro }, { name: "changes.ts", path: "src/lib/changes.ts", ro }]);
const stripTop = (name, standText, b, { mono = false, actions = edActions() } = {}) => `<div class="main">${topbar(crumbs(name, { mono }), standText ? standing(standText) : "", actions)}${b}</div>`;
const fileBarStrip = (b, t = tabs([{ name: "laravel.log", path: "storage/logs/laravel.log", on: true, ro: true }])) => `<div class="main"><div class="ed-main">${t}${b}</div></div>`;
const emptyArea = (icon, title, text, button = "") => `<div class="main"><div class="ed-main">${tabs([{ name: title, path: title, on: true }])}<div class="ed-empty"><div class="empty">${ic(icon)}${text}${button}</div></div></div></div>`;
section("editor", "3. The Editor page",
  p("<code>#/editor/&lt;folder&gt;</code>. The top bar is the folder's: its breadcrumbs, how it stands against main, Fetch and Open in. Under it, the file tree (260px; its button or Ctrl+B folds it) beside the tabs and the code. At the bottom, the status bar, which is the open file's. Under the top bar there is room for one bar about the folder (§5, §6, and read-only below); under the tabs, one about the open file (§7, and read-only below).")
  + notes(
    "<b>Top bar.</b> Editor / <b>gizai</b> (the path in its tooltip), then the branch in mono and how it stands in words, separated by <code>·</code>: “up to date with origin/main”, “3 behind origin/main”, “1 ahead”, “4 changed”. <b>Fetch</b> fetches now; its tooltip says when it last did. <b>Open in</b> lists the editors found on this computer; each opens the folder, at the open file and line.",
    "<b>Left panel.</b> Files | Search on top (a segmented control: Search is §4), Go to file (Ctrl+P) as a box, then the folder's name with New file, New folder and Fold every folder, then the tree. Folders first, then files, each A to Z without regard to case; dotfiles show, <code>.git</code> never.",
    "<b>The tree</b> (sizes and states in §9): ignored folders greyed and folded, their files read-only; an ignored file elsewhere, such as <code>.env</code>, greyed but it opens and saves as usual. git's mark on the right: a letter in its colour (M, A, R, U, C) with the word as tooltip; a folder that holds changes gets a dot. File icons by kind only (code, JSON, text, .env, image, other), all <code>text-3</code>: colour stays for meaning.",
    "<b>Tabs</b>: the file's name (and its folder when two have the same name, like the two README.md), git's mark, × or a dot while not saved; middle click or Ctrl+W closes. The open tab joins the code under it, with an accent line on top. Tabs are kept per folder on this computer.",
    "<b>The code</b>: CodeMirror in the app's mono font (Settings → Appearance), <code>t-code</code> (12.5/20, growing with the Interface size), line numbers in <code>text-3</code>, the active line on <code>hover</code>, matching brackets, fold carets that show when the gutter is hovered, no wrapping. Colours: D1's <code>syn-*</code> tokens (placeholders here).",
    "<b>Status bar</b>: the file's path and git status on the left; the cursor (a button: go to line), line endings, language and whether it's saved on the right. Not saved is a button that saves.")
  + both(win(sidebar({ editor: "open", on: "gizai" }), gizaiPage()), { label: "gizai, Sidebar.tsx not saved, the gutter hovered" })
  + h3("Go to file, find and replace, Open in")
  + p("Go to file (Ctrl+P) is the command palette's look over the Editor: matching files, the match in bold, each file's folder after it. Find and replace (Ctrl+F) is <code>@codemirror/search</code>'s panel on top of the code, drawn with the app's inputs: Match case, Words and Regular expression inside the box, “3 of 4”, previous and next, then Replace and Replace all. Open in lists only the editors found, each with the command it found.")
  + rows([
    { label: "Go to file (Ctrl+P)", inner: `<div class="main">${gizaiPage().replace(/^<div class="main">/, "").replace(/<\/div>$/, "")}${gotoPalette}</div>`, w: 900, h: 420, cls: "crop" },
    { label: "Open in", inner: stripTop("gizai", X.standOk, "", { actions: edActions({ open: true }) }) + OPEN_IN_MENU, w: 760, h: 230 },
    { label: "Open in: none found", inner: stripTop("gizai", X.standOk, "", { actions: edActions({ open: true }) }) + OPEN_IN_NONE, w: 760, h: 230 },
  ])
  + rows([{ label: "Find and replace (Ctrl+F): Match case on, the third of four matches", inner: `<div class="main"><div class="ed-main">${tabs(GIZAI_TABS)}${findPanel}${findCode()}</div></div>`, w: 1100, h: 420 }])
  + h3("The tree's menu, New file, Delete")
  + p("Right click on a row (or the context menu key, or Shift+F10): New file, New folder, Rename (F2), Delete, Copy path, Open in. New file and Rename type in place; a path makes its folders (<code>app/Exports/InvoicesCsv.php</code>). Delete asks under the row, in one sentence, never with a dialog; Enter or the button deletes, Escape cancels.")
  + rows([
    { label: "Right click on Invoice.php", inner: treeOnly(KADE_SMALL({ invoice: { is: "is-hover" } }), ctxMenu("left:150px;top:132px;min-width:220px"), 440), w: 420, h: 440 },
    { label: "New file: a name that is there", inner: treeOnly(KADE_SMALL({ newFile: "InvoicesCsv.php", note: X.treeExists, noteError: true })), w: 262, h: 420 },
    { label: "New file: a path into .git", inner: treeOnly(KADE_SMALL({ newFile: "../.git/hooks/pre-commit", note: X.treeGit, noteError: true })), w: 262, h: 420 },
    { label: "Delete asks first", inner: treeOnly(KADE_SMALL({ invoice: { selected: true, confirm: X.treeDelete } })), w: 262, h: 420 },
  ])
  + h3("Closing a tab that isn't saved")
  + rows([{ label: "Middle click or × on Sidebar.tsx", inner: `<div class="main"><div class="ed-main">${tabs(GIZAI_TABS.map((f, i) => i === 0 ? { ...f, is: "is-hover" } : f))}${sidebarCode()}</div></div>${askPop}`, w: 820, h: 260 }])
  + h3("Read-only: a card's worktree")
  + p("A card's worktree is read-only while an agent works on it, and editable when the card is yours. On another's card with no agent at work, the Editor says whose it is and offers Edit anyway; your changes then wait, uncommitted, for the agent's next run. The bar takes the folder bar's place; the tabs show a lock, the status bar says why.")
  + strips([
    { label: "An agent works on the card now", inner: stripTop("GA-118", "gizai/ga-118-show-a-run-s-changes · 3 changed", bar("", '<span class="pulse" aria-hidden="true"></span>', X.barLive, btn("Open card", { kind: "ghost" })), { mono: true, actions: openInBtn() }) },
    { label: "Another's card, no agent at work", inner: stripTop("GA-112", "gizai/ga-112-remember-the-open-tabs · 1 changed", bar("", ic("lock"), X.barIdle, btn("Edit anyway") + btn("Open card", { kind: "ghost" })), { mono: true, actions: openInBtn() }) },
    { label: "After Edit anyway", inner: stripTop("GA-112", "gizai/ga-112-remember-the-open-tabs · 2 changed", bar("", ic("pencil"), X.barEditing, btn("Done editing", { kind: "ghost" })), { mono: true, actions: openInBtn() }) },
    { label: "An agent starts while you edit", inner: stripTop("GA-112", "gizai/ga-112-remember-the-open-tabs · 2 changed", bar("refused", ic("lock"), X.barTaken, btn("Open card", { kind: "ghost" })), { mono: true, actions: openInBtn() }) },
  ])
  + rows([{ label: "Its tabs and status bar while an agent works on it", inner: `<div class="main"><div class="ed-main">${gaTabs(true)}<div class="ed-code" style="height:60px"></div>${status({ path: "src/components/RunHistory.tsx", git: gitWord("modified", "Modified"), cursor: "Ln 12, Col 1", lang: "TypeScript JSX", save: `<span class="item">${sm("lock")}Read-only: QA Agent is working on GA-118</span>` })}</div></div>`, w: 900, h: 140 }])
  + h3("Read-only files, files it can't show, no tab open")
  + strips([
    { label: "Over 1 MB", inner: fileBarStrip(bar("file", ic("lock"), X.fileBig, openInBtn())) },
    { label: "Not UTF-8", inner: fileBarStrip(bar("file", ic("lock"), X.fileUtf8, openInBtn()), tabs([{ name: "export-1998.csv", path: "storage/legacy/export-1998.csv", on: true, ro: true }])) },
    { label: "In an ignored folder", inner: fileBarStrip(bar("file", ic("lock"), X.fileIgnored), tabs([{ name: "Collection.php", path: "vendor/laravel/framework/src/Illuminate/Support/Collection.php", on: true, ro: true, ignored: true }])) },
    { label: "Save failed", inner: fileBarStrip(bar("file failed", ic("circle-alert"), X.saveFailed, btn("Try again")), tabs([{ name: "Invoice.php", path: "app/Models/Invoice.php", on: true, unsaved: true, mark: "modified" }])) },
  ])
  + rows([
    { label: "Over 5 MB", inner: emptyArea("file-x", "dump.sql", `<b>${X.fileTooBig}</b>`, `<button class="btn">${ic("square-arrow-out-up-right")}Open in VS Code</button>`), w: 420, h: 250 },
    { label: "A binary file", inner: emptyArea("binary", "logo.psd", `<b>${X.fileBinary}</b>`, `<button class="btn">${ic("square-arrow-out-up-right")}Open in VS Code</button>`), w: 420, h: 250 },
    { label: "Deleted, no changes here", inner: emptyArea("file-x", "OldCsv.php", `<b>${X.fileGone}</b>`, `<button class="btn">Close tab</button>`), w: 420, h: 250 },
    { label: "No tab open", inner: `<div class="main"><div class="ed-main"><div class="ed-tabs"></div><div class="ed-empty"><div class="empty">${ic("file-code")}<span>${X.editorEmpty}</span></div></div></div></div>`, w: 420, h: 250 },
    { label: "The folder can't open", inner: `<div class="main">${topbar(crumbs("old-site"), "", "")}<div class="ed-empty"><div class="empty">${ic("folder-x")}<b>${X.pageGone}</b><button class="btn">Remove from the list</button></div></div></div>`, w: 420, h: 250 },
  ]));

// 4. Search in the folder ----------------------------------------------------------------------------------------------------
const hitFile = (name, dir, n, open = true) => row([0, "file", name, { title: `${dir}/${name}`, count: n }]).replace('<span class="room"></span>', ic(open ? "chevron-down" : "chevron-right")).replace(`<span class="name">${name}</span>`, `<span class="name">${name}</span><span class="where">${dir}</span>`);
const hit = (no, text, m, on = false) => {
  const i = text.indexOf(m);
  const before = text.slice(0, i).trimStart();
  const lead = before.length > 14 ? `…${before.slice(-12)}` : before;
  return `<div class="ed-hit" role="option"${on ? ' aria-selected="true"' : ""} tabindex="-1"><span class="no">${no}</span><span class="tx">${esc(lead)}<mark>${esc(m)}</mark>${esc(text.slice(i + m.length))}</span></div>`;
};
const searchPanel = (sum, results, { query = "total()", sumCls = "", sumIcon = "" } = {}) => `<div class="ed-search-in"><input class="input" value="${q(query)}" placeholder="${q(X.searchBox)}" aria-label="Search in kade-portal">`
  + `<span class="ed-toggles"><button class="ed-toggle" aria-pressed="false" title="Match case">${ic("case-sensitive")}</button><button class="ed-toggle" aria-pressed="false" title="Words">${ic("whole-word")}</button><button class="ed-toggle" aria-pressed="false" title="Regular expression">${ic("regex")}</button></span></div>`
  + `<div class="ed-search-sum${sumCls ? ` ${sumCls}` : ""}" role="status">${sumIcon}${sum}</div><div class="ed-results" role="listbox" aria-label="Results">${results}</div>`;
const RESULTS = hitFile("Invoice.php", "app/Models", 1) + hit(LN_TOTAL, "    public function total(): int", "total()", true)
  + hitFile("InvoiceController.php", "app/Http/Controllers", 2) + hit(41, "            'total' => $invoice->total(),", "total()") + hit(58, "        return response()->json(['total' => $invoice->total()]);", "total()")
  + hitFile("show.blade.php", "resources/views/invoices", 2) + hit(18, '    <td class="amount">{{ money($invoice->total()) }}</td>', "total()") + hit(31, "    @if ($invoice->total() > 0)", "total()")
  + hitFile("InvoiceTotalTest.php", "tests/Feature", 3) + hit(14, "        $this->assertSame(12100, $invoice->total());", "total()") + hit(22, "        $this->assertSame(0, $empty->total());", "total()") + hit(30, "        $this->assertSame(9075, $discounted->total());", "total()");
const kadeTabs = (o = {}) => tabs([{ name: "Invoice.php", path: "app/Models/Invoice.php", on: true, mark: o.mark, unsaved: o.unsaved }, { name: "web.php", path: "routes/web.php" }, { name: "show.blade.php", path: "resources/views/invoices/show.blade.php" }]);
const kadeStatus = (save = SAVE.saved, o = {}) => status({ path: "app/Models/Invoice.php", git: o.git ?? "", cursor: o.cursor ?? `Ln ${LN_TOTAL}, Col 28`, lang: "PHP", save });
section("search", "4. Search in the folder",
  p("Ctrl+Shift+F, or Search at the top of the left panel, puts the search in the tree's place; Files or Escape brings the tree back. It searches as you type (after 300 ms), with <code>git grep</code> over the files git tracks and new files it doesn't ignore.")
  + notes(
    "The box with Match case, Words and Regular expression inside it, pressed like the Markdown toolbar's buttons. Under it, how many results; then the results by file: the file's name, its folder (<code>text-3</code>), how many hits; under it each hit's line number and line, the match marked as the Memory page marks its search hits.",
    "A click (or Enter) opens the file at that line and marks the line for a moment; the result stays selected. A long line shows from a little before its match. At most 2,000 results and 10 seconds; then it says so.",
    "Search in a file is Ctrl+F (§3); replacing across files waits for a later version.")
  + both(win(sidebar({ editor: "open", on: "kade-portal" }), editorMain({
    crumbs: crumbs("kade-portal", { title: "~/Herd/kade-portal" }), stand: standing("main · up to date with origin/main"),
    side: sidePanel({ title: "kade-portal", path: "~/Herd/kade-portal", mode: "search", search: searchPanel(X.searchSum, RESULTS) }),
    tabs: kadeTabs(), body: code(invoice(14, 44), { first: 14, active: LN_TOTAL, flash: LN_TOTAL, cursor: { line: LN_TOTAL, col: 20 } }),
    status: kadeStatus(SAVE.saved, { cursor: `Ln ${LN_TOTAL}, Col 21` }),
  })), { label: "kade-portal, “total()”, the first result opened" })
  + rows([
    { label: "Before a search", inner: `<div class="ed-side">${searchPanel(X.searchHint, "", { query: "", sumIcon: sm("info") })}</div>`, w: 262, h: 230 },
    { label: "While it searches", inner: `<div class="ed-side">${searchPanel(X.searchBusy, "", { query: "InvoiceLine", sumIcon: sm("loader-circle", "spin") })}</div>`, w: 262, h: 230 },
    { label: "Nothing found", inner: `<div class="ed-side">${searchPanel(X.searchNone, "", { query: "totaal", sumIcon: sm("info") })}</div>`, w: 262, h: 230 },
    { label: "2,000 results", inner: `<div class="ed-side">${searchPanel(X.searchCap, hitFile("app.css", "resources/css", 412, false), { query: "a", sumIcon: sm("info") })}</div>`, w: 262, h: 230 },
    { label: "Stopped after 10 seconds", inner: `<div class="ed-side">${searchPanel(X.searchTime, hitFile("seed.sql", "database/dumps", 1204, false), { query: "\\d+", sumIcon: sm("info") })}</div>`, w: 262, h: 230 },
    { label: "A bad regular expression", inner: `<div class="ed-side">${searchPanel(X.searchBad, "", { query: "total(", sumCls: "error", sumIcon: sm("circle-alert") }).replace('aria-pressed="false" title="Regular expression"', 'aria-pressed="true" title="Regular expression"')}</div>`, w: 262, h: 230 },
  ]));

// 5. Behind main and Update ----------------------------------------------------------------------------------------------------
const behindStrip = (b, standText = "main · 3 behind origin/main", o = {}) => stripTop("kade-portal", standText, b, o);
const updateBtn = (o = {}) => btn(o.busy ? "Updating…" : "Update", { kind: o.off || o.busy ? "" : "primary", icon: o.busy ? "loader-circle" : "arrow-down-to-line", off: o.off || o.busy, busy: o.busy, title: o.title ?? "" });
const refusal = (text) => bar("refused", ic("triangle-alert"), text, updateBtn({ off: true, title: plain(text).replace(/^.*?Update can't run: /, "Update can't run: ") }));
section("behind", "5. Behind main, and Update",
  p("Gizai fetches main when the Editor opens a folder, every 10 minutes while it shows it, and on Fetch: the same fetch as a card's start, into the same ref. The top bar always says how the folder stands. When it is behind, a bar under the top bar says by how much, when it was fetched, and what Update will do, in the plan's own words. Update is a fast-forward of main and nothing else (<code>checkout::plan</code>, then <code>checkout::update</code>).")
  + notes(
    "<b>Update</b> is the bar's one button, primary. Fetch is in the top bar only, so neither button is on screen twice; a failed fetch offers Try again in the bar.",
    "<b>Refused</b>: the bar says why, in the plan's sentence after “Update can't run:”, and Update stays, turned off, with the same sentence as its tooltip. Nothing happens by surprise. The Editor never switches branches, so its refusal for another branch says to switch first, in a terminal or your own editor (the plan's own sentence is meant for the Team Lead).",
    "<b>Updating</b>: the plan's words with a turning loader; Update and Fetch are off. <b>Done</b>: what it did, with a check in <code>success</code>, and Dismiss. <b>Stopped</b>: what failed and what it did before, on <code>danger-soft</code>, with the output to show.",
    "The sidebar's row says “updating”, then loses its “3 behind” without a reload.",
    "<b>Contract C3 needs one more field for this</b>: <code>EditorUpdated.output</code> (the end of the output when it stopped), for Show output. Everything else is in <code>FolderMain</code> and <code>EditorUpdated</code> as they are.")
  + both(win(sidebar({ editor: "open", on: "kade-portal" }), editorMain({
    crumbs: crumbs("kade-portal", { title: "~/Herd/kade-portal" }), stand: standing("main · 3 behind origin/main"),
    bar: bar("behind", ic("arrow-down-to-line"), X.behindCan, updateBtn({ title: "Move main 3 commits to 319bf0b and run composer install and npm ci" })),
    side: sidePanel({ title: "kade-portal", path: "~/Herd/kade-portal", rows: tree(KADE_TREE()) }),
    tabs: kadeTabs(), body: code(invoice(1, 44), { first: 1, active: 10, cursor: { line: 10, col: 6 } }), status: kadeStatus(SAVE.saved, { cursor: "Ln 10, Col 7" }),
  })), { label: "kade-portal, 3 behind, Update can run" })
  + h3("Its states")
  + strips([
    { label: "1 behind, nothing to install", inner: behindStrip(bar("behind", ic("arrow-down-to-line"), X.behindOne, updateBtn()), "main · 1 behind origin/main") },
    { label: "Refused: uncommitted changes", inner: behindStrip(refusal(X.refDirty), "main · 3 behind origin/main · 2 changed") },
    { label: "Refused: a merge in progress (a rebase, a cherry-pick and a revert say the same)", inner: behindStrip(refusal(X.refMerge), "main · 3 behind origin/main · 4 changed") },
    { label: "Refused: on another branch", inner: behindStrip(refusal(X.refBranch), "feature/pdf-export · 3 behind origin/main") },
    { label: "Refused: no branch", inner: behindStrip(refusal(X.refDetached), "no branch (3f5c1a2) · 3 behind origin/main") },
    { label: "Refused: main has its own commits", inner: behindStrip(refusal(X.refAhead), "main · 3 behind origin/main · 1 ahead") },
    { label: "Updating", inner: behindStrip(bar("", ic("loader-circle", "icon spin"), X.updating, updateBtn({ busy: true })), "main · 3 behind origin/main", { actions: edActions({ busy: false }).replace("<button class=\"btn ghost sm\"", "<button class=\"btn ghost sm\" disabled") }) },
    { label: "Done", inner: behindStrip(bar("done", ic("circle-check"), X.updated, dismiss), "main · up to date with origin/main") },
    { label: "Stopped, its output shown", inner: behindStrip(bar("failed", ic("circle-alert"), X.stopped, btn("Hide output", { kind: "ghost" }) + dismiss,
      `<pre class="update-output">npm error code EBADENGINE\nnpm error engine Unsupported engine\nnpm error notsup Required: {"node":"&gt;=22"}\nnpm error notsup Actual:   {"npm":"10.8.2","node":"v20.11.1"}</pre>`), "main · up to date with origin/main") },
    { label: "Refused when its turn came", inner: behindStrip(bar("refused", ic("triangle-alert"), X.didntStart, dismiss), "main · 3 behind origin/main · 1 changed") },
    { label: "Fetch failed", inner: behindStrip(bar("refused", ic("cloud-off"), X.fetchFailed, btn("Try again", { icon: "refresh-cw" })), "main · 3 behind origin/main") },
    { label: "Fetch failed, never fetched", inner: behindStrip(bar("refused", ic("cloud-off"), X.fetchFailedNever, btn("Try again", { icon: "refresh-cw" })), "main · not fetched yet") },
    { label: "Fetching (the top bar's button)", inner: behindStrip("", "main · 3 behind origin/main", { actions: edActions({ busy: true }) }) },
    { label: "No remote: no bar, no Fetch", inner: behindStrip("", X.standNoRemote, { actions: openInBtn() }) },
  ]));

// 6. A person's card ------------------------------------------------------------------------------------------------------------
const cardBar = ({ state, commit = "", push = "", hand = "", lead = st("in_progress"), kind = "hand", extra = "" }) => bar(kind, lead,
  `<span class="ed-card-id">KADE-41</span><span class="ed-card-title">Fix the invoice total</span><span class="ed-card-branch" title="gizai/kade-41-fix-the-invoice-total">${sm("git-branch")}<span>gizai/kade-41-fix-the-invoice-total</span></span><span class="ed-card-state">${state}</span>${extra}`,
  commit + push + hand);
const commitBtn = (o = {}) => btn("Commit…", { kind: o.primary ? "primary" : "", icon: "git-commit-horizontal", off: o.off, title: o.title, cls: o.cls });
const pushBtn = (o = {}) => btn("Push", { icon: "upload", off: o.off, title: o.title });
const handBtn = (o = {}) => btn(o.label ?? "Hand over to Testing", { kind: o.primary ? "primary" : "", icon: "send", off: o.off, title: o.title ?? "Pushes, moves KADE-41 to Testing for QA Agent and takes the pin off" });
const commitPop = `<div class="pop ed-commit" role="dialog" aria-label="Commit" style="right:248px;left:auto;top:100px">`
  + `<div class="field"><label>Commit message</label><textarea class="textarea" aria-label="Commit message">KADE-41: Fix the invoice total</textarea></div>`
  + `<ul aria-label="What it commits"><li>${mark("modified")}<span>app/Models/Invoice.php</span></li><li>${mark("untracked")}<span>tests/Feature/InvoiceTotalTest.php</span></li></ul>`
  + `<p style="margin-top:-4px;font-size:var(--fs-sm);color:var(--text-2)">Commits every change, as git sees it: 2 files.</p>`
  + `<div class="acts"><button class="btn ghost sm">Cancel</button><button class="btn primary sm">${ic("git-commit-horizontal")}Commit ${kbd("Ctrl Enter")}</button></div></div>`;
const taskTop = (o) => `<div class="main">${topbar(`<a>Tasks</a><span class="sep">/</span><a>Kade portal</a><span class="sep">/</span><b class="mono">KADE-41</b>`, "",
  `<button class="btn ghost sm" title="Copy the task ID">${ic("copy")}Copy ID</button>${o.button}${o.openIn ? openInBtn() : ""}<button class="btn ghost sm icon-only" aria-label="Show properties" title="Show properties (])">${ic("panel-right")}</button>`)}`
  + `${o.banner ?? ""}<div class="doc"><div class="doc-in" style="padding-top:18px;gap:10px"><div class="task-head">${st(o.st)}${pri("high")}<span class="id">KADE-41</span><span class="proj"><span class="dot" style="width:8px;height:8px;border-radius:50%;background:var(--c-orange)"></span>Kade portal</span>${o.badge ?? ""}</div>`
  + `<div class="t-task-title" style="margin:0">Fix the invoice total</div></div></div></div>`;
const byHandBadge = `<span class="badge" title="Pinned to Jeffrey Sevinga: agents leave it alone until it's handed over">${sm("keyboard")}${X.byHand}</span>`;
const openEditorBtn = (o = {}) => `<button class="btn sm" title="${q(o.title ?? "Work on KADE-41 by hand: Gizai makes its worktree, pins the card to you so agents leave it alone, and moves it to In progress.")}"${o.off ? " disabled" : ""}${o.busy ? ' disabled aria-busy="true"' : ""}>${ic(o.busy ? "loader-circle" : "square-code", o.busy ? "icon spin" : "icon")}${o.busy ? "Preparing the worktree…" : X.openInEditor}</button>`;
const boardCard = ({ id, title, pr = "", labels = "", hand = false, who = "", agent = false, working = "" }) => `<div class="card"><div class="top"><span class="id">${id}</span>${pr}</div><div class="title">${title}</div>`
  + `${working ? `<div class="working"><span class="pulse" aria-hidden="true"></span>${working} is working</div>` : ""}`
  + `<div class="meta">${labels}${hand ? `<span class="badge by-hand" title="Pinned to Jeffrey Sevinga: agents leave it alone until it's handed over">${sm("keyboard")}By hand</span>` : ""}${who ? `<span class="avatar sm${agent ? " agent" : ""}" title="${q(who)}">${agent ? "" : "JS"}</span>` : ""}</div></div>`;
const label = (name, c) => `<span class="label-pill"><span class="dot" style="background:var(--${c})"></span>${name}</span>`;
const agentAv = (icon) => `<span class="avatar sm agent">${ic(icon)}</span>`;
const board = `<div class="main">${topbar("<b>Tasks</b>", "", `<div class="seg"><button title="List">${ic("list")}</button><button class="on" title="Board">${ic("columns-3")}</button></div>`)}<div class="board">`
  + `<div class="col"><div class="col-head">${st("ready")}<span class="name">To do</span><span class="n">2</span></div><div class="col-note">Backend Agent takes cards by priority</div><div class="col-body">`
  + boardCard({ id: "KADE-44", title: "Send a reminder when an invoice is due", pr: pri("medium"), labels: label("Feature", "c-blue"), who: "Backend Agent", agent: true }).replace('<span class="avatar sm agent" title="Backend Agent"></span>', agentAv("server"))
  + boardCard({ id: "KADE-45", title: "Show the VAT rate on the invoice PDF", pr: pri("low"), labels: label("Feature", "c-blue") }) + `</div></div>`
  + `<div class="col"><div class="col-head">${st("in_progress")}<span class="name">In progress</span><span class="n">2</span></div><div class="col-body">`
  + boardCard({ id: "KADE-41", title: "Fix the invoice total", pr: pri("high"), labels: label("Bug", "c-red"), hand: true, who: "Jeffrey Sevinga" })
  + boardCard({ id: "KADE-39", title: "Export invoices as CSV", pr: pri("medium"), labels: label("Feature", "c-blue"), working: "Backend Agent", who: "Backend Agent", agent: true }).replace('<span class="avatar sm agent" title="Backend Agent"></span>', agentAv("server"))
  + `</div></div><div class="col"><div class="col-head">${st("testing")}<span class="name">Testing</span><span class="n">1</span></div><div class="col-body">`
  + boardCard({ id: "KADE-37", title: "Credit notes", pr: pri("medium"), labels: label("Feature", "c-blue"), working: "QA Agent", who: "QA Agent", agent: true }).replace('<span class="avatar sm agent" title="QA Agent"></span>', agentAv("flask-conical"))
  + `</div></div></div></div>`;
const kade41Page = ({ bar: b, overlay = "", unsaved = false } = {}) => editorMain({
  crumbs: crumbs("KADE-41", { mono: true, title: "~/.local/share/gizai/worktrees/KADE/KADE-41" }), stand: standing(X.standCard), bar: b,
  side: sidePanel({ title: "KADE-41", path: "~/.local/share/gizai/worktrees/KADE/KADE-41", rows: tree(KADE_TREE({ invoiceMark: "modified", extra: ["test"] })) }),
  tabs: tabs([{ name: "Invoice.php", path: "app/Models/Invoice.php", on: true, mark: "modified", unsaved }, { name: "InvoiceTotalTest.php", path: "tests/Feature/InvoiceTotalTest.php", mark: "untracked" }]),
  body: code(invoice(14, 44), { first: 14, active: LN_RETURN, cursor: { line: LN_RETURN, col: 61 } }),
  status: status({ path: "app/Models/Invoice.php", git: gitWord("modified", "Modified"), cursor: `Ln ${LN_RETURN}, Col 62`, lang: "PHP", save: SAVE.saved }), overlay,
});
section("card", "6. A person's card",
  p("A card assigned to you can be done by hand. <b>Open in editor</b> on the card makes its worktree as for an agent, pins the card to you so agents leave it alone, moves it to In progress, and opens the worktree here. A card bar then takes the folder bar's place: Commit…, Push and Hand over move it on as an agent's run would. On the board the card says “By hand” while it is pinned to a person.")
  + notes(
    "<b>On the card</b>: Open in editor (Lucide <code>square-code</code>) in the top bar's actions, for a card assigned to you that no agent is working on; its tooltip says what it does. While the worktree is prepared (composer install, npm ci) it says so and turns. Off, with the reason, while an agent works on it. A card started by hand gets the “By hand” badge in its head; Open in editor then just opens it.",
    "<b>The card bar</b>: the column's glyph, the identifier and title, the branch (mono, <code>text-3</code>), how far it is, then Commit…, Push and Hand over to Testing (to Review when the card's Testing switch is off). The button of the next step is primary: Commit… while there are changes, then Hand over, which pushes too. A button that can't act is off, with why as its tooltip.",
    "<b>Commit…</b> opens a small box under it: the message, filled in with “KADE-41: Fix the invoice total”, and the files it commits (every change, as git sees it). Ctrl+Enter commits. What each button did shows in the bar in place of how far it is, for a few seconds.",
    "<b>Handed over</b>: the bar says where it went; the worktree is read-only once QA works on it. <b>Back from QA</b>: the card is pinned to you again, and the bar says so in <code>needs</code> (magenta: a person is needed) until you open the card.",
    "<b>On the board</b>: the badge “By hand” (Lucide <code>keyboard</code>) before the person's avatar, never a working strip: no agent waits for it or works on it. The column's note doesn't count it.")
  + rows([
    { label: "Assigned to you, in To do", inner: taskTop({ st: "ready", button: openEditorBtn() }), w: 720, h: 150 },
    { label: "Preparing its worktree", inner: taskTop({ st: "ready", button: openEditorBtn({ busy: true }) }), w: 720, h: 150 },
    { label: "Started by hand", inner: taskTop({ st: "in_progress", button: openEditorBtn({ title: "Open KADE-41's worktree in the Editor" }), openIn: true, badge: byHandBadge }), w: 720, h: 150 },
    { label: "An agent works on it", inner: taskTop({ st: "in_progress", button: openEditorBtn({ off: true, title: X.openInEditorOff }) }), w: 720, h: 150 },
    { label: "Its worktree couldn't be made", inner: taskTop({ st: "ready", button: openEditorBtn(), banner: `<div class="error-banner" role="alert">${X.openFailed}<button class="btn ghost sm">Dismiss</button></div>` }), w: 720, h: 222 },
  ])
  + both(win(sidebar({ editor: "open", on: "KADE-41" }), kade41Page({
    bar: cardBar({ state: X.cardChanged, commit: commitBtn({ primary: true, cls: "is-pressed" }), push: pushBtn({ off: true, title: "Nothing to push" }), hand: handBtn({ off: true, title: "Commit your changes first: 2 files aren't committed." }) }),
    overlay: commitPop,
  })), { label: "KADE-41 by hand: 2 files changed, Commit… open" })
  + h3("The card bar's states")
  + strips([
    { label: "Nothing changed yet", inner: stripTop("KADE-41", "gizai/kade-41-fix-the-invoice-total", cardBar({ state: X.cardNone, commit: commitBtn({ off: true, title: "Nothing to commit" }), push: pushBtn({ off: true, title: "Nothing to push" }), hand: handBtn({ off: true, title: "Commit a change first" }) }), { mono: true }) },
    { label: "Committed: what Commit did, for a few seconds", inner: stripTop("KADE-41", "gizai/kade-41-fix-the-invoice-total · 1 not pushed", cardBar({ state: `${sm("check")} ${X.committed}`, commit: commitBtn({ off: true, title: "Nothing to commit" }), push: pushBtn(), hand: handBtn({ primary: true }) }), { mono: true }) },
    { label: "1 commit not pushed", inner: stripTop("KADE-41", "gizai/kade-41-fix-the-invoice-total · 1 not pushed", cardBar({ state: X.cardUnpushed, commit: commitBtn({ off: true, title: "Nothing to commit" }), push: pushBtn(), hand: handBtn({ primary: true }) }), { mono: true }) },
    { label: "Pushed", inner: stripTop("KADE-41", "gizai/kade-41-fix-the-invoice-total", cardBar({ state: X.cardPushed, commit: commitBtn({ off: true, title: "Nothing to commit" }), push: pushBtn({ off: true, title: "Nothing to push" }), hand: handBtn({ primary: true }) }), { mono: true }) },
    { label: "The card's Testing switch is off", inner: stripTop("KADE-41", "gizai/kade-41-fix-the-invoice-total", cardBar({ state: X.cardPushed, commit: commitBtn({ off: true, title: "Nothing to commit" }), push: pushBtn({ off: true, title: "Nothing to push" }), hand: handBtn({ primary: true, label: X.handOverReview, title: "Pushes, moves KADE-41 to Review for you and takes the pin off" }) }), { mono: true }) },
    { label: "Push failed", inner: stripTop("KADE-41", "gizai/kade-41-fix-the-invoice-total · 1 not pushed", bar("failed", ic("circle-alert"), X.pushFailed, btn("Try again", { icon: "upload" }) + dismiss), { mono: true }) },
    { label: "Commit refused by a git hook", inner: stripTop("KADE-41", "gizai/kade-41-fix-the-invoice-total · 2 changed", bar("failed", ic("circle-alert"), X.commitHook, btn("Show output", { kind: "ghost" }) + dismiss), { mono: true }) },
    { label: "Handed over", inner: stripTop("KADE-41", "gizai/kade-41-fix-the-invoice-total", bar("done", ic("circle-check"), X.cardHanded, btn("Open card", { kind: "ghost" })), { mono: true }) },
    { label: "Back from QA", inner: stripTop("KADE-41", "gizai/kade-41-fix-the-invoice-total", bar("needs", st("in_progress"), `${X.cardBack}`, btn("Open card", { kind: "primary" }) + commitBtn({ off: true, title: "Nothing to commit" }) + pushBtn({ off: true, title: "Nothing to push" }) + handBtn({ off: true, title: "Commit a change first" })), { mono: true }) },
  ])
  + h3("On the board")
  + both(board, { h: 520, label: "KADE-41 by hand beside an agent's card", cls: "noside" }));

// 7. Changed on disk ------------------------------------------------------------------------------------------------------------
const diskPage = (b, { unsaved = true, save = SAVE.unsaved } = {}) => editorMain({
  crumbs: crumbs("kade-portal", { title: "~/Herd/kade-portal" }), stand: standing("main · up to date with origin/main · 1 changed"),
  side: sidePanel({ title: "kade-portal", path: "~/Herd/kade-portal", rows: tree(KADE_TREE({ invoiceMark: "modified" })) }),
  tabs: kadeTabs({ mark: "modified", unsaved }), filebar: b,
  body: code(invoice(14, 44), { first: 14, active: LN_RETURN, cursor: { line: LN_RETURN, col: 30 } }),
  status: kadeStatus(save, { git: gitWord("modified", "Modified"), cursor: `Ln ${LN_RETURN}, Col 31` }),
});
const diskBar = (k) => ({
  changed: bar("file refused", ic("file-warning"), X.diskChanged, btn("Reload", { title: "Show the version on disk: your unsaved changes here are lost" }) + btn("Keep mine", { kind: "ghost", title: "Keep your version: saving it replaces the one on disk" })),
  keeping: bar("file", ic("file-warning"), X.diskKeeping, btn("Reload", { kind: "ghost", title: "Show the version on disk: your unsaved changes here are lost" })),
  confirm: bar("file refused", ic("triangle-alert"), X.diskConfirm, btn("Save over it", { kind: "danger" }) + btn("Cancel", { kind: "ghost" })),
  deleted: bar("file refused", ic("file-x"), X.diskDeleted, btn("Save it again") + btn("Close tab", { kind: "ghost" })),
}[k]);
section("disk", "7. Changed on disk",
  p("An agent, a terminal or another editor can change a file that is open here. The Editor asks every 3 seconds for the open files' versions. A tab you haven't touched reloads by itself, and the status bar says so for a few seconds. A tab with changes you haven't saved gets a bar over its code; Save refuses until you pick.")
  + notes(
    "<b>Reload</b> shows the version on disk; your unsaved changes here are lost (its tooltip says so). <b>Keep mine</b> keeps your version; the next Ctrl+S then asks once more in the same bar, and the second one saves over the newer file.",
    "<b>Deleted on disk</b>: Save it again writes your version back; Close tab drops it. With no changes here, the tab says the file isn't there any more (§3).",
    "The bar uses the <code>warning</code> icon: a save could lose someone's work. Only the tab's file gets the bar, under the tabs; another tab with the same trouble gets its bar when it is opened.")
  + both(win(sidebar({ editor: "open", on: "kade-portal" }), diskPage(diskBar("changed"))), { label: "Invoice.php changed on disk while not saved here" })
  + strips([
    { label: "After Keep mine", inner: fileBarStrip(diskBar("keeping"), kadeTabs({ mark: "modified", unsaved: true })) },
    { label: "The first Ctrl+S after Keep mine", inner: fileBarStrip(diskBar("confirm"), kadeTabs({ mark: "modified", unsaved: true })) },
    { label: "Deleted on disk", inner: fileBarStrip(diskBar("deleted"), kadeTabs({ mark: "modified", unsaved: true })) },
  ])
  + rows([
    { label: "An untouched tab reloaded", inner: `<div class="main"><div class="ed-main">${kadeStatus(SAVE.reloaded, { git: gitWord("modified", "Modified") })}</div></div>`, w: 760, h: 26 },
    { label: "Saving", inner: `<div class="main"><div class="ed-main">${kadeStatus(SAVE.saving, { git: gitWord("modified", "Modified") })}</div></div>`, w: 760, h: 26 },
    { label: "Save failed", inner: `<div class="main"><div class="ed-main">${kadeStatus(SAVE.failed, { git: gitWord("modified", "Modified") })}</div></div>`, w: 760, h: 26 },
  ]));

// 8. Settings → General → Editor folders ------------------------------------------------------------------------------------------
const folderRow = (value, note = "", kind = "hint") => `<div class="folder-row"><div class="input-group"><input class="input mono" value="${q(value)}" aria-label="Folder" placeholder="~/Herd">`
  + `<button type="button" class="btn ghost sm icon-only" aria-label="Remove ${q(value)}" title="Remove">${ic("x")}</button></div>${note ? `<span class="${kind}">${note}</span>` : ""}</div>`;
const settingsSection = (rowsHtml, { empty = false } = {}) => `<section class="form-section"><header><h3>${X.setTitle}</h3><p>Where your projects are. Besides projects' linked folders and card worktrees, the Editor opens every folder directly inside these.</p></header>`
  + `<div class="fields"><div class="field wide"><label>Folders</label><div class="folder-list">${empty ? `<span class="hint">${X.setEmpty}</span>` : rowsHtml}`
  + `<div style="display:flex;gap:8px"><button type="button" class="btn sm">${ic("plus")}Add folder</button><button type="button" class="btn sm ghost">${ic("folder-open")}Browse…</button></div></div>`
  + `<span class="hint">${X.setHint}</span></div></div></section>`;
const settingsPage = `<div class="main">${topbar("<b>Settings</b>", "", '<button class="btn primary">Save settings</button>')}<div class="content"><div class="page settings-page" style="max-width:1100px">`
  + `<div class="tabs" role="tablist" aria-label="Settings">${[["sliders-horizontal", "General", true], ["palette", "Appearance"], ["bell", "Notifications"], ["bot", "Agents and runs"], ["plug", "MCP servers"], ["git-pull-request", "GitHub and Bitbucket"]].map(([i, l, on]) => `<button class="tab" role="tab" aria-selected="${!!on}">${ic(i)}${l}</button>`).join("")}</div>`
  + `<div class="form settings-panel" role="tabpanel" aria-label="General"><section class="form-section dz-faded"><header><h3>Updates</h3><p>Gizai updates from its releases on GitHub. An update builds the new version from source in the background, backs up your data, installs it and offers a restart.</p></header><div class="fields"><div class="field wide"><label>Version</label><span>0.7.2</span></div></div></section>`
  + settingsSection(folderRow("~/Herd", X.setOk) + folderRow("~/Code", "3 folders: gizai, gizai-site-old and notes."))
  + `<section class="form-section dz-faded"><header><h3>Data</h3><p>Everything Gizai stores lives in this folder on this computer.</p></header><div class="fields"><div class="field wide"><label>Data folder</label><span class="mono">/home/jeffrey/.local/share/gizai</span></div></div></section>`
  + `</div></div></div></div>`;
section("settings", "8. Settings → General → Editor folders",
  p("Where your projects are. The folder picker lists every folder directly inside these (default <code>~/Herd</code>); projects' linked folders and card worktrees open wherever they are. A section of General, after Updates and before Data, with the same rows as an agent's Folders: a path in mono, × to remove, Add folder, and Browse… for the system's folder picker.")
  + notes(
    "Each row is checked as you type, with the refusals agent folders already have (<code>folders.rs</code>), written for this field. Under a good row: how many folders it holds and the first names, so you see what the picker will list. A missing folder is a warning, not a refusal: it may be a disk that isn't mounted.",
    "Saved as soon as you add, change or remove one, like Settings' other lists. The Team Lead's copies (<code>&lt;data&gt;/code</code>) and Gizai's data folder never open, whatever is here.")
  + both(win(sidebar({ editor: "open", page: "settings" }), settingsPage), { label: "General, scrolled to Editor folders" })
  + rows([
    { label: "Refused and warned rows", inner: `<div class="main" style="padding:16px 20px"><div class="folder-list" style="display:flex;flex-direction:column;gap:10px">`
      + [["~", X.setHome], ["~/.ssh", X.setKeys], ["~/.local/share/gizai/worktrees", X.setData], ["/", X.setDisk], ["Herd", X.setFull], ["~/Herd", X.setTwice], ["~/Herd/notes.md", X.setFile]].map(([v, e]) => `<div class="field">${folderRow(v, e, "error")}</div>`).join("")
      + `<div class="field">${folderRow("/mnt/projects", X.setMissing, "warn")}</div></div></div>`, w: 620, h: 640 },
    { label: "No folders", inner: `<div class="main" style="padding:16px 20px"><div class="form">${settingsSection("", { empty: true })}</div></div>`, w: 860, h: 260 },
  ]));

// 9. Specs ------------------------------------------------------------------------------------------------------------------------
const table = (head, body) => `<table class="dz-table"><thead><tr>${head.map((h) => `<th>${h}</th>`).join("")}</tr></thead><tbody>${body.map((r) => `<tr>${r.map((c) => `<td>${c}</td>`).join("")}</tr>`).join("")}</tbody></table>`;
const c = (s) => `<code>${s}</code>`;
const gallery = (items, width = 262) => `<div class="variants"><div class="vrow dark"><div class="vhead theme-name">Dark</div><div class="vitems">`
  + frame("dark", `<table class="dz-gallery"><tbody>${items.map(([name, html, what]) => `<tr><th>${name}</th><td style="width:${width}px">${html}</td><td>${what}</td></tr>`).join("")}</tbody></table>`, { cls: "strip gal" })
  + `</div></div></div>`;
const inTree = (r) => `<div class="ed-side gal-cell"><div class="ed-tree">${r}</div></div>`;
const inTabs = (tb) => `<div class="ed-main gal-cell">${tabs(tb)}</div>`;
const inStatus = (s) => `<div class="ed-main gal-cell">${s}</div>`;
const SIZE_TREE = [
  ["Width", "260px; 6px more each Interface text step", c("--size-tree"), "new"],
  ["Row height", "28px (24px compact)", c("--size-tree-row"), "new"],
  ["Left padding, and a level's indent", "4px, then 16px a level", `${c("--space-1")}, ${c("--space-4")} × depth`, ""],
  ["Right padding", "8px", c("--space-2"), ""],
  ["Caret, icon, name", "14px, 14px; 4px apart, the name 8px after the icon", `${c(".icon.sm")}; ${c("--space-1")}`, ""],
  ["Name", "13.5px, regular; grows with the Interface size", c("--fs"), ""],
  ["git's mark", "11.5px mono, bold, 12px wide", `${c("--fs-xs")}, ${c("--space-3")}`, ""],
  ["Lock (read-only), dot (changes inside)", "12px, 6px", "", ""],
  ["Corner", "6px", c("--radius"), ""],
  ["Panel", `on ${c("side")}, a ${c("line")} border on the right`, "", ""],
];
const SIZE_TABS = [
  ["Strip height", "36px (32px compact)", c("--size-tab"), "new"],
  ["Tab", "12px left, 4px right, 8px between its parts; at most 240px, the name cut off", `${c("--space-3")}, ${c("--space-1")}, ${c("--space-2")}`, ""],
  ["Name", "13.5px", c("--fs"), ""],
  ["Folder hint", "11.5px", c("--fs-xs"), ""],
  ["×", "20px button, 14px icon; the unsaved dot 8px", c("--radius-s"), ""],
  ["Open tab", `${c("bg")} (joins the code), a 2px ${c("accent")} line on top`, "", ""],
  ["Strip", `on ${c("side")}, a ${c("line")} line under it; a ${c("line")} border after each tab`, "", ""],
];
const SIZE_STATUS = [
  ["Height", "24px (20px compact)", c("--size-statusbar"), "new"],
  ["Padding", "12px left, 8px right; 8px between items", `${c("--space-3")}, ${c("--space-2")}`, ""],
  ["Text", "11.5px; the path in mono", c("--fs-xs"), ""],
  ["An item", "18px high, 6px to its sides, 14px icons", c("--radius-s"), ""],
  ["Bar", `on ${c("side")}, a ${c("line")} line above it; text ${c("text-3")}, the path ${c("text-2")}`, "", ""],
];
const sizeTable = (rowsArr) => table(["Part", "Size", "Token", ""], rowsArr.map(([a, b, tk, n]) => [a, b, tk, n ? '<span class="badge info">New</span>' : ""]));
const TOKENS_SYN = [
  ["syn-keyword", "#b197fc", "6.36", "#6f42c1", "5.36", "keywords, modifiers, this"],
  ["syn-string", "#a6d189", "8.84", "#2f6f1f", "5.06", "strings, regular expressions, attribute values"],
  ["syn-number", "#f29a4a", "6.94", "#a14a06", "4.94", "numbers, true, false, null"],
  ["syn-comment", "#8b8f99", "4.74", "#5f636e", "4.94", "comments"],
  ["syn-function", "#7fa6ff", "6.43", "#2556c6", "5.36", "functions called and defined"],
  ["syn-type", "#e3bd3c", "8.48", "#7d5800", "5.29", "types, classes, namespaces"],
  ["syn-variable", "#ededf0", "13.13", "#14161b", "14.89", "variables"],
  ["syn-property", "#c3cbef", "9.58", "#35447a", "7.65", "properties"],
  ["syn-tag", "#f2706b", "5.34", "#c0302b", "4.67", "HTML and JSX tags"],
  ["syn-attribute", "#f29a4a", "6.94", "#a14a06", "4.94", "attributes"],
  ["syn-operator", "#a5a8b1", "6.45", "#4b4f59", "6.74", "operators"],
  ["syn-punctuation", "#a5a8b1", "6.45", "#4b4f59", "6.74", "brackets, separators"],
  ["syn-meta", "#f78c6c", "6.52", "#a83f18", "5.10", "<?php, Blade directives, attributes, macros"],
  ["syn-invalid", "#f2706b", "5.34", "#c0302b", "4.67", "what doesn't parse"],
  ["diff-add", "#5fcf8f", "7.89", "#13703a", "5.07", "D1: + and added lines; here git's A and U"],
  ["diff-del", "#f47a75", "5.76", "#bf302a", "4.71", "D1: − and removed lines; here git's D and C"],
  ["git-modified", "#e3bd3c", "8.48", "#7d5800", "5.29", "New (D2): git's M and R, a folder's dot"],
  ["git-added", "= diff-add", "", "= diff-add", "", "New (D2): git's A and U"],
  ["git-deleted", "= diff-del", "", "= diff-del", "", "New (D2): git's D and C"],
];
const sw = (hex) => hex.startsWith("#") ? `<span class="dz-sw" style="background:${hex}"></span><code>${hex}</code>` : hex;
const CM_THEME = [
  ["&", "background bg; color text; font-family font-mono; font-size 12.5px, line-height 20px (t-code), growing with the Interface size"],
  [".cm-gutters", "background bg; no border; color text-3"],
  [".cm-lineNumbers .cm-gutterElement", "padding 0 12px 0 16px, right-aligned, at least 3 digits wide"],
  [".cm-foldGutter", "14px carets (chevron-down, chevron-right) in text-3, shown while the gutter is hovered, and on folded lines"],
  [".cm-activeLine, .cm-activeLineGutter", "background hover; the line number in text"],
  [".cm-cursor", "a 2px border-left in text"],
  [".cm-selectionBackground, ::selection", "accent-soft (as the Markdown editor)"],
  [".cm-matchingBracket", "background selected; inset 1px line-2; .cm-nonmatchingBracket: syn-invalid"],
  [".cm-searchMatch", "background accent-soft; the current one (.cm-searchMatch-selected) with an inset 1px accent"],
  [".cm-foldPlaceholder", "background selected; color text-2; no border; radius-s; “…”"],
  [".cm-panels", "background raised; a line border; the find panel drawn with .input, .ed-toggle and .btn (§3)"],
  [".cm-tooltip", "background overlay; border line-2; radius; shadow-pop"],
  ["Read-only", "no cursor; the bar or status bar says why; the same colours"],
];
const KEYS = [
  ["Ctrl+P", "Go to file"], ["Ctrl+Shift+F", "Search in the folder"], ["Ctrl+F", "Find and replace in the file"], ["F3 or Ctrl+G, Shift+F3 or Shift+Ctrl+G", "Next and previous match"],
  ["Ctrl+Alt+G", "Go to line"], ["Ctrl+D", "Select the next occurrence"], ["Ctrl+S", "Save (twice after Keep mine)"], ["Ctrl+W, middle click", "Close the tab"],
  ["Ctrl+Tab, Ctrl+Shift+Tab", "Next and previous tab"], ["Ctrl+B", "Fold or show the file tree"], ["F2, Delete (in the tree)", "Rename, Delete (asks first)"],
  ["↑ ↓ ← → Enter (in the tree)", "Move, fold and open folders, open a file"], ["Shift+F10, the context menu key", "The tree's or a sidebar row's menu"], ["Escape", "Closes find, search, a menu or a box"],
];
section("specs", "9. Specs: the tree, the tabs, the status bar, colours, keys",
  p("Sizes are tokens: the four new ones are in <code>editor-tokens.css</code> for F2 to add to <code>tokens.json</code> and, growing with the density and the Interface text size, to <code>appearance.css</code>; the rest are the spacing, text and radius tokens the app has. States follow the design system: hover the <code>hover</code> fill, selected the <code>selected</code> fill, focus a 2px <code>focus</code> ring (inside the row or tab, since they touch), disabled at 45%.")
  + h3("The tree's rows")
  + sizeTable(SIZE_TREE)
  + gallery([
    ["Default", inTree(row([1, "file", "InvoiceLine.php"])), "Name text-2, icon text-3"],
    ["Hover", inTree(row([1, "file", "InvoiceLine.php", { is: "is-hover" }])), "hover fill, name text"],
    ["Selected", inTree(row([1, "file", "Invoice.php", { selected: true }])), "The open tab's file: selected fill, name text"],
    ["Focus", inTree(row([1, "file", "InvoiceLine.php", { is: "is-focus" }])), "Keyboard focus: a 2px focus ring inside the row"],
    ["Unsaved", inTree(row([1, "file", "Invoice.php", { mark: "modified" }])), "Not shown here, on purpose: the tree is what is on disk; the tab and the status bar say “not saved”"],
    ["Read-only", inTree(row([1, "file", "laravel.log", { ro: "Read-only: over 1 MB" }])), "A 12px lock; the tooltip says why (over 1 MB, not UTF-8 text)"],
    ["Ignored folder", inTree(row([0, "dir", "vendor", { ignored: true }])), "Greyed (text-3) and folded; its files read-only; search skips it"],
    ["Ignored file", inTree(row([0, "file", ".env", { ignored: true }])), "Greyed; opens and saves as usual"],
    ["In an ignored folder", inTree(row([1, "file", "autoload.php", { ignored: true, ro: X.fileIgnored }])), "Greyed with the lock"],
    ["git's marks", inTree(row([1, "file", "Invoice.php", { mark: "modified" }]) + row([1, "file", "InvoicesCsv.php", { mark: "added" }]) + row([1, "file", "InvoiceTotalTest.php", { mark: "untracked" }]) + row([1, "file", "Bill.php", { mark: "renamed" }]) + row([1, "file", "web.php", { mark: "conflicted" }])), "M and R git-modified; A and U git-added; C git-deleted; the word in the tooltip"],
    ["Changes inside", inTree(row([0, "dir", "Models", { dot: true }])), "A 6px dot in git-modified"],
    ["Opened folder", inTree(row([0, "dir", "app", { open: true }])), "chevron-down and folder-open"],
    ["Loading, empty, can't read", inTree(row([0, "dir", "storage", { open: true }]) + `<div class="ed-row-note" style="--depth:1">${X.treeLoading}</div>` + row([0, "dir", "logs", { open: true }]) + `<div class="ed-row-note" style="--depth:1">${X.treeEmptyDir}</div>` + row([0, "dir", "private", { open: true }]) + `<div class="ed-row-note error" style="--depth:1">${X.treeCantRead}</div>`), "Text-3 notes under the folder; the error in danger"],
    ["Renaming", inTree(row([1, "file", "", { input: "Invoice.php" }])), "An input in the name's place; Enter renames, Escape cancels"],
  ])
  + h3("The tabs")
  + sizeTable(SIZE_TABS)
  + gallery([
    ["Default", inTabs([{ name: "web.php", path: "routes/web.php" }]), "Name text-2; × hidden"],
    ["Hover", inTabs([{ name: "web.php", path: "routes/web.php", is: "is-hover" }]), "hover fill, name text, × shows"],
    ["Selected", inTabs([{ name: "Invoice.php", path: "app/Models/Invoice.php", on: true }]), "bg (joins the code), name text, a 2px accent line on top, × shows"],
    ["Focus", inTabs([{ name: "web.php", path: "routes/web.php", is: "is-focus" }]), "A 2px focus ring inside the tab; ← and → move between tabs"],
    ["Unsaved", inTabs([{ name: "Invoice.php", path: "app/Models/Invoice.php", on: true, unsaved: true }]), "An 8px dot (text-2) in ×'s place"],
    ["Unsaved, hovered", inTabs([{ name: "Invoice.php", path: "app/Models/Invoice.php", unsaved: true, is: "is-hover" }]), "× in the dot's place"],
    ["Read-only", inTabs([{ name: "laravel.log", path: "storage/logs/laravel.log", on: true, ro: true }]), "A 12px lock before the name"],
    ["Ignored", inTabs([{ name: ".env", path: ".env", ignored: true }]), "The name in text-3, like the tree"],
    ["In an ignored folder", inTabs([{ name: "autoload.php", path: "vendor/autoload.php", ignored: true, ro: true }]), "Greyed with the lock"],
    ["git's mark", inTabs([{ name: "Invoice.php", path: "app/Models/Invoice.php", mark: "modified" }]), "The tree's letter after the name"],
    ["Same name", inTabs([{ name: "README.md", path: "README.md" }, { name: "README.md", dir: "design", path: "design/README.md" }]), "Its folder after the name, text-3"],
  ], 360)
  + h3("The status bar")
  + sizeTable(SIZE_STATUS)
  + gallery([
    ["Saved", inStatus(kadeStatus(SAVE.saved)), ""],
    ["Unsaved", inStatus(kadeStatus(SAVE.unsaved, { git: gitWord("modified", "Modified") })), "“Not saved”, in text, bold, with a dot; a button that saves"],
    ["Hover", inStatus(kadeStatus(SAVE.saved).replace('<button class="item"', '<button class="item is-hover"')), "Its buttons (the cursor, Not saved): hover fill, text"],
    ["Focus", inStatus(kadeStatus(SAVE.unsaved).replace('<button class="item unsaved"', '<button class="item unsaved is-focus"')), "A 2px focus ring around the item"],
    ["Selected", inStatus(kadeStatus(SAVE.saved, { cursor: `Ln ${LN_TOTAL}, Col 28 (12 selected)` })), "No selected state: a selection in the code shows as its count"],
    ["Read-only", inStatus(status({ path: "storage/logs/laravel.log", cursor: "Ln 1, Col 1", lang: "Plain text", save: SAVE.ro })), "A lock and why"],
    ["Ignored", inStatus(status({ path: ".env", git: gitWord("", X.stIgnored), cursor: "Ln 4, Col 18", lang: "Env", save: SAVE.saved })), "In git's place: “Ignored by git”"],
    ["Reloaded", inStatus(kadeStatus(SAVE.reloaded)), "For 5 seconds after an untouched tab reloads"],
    ["Couldn't save", inStatus(kadeStatus(SAVE.failed, { git: gitWord("modified", "Modified") })), "In danger; the file bar says why"],
    ["CRLF, BOM", inStatus(status({ path: "legacy/import.php", cursor: "Ln 3, Col 1", eol: "CRLF · UTF-8 with BOM", lang: "PHP", save: SAVE.saved })), "Kept as the file has them"],
  ], 700)
  + h3("The code's colours")
  + p("CodeMirror's theme for code (F0, <code>src/lib/codeTheme.ts</code>), on the app's tokens; the Markdown editor's theme stays as it is.")
  + table(["CodeMirror", "Values"], CM_THEME.map(([a, b]) => [c(a), b]))
  + p("D1's syntax tokens with the placeholder values this page uses (D1 sets the real ones), the lowest contrast of each on <code>bg</code>, <code>raised</code>, <code>side</code>, <code>hover</code> and <code>selected</code>, and git's tokens. No teal or magenta hue: those mean an agent at work and “needs you”.")
  + table(["Token", "Dark", "Lowest", "Light", "Lowest", "Used for"], TOKENS_SYN.map(([n, d, dc, l, lc, u]) => [c(`--${n}`), sw(d), dc, sw(l), lc, u]))
  + p("Which of Lezer's tags each token colours: the map this page uses to colour TSX with <code>@lezer/javascript</code>, offered for F0's <code>HighlightStyle</code> (D1 may move a tag).")
  + table(["Token", "Lezer tags"], TAG_TOKENS.map(([tk, , tags]) => [c(`--syn-${tk}`), esc(tags)]))
  + h3("Keys")
  + p("Cmd for Ctrl on macOS (the app's <code>modKey()</code>). The app's own keys (N, T, D, Ctrl+K) don't fire in the code, as in the Markdown editor.")
  + table(["Keys", "What they do"], KEYS.map(([k, w]) => [`<span class="kbd-run">${k}</span>`, w])));

section("texts", "10. Every text",
  p("Every bar, button and refusal on this page, in sentence case, in the order of the page. Numbers, names and paths are the examples'; F2, F3 and F4 put in the real ones. A sentence that Gizai's backend makes (the plan's, git's) is shown as the Editor shows it.")
  + table(["Where", "When", "Text", "Buttons, tooltips"], TEXTS.map((x) => [x.where, x.when, x.text, x.buttons])));

// ---------- the page ----------
const toc = sections.map((s) => `<a href="#${s.id}">${s.title}</a>`).join("");
const body = `<main class="dz">
<header class="dz-head">
  <div class="dz-kicker"><span class="badge outline">GA-105</span><span>Code Editor · D2</span><span class="faint">for Jefsev's review</span></div>
  <h1>The Editor</h1>
  <p class="dz-lead">The design of research §5.2: the sidebar's Editor section, the folder picker, the Editor page with its tree, tabs, code and status bar, search in the folder, behind main and Update, a person's card, changed on disk, and Settings → Editor folders. F2 builds the page and the sidebar, F3 behind main, F4 a person's card.</p>
  <p class="dz-p">Every screen is drawn with the app's own stylesheets (<code>src/styles</code>), so it looks as the app will; the new components are in <code>editor.css</code>, written for F2 to move into <code>components.css</code>, and the new tokens in <code>editor-tokens.css</code>. The code's colours use D1's token names (<code>syn-*</code>): D1 isn't approved yet, so their values here are placeholders. Each screen shows dark, then light; the switch shows one theme. Hover a control for its tooltip. Built by <code>node design/code-editor/gen-editor.mjs</code>.</p>
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
<title>Gizai · Code Editor D2: the Editor</title>
<!-- Generated by design/code-editor/gen-editor.mjs: change that file, then run it. -->
<!-- The app's stylesheets in main.tsx's order. editor-tokens.css comes first, so the app's own tokens win once they have its names. -->
<link rel="stylesheet" href="editor-tokens.css">
<link rel="stylesheet" href="../../src/styles/fonts.css">
<link rel="stylesheet" href="../../src/styles/tokens.css">
<link rel="stylesheet" href="../../src/styles/components.css">
<link rel="stylesheet" href="../../src/styles/app.css">
<link rel="stylesheet" href="../../src/styles/appearance.css">
<link rel="stylesheet" href="editor.css">
<!-- This page only: its layout around the frames, and stand-ins for what CodeMirror draws -->
<link rel="stylesheet" href="editor-page.css">
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
fs.writeFileSync(`${HERE}/editor.html`, html);
console.log(`wrote design/code-editor/editor.html: ${sections.length} sections, ${TEXTS.length} texts, ${used.size} icons, ${(html.length / 1024).toFixed(0)} KB`);
