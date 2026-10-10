// The memory graph (GA-69), its pure parts, so they are tested. buildGraph makes the dots and lines from the notes'
// text: [[wikilinks]] and embeds between notes (a link that finds no note yet ends at a dim dot of its own), card
// references (KADE-12), @mentions, the @ picker's gizai: links and the client: and project: properties, so the cards,
// projects, clients, agents and people that notes name are dots of their own kinds, and tags when they are shown.
// graphView narrows it to what the settings show: the local graph around a note (depth, incoming, outgoing) and the
// filters (the search query with path: and tag:, tags, orphans, existing notes only, the kinds). Groups colour a note
// by the first query it matches, from a calm palette without teal or magenta. Then the settings as kept, with their
// defaults, and the sums the canvas needs: a dot's size, a label's fade, fitting the graph in view, the order it grows
// in, and which dot an arrow key goes to.
import type { MemoryNote } from "../types";
import { ITEM_LINK_SOURCE, parseItemUrl } from "./itemLinks";
import { AGENTS, folderOf, LEAD, newNotePath, property, TASK_REF, tags as noteTags, titleOf, wikilinks } from "./memory";

const lower = (s: string) => s.toLowerCase();

// ---- The graph -------------------------------------------------------------------------------------------------------

/** A dot's kind: a note, a link's target that is no note yet, a tag, or a card, project, client, agent or person that
 *  a note names. */
export const NODE_KINDS = ["note", "missing", "tag", "card", "project", "client", "agent", "person"] as const;
export type NodeKind = (typeof NODE_KINDS)[number];
/** What notes name: each a kind of dot of its own, which can be hidden. */
export const EXTRA_KINDS = ["card", "project", "client", "agent", "person"] as const;
export type ExtraKind = (typeof EXTRA_KINDS)[number];
export const KIND_NAME: Record<NodeKind, string> = {
  note: "Note", missing: "No note yet", tag: "Tag", card: "Card", project: "Project", client: "Client", agent: "Agent", person: "Person",
};
export const EXTRA_NAME: Record<ExtraKind, string> = { card: "Cards", project: "Projects", client: "Clients", agent: "Agents", person: "People" };

export type GraphNode = {
  /** `note:<id>`, `missing:<name>`, `tag:<tag>`, `card:<IDENTIFIER>`, `project:<id>`, `client:<id>`, `agent:<id>` or `person:<id>`. */
  id: string;
  kind: NodeKind;
  /** Its name: a note's title, a card's identifier, a tag with its #. */
  label: string;
  /** More about it: a note's folder, a card's title. */
  detail?: string;
  /** What opens it: a note's id, a card's identifier, a project's key, a client's, agent's or person's id, a tag. */
  ref: string;
  /** A note's path; for a note that isn't there yet, the path it gets when it is made (next to the first note linking to it). */
  path?: string;
  /** How many lines it has (a note's tags not counted): its size. */
  links: number;
  /** When it came: a note when it was made, anything else with the first note that names it. */
  born: number;
};
/** A line: a link or embed between notes, a card reference, a mention, a client: or project: property, a tag. */
export type LinkKind = "link" | "embed" | "card" | "mention" | "property" | "tag";
export type GraphLink = { source: string; target: string; kind: LinkKind };
export type Graph = { nodes: GraphNode[]; links: GraphLink[] };

/** What the graph is made from: every note with its text, and what a note can name. */
export type GraphSources = {
  notes: readonly MemoryNote[];
  cards?: readonly { identifier: string; title: string }[];
  projects?: readonly { id: string; key: string; name: string }[];
  clients?: readonly { id: string; name: string }[];
  agents?: readonly { actorId: string; name: string; handle: string }[];
  people?: readonly { id: string; name: string; handle: string }[];
};

/** When a note was made: Gizai's ids are UUIDv7, which start with their time in ms. Else when it last changed. */
export function bornAt(note: Pick<MemoryNote, "id" | "updatedAt">): number {
  const hex = note.id.replace(/-/g, "");
  return /^[0-9a-f]{32}$/i.test(hex) && hex[12] === "7" ? parseInt(hex.slice(0, 12), 16) : note.updatedAt;
}

/** The @handles in a text, lower case, as gizai_core::memory::mentions finds them: not an email address's @. */
export function mentionHandles(body: string): string[] {
  const out: string[] = [];
  for (let i = body.indexOf("@"); i >= 0; i = body.indexOf("@", i + 1)) {
    if (i > 0 && /[A-Za-z0-9_.\-/]/.test(body[i - 1]!)) continue;
    let j = i + 1;
    while (j < body.length && /[A-Za-z0-9\-_.]/.test(body[j]!)) j++;
    const h = lower(body.slice(i + 1, j).replace(/[.\-_]+$/, ""));
    if (h && !out.includes(h)) out.push(h);
  }
  return out;
}

/** `resolve` from ./memory for many links at once: the same note for every link (by title, case ignored; a path or
 *  the end of one; of several the one in the linking note's folder, else the shortest path), found through an index. */
export function resolver(notes: readonly Pick<MemoryNote, "path">[]): (target: string, folder: string) => number {
  const byTitle = new Map<string, number[]>();
  const byPath = new Map<string, number[]>();
  notes.forEach((n, i) => {
    const t = lower(titleOf(n.path)), p = lower(n.path);
    byTitle.set(t, [...(byTitle.get(t) ?? []), i]);
    byPath.set(p, [...(byPath.get(p) ?? []), i]);
  });
  return (target, folder) => {
    let t = lower(target.trim()).replace(/^\/+|\/+$/g, "");
    if (t.endsWith(".md")) t = t.slice(0, -3);
    if (!t) return -1;
    let cands: number[];
    if (t.includes("/")) {
      cands = byPath.get(t) ?? [];
      if (!cands.length) cands = notes.flatMap((n, i) => (lower(n.path).endsWith(`/${t}`) ? [i] : []));
    } else {
      cands = byTitle.get(t) ?? [];
    }
    if (cands.length < 2) return cands[0] ?? -1;
    const f = lower(folder);
    return [...cands].sort((a, b) => {
      const pa = notes[a]!.path, pb = notes[b]!.path;
      return Number(lower(folderOf(pa)) !== f) - Number(lower(folderOf(pb)) !== f) || pa.length - pb.length || (lower(pa) < lower(pb) ? -1 : lower(pa) > lower(pb) ? 1 : 0);
    })[0]!;
  };
}

/** Every dot and line of the notes: what each note links to and names. One line per pair of dots (the first kind
 *  found), none from a note to itself. A name that finds nothing (a card, project, client or handle that isn't there)
 *  makes no dot; a wikilink that finds no note makes a dim one, which the note gets when it is made. */
export function buildGraph(src: GraphSources): Graph {
  const { notes } = src;
  const nodes = new Map<string, GraphNode>();
  const links = new Map<string, GraphLink>();
  const add = (n: Omit<GraphNode, "links">): string => {
    const had = nodes.get(n.id);
    if (!had) nodes.set(n.id, { ...n, links: 0 });
    else if (n.born < had.born) had.born = n.born;
    return n.id;
  };
  const line = (source: string, target: string, kind: LinkKind) => {
    if (source !== target && !links.has(`${source}\n${target}`)) links.set(`${source}\n${target}`, { source, target, kind });
  };
  const find = resolver(notes);
  const ids = new Map(notes.map((n) => [n.id, n]));
  const cards = new Map((src.cards ?? []).map((c) => [c.identifier.toUpperCase(), c]));
  const projects = src.projects ?? [];
  const clients = src.clients ?? [];
  // By handle (an @mention, an agent's before a person's), and by kind and id (a gizai: link).
  const actors = new Map<string, { kind: "agent" | "person"; id: string; name: string }>();
  const actorIds = new Map<string, { kind: "agent" | "person"; id: string; name: string }>();
  for (const p of src.people ?? []) {
    const a = { kind: "person" as const, id: p.id, name: p.name };
    if (p.handle) actors.set(lower(p.handle), a);
    actorIds.set(`person:${p.id}`, a);
  }
  for (const m of src.agents ?? []) {
    const a = { kind: "agent" as const, id: m.actorId, name: m.name };
    if (m.handle) actors.set(lower(m.handle), a);
    actorIds.set(`agent:${m.actorId}`, a);
  }
  const noteDot = (n: MemoryNote) => `note:${n.id}`;
  const card = (identifier: string, born: number) => {
    const c = cards.get(identifier.toUpperCase());
    return c ? add({ id: `card:${c.identifier.toUpperCase()}`, kind: "card", label: c.identifier, detail: c.title, ref: c.identifier, born }) : null;
  };
  const project = (p: { id: string; key: string; name: string }, born: number) =>
    add({ id: `project:${p.id}`, kind: "project", label: p.name, detail: p.key, ref: p.key, born });
  const client = (c: { id: string; name: string }, born: number) => add({ id: `client:${c.id}`, kind: "client", label: c.name, ref: c.id, born });
  const actor = (a: { kind: "agent" | "person"; id: string; name: string }, born: number) => add({ id: `${a.kind}:${a.id}`, kind: a.kind, label: a.name, ref: a.id, born });

  for (const n of notes) add({ id: noteDot(n), kind: "note", label: titleOf(n.path), detail: folderOf(n.path), ref: n.id, path: n.path, born: bornAt(n) });
  for (const n of notes) {
    const from = noteDot(n);
    const born = bornAt(n);
    const body = n.bodyMd;
    for (const l of wikilinks(body)) {
      const i = find(l.target, folderOf(n.path));
      const kind = l.embed ? "embed" : "link";
      if (i >= 0) { line(from, noteDot(notes[i]!), kind); continue; }
      const path = newNotePath(l.target, folderOf(n.path));
      line(from, add({ id: `missing:${lower(l.target)}`, kind: "missing", label: l.target, detail: folderOf(path), ref: l.target, path, born }), kind);
    }
    for (const r of body.match(TASK_REF) ?? []) { const c = card(r, born); if (c) line(from, c, "card"); }
    for (const h of mentionHandles(body)) { const a = actors.get(h); if (a) line(from, actor(a, born), "mention"); }
    for (const m of body.matchAll(new RegExp(ITEM_LINK_SOURCE, "g"))) {
      const it = parseItemUrl(`gizai:${m[2]}/${m[3]}`);
      if (!it) continue;
      if (it.kind === "task") { const c = card(it.key, born); if (c) line(from, c, "card"); continue; }
      if (it.kind === "doc") { const to = ids.get(it.key); if (to) line(from, noteDot(to), "link"); continue; }
      if (it.kind === "project") { const p = projects.find((x) => lower(x.key) === lower(it.key) || x.id === it.key); if (p) line(from, project(p, born), "mention"); continue; }
      if (it.kind === "client") { const c = clients.find((x) => x.id === it.key); if (c) line(from, client(c, born), "mention"); continue; }
      const a = actorIds.get(`${it.kind}:${it.key}`);
      if (a) line(from, actor(a, born), "mention");
    }
    for (const v of property(body, "project")) {
      const p = projects.find((x) => lower(x.key) === lower(v)) ?? projects.find((x) => lower(x.name) === lower(v));
      if (p) line(from, project(p, born), "property");
    }
    for (const v of property(body, "client")) {
      const c = clients.find((x) => lower(x.name) === lower(v)) ?? clients.find((x) => x.id === v);
      if (c) line(from, client(c, born), "property");
    }
    for (const t of noteTags(body)) line(from, add({ id: `tag:${t}`, kind: "tag", label: `#${t}`, ref: t, born }), "tag");
  }
  // A note's size is its links; a tag's is its notes.
  for (const l of links.values()) {
    if (l.kind !== "tag") nodes.get(l.source)!.links++;
    nodes.get(l.target)!.links++;
  }
  return { nodes: [...nodes.values()], links: [...links.values()] };
}

// ---- The search query (Filters and Groups) ----------------------------------------------------------------------------

/** A query as Memory's search reads it (gizai_core::memory::parse_query): words and "phrases", path: and tag:. */
export type GraphQuery = { words: string[]; paths: string[]; tags: string[] };

export function parseGraphQuery(q: string): GraphQuery {
  const out: GraphQuery = { words: [], paths: [], tags: [] };
  let rest = q.trim();
  while (rest) {
    const l = lower(rest);
    const [key, after] = l.startsWith("path:") ? ["path", rest.slice(5)] : l.startsWith("tag:") ? ["tag", rest.slice(4)] : ["", rest];
    let value: string, next: string;
    if (after.startsWith('"')) {
      const e = after.indexOf('"', 1);
      [value, next] = e < 0 ? [after.slice(1), ""] : [after.slice(1, e), after.slice(e + 1)];
    } else {
      const e = after.search(/\s/);
      [value, next] = e < 0 ? [after, ""] : [after.slice(0, e), after.slice(e)];
    }
    const v = lower(value.trim());
    if (v) {
      // A `/` at the end keeps to that folder: `path:Agents/QA/` is not also `Agents/QA 2/`.
      if (key === "path") { const p = v.replace(/^\/+|\/+$/g, ""); out.paths.push(v.endsWith("/") && p ? `${p}/` : p); }
      else if (key === "tag") out.tags.push(v.replace(/^#+/, ""));
      else out.words.push(v);
    }
    rest = next.trimStart();
  }
  return out;
}

export const emptyQuery = (q: GraphQuery) => !q.words.length && !q.paths.length && !q.tags.length;

/** Whether a note matches a query, as Memory's search does: its path starts with every path:, it has every tag: (or
 *  one under it), and every word is in its path or text, case ignored. An empty query matches every note. */
export function matchesQuery(note: Pick<MemoryNote, "path" | "bodyMd">, q: GraphQuery): boolean {
  const path = lower(note.path);
  if (!q.paths.every((p) => path.startsWith(p))) return false;
  if (q.tags.length) {
    const have = noteTags(note.bodyMd);
    if (!q.tags.every((t) => have.some((h) => h === t || h.startsWith(`${t}/`)))) return false;
  }
  if (!q.words.length) return true;
  const body = lower(note.bodyMd);
  return q.words.every((w) => path.includes(w) || body.includes(w));
}

// ---- Colours ---------------------------------------------------------------------------------------------------------

/** The groups' colours: calm, for a dark canvas first, and never teal or magenta (Gizai keeps those for an agent at
 *  work and "needs you"); the blue accent is for the dot you point at or have open. Each with its light-theme shade. */
export const PALETTE = [
  { key: "sand", name: "Sand", dark: "#cfb47e", light: "#8a6a2c" },
  { key: "sage", name: "Sage", dark: "#9ab585", light: "#4d6e3a" },
  { key: "lavender", name: "Lavender", dark: "#a99cdc", light: "#5c4cae" },
  { key: "clay", name: "Clay", dark: "#d39679", light: "#9a5034" },
  { key: "slate", name: "Slate", dark: "#93a8c6", light: "#475c7c" },
  { key: "olive", name: "Olive", dark: "#bdb26b", light: "#6b6424" },
  { key: "brick", name: "Brick", dark: "#c98478", light: "#93453a" },
  { key: "moss", name: "Moss", dark: "#83a986", light: "#3f6b45" },
  { key: "stone", name: "Stone", dark: "#b8ad9e", light: "#6b604f" },
  { key: "iris", name: "Iris", dark: "#8f98d6", light: "#434ea6" },
] as const;
export type PaletteKey = (typeof PALETTE)[number]["key"];
export const paletteColor = (key: string, theme: "dark" | "light" = "dark"): string => (PALETTE.find((p) => p.key === key) ?? PALETTE[0])[theme];

/** Each extra kind's own look: a shape and a palette colour (notes are grey round dots, or their group's colour). */
export const KIND_LOOK: Record<Exclude<NodeKind, "note" | "missing">, { shape: "circle" | "square" | "diamond" | "hexagon" | "triangle" | "ring"; color: PaletteKey }> = {
  tag: { shape: "circle", color: "sage" },
  card: { shape: "square", color: "sand" },
  project: { shape: "diamond", color: "lavender" },
  client: { shape: "hexagon", color: "clay" },
  agent: { shape: "triangle", color: "slate" },
  person: { shape: "ring", color: "stone" },
};

/** A hex colour's hue in degrees (0–360), and its saturation (0–1), for checking the palette. */
export function hueOf(hex: string): { hue: number; sat: number } {
  const n = parseInt(hex.replace("#", ""), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((x) => x / 255) as [number, number, number];
  const max = Math.max(r, g, b), min = Math.min(r, g, b), d = max - min;
  const l = (max + min) / 2;
  const sat = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
  if (d === 0) return { hue: 0, sat };
  const h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return { hue: (h * 60 + 360) % 360, sat };
}

// ---- Groups ----------------------------------------------------------------------------------------------------------

/** A group: the notes its query matches get its colour. */
export type GraphGroup = { query: string; color: PaletteKey };

/** The groups to start with: one per top folder that has notes (the Team Lead's first, the shared folders by name,
 *  Agents last), each its own colour from the palette. */
export function defaultGroups(notes: readonly Pick<MemoryNote, "path">[]): GraphGroup[] {
  const tops = new Map<string, string>();
  for (const n of notes) {
    const top = n.path.includes("/") ? n.path.slice(0, n.path.indexOf("/")).trim() : "";
    if (top && !tops.has(lower(top))) tops.set(lower(top), top);
  }
  const rank = (t: string) => (lower(t) === lower(LEAD) ? 0 : lower(t) === lower(AGENTS) ? 2 : 1);
  return [...tops.values()].sort((a, b) => rank(a) - rank(b) || (lower(a) < lower(b) ? -1 : lower(a) > lower(b) ? 1 : 0))
    .map((t, i) => ({ query: /\s/.test(t) ? `path:"${t}/"` : `path:${t}/`, color: PALETTE[i % PALETTE.length]!.key }));
}

/** The colour key of the first group whose query matches the note, or null (it stays grey). */
export function groupOf(note: Pick<MemoryNote, "path" | "bodyMd">, groups: readonly { query: GraphQuery; color: PaletteKey }[]): PaletteKey | null {
  for (const g of groups) if (!emptyQuery(g.query) && matchesQuery(note, g.query)) return g.color;
  return null;
}

// ---- Settings --------------------------------------------------------------------------------------------------------

/** The graph's settings panel: Filters, Groups, Display and Forces, and the local graph's depth and directions. */
export type GraphSettings = {
  query: string;
  /** Tags as dots. */
  tags: boolean;
  /** Dots without lines. */
  orphans: boolean;
  /** Leave out the dots of links that find no note. */
  existingOnly: boolean;
  /** The extra kinds hidden. */
  hidden: ExtraKind[];
  /** Null until changed: one group per top folder. */
  groups: GraphGroup[] | null;
  arrows: boolean;
  /** When labels fade in: higher needs more zoom. */
  textFade: number;
  nodeSize: number;
  linkThickness: number;
  centre: number;
  repel: number;
  linkForce: number;
  linkDistance: number;
  /** The local graph: how many links away, and which way. */
  depth: number;
  incoming: boolean;
  outgoing: boolean;
  /** The settings panel is open, and which of its sections. */
  panel: boolean;
  open: string[];
};

/** Each slider's range, step and default. */
export const RANGES = {
  textFade: { min: -3, max: 3, step: 0.1, value: 0 },
  nodeSize: { min: 0.5, max: 3, step: 0.05, value: 1 },
  linkThickness: { min: 0.5, max: 3, step: 0.05, value: 1 },
  centre: { min: 0, max: 1, step: 0.01, value: 0.5 },
  repel: { min: 0, max: 20, step: 0.5, value: 10 },
  linkForce: { min: 0, max: 1, step: 0.01, value: 1 },
  linkDistance: { min: 10, max: 300, step: 5, value: 60 },
  depth: { min: 1, max: 5, step: 1, value: 1 },
} as const;
export type RangeKey = keyof typeof RANGES;

export const GRAPH_SECTIONS = ["filters", "groups", "display", "forces"] as const;

export const GRAPH_DEFAULTS: GraphSettings = {
  query: "", tags: false, orphans: true, existingOnly: false, hidden: [], groups: null,
  arrows: false, textFade: RANGES.textFade.value, nodeSize: RANGES.nodeSize.value, linkThickness: RANGES.linkThickness.value,
  centre: RANGES.centre.value, repel: RANGES.repel.value, linkForce: RANGES.linkForce.value, linkDistance: RANGES.linkDistance.value,
  depth: RANGES.depth.value, incoming: true, outgoing: true, panel: false, open: ["filters"],
};

/** The global graph's and the local graph's settings, each kept on its own. */
export type GraphPrefs = { global: GraphSettings; local: GraphSettings };

/** Settings as kept (JSON from an older or newer Gizai, or hand-edited): anything missing or wrong at its default,
 *  numbers within their range. */
export function parseGraphSettings(raw: unknown): GraphSettings {
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const bool = (k: keyof GraphSettings) => (typeof o[k] === "boolean" ? o[k] as boolean : GRAPH_DEFAULTS[k] as boolean);
  const num = (k: RangeKey) => {
    const v = o[k], r = RANGES[k];
    if (typeof v !== "number" || !Number.isFinite(v)) return r.value;
    const n = Math.min(r.max, Math.max(r.min, v));
    return k === "depth" ? Math.round(n) : n;
  };
  const keys = PALETTE.map((p) => p.key) as string[];
  const groups = Array.isArray(o.groups)
    ? o.groups.flatMap((g): GraphGroup[] => (g && typeof g === "object" && typeof (g as GraphGroup).query === "string"
      ? [{ query: (g as GraphGroup).query, color: (keys.includes((g as GraphGroup).color) ? (g as GraphGroup).color : PALETTE[0].key) }] : []))
    : null;
  return {
    query: typeof o.query === "string" ? o.query : "",
    tags: bool("tags"), orphans: bool("orphans"), existingOnly: bool("existingOnly"),
    hidden: Array.isArray(o.hidden) ? [...new Set(o.hidden.filter((k): k is ExtraKind => (EXTRA_KINDS as readonly unknown[]).includes(k)))] : [],
    groups,
    arrows: bool("arrows"), textFade: num("textFade"), nodeSize: num("nodeSize"), linkThickness: num("linkThickness"),
    centre: num("centre"), repel: num("repel"), linkForce: num("linkForce"), linkDistance: num("linkDistance"),
    depth: num("depth"), incoming: bool("incoming"), outgoing: bool("outgoing"), panel: bool("panel"),
    open: Array.isArray(o.open) ? o.open.filter((s): s is string => (GRAPH_SECTIONS as readonly unknown[]).includes(s)) : [...GRAPH_DEFAULTS.open],
  };
}

export function parseGraphPrefs(json: string | null): GraphPrefs {
  let o: Record<string, unknown> = {};
  try { const v = JSON.parse(json ?? "{}"); if (v && typeof v === "object") o = v as Record<string, unknown>; } catch { /* the defaults */ }
  return { global: parseGraphSettings(o.global), local: parseGraphSettings(o.local) };
}

// ---- What the graph shows --------------------------------------------------------------------------------------------

/** The dots `depth` links away from `centre` (it included), following links out of a dot (`outgoing`) and into it
 *  (`incoming`). */
export function localGraph(g: Pick<Graph, "links">, centre: string, depth: number, incoming: boolean, outgoing: boolean): Set<string> {
  const out = new Map<string, string[]>(), into = new Map<string, string[]>();
  for (const l of g.links) {
    out.set(l.source, [...(out.get(l.source) ?? []), l.target]);
    into.set(l.target, [...(into.get(l.target) ?? []), l.source]);
  }
  const seen = new Set([centre]);
  let ring = [centre];
  for (let d = 0; d < depth && ring.length; d++) {
    const next: string[] = [];
    for (const id of ring) {
      for (const to of [...(outgoing ? out.get(id) ?? [] : []), ...(incoming ? into.get(id) ?? [] : [])]) {
        if (!seen.has(to)) { seen.add(to); next.push(to); }
      }
    }
    ring = next;
  }
  return seen;
}

/** What the graph shows with these settings: without the kinds that are off (tags unless on, missing notes when
 *  existing notes only, the hidden extras) and the notes outside `scope`, with what only they link to and name; then the
 *  local graph around `centre`, when there is one (nothing when that note isn't in the graph); then only the notes the
 *  query matches, with what they link to and name; then without the dots that have no line left, unless orphans are on.
 *  `centre` always stays. */
export function graphView(g: Graph, s: Pick<GraphSettings, "query" | "tags" | "orphans" | "existingOnly" | "hidden" | "depth" | "incoming" | "outgoing">,
  notes: ReadonlyMap<string, Pick<MemoryNote, "path" | "bodyMd">>, opts: { centre?: string | null; scope?: (noteId: string) => boolean } = {}): Graph {
  const { centre, scope } = opts;
  const kindOn = (n: GraphNode) => n.kind === "note" ? !scope || scope(n.ref) || n.id === centre
    : n.kind === "missing" ? !s.existingOnly : n.kind === "tag" ? s.tags : !s.hidden.includes(n.kind);
  let nodes = g.nodes.filter(kindOn);
  let keep = new Set(nodes.map((n) => n.id));
  let links = g.links.filter((l) => keep.has(l.source) && keep.has(l.target));
  // On a page, a dot that isn't a note (a card, an agent, a link that finds no note, a tag) stays only when one of the
  // page's own notes links to it or names it: not for the notes of other folders, which aren't shown.
  if (scope) {
    const own = new Set(nodes.flatMap((n) => (n.kind === "note" ? [n.id] : [])));
    keep = new Set(own);
    for (const l of links) {
      if (own.has(l.source)) keep.add(l.target);
      if (own.has(l.target)) keep.add(l.source);
    }
    nodes = nodes.filter((n) => keep.has(n.id));
    links = links.filter((l) => keep.has(l.source) && keep.has(l.target));
  }
  // A local graph around a note that isn't there (yet) is empty.
  if (centre) {
    keep = keep.has(centre) ? localGraph({ links }, centre, s.depth, s.incoming, s.outgoing) : new Set();
    nodes = nodes.filter((n) => keep.has(n.id));
    links = links.filter((l) => keep.has(l.source) && keep.has(l.target));
  }
  const q = parseGraphQuery(s.query);
  if (!emptyQuery(q)) {
    const match = new Set(nodes.filter((n) => n.kind === "note" && (n.id === centre || (notes.get(n.ref) && matchesQuery(notes.get(n.ref)!, q)))).map((n) => n.id));
    // What a matching note links to or names stays (not other notes).
    const near = new Set<string>(match);
    for (const l of links) {
      if (match.has(l.source)) near.add(l.target);
      if (match.has(l.target)) near.add(l.source);
    }
    const byId = new Map(nodes.map((n) => [n.id, n]));
    keep = new Set([...near].filter((id) => match.has(id) || byId.get(id)?.kind !== "note"));
    nodes = nodes.filter((n) => keep.has(n.id));
    links = links.filter((l) => keep.has(l.source) && keep.has(l.target));
  }
  if (!s.orphans) {
    const linked = new Set(links.flatMap((l) => [l.source, l.target]));
    nodes = nodes.filter((n) => linked.has(n.id) || n.id === centre);
  }
  return { nodes, links };
}

/** The dots a dot has a line with, and which way: `out` it links to them, `in` they link to it, `both`. By name. */
export type Connection = { node: GraphNode; way: "in" | "out" | "both"; kind: LinkKind };
export function connections(g: Graph, id: string): Connection[] {
  const byId = new Map(g.nodes.map((n) => [n.id, n]));
  const out = new Map<string, Connection>();
  for (const l of g.links) {
    const other = l.source === id ? l.target : l.target === id ? l.source : null;
    if (!other || !byId.has(other)) continue;
    const way = l.source === id ? "out" : "in";
    const had = out.get(other);
    out.set(other, had ? { ...had, way: had.way === way ? way : "both" } : { node: byId.get(other)!, way, kind: l.kind });
  }
  return [...out.values()].sort((a, b) => (lower(a.node.label) < lower(b.node.label) ? -1 : lower(a.node.label) > lower(b.node.label) ? 1 : 0));
}

/** The dot and its neighbours, and its lines: what pointing at a dot lights up. */
export function neighbourhood(g: Pick<Graph, "links">, id: string): { nodes: Set<string>; links: Set<GraphLink> } {
  const nodes = new Set([id]);
  const links = new Set<GraphLink>();
  for (const l of g.links) if (l.source === id || l.target === id) { links.add(l); nodes.add(l.source); nodes.add(l.target); }
  return { nodes, links };
}

// ---- The canvas's sums -----------------------------------------------------------------------------------------------

/** A dot's radius in the graph's units: it grows with its links (by the square root, so a hub doesn't swamp the rest). */
export const nodeRadius = (links: number, size = 1) => (3 + Math.sqrt(Math.max(0, links)) * 1.6) * size;

/** The zoom at which labels start to show for a text fade threshold: 1.2 at 0, a quarter of that at -3, four times at 3. */
export const fadeZoom = (textFade: number) => 1.2 * Math.pow(2, textFade / 1.5);

/** How much the labels show at zoom `k`: nothing below the fade zoom, fully at one and a half times it. */
export function labelAlpha(k: number, textFade: number): number {
  const at = fadeZoom(textFade);
  return Math.max(0, Math.min(1, (k - at) / (at * 0.5)));
}

/** The zoom and offset that fit points (with their radius) in a `w` × `h` view with `pad` around them: never closer
 *  than `maxK`, so one dot or a few aren't blown up. The view's centre for none. */
export function fitTransform(points: readonly { x: number; y: number; r?: number }[], w: number, h: number, pad = 40, maxK = 2): { k: number; x: number; y: number } {
  if (!points.length) return { k: 1, x: w / 2, y: h / 2 };
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of points) {
    const r = p.r ?? 0;
    x0 = Math.min(x0, p.x - r); y0 = Math.min(y0, p.y - r); x1 = Math.max(x1, p.x + r); y1 = Math.max(y1, p.y + r);
  }
  const k = Math.max(0.02, Math.min(maxK, (w - pad * 2) / Math.max(1, x1 - x0), (h - pad * 2) / Math.max(1, y1 - y0)));
  return { k, x: w / 2 - ((x0 + x1) / 2) * k, y: h / 2 - ((y0 + y1) / 2) * k };
}

/** The order Animate shows the dots in: as they came (a note when it was made), then by name. */
export function growthOrder(nodes: readonly GraphNode[]): GraphNode[] {
  return [...nodes].sort((a, b) => a.born - b.born || (lower(a.label) < lower(b.label) ? -1 : lower(a.label) > lower(b.label) ? 1 : 0));
}

/** The dot an arrow key goes to from `from`: the nearest one that way (within 60° of it, nearer the line counts more),
 *  or null when there is none. */
export function nextDot<P extends { id: string; x: number; y: number }>(points: readonly P[], from: P, dir: "left" | "right" | "up" | "down"): P | null {
  const [dx, dy] = { left: [-1, 0], right: [1, 0], up: [0, -1], down: [0, 1] }[dir] as [number, number];
  let best: P | null = null, bestScore = Infinity;
  for (const p of points) {
    if (p.id === from.id) continue;
    const vx = p.x - from.x, vy = p.y - from.y;
    const dist = Math.hypot(vx, vy);
    if (dist === 0) continue;
    const along = (vx * dx + vy * dy) / dist;
    if (along < 0.5) continue;
    const score = dist * (2 - along);
    if (score < bestScore) { best = p; bestScore = score; }
  }
  return best;
}
