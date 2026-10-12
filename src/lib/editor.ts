// The Editor (GA-113, F2 of the Code Editor; research §5.2, design design/code-editor/editor.html): what its page, tree,
// tabs, status bar and the sidebar's Editor section work out from the backend's answers (contract C2), kept apart from
// the screen so it can be tested. What is kept on this computer (the folders you opened, each folder's tabs) is in
// localStorage, read inside try/catch: the tests run in Node, which has none.
import { useEffect, useState } from "react";
import type { DirEntry, EditorFolder, FolderGit, GitMark, SearchHit, SearchResult } from "../types";

// ---- the folders you opened: the sidebar's Editor section, newest first, at most 8 ----

export const OPENED_KEY = "gizai.editor.opened";
export const MAX_OPENED = 8;
/** Sent on the window when the folders you opened change, so the sidebar follows the page at once. */
const OPENED_EVENT = "gizai-editor-opened";

/** The folders (EditorFolder ids) in what localStorage keeps; nothing kept, or something unreadable, is none. */
export function openedOf(kept: string | null): string[] {
  try {
    const list: unknown = JSON.parse(kept ?? "[]");
    return Array.isArray(list) ? list.filter((x): x is string => typeof x === "string" && x !== "").slice(0, MAX_OPENED) : [];
  } catch { return []; }
}

/** What to keep after opening `id`: it goes to the top (once), the oldest beyond 8 drop off. */
export function withOpened(kept: string | null, id: string): string {
  return JSON.stringify([id, ...openedOf(kept).filter((x) => x !== id)].slice(0, MAX_OPENED));
}

/** What to keep after Remove from the list. */
export function withoutOpened(kept: string | null, id: string): string {
  return JSON.stringify(openedOf(kept).filter((x) => x !== id));
}

function readKey(key: string): string | null { try { return localStorage.getItem(key); } catch { return null; } }
function writeKey(key: string, value: string) { try { localStorage.setItem(key, value); } catch { /* private mode */ } }

function keepOpened(next: string) {
  writeKey(OPENED_KEY, next);
  try { window.dispatchEvent(new Event(OPENED_EVENT)); } catch { /* no window in tests */ }
}
/** Puts the folder at the top of the sidebar's Editor section. */
export const rememberFolder = (id: string) => keepOpened(withOpened(readKey(OPENED_KEY), id));
/** Takes the folder off the sidebar's Editor section (Remove from the list). */
export const forgetFolder = (id: string) => keepOpened(withoutOpened(readKey(OPENED_KEY), id));

/** The folders you opened, newest first; follows rememberFolder and forgetFolder. */
export function useOpenedFolders(): string[] {
  const [list, setList] = useState<string[]>(() => openedOf(readKey(OPENED_KEY)));
  useEffect(() => {
    const on = () => setList(openedOf(readKey(OPENED_KEY)));
    window.addEventListener(OPENED_EVENT, on);
    window.addEventListener("storage", on);
    return () => { window.removeEventListener(OPENED_EVENT, on); window.removeEventListener("storage", on); };
  }, []);
  return list;
}

// ---- the open tabs, kept per folder on this computer ----

export const TABS_KEY = "gizai.editor.tabs";
export type KeptTabs = { paths: string[]; active: string | null };

/** The tabs kept for `folder`: their paths in order, and the one that was open. */
export function keptTabs(kept: string | null, folder: string): KeptTabs {
  try {
    const all = JSON.parse(kept ?? "{}") as Record<string, KeptTabs>;
    const t = all?.[folder];
    if (!t || !Array.isArray(t.paths)) return { paths: [], active: null };
    const paths = t.paths.filter((p) => typeof p === "string" && p !== "");
    return { paths, active: typeof t.active === "string" && paths.includes(t.active) ? t.active : paths[0] ?? null };
  } catch { return { paths: [], active: null }; }
}

/** What to keep after `folder`'s tabs changed; a folder without tabs is left out. */
export function withTabs(kept: string | null, folder: string, tabs: KeptTabs): string {
  let all: Record<string, KeptTabs> = {};
  try {
    const parsed: unknown = JSON.parse(kept ?? "{}");
    // Anything but a plain object of folders (a list, a number, null) is started over.
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) all = parsed as Record<string, KeptTabs>;
  } catch { all = {}; }
  if (tabs.paths.length) all[folder] = tabs;
  else delete all[folder];
  return JSON.stringify(all);
}

export const loadTabs = (folder: string) => keptTabs(readKey(TABS_KEY), folder);
export const keepTabs = (folder: string, tabs: KeptTabs) => writeKey(TABS_KEY, withTabs(readKey(TABS_KEY), folder, tabs));

// ---- paths ----

/** The folder a path is in ("" at the top): "src/lib/editor.ts" → "src/lib". */
export const parentOf = (path: string) => (path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "");
/** The last part of a path: "src/lib/editor.ts" → "editor.ts". */
export const nameOf = (path: string) => path.slice(path.lastIndexOf("/") + 1);
/** `path` itself and the folders it is in, the top one first: "a/b/c.ts" → ["a", "a/b"]. */
export function foldersOf(path: string): string[] {
  const parts = path.split("/").slice(0, -1);
  return parts.map((_, i) => parts.slice(0, i + 1).join("/"));
}
/** A name typed for New file, New folder or Rename, as a path in `dir`: a leading / starts at the folder's top, slashes
 *  and spaces around them are tidied; null when nothing is left. */
export function typedPath(dir: string, typed: string): string | null {
  const t = typed.trim().replace(/\\/g, "/");
  if (!t) return null;
  const top = t.startsWith("/");
  const parts = t.split("/").map((p) => p.trim()).filter(Boolean);
  if (!parts.length) return null;
  return [...(top || !dir ? [] : [dir]), ...parts].join("/");
}

/** How the file already indents, for Tab and new lines: a tab when its lines start with tabs, else the fewest spaces a
 *  line starts with (2 or 4…), 4 when nothing is indented yet. */
export function indentOf(text: string): string {
  let tabs = 0, fewest = 0;
  for (const line of text.split("\n", 2000)) {
    if (line.startsWith("\t")) tabs++;
    const m = /^( +)\S/.exec(line);
    if (m && m[1]!.length >= 2 && (fewest === 0 || m[1]!.length < fewest)) fewest = m[1]!.length;
  }
  if (tabs > 0 && (fewest === 0 || tabs >= 3)) return "\t";
  return " ".repeat(fewest >= 2 && fewest <= 8 ? fewest : 4);
}

// ---- git's marks ----

export const MARK_LETTER: Record<GitMark, string> = { modified: "M", added: "A", deleted: "D", renamed: "R", untracked: "U", conflicted: "C" };
/** The mark's tooltip and name in the tree and on a tab. */
export const MARK_TIP: Record<GitMark, string> = {
  modified: "Modified", added: "Added", deleted: "Deleted", renamed: "Renamed",
  untracked: "Untracked: new, not in git yet", conflicted: "Conflicted: finish the merge in a terminal",
};
/** The mark's word in the status bar. */
export const MARK_WORD: Record<GitMark, string> = { modified: "Modified", added: "Added", deleted: "Deleted", renamed: "Renamed", untracked: "Untracked", conflicted: "Conflicted" };

/** A file's mark: from the folder's git status when Gizai has it (it follows saves and outside changes), else from the
 *  listing. */
export function markOf(path: string, git: FolderGit | null, listed?: GitMark | null): GitMark | null {
  if (git) return git.files[path] ?? null;
  return listed ?? null;
}

/** Whether something inside the folder `dir` changed (its dot in the tree). */
export function holdsChanges(dir: string, git: FolderGit | null, listed?: GitMark | null): boolean {
  if (!git) return !!listed;
  const pre = `${dir}/`;
  return Object.keys(git.files).some((p) => p.startsWith(pre));
}

// ---- the tree ----

/** What the tree knows of a folder level: loading, its entries, or why it can't be read. */
export type Level = "loading" | DirEntry[] | { error: string };

/** A row of the tree as shown: an entry, or a note under an open folder (loading, empty, can't read). */
export type TreeRow =
  | { kind: "entry"; entry: DirEntry; depth: number; inIgnored: boolean }
  | { kind: "note"; dir: string; depth: number; text: string; error: boolean };

/** The tree's rows from the levels Gizai has loaded and the folders that are open, top to bottom. Inside an ignored
 *  folder everything is greyed and read-only (`inIgnored`). */
export function treeRows(levels: Map<string, Level>, open: Set<string>): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (dir: string, depth: number, inIgnored: boolean) => {
    const level = levels.get(dir);
    if (level === undefined || level === "loading") { rows.push({ kind: "note", dir, depth, text: "Loading…", error: false }); return; }
    if (!Array.isArray(level)) { rows.push({ kind: "note", dir, depth, text: level.error, error: true }); return; }
    if (level.length === 0) { rows.push({ kind: "note", dir, depth, text: "Empty folder", error: false }); return; }
    for (const entry of level) {
      rows.push({ kind: "entry", entry, depth, inIgnored });
      if (entry.dir && open.has(entry.path)) walk(entry.path, depth + 1, inIgnored || entry.ignored);
    }
  };
  walk("", 0, false);
  return rows;
}

/** "Can't read this folder: permission denied." from the backend's refusal. */
export function unreadable(error: string): string {
  const why = error.replace(/^.*?:\s*/, "").replace(/\.$/, "");
  return /permission denied/i.test(error) ? "Can't read this folder: permission denied." : `Can't read this folder: ${why || error}.`;
}

// ---- tabs ----

/** Each tab's name, and its folder when two open tabs have the same name (the two README.md). */
export function tabNames(paths: string[]): Map<string, { name: string; dir: string | null }> {
  const count = new Map<string, number>();
  for (const p of paths) count.set(nameOf(p), (count.get(nameOf(p)) ?? 0) + 1);
  return new Map(paths.map((p) => {
    const name = nameOf(p);
    const dir = (count.get(name) ?? 0) > 1 ? nameOf(parentOf(p)) || null : null;
    return [p, { name, dir }];
  }));
}

/** The tab next to `path` after it closes: the one after it, else the one before; null when it was the last. */
export function nextAfterClose(paths: string[], path: string): string | null {
  const i = paths.indexOf(path);
  const rest = paths.filter((p) => p !== path);
  if (!rest.length) return null;
  return rest[Math.min(Math.max(i, 0), rest.length - 1)] ?? null;
}

/** Ctrl+Tab and Ctrl+Shift+Tab: the next or previous tab, round. */
export function stepTab(paths: string[], active: string | null, by: 1 | -1): string | null {
  if (!paths.length) return null;
  const i = active ? paths.indexOf(active) : -1;
  return paths[(i + by + paths.length) % paths.length] ?? null;
}

// ---- outside changes (research §5.3: the open files' versions every 3 seconds) ----

/** What a tab does about the version on disk: nothing; reload by itself (no changes here); ask Reload or Keep mine
 *  (changes here); say it is gone (no changes here); or offer Save it again (changes here). `seen` is the version on
 *  disk it already knows of (Keep mine), or its own. */
export type DiskAction = "none" | "reload" | "ask" | "gone" | "gone-ask";
export function diskAction(tab: { version: string; seen: string | null; dirty: boolean }, onDisk: string): DiskAction {
  if (onDisk === (tab.seen ?? tab.version)) return "none";
  if (onDisk === "") return tab.dirty ? "gone-ask" : "gone";
  return tab.dirty ? "ask" : "reload";
}

// ---- the status bar ----

/** "Ln 75, Col 79", and "(12 selected)" with a selection. */
export const cursorLabel = (line: number, col: number, selected: number) => `Ln ${line}, Col ${col}${selected > 0 ? ` (${selected} selected)` : ""}`;
/** The line endings, and the byte order mark when the file has one: "LF", "CRLF · UTF-8 with BOM". */
export const endingsLabel = (crlf: boolean, bom: boolean) => `${crlf ? "CRLF" : "LF"}${bom ? " · UTF-8 with BOM" : ""}`;
/** A file's size for a sentence: "48 KB", "3.4 MB". */
export function sizeLabel(bytes: number): string {
  if (bytes < 1024) return `${bytes} bytes`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** The backend's reasons a file is read-only (gizai_agents::editor). Any other reason is a folder's: an agent at work. */
export const WHY = { over5: "Over 5 MB", binary: "A binary file", over1: "Over 1 MB", ignored: "Ignored by git", notUtf8: "Not UTF-8 text", onDisk: "Read-only on disk" } as const;

/** The status bar's words for a read-only file: "Read-only: over 1 MB", "Read-only: QA Agent is working on GA-118". */
export function readOnlyStatus(why: string | null): string {
  const w = why ?? "";
  const short: Record<string, string> = { [WHY.over1]: "over 1 MB", [WHY.ignored]: "ignored by git", [WHY.notUtf8]: "not UTF-8", [WHY.onDisk]: "read-only on disk", [WHY.over5]: "over 5 MB", [WHY.binary]: "a binary file" };
  return `Read-only: ${short[w] ?? w}`.replace(/: $/, "");
}

/** The file bar's sentence for a read-only file, or null when the folder's bar says it (an agent at work). `size` in
 *  bytes; `ignoredIn` the ignored folder it is in ("vendor"), when the tree knows it. */
export function readOnlyBar(why: string | null, size: number | null, ignoredIn: string | null): string | null {
  switch (why) {
    case WHY.over1: return `Read-only: this file is over 1 MB${size ? ` (${sizeLabel(size)})` : ""}. Change it in your own editor.`;
    case WHY.notUtf8: return "Read-only: this file isn't UTF-8 text, and Gizai keeps files as they are.";
    case WHY.ignored: return ignoredIn ? `Read-only: it's in ${ignoredIn}/, which git ignores.` : "Read-only: it's in a folder git ignores.";
    case WHY.onDisk: return "Read-only: this file is read-only on disk.";
    default: return null;
  }
}

/** The top ignored folder `path` is in, from the tree's levels ("vendor" for vendor/laravel/x.php), or null. */
export function ignoredFolderOf(path: string, levels: Map<string, Level>): string | null {
  for (const dir of foldersOf(path)) {
    const level = levels.get(parentOf(dir));
    if (Array.isArray(level) && level.some((e) => e.path === dir && e.ignored)) return dir;
  }
  return null;
}

// ---- search in the folder ----

/** The hits by file, in the order git found them. */
export function hitsByFile(hits: SearchHit[]): { path: string; hits: SearchHit[] }[] {
  const out: { path: string; hits: SearchHit[] }[] = [];
  const at = new Map<string, number>();
  for (const h of hits) {
    let i = at.get(h.path);
    if (i === undefined) { i = out.length; at.set(h.path, i); out.push({ path: h.path, hits: [] }); }
    out[i]!.hits.push(h);
  }
  return out;
}

const n = (x: number) => x.toLocaleString("en-US");
const plural = (x: number, one: string, many: string) => `${n(x)} ${x === 1 ? one : many}`;

/** What the search panel says under its box once a search is done. */
export function searchSummary(r: SearchResult): string {
  if (!r.hits.length) return "No results. Ignored files (node_modules/, vendor/, .env) aren't searched.";
  if (r.truncated && r.hits.length >= 2000) return `${n(r.hits.length)} results shown: narrow the search for more.`;
  if (r.truncated) return `Search stopped after 10 seconds, at ${n(r.hits.length)} results: narrow it for the rest.`;
  return `${plural(r.hits.length, "result", "results")} in ${plural(hitsByFile(r.hits).length, "file", "files")}`;
}

/** A hit's line as shown: from a little before its match when the line is long, with the match apart to mark it. The
 *  match is the query's length from the hit's column for a plain search, or what the regular expression finds there. */
export function hitParts(hit: SearchHit, query: string, o: { regex: boolean; caseSensitive: boolean }): { before: string; match: string; after: string } {
  // The column counts a tab as one, so the line is cut on its own text and its tabs are widened afterwards.
  const text = hit.text;
  const start = Math.max(0, Math.min(hit.column - 1, text.length));
  let len = query.length;
  if (o.regex) {
    try {
      const m = new RegExp(query, o.caseSensitive ? "y" : "iy");
      m.lastIndex = start;
      len = m.exec(text)?.[0].length ?? 0;
    } catch { len = 0; }
  }
  const from = start > 40 ? start - 24 : 0;
  const shown = (s: string) => s.replace(/\t/g, "  ");
  return {
    before: (from > 0 ? "…" : "") + shown(text.slice(from, start)),
    match: shown(text.slice(start, start + len)),
    after: shown(text.slice(start + len)),
  };
}

// ---- Go to file ----

/** The files whose path holds the typed letters in order, best first: letters in the name before letters in the
 *  folders, together before apart, short paths first. `marks` are the matched positions in the path, to show in bold. */
export function matchFiles(files: string[], query: string, max = 60): { path: string; marks: number[] }[] {
  const q = query.trim().toLowerCase().replace(/\s+/g, "");
  if (!q) return files.slice(0, max).map((path) => ({ path, marks: [] }));
  const found: { path: string; marks: number[]; score: number }[] = [];
  for (const path of files) {
    const hit = fuzzy(path, q);
    if (hit) found.push({ path, ...hit });
  }
  found.sort((a, b) => a.score - b.score || a.path.length - b.path.length || a.path.localeCompare(b.path));
  return found.slice(0, max).map(({ path, marks }) => ({ path, marks }));
}

/** Where `q` (lower case) is in `path` letter by letter, and how good that is (lower is better): tried in the name
 *  first, then over the whole path. */
function fuzzy(path: string, q: string): { marks: number[]; score: number } | null {
  const lower = path.toLowerCase();
  const nameAt = path.lastIndexOf("/") + 1;
  const run = (from: number): number[] | null => {
    const marks: number[] = [];
    let i = from;
    for (const c of q) {
      const at = lower.indexOf(c, i);
      if (at < 0) return null;
      marks.push(at);
      i = at + 1;
    }
    return marks;
  };
  const inName = run(nameAt);
  const marks = inName ?? run(0);
  if (!marks) return null;
  const gaps = marks.slice(1).reduce((s, m, i) => s + (m - marks[i]! - 1), 0);
  const whole = lower.includes(q, nameAt) ? 0 : lower.includes(q) ? 50 : 100;
  return { marks, score: (inName ? 0 : 1000) + whole + gaps };
}

// ---- the folder picker and the sidebar ----

export type PickerGroup = { title: string; folders: EditorFolder[] };

/** The folder picker's groups: Projects, one "In ~/Herd" per Editor folder (the folders directly inside it), then
 *  Cards (`first`: the cards to list first, yours by hand). `query` filters them all by name and path. */
export function pickerGroups(folders: EditorFolder[], roots: string[], query: string, first: (f: EditorFolder) => boolean = () => false): PickerGroup[] {
  const q = query.trim().toLowerCase();
  const hit = (f: EditorFolder) => !q || f.name.toLowerCase().includes(q) || f.path.toLowerCase().includes(q);
  const trim = (p: string) => p.replace(/[\\/]+$/, "");
  const shown = folders.filter(hit);
  const groups: PickerGroup[] = [{ title: "Projects", folders: shown.filter((f) => f.kind === "project") }];
  const inRoots = shown.filter((f) => f.kind === "root");
  const rootList = roots.map(trim);
  for (const r of rootList) groups.push({ title: `In ${r}`, folders: inRoots.filter((f) => trim(parentOf(f.path.replace(/\\/g, "/"))) === r.replace(/\\/g, "/")) });
  const placed = new Set(groups.flatMap((g) => g.folders));
  const elsewhere = inRoots.filter((f) => !placed.has(f));
  for (const f of elsewhere) {
    const title = `In ${parentOf(f.path.replace(/\\/g, "/"))}`;
    const g = groups.find((x) => x.title === title);
    if (g) g.folders.push(f); else groups.push({ title, folders: [f] });
  }
  const cards = shown.filter((f) => f.kind === "card");
  groups.push({ title: "Cards", folders: [...cards.filter(first), ...cards.filter((f) => !first(f))] });
  return groups.filter((g) => g.folders.length > 0);
}

/** How a folder stands, in one word on its sidebar row ("4 changed"), from its git status; "" when there is nothing to
 *  say. Behind and ahead come with F3. */
export function standingWord(git: FolderGit | null): string {
  return git && git.uncommitted > 0 ? `${n(git.uncommitted)} changed` : "";
}

/** The standing's tooltip: "4 files changed, not committed". */
export function standingTip(git: FolderGit | null): string {
  return git && git.uncommitted > 0 ? `${plural(git.uncommitted, "file", "files")} changed, not committed` : "";
}
