// The Memory page (GA-68), its pure parts, so they are tested: wikilinks and the note each finds (the rules of
// gizai_core::memory, see docs/memory.md), the folder tree, tags, properties, the outline, backlinks and unlinked
// mentions, New note's templates, the [[ autocomplete and the search's highlights.
import type { MemoryNote } from "../types";
import { fold } from "./palette";

/** The shared folders: agents read them; the Team Lead and people write them. */
export const SHARED_FOLDERS = ["Clients", "Projects", "Standards", "Workflows", "Deployments", "Dependencies", "Decisions", "Lessons"] as const;
/** `Agents/<name>/`: each agent's own folder. */
export const AGENTS = "Agents";
/** The Team Lead's own folder. */
export const LEAD = "Team Lead";
/** The note the Team Lead and each agent keep in their own folder. */
export const NOTES = "Notes";

export const titleOf = (path: string) => path.slice(path.lastIndexOf("/") + 1);
export const folderOf = (path: string) => (path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "");
const lower = (s: string) => s.toLowerCase();

/** What a note's folder or title can't hold: * " \ / < > : | ? # ^ [ ]. */
export const BAD_TITLE = /[*"\\/<>:|?#^[\]]/;
/** Why a title (or a folder's name) can't be used, or null when it can. */
export function titleProblem(title: string): string | null {
  const t = title.trim();
  if (!t) return "Give it a name.";
  const bad = BAD_TITLE.exec(t);
  if (bad) return `A name can't hold ${bad[0]}: leave out * " \\ / < > : | ? # ^ [ ]`;
  if (t === "." || t === "..") return "Give it a name.";
  return null;
}

/** An agent's name as its folder's name (gizai_core::memory::folder_name): what a name can't hold becomes a dash. */
export function agentFolder(name: string): string {
  // eslint-disable-next-line no-control-regex
  const s = name.trim().replace(/[*"\\/<>:|?#^[\]\u0000-\u001f\u007f]/g, "-").trim().replace(/^\.+|\.+$/g, "").trim();
  return s || "Agent";
}

// ---- Which notes a page shows -------------------------------------------------------------------------------------

/** What the Memory page shows: everything (the Team Lead's view), the shared folders (no Team Lead yet), or one agent's
 *  own folder. */
export type MemoryScope = { kind: "all" } | { kind: "shared" } | { kind: "agent"; agentId: string; folder: string };

/** The scope of a route's `scope` ("shared", or an agent's id with its name). */
export function memoryScope(scope: string | undefined, agentName?: string | null): MemoryScope {
  if (!scope) return { kind: "all" };
  if (scope === "shared") return { kind: "shared" };
  return { kind: "agent", agentId: scope, folder: `${AGENTS}/${agentFolder(agentName ?? "")}` };
}

export function inScope(note: Pick<MemoryNote, "path" | "scope" | "ownerId">, scope: MemoryScope): boolean {
  if (scope.kind === "all") return true;
  if (scope.kind === "shared") return note.scope === "shared";
  return note.ownerId === scope.agentId || lower(note.path).startsWith(lower(`${scope.folder}/`));
}

/** The folders a scope always shows, also while empty: where a note can go. */
export function scopeRoots(scope: MemoryScope): string[] {
  if (scope.kind === "agent") return [scope.folder];
  if (scope.kind === "shared") return [...SHARED_FOLDERS];
  return [LEAD, ...SHARED_FOLDERS, AGENTS];
}

/** The Team Lead among a team's agents: the one with the lead role that answers in Chat, else the first with the lead
 *  role (as gizai_core::memory finds it). Null when there is none yet. */
export function leadOf<M extends { isLead: boolean; chatEnabled: boolean }>(agents: readonly M[]): M | null {
  return agents.find((a) => a.isLead && a.chatEnabled) ?? agents.find((a) => a.isLead) ?? null;
}

/** How many notes a sidebar entry opens: the Team Lead every note; another agent its own folder's. */
export function noteCount(notes: readonly Pick<MemoryNote, "path" | "scope" | "ownerId">[], scope: MemoryScope): number {
  return notes.filter((n) => inScope(n, scope)).length;
}

/** A folder a note may go in: a shared folder, an agent's folder or the Team Lead's, or a folder inside one. */
export function isNoteFolder(folder: string): boolean {
  const [top, second] = folder.split("/");
  if (!top) return false;
  if (lower(top) === lower(AGENTS)) return !!second;
  return lower(top) === lower(LEAD) || SHARED_FOLDERS.some((f) => lower(f) === lower(top));
}

/** A folder that can be renamed or moved: one inside a memory folder, never a memory folder itself. */
export function isOwnFolder(folder: string): boolean {
  const parts = folder.split("/");
  return isNoteFolder(folder) && parts.length > (lower(parts[0] ?? "") === lower(AGENTS) ? 2 : 1);
}

// ---- Wikilinks ----------------------------------------------------------------------------------------------------

/** A `[[wikilink]]` or `![[embed]]`: its note part, `#heading` and `|alias`, and where it is in the text (the `!`
 *  included). `target` is "" for a link to a heading in the same note (`[[#Heading]]`). */
export type WikiLink = { target: string; heading?: string; alias?: string; embed: boolean; start: number; end: number };

/** The wikilinks in a text: `[[Note]]`, `[[Folder/Note|text]]`, `[[Note#Heading]]`, `![[Note]]`. A link to a heading of
 *  the same note (`[[#Heading]]`) is left out, as Gizai's link index does, unless `local`. */
export function wikilinks(body: string, local = false): WikiLink[] {
  const out: WikiLink[] = [];
  let i = 0;
  for (;;) {
    const start = body.indexOf("[[", i);
    if (start < 0) break;
    const close = body.indexOf("]]", start + 2);
    if (close < 0) break;
    const inner = body.slice(start + 2, close);
    const end = close + 2;
    if (inner.includes("\n") || inner.includes("[[")) { i = start + 2; continue; }
    const embed = start > 0 && body[start - 1] === "!";
    const bar = inner.indexOf("|");
    const link = bar < 0 ? inner : inner.slice(0, bar);
    const alias = bar < 0 ? undefined : inner.slice(bar + 1).trim();
    const hash = link.indexOf("#");
    const notePart = hash < 0 ? link : link.slice(0, hash);
    const heading = hash < 0 ? undefined : link.slice(hash + 1).trim();
    let target = notePart.trim();
    if (target.endsWith(".md")) target = target.slice(0, -3);
    target = target.trim().replace(/^\/+|\/+$/g, "");
    if (target || (local && heading)) {
      out.push({ target, ...(heading !== undefined ? { heading } : {}), ...(alias !== undefined ? { alias } : {}), embed, start: embed ? start - 1 : start, end });
    }
    i = end;
  }
  return out;
}

/** Which note a link's note part finds, as Gizai does (and Obsidian): by title, case ignored; a path, or the end of
 *  one, picks between notes with the same title. Of several, the one in `folder` (the linking note's), else the
 *  shortest path. -1 when none does. */
export function resolve(notes: readonly Pick<MemoryNote, "path">[], target: string, folder: string): number {
  let t = lower(target.trim()).replace(/^\/+|\/+$/g, "");
  if (t.endsWith(".md")) t = t.slice(0, -3);
  if (!t) return -1;
  let cands: number[];
  if (t.includes("/")) {
    cands = notes.flatMap((n, i) => (lower(n.path) === t ? [i] : []));
    if (!cands.length) cands = notes.flatMap((n, i) => (lower(n.path).endsWith(`/${t}`) ? [i] : []));
  } else {
    cands = notes.flatMap((n, i) => (lower(titleOf(n.path)) === t ? [i] : []));
  }
  const f = lower(folder);
  cands.sort((a, b) => {
    const pa = notes[a]!.path, pb = notes[b]!.path;
    return Number(lower(folderOf(pa)) !== f) - Number(lower(folderOf(pb)) !== f) || pa.length - pb.length || (lower(pa) < lower(pb) ? -1 : lower(pa) > lower(pb) ? 1 : 0);
  });
  return cands[0] ?? -1;
}

/** The note a link finds from `from` (the linking note), or null. */
export function linkedNote<N extends Pick<MemoryNote, "path">>(notes: readonly N[], target: string, from: Pick<MemoryNote, "path"> | null): N | null {
  const i = resolve(notes, target, from ? folderOf(from.path) : "");
  return i < 0 ? null : notes[i]!;
}

/** How to link to `note` from a note in `folder`: its title when that finds it, else its path. */
export function linkTarget(notes: readonly Pick<MemoryNote, "path">[], note: Pick<MemoryNote, "path">, folder: string): string {
  const title = titleOf(note.path);
  const i = resolve(notes, title, folder);
  return i >= 0 && notes[i]!.path === note.path ? title : note.path;
}

/** Where a new note for a link that finds none goes: the link's path when it names a folder, else next to the linking
 *  note (in its folder), else in `fallback`. */
export function newNotePath(target: string, fromFolder: string, fallback = "Lessons"): string {
  const t = target.trim().replace(/^\/+|\/+$/g, "");
  if (t.includes("/")) return t;
  return `${fromFolder || fallback}/${t}`;
}

/** Task references in a text, like KADE-12 (gizai_core::memory::task_refs, as the editor's chips find them). */
export const TASK_REF = /(?<![A-Za-z0-9_-])[A-Z][A-Z0-9]{0,9}-\d+(?![A-Za-z0-9_])/g;

// ---- Properties, tags, outline ------------------------------------------------------------------------------------

/** The frontmatter block at the start of a note (`---` … `---`): where it ends (the offset after its closing line) and
 *  its lines; null when the note has none. */
export function frontmatterBlock(body: string): { end: number; lines: string[] } | null {
  const lines = body.split("\n");
  if ((lines[0] ?? "").trimEnd() !== "---") return null;
  let offset = (lines[0] ?? "").length + 1;
  for (let i = 1; i < lines.length; i++) {
    const t = lines[i]!.trimEnd();
    if (t === "---" || t === "...") return { end: Math.min(body.length, offset + lines[i]!.length + 1), lines: lines.slice(1, i) };
    offset += lines[i]!.length + 1;
  }
  return null;
}

const cleanValue = (v: string) => {
  let x = v.trim().replace(/^["']+|["']+$/g, "").trim();
  if (x.startsWith("[[") && x.endsWith("]]")) x = x.slice(2, -2);
  return (x.split("|")[0] ?? "").trim();
};

/** A note's YAML properties (gizai_core::memory::properties): keys in lower case, in their order, each a list of
 *  values: `tags: [a, b]`, a `- item` list, or one value. Quotes and the [[ ]] around a link are taken off. */
export function properties(body: string): [string, string[]][] {
  const block = frontmatterBlock(body);
  const out: [string, string[]][] = [];
  if (!block) return out;
  let current: [string, string[]] | null = null;
  for (const line of block.lines) {
    const t = line.trimEnd();
    const item = /^\s*-\s(.*)$/.exec(t)?.[1] ?? (t.trim() === "-" ? "" : null);
    if (item !== null) {
      const v = cleanValue(item);
      if (current && v) current[1].push(v);
      continue;
    }
    const colon = t.indexOf(":");
    if (colon < 0 || t.startsWith(" ")) continue;
    const key = lower(t.slice(0, colon).trim());
    const v = t.slice(colon + 1).trim();
    const vals = v.startsWith("[") && v.endsWith("]") && !v.startsWith("[[") ? v.slice(1, -1).split(",").map(cleanValue).filter(Boolean)
      : v ? [cleanValue(v)] : [];
    const existing = out.findIndex(([k]) => k === key);
    current = [key, vals];
    if (existing >= 0) out[existing] = current; else out.push(current);
  }
  return out;
}

export const property = (body: string, key: string): string[] => properties(body).find(([k]) => k === lower(key))?.[1] ?? [];

/** Properties written as a list, also with one value. */
export const LIST_PROPERTIES = ["tags", "aliases", "applies_to"];

const yamlValue = (v: string) => (/^[\s[\]{}#&*!|>'"%@`,-]|:\s|\s#|\s$/.test(v) || v === "" ? JSON.stringify(v) : v);

/** The line(s) a property is written as: `key: value`, or `key: [a, b]` for a list. */
export function propertyLine(key: string, values: string[]): string {
  const vals = values.map((v) => v.trim()).filter(Boolean);
  if (LIST_PROPERTIES.includes(lower(key)) || vals.length > 1) return `${key}: [${vals.map(yamlValue).join(", ")}]`;
  return vals.length ? `${key}: ${yamlValue(vals[0]!)}` : `${key}:`;
}

/** The note with property `key` set to `values` (null takes it out): its line rewritten where it is (with a `- item`
 *  list under it), added at the end of the frontmatter, or a new frontmatter at the top. The rest stays as it was. */
export function setProperty(body: string, key: string, values: string[] | null): string {
  const block = frontmatterBlock(body);
  const k = key.trim();
  if (!k) return body;
  if (!block) {
    if (values === null) return body;
    return `---\n${propertyLine(k, values)}\n---\n${body}`;
  }
  const lines = [...block.lines];
  const at = lines.findIndex((l) => !l.startsWith(" ") && l.includes(":") && lower(l.slice(0, l.indexOf(":")).trim()) === lower(k));
  if (at >= 0) {
    let to = at + 1;
    while (to < lines.length && (/^\s*-(\s|$)/.test(lines[to]!) || (lines[to]!.startsWith(" ") && lines[to]!.trim() !== ""))) to++;
    lines.splice(at, to - at, ...(values === null ? [] : [propertyLine(lines[at]!.slice(0, lines[at]!.indexOf(":")).trim(), values)]));
  } else if (values !== null) {
    lines.push(propertyLine(k, values));
  }
  return `---\n${lines.join("\n")}${lines.length ? "\n" : ""}---\n${body.slice(block.end)}`;
}

/** A note's tags (gizai_core::memory::tags): its `tags` property and the #tags in its text, lower case, without #. */
export function tags(body: string): string[] {
  const out = property(body, "tags").map((t) => lower(t.replace(/^#+/, ""))).filter(Boolean);
  for (let i = body.indexOf("#"); i >= 0; i = body.indexOf("#", i + 1)) {
    const prev = body[i - 1];
    if (i > 0 && !/\s/.test(prev ?? "") && prev !== "(") continue;
    let j = i + 1;
    while (j < body.length && /[A-Za-z0-9\-_/]/.test(body[j]!)) j++;
    const t = lower(body.slice(i + 1, j));
    if (t && !/^\d+$/.test(t) && !out.includes(t)) out.push(t);
  }
  return [...new Set(out)];
}

/** Every tag of the notes with how many notes have it: most used first, then by name. */
export function tagCounts(notes: readonly Pick<MemoryNote, "bodyMd">[]): { tag: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const n of notes) for (const t of tags(n.bodyMd)) counts.set(t, (counts.get(t) ?? 0) + 1);
  return [...counts].map(([tag, count]) => ({ tag, count })).sort((a, b) => b.count - a.count || (a.tag < b.tag ? -1 : 1));
}

/** Whether a note has a tag: the tag itself or one under it (`rust` matches `rust/style`), as search's tag: does. */
export const hasTag = (body: string, tag: string) => tags(body).some((t) => t === lower(tag) || t.startsWith(`${lower(tag)}/`));

/** A heading of a note: its level, text, and where its line starts. */
export type Heading = { level: number; text: string; pos: number; line: number };

/** A note's headings, in order, without those in its frontmatter or in code blocks. */
export function outline(body: string): Heading[] {
  const out: Heading[] = [];
  const skip = frontmatterBlock(body)?.end ?? 0;
  let pos = 0;
  let code = false;
  body.split("\n").forEach((l, line) => {
    const here = pos;
    pos += l.length + 1;
    if (here < skip) return;
    if (/^\s*(```|~~~)/.test(l)) { code = !code; return; }
    if (code) return;
    const m = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(l);
    if (m && m[2]) out.push({ level: m[1]!.length, text: m[2], pos: here, line: line + 1 });
  });
  return out;
}

/** The text of a heading's section (the heading and what follows up to the next heading of its level or higher), or
 *  null when the note has no such heading. Case ignored. */
export function section(body: string, heading: string): string | null {
  const hs = outline(body);
  const at = hs.findIndex((h) => lower(h.text) === lower(heading.trim()));
  if (at < 0) return null;
  const h = hs[at]!;
  const next = hs.slice(at + 1).find((x) => x.level <= h.level);
  return body.slice(h.pos, next ? next.pos : body.length).trimEnd();
}

/** A note's text without its frontmatter (what the reading view and a preview show). */
export const withoutFrontmatter = (body: string) => body.slice(frontmatterBlock(body)?.end ?? 0).replace(/^\s*\n/, "");

// ---- Backlinks and unlinked mentions ------------------------------------------------------------------------------

/** A line of another note that names this one: the note, the line's text, and where the name is in the note. */
export type Mention = { note: MemoryNote; line: string; start: number; end: number };

const lineAround = (body: string, start: number, end: number) => {
  const from = body.lastIndexOf("\n", start - 1) + 1;
  const to = body.indexOf("\n", end);
  return body.slice(from, to < 0 ? body.length : to).trim();
};

/** Linked mentions: the links in other notes that find `target`, each with its line, by note path. */
export function backlinks(notes: readonly MemoryNote[], target: Pick<MemoryNote, "id">): Mention[] {
  const out: Mention[] = [];
  for (const n of notes) {
    if (n.id === target.id) continue;
    for (const l of wikilinks(n.bodyMd)) {
      const i = resolve(notes, l.target, folderOf(n.path));
      if (i >= 0 && notes[i]!.id === target.id) out.push({ note: n, line: lineAround(n.bodyMd, l.start, l.end), start: l.start, end: l.end });
    }
  }
  return out;
}

/** Where a text has parts that can't take a link: its frontmatter, code blocks and inline code, and links. */
function blocked(body: string): [number, number][] {
  const out: [number, number][] = [];
  const fm = frontmatterBlock(body);
  if (fm) out.push([0, fm.end]);
  for (const re of [/(^|\n)(```|~~~)[\s\S]*?(\n(```|~~~)|$)/g, /`[^`\n]*`/g, /!?\[\[[^\]\n]*\]\]/g, /\[[^\]\n]*\]\([^)\n]*\)/g, /<[^>\n]+>/g]) {
    for (let m = re.exec(body); m; m = re.exec(body)) out.push([m.index, m.index + m[0].length]);
  }
  return out;
}

const WORD = /[\p{L}\p{N}_]/u;

/** Unlinked mentions: other notes that name `target`'s title (case ignored, a whole word or words) where it isn't a
 *  link, nor in their properties or code. One entry per place. */
export function unlinkedMentions(notes: readonly MemoryNote[], target: Pick<MemoryNote, "id" | "path">): Mention[] {
  const title = titleOf(target.path).trim();
  if (title.length < 2) return [];
  const want = lower(title);
  const out: Mention[] = [];
  for (const n of notes) {
    if (n.id === target.id) continue;
    const body = n.bodyMd;
    const low = lower(body);
    if (!low.includes(want)) continue;
    const skip = blocked(body);
    for (let i = low.indexOf(want); i >= 0; i = low.indexOf(want, i + 1)) {
      const end = i + want.length;
      if ((i > 0 && WORD.test(body[i - 1]!)) || (end < body.length && WORD.test(body[end]!))) continue;
      if (skip.some(([a, b]) => i < b && end > a)) continue;
      out.push({ note: n, line: lineAround(body, i, end), start: i, end });
    }
  }
  return out;
}

/** `body` with the unlinked mention at `start`–`end` made a link to `target`: `[[Title]]` when it reads as the title,
 *  else `[[Title|what it says]]` (the path when the title alone finds another note from `folder`). */
export function linkMention(body: string, start: number, end: number, notes: readonly Pick<MemoryNote, "path">[], target: Pick<MemoryNote, "path">, folder: string): string {
  const said = body.slice(start, end);
  const to = linkTarget(notes, target, folder);
  const link = said === to ? `[[${to}]]` : `[[${to}|${said}]]`;
  return body.slice(0, start) + link + body.slice(end);
}

/** Outgoing links: each link in a text once, with the note it finds (null: none yet, a link to make it). */
export function outgoing<N extends MemoryNote>(body: string, notes: readonly N[], from: Pick<MemoryNote, "path"> | null): { link: WikiLink; note: N | null }[] {
  const seen = new Set<string>();
  const out: { link: WikiLink; note: N | null }[] = [];
  for (const l of wikilinks(body)) {
    const note = linkedNote(notes, l.target, from);
    const key = note ? `id:${note.id}` : `new:${lower(l.target)}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ link: l, note });
  }
  return out;
}

// ---- The tree -----------------------------------------------------------------------------------------------------

/** A folder in the file explorer: its path, name, folders and notes (by name), and how many notes are in it in all. */
export type TreeFolder = { path: string; name: string; folders: TreeFolder[]; notes: MemoryNote[]; count: number };

const byName = (a: string, b: string) => (lower(a) < lower(b) ? -1 : lower(a) > lower(b) ? 1 : a < b ? -1 : a > b ? 1 : 0);
/** At the top: the Team Lead's folder first and Agents last, the shared folders between them by name. */
const topRank = (name: string) => (name === LEAD ? 0 : name === AGENTS ? 2 : 1);

/** The folder tree of `notes`, with `folders` (memory's own and empty folders made in the explorer) also when they
 *  hold no note. Folders by name before notes by title. */
export function buildTree(notes: readonly MemoryNote[], folders: readonly string[] = []): TreeFolder {
  const root: TreeFolder = { path: "", name: "", folders: [], notes: [], count: 0 };
  const folderAt = (path: string): TreeFolder => {
    let node = root;
    if (!path) return node;
    let at = "";
    for (const part of path.split("/")) {
      at = at ? `${at}/${part}` : part;
      let next = node.folders.find((f) => lower(f.name) === lower(part));
      if (!next) { next = { path: at, name: part, folders: [], notes: [], count: 0 }; node.folders.push(next); }
      node = next;
    }
    return node;
  };
  for (const f of folders) if (f.trim()) folderAt(f.trim().replace(/^\/+|\/+$/g, ""));
  for (const n of notes) folderAt(folderOf(n.path)).notes.push(n);
  const finish = (f: TreeFolder, top: boolean): number => {
    f.folders.sort((a, b) => (top ? topRank(a.name) - topRank(b.name) : 0) || byName(a.name, b.name));
    f.notes.sort((a, b) => byName(titleOf(a.path), titleOf(b.path)));
    f.count = f.notes.length + f.folders.reduce((s, x) => s + finish(x, false), 0);
    return f.count;
  };
  finish(root, true);
  return root;
}

/** The folder at `path` in a tree, or null. */
export function findFolder(tree: TreeFolder, path: string): TreeFolder | null {
  if (!path) return tree;
  let node: TreeFolder | undefined = tree;
  for (const part of path.split("/")) {
    node = node?.folders.find((f) => lower(f.name) === lower(part));
    if (!node) return null;
  }
  return node ?? null;
}

/** Every note in a folder and the folders inside it. */
export const notesIn = (f: TreeFolder): MemoryNote[] => [...f.notes, ...f.folders.flatMap(notesIn)];

/** The folders from the top down to `path`: the ones to open to show a note there. */
export function foldersTo(path: string): string[] {
  const parts = path.split("/").filter(Boolean);
  return parts.map((_, i) => parts.slice(0, i + 1).join("/"));
}

/** Where a note or folder lands when moved into `folder`: the path `memory_move` gets. Null when that changes nothing
 *  or can't be: a folder into itself or a folder inside it. */
export function moveTarget(item: { kind: "note" | "folder"; path: string }, folder: string): string | null {
  const name = titleOf(item.path);
  const to = folder ? `${folder}/${name}` : name;
  if (lower(to) === lower(item.path)) return null;
  if (item.kind === "folder" && (lower(folder) === lower(item.path) || lower(folder).startsWith(lower(`${item.path}/`)))) return null;
  return to;
}

/** `path` with its start `from` (a folder) changed to `to`: where a note goes when its folder is renamed or moved. */
export function rebase(path: string, from: string, to: string): string {
  return lower(path).startsWith(lower(`${from}/`)) ? `${to}${path.slice(from.length)}` : path;
}

// ---- New note: a template per type ---------------------------------------------------------------------------------

/** A note's `type`, as New note asks for it, and the folder a type goes in. */
export const NOTE_TYPES = ["note", "decision", "lesson", "standard", "workflow", "client", "project", "deployment", "dependency"] as const;
export type NoteType = (typeof NOTE_TYPES)[number];
export const TYPE_NAME: Record<NoteType, string> = {
  note: "Note", decision: "Decision", lesson: "Lesson", standard: "Standard", workflow: "Workflow", client: "Client", project: "Project",
  deployment: "Deployment", dependency: "Dependency",
};
export const TYPE_FOLDER: Record<NoteType, string | null> = {
  note: null, decision: "Decisions", lesson: "Lessons", standard: "Standards", workflow: "Workflows", client: "Clients", project: "Projects",
  deployment: "Deployments", dependency: "Dependencies",
};
/** What each type is for, shown under the choice. */
export const TYPE_HINT: Record<NoteType, string> = {
  note: "Anything else worth keeping.",
  decision: "What was decided, why, and what else was considered.",
  lesson: "What went wrong or well, and what to do next time.",
  standard: "A rule for how work is done here, for the roles it applies to.",
  workflow: "The steps of a recurring job.",
  client: "Who a client is, their contacts and preferences.",
  project: "Where a project's things are and what to know about it.",
  deployment: "Where something runs and how to deploy and roll back.",
  dependency: "A library or service, its version and its gotchas.",
};

const HEADINGS: Record<NoteType, string[]> = {
  note: [],
  decision: ["Decision", "Why", "Alternatives considered"],
  lesson: ["What happened", "What we learned", "Next time"],
  standard: ["The rule", "Why", "Examples"],
  workflow: ["When", "Steps", "Done when"],
  client: ["Who they are", "Contacts", "Preferences", "Projects"],
  project: ["What it is", "Where things are", "Decisions", "Gotchas"],
  deployment: ["Where it runs", "How to deploy", "How to roll back", "Checks after a deploy"],
  dependency: ["What it is", "Version and why", "Gotchas"],
};

/** The properties a type starts with, in order (docs/memory.md → Properties). */
function templateProps(type: NoteType, title: string, today: string): [string, string[]][] {
  const props: [string, string[]][] = [["type", [type]], ["tags", []]];
  if (type === "client") props.push(["client", [title]]);
  if (type === "project") props.push(["project", [title]], ["client", []]);
  if (type === "deployment" || type === "dependency" || type === "decision") props.push(["project", []]);
  if (type === "standard" || type === "workflow") props.push(["applies_to", ["all"]]);
  if (type === "lesson") props.push(["applies_to", []]);
  props.push(["updated", [today]]);
  if (type === "decision" || type === "lesson") props.push(["source", []]);
  return props;
}

/** A new note of `type`: its properties and its headings, short. `today`: YYYY-MM-DD. */
export function noteTemplate(type: NoteType, title: string, today: string): string {
  const props = templateProps(type, title.trim(), today).map(([k, v]) => propertyLine(k, v)).join("\n");
  const heads = HEADINGS[type].map((h) => `## ${h}\n`).join("\n");
  return `---\n${props}\n---\n# ${title.trim()}\n\n${heads}`;
}

/** Today as YYYY-MM-DD, in local time. */
export function today(d = new Date()): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

// ---- The [[ autocomplete -------------------------------------------------------------------------------------------

/** The `[[…` right before the cursor: what is typed after `[[` (`part`: a note's name, a `#heading` of it, or an
 *  `|alias`), the note part, and where the typed text starts (`from`, plus `offset`). Null outside one. */
export type WikiTrigger = { from: number; query: string; part: "note" | "heading" | "alias"; note: string; embed: boolean };

export function findWikiTrigger(before: string, offset = 0): WikiTrigger | null {
  const at = before.lastIndexOf("[[");
  if (at < 0) return null;
  const inner = before.slice(at + 2);
  if (/[\]\n[]/.test(inner) || inner.length > 120) return null;
  const embed = before[at - 1] === "!";
  const bar = inner.indexOf("|");
  if (bar >= 0) {
    const notePart = inner.slice(0, bar);
    const hash = notePart.indexOf("#");
    return { from: offset + at + 2 + bar + 1, query: inner.slice(bar + 1), part: "alias", note: (hash < 0 ? notePart : notePart.slice(0, hash)).trim(), embed };
  }
  const hash = inner.indexOf("#");
  if (hash >= 0) return { from: offset + at + 2 + hash + 1, query: inner.slice(hash + 1), part: "heading", note: inner.slice(0, hash).trim(), embed };
  return { from: offset + at + 2, query: inner, part: "note", note: "", embed };
}

/** A row of the [[ picker: a note (`insert` is how the link names it), a heading or an alias of one. */
export type WikiRow =
  | { type: "note"; note: MemoryNote; insert: string }
  | { type: "heading"; text: string; level: number; insert: string }
  | { type: "alias"; text: string; insert: string };

/** At most this many notes in the picker. */
export const WIKI_ROWS = 30;

/** The rows for what was typed after [[: notes whose title or path has every word (a title that starts with the first
 *  word first, then a title that has them, then a path; shorter paths first); after #, the headings of that note (of
 *  `current` with no note part); after |, its `aliases`. `current`: the note being written, never offered for itself. */
export function wikiRows(t: WikiTrigger, notes: readonly MemoryNote[], current: MemoryNote | null): WikiRow[] {
  const words = fold(t.query).split(/\s+/).filter(Boolean);
  const has = (s: string) => words.every((w) => fold(s).includes(w));
  if (t.part === "note") {
    const folder = current ? folderOf(current.path) : "";
    const scored = notes.flatMap((n, i) => {
      if (current && n.id === current.id) return [];
      const title = titleOf(n.path);
      if (!has(n.path)) return [];
      const ft = fold(title);
      const score = words.length === 0 ? 0 : ft.startsWith(words[0]!) ? 0 : has(title) ? 1 : 2;
      return [{ n, score, i }];
    });
    scored.sort((a, b) => a.score - b.score || a.n.path.length - b.n.path.length || a.i - b.i);
    return scored.slice(0, WIKI_ROWS).map(({ n }) => ({ type: "note" as const, note: n, insert: linkTarget(notes, n, folder) }));
  }
  const target = t.note ? linkedNote(notes, t.note, current) : current;
  if (!target) return [];
  if (t.part === "heading") {
    return outline(target.bodyMd).filter((h) => has(h.text)).slice(0, WIKI_ROWS)
      .map((h) => ({ type: "heading" as const, text: h.text, level: h.level, insert: h.text }));
  }
  return property(target.bodyMd, "aliases").filter(has).map((a) => ({ type: "alias" as const, text: a, insert: a }));
}

// ---- Search ---------------------------------------------------------------------------------------------------------

/** The words and "phrases" of a search (its path: and tag: left out), lower case: what a snippet marks. */
export function searchWords(query: string): string[] {
  const out: string[] = [];
  const re = /(?:(path|tag):)?(?:"([^"]*)"?|(\S+))/gi;
  for (let m = re.exec(query); m; m = re.exec(query)) {
    if (m[1]) continue;
    const w = lower((m[2] ?? m[3] ?? "").trim());
    if (w) out.push(w);
  }
  return out;
}

/** A text cut into the parts a search marks (`hit`) and the rest, case ignored. */
export function highlight(text: string, words: readonly string[]): { text: string; hit: boolean }[] {
  const ws = words.filter(Boolean).map(lower);
  if (!ws.length || !text) return text ? [{ text, hit: false }] : [];
  const low = lower(text);
  const marks: [number, number][] = [];
  for (const w of ws) for (let i = low.indexOf(w); i >= 0; i = low.indexOf(w, i + w.length)) marks.push([i, i + w.length]);
  marks.sort((a, b) => a[0] - b[0]);
  const out: { text: string; hit: boolean }[] = [];
  let at = 0;
  for (const [a, b] of marks) {
    if (b <= at) continue;
    const from = Math.max(a, at);
    if (from > at) out.push({ text: text.slice(at, from), hit: false });
    out.push({ text: text.slice(from, b), hit: true });
    at = b;
  }
  if (at < text.length) out.push({ text: text.slice(at), hit: false });
  return out;
}

/** A search limited to a scope: an agent's page searches only its folder. */
export function scopedQuery(query: string, scope: MemoryScope): string {
  if (scope.kind !== "agent") return query;
  return `path:"${scope.folder}/" ${query}`;
}
