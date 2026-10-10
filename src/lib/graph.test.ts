// GA-69: the memory graph's pure parts (src/lib/graph.ts): buildGraph's dots and lines (notes, wikilinks and embeds, a
// dim dot for a link that finds no note, cards, projects, clients, agents and people that notes name, tags), what the
// settings show (graphView: the kinds, the page's scope, the local graph's depth and directions, the query with path:
// and tag:, orphans), the connections list, the groups and the calm palette (no teal, no magenta), the kept settings,
// and the canvas's sums (a dot's size, the labels' fade, fitting, Animate's order, the arrow keys). Then 500 notes.
import { describe, expect, it } from "vitest";
import {
  bornAt, buildGraph, connections, defaultGroups, EXTRA_KINDS, fadeZoom, fitTransform, GRAPH_DEFAULTS, graphView, groupOf, growthOrder, hueOf,
  KIND_LOOK, labelAlpha, localGraph, matchesQuery, mentionHandles, neighbourhood, nextDot, nodeRadius, PALETTE, paletteColor, parseGraphPrefs,
  parseGraphQuery, parseGraphSettings, RANGES, resolver, type Graph, type GraphSettings, type GraphSources,
} from "./graph";
import { resolve } from "./memory";
import type { MemoryNote } from "../types";
import graphTs from "./graph.ts?raw";
import canvasTsx from "../components/memory/GraphCanvas.tsx?raw";
import panelTsx from "../components/memory/GraphPanel.tsx?raw";
import memoryGraphTsx from "../components/memory/MemoryGraph.tsx?raw";

/** A note at `path` (its id is `id:<path>`, so `note:id:<path>` is its dot). */
const note = (path: string, bodyMd = "", more: Partial<MemoryNote> = {}): MemoryNote => ({
  id: `id:${path}`, path, scope: /^(agents|team lead)\//i.test(path) ? "agent" : "shared", ownerId: null, bodyMd, currentVersion: 1,
  updatedAt: 0, updatedBy: null, chars: bodyMd.length, ...more,
});
const dot = (path: string) => `note:id:${path}`;
const ids = (g: Pick<Graph, "nodes">) => g.nodes.map((n) => n.id).sort();
const lines = (g: Pick<Graph, "links">) => g.links.map((l) => `${l.source} -> ${l.target} (${l.kind})`).sort();
const node = (g: Graph, id: string) => g.nodes.find((n) => n.id === id);
const byId = (notes: readonly MemoryNote[]) => new Map(notes.map((n) => [n.id, n]));
const settings = (patch: Partial<GraphSettings> = {}): GraphSettings => ({ ...GRAPH_DEFAULTS, ...patch });

const SOURCES: Omit<GraphSources, "notes"> = {
  cards: [{ identifier: "KADE-1", title: "Set up the repo" }, { identifier: "KADE-2", title: "Deploy" }],
  projects: [{ id: "p1", key: "KADE", name: "Kade shop" }, { id: "p2", key: "WEB", name: "Website" }],
  clients: [{ id: "c1", name: "Acme" }, { id: "c2", name: "Globex" }],
  agents: [{ actorId: "a1", name: "Backend Agent", handle: "backend" }],
  people: [{ id: "u1", name: "Jeffrey", handle: "jeffrey" }],
};

// ---- buildGraph ------------------------------------------------------------------------------------------------------

describe("buildGraph: notes are dots, links are lines", () => {
  it("makes a dot per note (its title, folder and path) and a line per wikilink, an embed its own kind", () => {
    const notes = [
      note("Decisions/Use SQLite", "See [[Deploy steps]] and [[Deploy steps#Rollback|roll back]].\n\n![[Release checklist]]"),
      note("Workflows/Deploy steps", "# Deploy steps"),
      note("Standards/Release checklist", "Read the changelog."),
    ];
    const g = buildGraph({ notes });
    expect(ids(g)).toEqual([dot("Decisions/Use SQLite"), dot("Standards/Release checklist"), dot("Workflows/Deploy steps")]);
    const sqlite = node(g, dot("Decisions/Use SQLite"))!;
    expect(sqlite).toMatchObject({ kind: "note", label: "Use SQLite", detail: "Decisions", ref: "id:Decisions/Use SQLite", path: "Decisions/Use SQLite" });
    // Two links to the same note (one with a heading and an alias) are one line.
    expect(lines(g)).toEqual([
      `${dot("Decisions/Use SQLite")} -> ${dot("Standards/Release checklist")} (embed)`,
      `${dot("Decisions/Use SQLite")} -> ${dot("Workflows/Deploy steps")} (link)`,
    ]);
  });

  it("draws a line each way when two notes link each other, and none from a note to itself or a heading of its own", () => {
    const notes = [note("Lessons/A", "[[B]] [[A]] [[#Top]]"), note("Lessons/B", "[[A]]")];
    expect(lines(buildGraph({ notes }))).toEqual([`${dot("Lessons/A")} -> ${dot("Lessons/B")} (link)`, `${dot("Lessons/B")} -> ${dot("Lessons/A")} (link)`]);
  });

  it("finds a link's note the way Memory does: by title, case ignored, by path, the linking note's folder first", () => {
    const notes = [
      note("Lessons/Setup", "[[setup]] [[Standards/Setup]]"),
      note("Standards/Setup", "[[Setup]]"),
      note("Decisions/Plan", "[[setup]]"),
    ];
    expect(lines(buildGraph({ notes }))).toEqual([
      // Decisions/Plan: of the two Setups, the shortest path (Lessons/Setup)
      `${dot("Decisions/Plan")} -> ${dot("Lessons/Setup")} (link)`,
      // Lessons/Setup: [[setup]] is itself (no line), the path finds the other one
      `${dot("Lessons/Setup")} -> ${dot("Standards/Setup")} (link)`,
      // Standards/Setup: the one in its own folder, itself: no line
    ]);
  });

  it("ends a link that finds no note at one dim dot per name, which knows where the note will go", () => {
    const notes = [note("Decisions/Use SQLite", "Not yet: [[Backup plan]]"), note("Lessons/Outage", "[[backup plan]] and [[Workflows/Restore]]")];
    const g = buildGraph({ notes });
    const missing = g.nodes.filter((n) => n.kind === "missing");
    expect(missing.map((n) => n.id).sort()).toEqual(["missing:backup plan", "missing:workflows/restore"]);
    // Next to the first note that links to it; a path keeps its folder.
    expect(node(g, "missing:backup plan")).toMatchObject({ label: "Backup plan", path: "Decisions/Backup plan", detail: "Decisions", ref: "Backup plan", links: 2 });
    expect(node(g, "missing:workflows/restore")).toMatchObject({ path: "Workflows/Restore", links: 1 });
    expect(lines(g)).toContain(`${dot("Lessons/Outage")} -> missing:backup plan (link)`);
  });

  it("makes a link's note out of a dim dot once the note is there", () => {
    const before = buildGraph({ notes: [note("Decisions/Use SQLite", "[[Backup plan]]")] });
    const after = buildGraph({ notes: [note("Decisions/Use SQLite", "[[Backup plan]]"), note("Decisions/Backup plan")] });
    expect(ids(before)).toContain("missing:backup plan");
    expect(ids(after)).not.toContain("missing:backup plan");
    expect(lines(after)).toEqual([`${dot("Decisions/Use SQLite")} -> ${dot("Decisions/Backup plan")} (link)`]);
  });

  it("counts a dot's lines in and out as its links, not a note's tags; a tag counts its notes", () => {
    const notes = [
      note("Lessons/Hub", "[[A]] [[B]] [[C]] #ops"),
      note("Lessons/A", "[[Hub]] #ops"),
      note("Lessons/B"),
      note("Lessons/C"),
      note("Lessons/Alone", "#ops"),
    ];
    const g = buildGraph({ notes });
    expect(node(g, dot("Lessons/Hub"))!.links).toBe(4); // 3 out, 1 in
    expect(node(g, dot("Lessons/A"))!.links).toBe(2);
    expect(node(g, dot("Lessons/B"))!.links).toBe(1);
    expect(node(g, dot("Lessons/Alone"))!.links).toBe(0);
    expect(node(g, "tag:ops")).toMatchObject({ kind: "tag", label: "#ops", ref: "ops", links: 3 });
    expect(g.links.filter((l) => l.kind === "tag").map((l) => l.source).sort()).toEqual([dot("Lessons/A"), dot("Lessons/Alone"), dot("Lessons/Hub")]);
  });
});

describe("buildGraph: what notes name are dots of their own kinds", () => {
  it("makes a card dot for a card reference and a gizai:task link, one per card; an unknown card makes none", () => {
    const notes = [
      note("Lessons/A", "First seen on KADE-1 and kade-1, then [the deploy](gizai:task/KADE-2). Not a card: NOPE-9, KADE-99."),
      note("Lessons/B", "Also KADE-1."),
    ];
    const g = buildGraph({ notes, ...SOURCES });
    expect(ids(g).filter((id) => id.startsWith("card:"))).toEqual(["card:KADE-1", "card:KADE-2"]);
    expect(node(g, "card:KADE-1")).toMatchObject({ kind: "card", label: "KADE-1", detail: "Set up the repo", ref: "KADE-1", links: 2 });
    expect(lines(g).filter((l) => l.includes("card:"))).toEqual([
      `${dot("Lessons/A")} -> card:KADE-1 (card)`, `${dot("Lessons/A")} -> card:KADE-2 (card)`, `${dot("Lessons/B")} -> card:KADE-1 (card)`,
    ]);
  });

  it("makes agent and person dots for @mentions of their handles (case ignored), not for an e-mail address or an unknown handle", () => {
    const notes = [note("Lessons/A", "Asked @Backend and @jeffrey. Mail jeffrey@example.com. Nobody: @ghost.")];
    const g = buildGraph({ notes, ...SOURCES });
    expect(node(g, "agent:a1")).toMatchObject({ kind: "agent", label: "Backend Agent", ref: "a1" });
    expect(node(g, "person:u1")).toMatchObject({ kind: "person", label: "Jeffrey", ref: "u1" });
    expect(g.nodes.filter((n) => n.kind === "agent" || n.kind === "person")).toHaveLength(2);
    expect(lines(g)).toEqual([`${dot("Lessons/A")} -> agent:a1 (mention)`, `${dot("Lessons/A")} -> person:u1 (mention)`]);
  });

  it("follows the @ picker's gizai: links to a project, client, agent, person and note", () => {
    const notes = [
      note("Lessons/A", "[Kade](gizai:project/KADE) [Acme](gizai:client/c1) [BE](gizai:agent/a1) [J](gizai:person/u1) [B](gizai:doc/0192-b) [x](gizai:client/zz)"),
      note("Lessons/B", "", { id: "0192-b" }),
    ];
    const g = buildGraph({ notes, ...SOURCES });
    expect(lines(g)).toEqual([
      `${dot("Lessons/A")} -> agent:a1 (mention)`, `${dot("Lessons/A")} -> client:c1 (mention)`, `${dot("Lessons/A")} -> note:0192-b (link)`,
      `${dot("Lessons/A")} -> person:u1 (mention)`, `${dot("Lessons/A")} -> project:p1 (mention)`,
    ]);
    expect(node(g, "project:p1")).toMatchObject({ kind: "project", label: "Kade shop", detail: "KADE", ref: "KADE" });
    expect(node(g, "client:c1")).toMatchObject({ kind: "client", label: "Acme", ref: "c1" });
  });

  it("follows the client: and project: properties, by name or key", () => {
    const notes = [note("Clients/Acme", "---\nclient: \"[[Acme]]\"\nproject: [web, Kade shop]\n---\nText")];
    const g = buildGraph({ notes, ...SOURCES });
    expect(lines(g)).toEqual([
      `${dot("Clients/Acme")} -> client:c1 (property)`, `${dot("Clients/Acme")} -> project:p1 (property)`, `${dot("Clients/Acme")} -> project:p2 (property)`,
    ]);
  });

  it("knows each kind of dot it makes", () => {
    const notes = [note("Lessons/A", "[[Gone]] KADE-1 @backend @jeffrey #ops [P](gizai:project/WEB) [C](gizai:client/c2)"), note("Lessons/B", "[[A]]")];
    const kinds = new Set(buildGraph({ notes, ...SOURCES }).nodes.map((n) => n.kind));
    expect([...kinds].sort()).toEqual(["agent", "card", "client", "missing", "note", "person", "project", "tag"]);
  });
});

describe("buildGraph: when each dot came (Animate's order)", () => {
  it("reads a note's time from its UUIDv7 id, else takes when it changed", () => {
    expect(bornAt({ id: "01928a3b-4c5d-7abc-8def-0123456789ab", updatedAt: 5 })).toBe(0x01928a3b4c5d);
    expect(bornAt({ id: "01928a3b-4c5d-4abc-8def-0123456789ab", updatedAt: 5 })).toBe(5); // a v4 id
    expect(bornAt({ id: "id:Lessons/A", updatedAt: 7 })).toBe(7);
  });

  it("gives what notes name the time of the first note that names it", () => {
    const notes = [
      note("Lessons/Late", "KADE-1 [[Gone]]", { id: "01928a3b-4c5d-7000-8000-000000000002" }),
      note("Lessons/Early", "KADE-1 [[Gone]]", { id: "01928a3b-0000-7000-8000-000000000001" }),
    ];
    const g = buildGraph({ notes, ...SOURCES });
    expect(node(g, "card:KADE-1")!.born).toBe(0x01928a3b0000);
    expect(node(g, "missing:gone")!.born).toBe(0x01928a3b0000);
    expect(growthOrder(g.nodes).map((n) => n.label)).toEqual(["Early", "Gone", "KADE-1", "Late"]);
  });
});

describe("resolver: Memory's resolve for many links at once", () => {
  const notes = [
    note("Lessons/Setup"), note("Standards/Setup"), note("Standards/Sub/Setup"), note("Decisions/Plan"), note("Agents/QA/Notes"), note("Team Lead/Notes"),
    note("Workflows/Deploy steps"), note("Lessons/deploy STEPS"),
  ];
  const targets = ["setup", "Setup", "Standards/Setup", "Sub/Setup", "/Standards/Setup/", "Setup.md", "notes", "QA/Notes", "Plan", "nothing", "", "deploy steps"];
  const folders = ["", "Lessons", "Standards", "Standards/Sub", "Agents/QA", "Team Lead", "Workflows"];
  it("finds the same note as resolve for every target from every folder", () => {
    const find = resolver(notes);
    for (const t of targets) for (const f of folders) expect([t, f, find(t, f)]).toEqual([t, f, resolve(notes, t, f)]);
  });
});

describe("mentionHandles", () => {
  it("finds @handles in lower case without an e-mail address's @ or trailing punctuation", () => {
    expect(mentionHandles("Ask @Backend, then @qa-agent. Mail a@b.com or x.@y; (@jeffrey) @@ @")).toEqual(["backend", "qa-agent", "jeffrey"]);
  });
});

// ---- graphView -------------------------------------------------------------------------------------------------------

describe("graphView: what the settings show", () => {
  const notes = [
    note("Decisions/Use SQLite", "[[Deploy steps]] [[Backup plan]] KADE-1 @backend #storage"),
    note("Workflows/Deploy steps", "[[Release checklist]] #ops"),
    note("Standards/Release checklist", "Read the changelog."),
    note("Lessons/Alone", "Nothing links here."),
    note("Agents/QA/Notes", "Tested [[Use SQLite]] and KADE-2."),
  ];
  const g = buildGraph({ notes, ...SOURCES });
  const all = byId(notes);

  it("shows notes, dim dots and what notes name by default, without tags", () => {
    const v = graphView(g, settings(), all);
    expect(ids(v)).toEqual([
      "agent:a1", "card:KADE-1", "card:KADE-2", "missing:backup plan", dot("Agents/QA/Notes"), dot("Decisions/Use SQLite"), dot("Lessons/Alone"),
      dot("Standards/Release checklist"), dot("Workflows/Deploy steps"),
    ]);
    expect(v.links.every((l) => l.kind !== "tag")).toBe(true);
  });

  it("shows tags as dots when Tags is on", () => {
    const v = graphView(g, settings({ tags: true }), all);
    expect(ids(v)).toEqual(expect.arrayContaining(["tag:ops", "tag:storage"]));
    expect(lines(v)).toContain(`${dot("Workflows/Deploy steps")} -> tag:ops (tag)`);
  });

  it("leaves out the dim dots with Existing notes only", () => {
    const v = graphView(g, settings({ existingOnly: true }), all);
    expect(v.nodes.some((n) => n.kind === "missing")).toBe(false);
    expect(v.links.some((l) => l.target.startsWith("missing:"))).toBe(false);
  });

  it("hides each kind of what notes name on its own, with its lines", () => {
    for (const kind of EXTRA_KINDS) {
      const v = graphView(g, settings({ hidden: [kind] }), all);
      expect(v.nodes.some((n) => n.kind === kind)).toBe(false);
      expect(v.links.some((l) => l.target.startsWith(`${kind}:`))).toBe(false);
    }
    const none = graphView(g, settings({ hidden: [...EXTRA_KINDS] }), all);
    expect(none.nodes.map((n) => n.kind).filter((k) => k !== "note" && k !== "missing")).toEqual([]);
  });

  it("leaves out the dots without a line when Orphans is off", () => {
    expect(ids(graphView(g, settings({ orphans: true }), all))).toContain(dot("Lessons/Alone"));
    expect(ids(graphView(g, settings({ orphans: false }), all))).not.toContain(dot("Lessons/Alone"));
  });

  it("shows only the page's notes: an agent's page its folder's", () => {
    const qa = (id: string) => id === "id:Agents/QA/Notes";
    const v = graphView(g, settings(), all, { scope: qa });
    expect(v.nodes.filter((n) => n.kind === "note").map((n) => n.id)).toEqual([dot("Agents/QA/Notes")]);
    // Its links to notes outside the page go with them.
    expect(lines(v)).toEqual([`${dot("Agents/QA/Notes")} -> card:KADE-2 (card)`]);
  });

  it("shows on a page only what the page's own notes name, not the cards or missing notes of other folders", () => {
    // The QA agent's page: its note names KADE-2 only. KADE-1, @backend and [[Backup plan]] are in Decisions/Use SQLite.
    const qa = (id: string) => id === "id:Agents/QA/Notes";
    const v = graphView(g, settings(), all, { scope: qa });
    expect(ids(v)).toEqual(["card:KADE-2", dot("Agents/QA/Notes")]);
  });

  it("keeps the notes a query matches, with what they link to and name, but no other notes", () => {
    const v = graphView(g, settings({ query: "path:Decisions/" }), all);
    expect(ids(v)).toEqual(["agent:a1", "card:KADE-1", "missing:backup plan", dot("Decisions/Use SQLite")]);
    const words = graphView(g, settings({ query: "changelog" }), all);
    expect(ids(words)).toEqual([dot("Standards/Release checklist")]);
    const tag = graphView(g, settings({ query: "tag:ops" }), all);
    expect(ids(tag)).toEqual([dot("Workflows/Deploy steps")]);
    const none = graphView(g, settings({ query: "zzqq" }), all);
    expect(none.nodes).toEqual([]);
  });
});

describe("graphView on a page: only what the page's own notes link to and name, with the filters on top", () => {
  // Shared notes name KADE-1, @backend, [[Backup plan]], #storage and #ops; the QA agent's notes name KADE-2, @jeffrey,
  // the WEB project, Acme, [[QA plan]] and #qa, and also KADE-1, [[Backup plan]] and #storage.
  const notes = [
    note("Decisions/Use SQLite", "[[Deploy steps]] [[Backup plan]] KADE-1 @backend #storage"),
    note("Workflows/Deploy steps", "[[Release checklist]] #ops"),
    note("Standards/Release checklist", "Read the changelog."),
    note("Lessons/Alone", "Nothing links here."),
    note("Agents/QA/Notes", "---\nproject: WEB\n---\nTested [[Use SQLite]] and KADE-2 with @jeffrey. Next: [[QA plan]] [C](gizai:client/c1) #qa"),
    note("Agents/QA/Shared names", "KADE-1 and [[Backup plan]] #storage"),
    note("Agents/QA/Quiet", "Nothing here either."),
  ];
  const g = buildGraph({ notes, ...SOURCES });
  const all = byId(notes);
  const shared = (id: string) => !all.get(id)!.path.startsWith("Agents/");
  const qa = (id: string) => all.get(id)!.path.startsWith("Agents/QA/");
  const sharedNotes = [dot("Decisions/Use SQLite"), dot("Lessons/Alone"), dot("Standards/Release checklist"), dot("Workflows/Deploy steps")];
  const qaNotes = [dot("Agents/QA/Notes"), dot("Agents/QA/Quiet"), dot("Agents/QA/Shared names")];
  const ends = (v: Graph) => v.links.every((l) => v.nodes.some((n) => n.id === l.source) && v.nodes.some((n) => n.id === l.target));

  it("shows on the shared page the shared notes and what they name, not what only an agent's notes name", () => {
    const v = graphView(g, settings(), all, { scope: shared });
    expect(ids(v)).toEqual(["agent:a1", "card:KADE-1", "missing:backup plan", ...sharedNotes].sort());
    // The QA note's line into Use SQLite goes with the QA note.
    expect(v.links.some((l) => l.source.startsWith("note:id:Agents/"))).toBe(false);
    expect(ends(v)).toBe(true);
  });

  it("shows on an agent's page its notes and everything they name, also what shared notes name too", () => {
    const v = graphView(g, settings(), all, { scope: qa });
    expect(ids(v)).toEqual(["card:KADE-1", "card:KADE-2", "client:c1", "missing:backup plan", "missing:qa plan", "person:u1", "project:p2", ...qaNotes].sort());
    expect(ids(v)).not.toContain("agent:a1");
    expect(lines(v)).not.toContain(`${dot("Agents/QA/Notes")} -> ${dot("Decisions/Use SQLite")} (link)`);
    expect(ends(v)).toBe(true);
  });

  it("shows only the page's own notes' tags when Tags is on", () => {
    const sharedTags = graphView(g, settings({ tags: true }), all, { scope: shared }).nodes.filter((n) => n.kind === "tag").map((n) => n.id).sort();
    expect(sharedTags).toEqual(["tag:ops", "tag:storage"]);
    const qaTags = graphView(g, settings({ tags: true }), all, { scope: qa }).nodes.filter((n) => n.kind === "tag").map((n) => n.id).sort();
    expect(qaTags).toEqual(["tag:qa", "tag:storage"]);
  });

  it("shows everything on the all-notes page, where every note is in scope, as without a scope", () => {
    for (const patch of [{}, { tags: true }, { orphans: false }, { query: "path:Agents/" }, { existingOnly: true }] as Partial<GraphSettings>[]) {
      const v = graphView(g, settings(patch), all, { scope: () => true });
      const plain = graphView(g, settings(patch), all);
      expect(ids(v)).toEqual(ids(plain));
      expect(lines(v)).toEqual(lines(plain));
    }
  });

  it("applies the query (path: and tag:) on top of the page", () => {
    const path = graphView(g, settings({ query: "path:Agents/QA/Shared" }), all, { scope: qa });
    expect(ids(path)).toEqual(["card:KADE-1", "missing:backup plan", dot("Agents/QA/Shared names")]);
    const tag = graphView(g, settings({ query: "tag:qa" }), all, { scope: qa });
    expect(ids(tag)).toEqual(["card:KADE-2", "client:c1", "missing:qa plan", dot("Agents/QA/Notes"), "person:u1", "project:p2"]);
    // A query for another folder's notes finds nothing on this page.
    expect(graphView(g, settings({ query: "path:Decisions/" }), all, { scope: qa }).nodes).toEqual([]);
    expect(graphView(g, settings({ query: "tag:qa" }), all, { scope: shared }).nodes).toEqual([]);
  });

  it("applies Existing notes only, each hidden kind and Orphans off on top of the page", () => {
    const existing = graphView(g, settings({ existingOnly: true }), all, { scope: qa });
    expect(existing.nodes.some((n) => n.kind === "missing")).toBe(false);
    expect(ids(existing)).toContain("card:KADE-2");
    for (const kind of EXTRA_KINDS) {
      const v = graphView(g, settings({ hidden: [kind] }), all, { scope: qa });
      expect(v.nodes.some((n) => n.kind === kind)).toBe(false);
      expect(ends(v)).toBe(true);
    }
    const lonely = graphView(g, settings({ orphans: false }), all, { scope: qa });
    expect(ids(lonely)).not.toContain(dot("Agents/QA/Quiet"));
    expect(ids(lonely)).toContain(dot("Agents/QA/Notes"));
    // Every card hidden: Shared names still links its dim dot, so it stays; with Existing notes only too, it goes.
    const bare = graphView(g, settings({ orphans: false, hidden: ["card"], existingOnly: true }), all, { scope: qa });
    expect(ids(bare)).not.toContain(dot("Agents/QA/Shared names"));
  });
});

describe("graphView and localGraph: the local graph around the open note", () => {
  // A -> B -> C -> D -> E -> F -> G, and X -> A
  const chain = ["A", "B", "C", "D", "E", "F", "G"];
  const notes = [
    ...chain.map((t, i) => note(`Lessons/${t}`, chain[i + 1] ? `[[${chain[i + 1]}]]` : "")),
    note("Lessons/X", "[[A]]"),
    note("Lessons/Far", "Not linked."),
  ];
  const g = buildGraph({ notes });
  const all = byId(notes);
  const local = (centre: string, patch: Partial<GraphSettings>) =>
    graphView(g, settings(patch), all, { centre: dot(`Lessons/${centre}`) }).nodes.map((n) => n.label).sort();

  it("shows the note and its neighbours, 1 to 5 links deep", () => {
    expect(local("D", { depth: 1 })).toEqual(["C", "D", "E"]);
    expect(local("D", { depth: 2 })).toEqual(["B", "C", "D", "E", "F"]);
    expect(local("D", { depth: 3 })).toEqual(["A", "B", "C", "D", "E", "F", "G"]);
    expect(local("D", { depth: 4 })).toEqual(["A", "B", "C", "D", "E", "F", "G", "X"]);
    expect(local("D", { depth: 5 })).toEqual(["A", "B", "C", "D", "E", "F", "G", "X"]);
  });

  it("follows only outgoing or only incoming links when the other is off", () => {
    expect(local("C", { depth: 2, incoming: false })).toEqual(["C", "D", "E"]);
    expect(local("C", { depth: 2, outgoing: false })).toEqual(["A", "B", "C"]);
    expect(local("C", { depth: 2, incoming: false, outgoing: false })).toEqual(["C"]);
  });

  it("keeps the note itself when it has no links, also with Orphans off, and is empty for a note not in the graph", () => {
    expect(local("Far", { orphans: false })).toEqual(["Far"]);
    expect(graphView(g, settings(), all, { centre: "note:nope" }).nodes).toEqual([]);
  });

  it("follows the open note: another centre, another neighbourhood", () => {
    expect(local("A", { depth: 1 })).toEqual(["A", "B", "X"]);
    expect(local("G", { depth: 1 })).toEqual(["F", "G"]);
  });

  it("localGraph walks the lines both ways from the centre", () => {
    const links = [{ source: "a", target: "b" }, { source: "c", target: "a" }, { source: "b", target: "d" }].map((l) => ({ ...l, kind: "link" as const }));
    expect([...localGraph({ links }, "a", 1, true, true)].sort()).toEqual(["a", "b", "c"]);
    expect([...localGraph({ links }, "a", 2, false, true)].sort()).toEqual(["a", "b", "d"]);
    expect([...localGraph({ links }, "a", 5, true, false)].sort()).toEqual(["a", "c"]);
  });
});

describe("connections and neighbourhood: a dot's links", () => {
  const notes = [note("Lessons/Hub", "[[Out]] [[Both]] KADE-1"), note("Lessons/Out"), note("Lessons/Both", "[[Hub]]"), note("Lessons/In", "[[Hub]]"), note("Lessons/Other", "[[Out]]")];
  const g = buildGraph({ notes, ...SOURCES });

  it("lists every dot a dot has a line with, by name, and which way", () => {
    expect(connections(g, dot("Lessons/Hub")).map((c) => [c.node.label, c.way, c.kind])).toEqual([
      ["Both", "both", "link"], ["In", "in", "link"], ["KADE-1", "out", "card"], ["Out", "out", "link"],
    ]);
    expect(connections(g, "card:KADE-1").map((c) => [c.node.label, c.way])).toEqual([["Hub", "in"]]);
    expect(connections(g, dot("Lessons/Nothing"))).toEqual([]);
  });

  it("lights up a dot, its neighbours and its lines, and nothing else", () => {
    const n = neighbourhood(g, dot("Lessons/Out"));
    expect([...n.nodes].sort()).toEqual([dot("Lessons/Hub"), dot("Lessons/Other"), dot("Lessons/Out")]);
    expect(n.links.size).toBe(2);
  });
});

// ---- The query, groups and colours -----------------------------------------------------------------------------------

describe("parseGraphQuery and matchesQuery: the search in Filters and Groups", () => {
  it("reads words, \"phrases\", path: (quoted, with a folder's slash) and tag: (with or without #)", () => {
    expect(parseGraphQuery('Deploy "release build" path:"Team Lead/" PATH:Lessons tag:#Ops tag:rust/style')).toEqual({
      words: ["deploy", "release build"], paths: ["team lead/", "lessons"], tags: ["ops", "rust/style"],
    });
    expect(parseGraphQuery("   ")).toEqual({ words: [], paths: [], tags: [] });
  });

  it("matches a note with every path:, tag: (or one under it) and word, case ignored", () => {
    const n = note("Agents/QA/Notes", "---\ntags: [rust/style]\n---\nRun the Release build. #ops");
    expect(matchesQuery(n, parseGraphQuery("path:agents/qa"))).toBe(true);
    expect(matchesQuery(n, parseGraphQuery("path:Agents/QA/"))).toBe(true);
    expect(matchesQuery(note("Agents/QA 2/Notes"), parseGraphQuery("path:Agents/QA/"))).toBe(false);
    expect(matchesQuery(n, parseGraphQuery("tag:rust tag:ops"))).toBe(true);
    expect(matchesQuery(n, parseGraphQuery("tag:rus"))).toBe(false);
    expect(matchesQuery(n, parseGraphQuery('"release build" notes'))).toBe(true);
    expect(matchesQuery(n, parseGraphQuery("release deploy"))).toBe(false);
    expect(matchesQuery(n, parseGraphQuery(""))).toBe(true);
  });
});

describe("defaultGroups and groupOf: colour by query", () => {
  const notes = [note("Lessons/A"), note("Agents/QA/Notes"), note("Team Lead/Notes"), note("Decisions/B"), note("lessons/C"), note("Loose")];

  it("starts with one group per top folder: the Team Lead's first, Agents last, each its own colour", () => {
    const groups = defaultGroups(notes);
    expect(groups.map((g) => g.query)).toEqual(['path:"Team Lead/"', "path:Decisions/", "path:Lessons/", "path:Agents/"]);
    expect(new Set(groups.map((g) => g.color)).size).toBe(groups.length);
    expect(defaultGroups([])).toEqual([]);
  });

  it("gives a note the colour of the first group it matches, an empty query none", () => {
    const parsed = (gs: { query: string; color: (typeof PALETTE)[number]["key"] }[]) => gs.map((g) => ({ query: parseGraphQuery(g.query), color: g.color }));
    const groups = parsed([{ query: "", color: "olive" }, { query: "path:Lessons/", color: "sage" }, { query: "tag:ops", color: "clay" }]);
    expect(groupOf(note("Lessons/A", "#ops"), groups)).toBe("sage");
    expect(groupOf(note("Decisions/B", "#ops"), groups)).toBe("clay");
    expect(groupOf(note("Decisions/B"), groups)).toBe(null);
    // Every default group colours the notes of its folder.
    const defaults = parsed(defaultGroups(notes));
    expect(notes.filter((n) => n.path.includes("/")).every((n) => groupOf(n, defaults) !== null)).toBe(true);
  });
});

// Vitest hands a .css file back empty, even with ?raw, so the stylesheets are read from disk. node:fs comes in through a
// computed specifier: the build type-checks tests without Node's types.
const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const css = (name: string): string => fs.readFileSync(new URL(`../styles/${name}`, import.meta.url), "utf8");

describe("the graph's colours: calm, never teal or magenta", () => {
  // Gizai keeps teal (--live) for an agent working now and magenta (--needs) for "needs you", in both themes.
  const tokens = css("tokens.css");
  const tokenValues = (name: string) => [...tokens.matchAll(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})`, "g"))].map((m) => m[1]!);
  const reserved = [...tokenValues("live"), ...tokenValues("needs")];
  const hueGap = (a: number, b: number) => Math.min(Math.abs(a - b), 360 - Math.abs(a - b));

  it("reads both themes' teal and magenta from the design tokens", () => {
    expect(reserved).toHaveLength(4);
    for (const c of reserved) expect(hueOf(c).sat).toBeGreaterThan(0.5);
  });

  it("keeps every palette colour, in dark and light, at least 30° of hue away from teal and magenta", () => {
    for (const p of PALETTE) for (const c of [p.dark, p.light]) {
      for (const r of reserved) expect([p.key, c, r, hueGap(hueOf(c).hue, hueOf(r).hue) >= 30]).toEqual([p.key, c, r, true]);
    }
  });

  it("gives every kind of dot a palette colour, and each a shape of its own", () => {
    const looks = Object.values(KIND_LOOK);
    for (const l of looks) expect(PALETTE.map((p) => p.key)).toContain(l.color);
    expect(new Set(looks.map((l) => l.shape)).size).toBe(looks.length);
    expect(paletteColor("nope")).toBe(PALETTE[0].dark);
    expect(paletteColor("sage", "light")).toBe(PALETTE.find((p) => p.key === "sage")!.light);
  });

  it("never uses the teal or magenta tokens or colours in the graph's code or styles", () => {
    const app = css("app.css");
    const graphCss = app.slice(app.indexOf("The memory graph (GA-69)"));
    expect(graphCss.length).toBeGreaterThan(500);
    const lowered = reserved.map((c) => c.toLowerCase());
    for (const [name, src] of [["graph.ts", graphTs], ["GraphCanvas.tsx", canvasTsx], ["GraphPanel.tsx", panelTsx], ["MemoryGraph.tsx", memoryGraphTsx], ["app.css (graph)", graphCss]]) {
      expect([name, /--live|--needs/.test(src)]).toEqual([name, false]);
      for (const c of lowered) expect([name, src.toLowerCase().includes(c)]).toEqual([name, false]);
    }
  });

  it("draws the lit dot and the open note in the accent, the rest from the text tokens", () => {
    expect(canvasTsx).toMatch(/accent: v\("--accent"/);
    expect(canvasTsx).toMatch(/dot: v\("--text-3"/);
    expect(canvasTsx).toMatch(/bg: v\("--bg"/);
  });
});

// ---- The kept settings -----------------------------------------------------------------------------------------------

describe("parseGraphSettings and parseGraphPrefs: the settings panel as kept", () => {
  it("starts with the defaults: orphans and both directions on, tags and arrows off, depth 1, the panel closed", () => {
    expect(parseGraphPrefs(null)).toEqual({ global: GRAPH_DEFAULTS, local: GRAPH_DEFAULTS });
    expect(parseGraphPrefs("not json")).toEqual({ global: GRAPH_DEFAULTS, local: GRAPH_DEFAULTS });
    expect(parseGraphPrefs("[1,2]").global).toEqual(GRAPH_DEFAULTS);
    expect(GRAPH_DEFAULTS).toMatchObject({ query: "", tags: false, orphans: true, existingOnly: false, hidden: [], groups: null, arrows: false, depth: 1, incoming: true, outgoing: true, panel: false });
  });

  it("gives back what was kept, the global and the local graph each their own", () => {
    const global = settings({ query: "path:Lessons/", tags: true, orphans: false, existingOnly: true, hidden: ["card", "agent"], groups: [{ query: "tag:ops", color: "clay" }],
      arrows: true, textFade: -1.5, nodeSize: 2, linkThickness: 0.75, centre: 0.2, repel: 3, linkForce: 0.4, linkDistance: 150, panel: true, open: ["groups", "forces"] });
    const local = settings({ depth: 4, incoming: false });
    expect(parseGraphPrefs(JSON.stringify({ global, local }))).toEqual({ global, local });
  });

  it("puts anything missing or wrong at its default and numbers within their range", () => {
    const s = parseGraphSettings({ query: 5, tags: "yes", nodeSize: 99, linkDistance: -4, repel: Number.NaN, depth: 3.6, centre: "1",
      hidden: ["card", "card", "note", 3], groups: [{ query: "path:A/", color: "teal" }, { color: "sage" }, null], open: ["forces", "magic"] });
    expect(s).toMatchObject({ query: "", tags: false, nodeSize: RANGES.nodeSize.max, linkDistance: RANGES.linkDistance.min, repel: RANGES.repel.value, depth: 4,
      centre: RANGES.centre.value, hidden: ["card"], groups: [{ query: "path:A/", color: PALETTE[0].key }], open: ["forces"] });
    expect(parseGraphSettings({ depth: 0 }).depth).toBe(1);
    expect(parseGraphSettings({ depth: 9 }).depth).toBe(5);
  });
});

// ---- The canvas's sums -----------------------------------------------------------------------------------------------

describe("the canvas's sums", () => {
  it("grows a dot with its links, by the square root, times the node size", () => {
    const sizes = [0, 1, 2, 5, 10, 50].map((n) => nodeRadius(n));
    for (let i = 1; i < sizes.length; i++) expect(sizes[i]!).toBeGreaterThan(sizes[i - 1]!);
    expect(nodeRadius(50) / nodeRadius(0)).toBeLessThan(5);
    expect(nodeRadius(4, 2)).toBeCloseTo(nodeRadius(4) * 2);
    expect(nodeRadius(-3)).toBe(nodeRadius(0));
  });

  it("fades labels in as the zoom grows; a higher threshold needs more zoom", () => {
    expect(labelAlpha(0.5, 0)).toBe(0);
    expect(labelAlpha(fadeZoom(0), 0)).toBe(0);
    expect(labelAlpha(fadeZoom(0) * 1.25, 0)).toBeCloseTo(0.5);
    expect(labelAlpha(fadeZoom(0) * 1.5, 0)).toBeCloseTo(1);
    expect(labelAlpha(8, 0)).toBe(1);
    const at = (fade: number) => [0.3, 0.6, 1, 1.5, 2, 3, 5].map((k) => labelAlpha(k, fade));
    for (const fade of [-3, 0, 3]) { const a = at(fade); for (let i = 1; i < a.length; i++) expect(a[i]!).toBeGreaterThanOrEqual(a[i - 1]!); }
    expect(fadeZoom(3)).toBeGreaterThan(fadeZoom(0));
    expect(fadeZoom(-3)).toBeLessThan(fadeZoom(0));
    expect(labelAlpha(1, -3)).toBe(1);
    expect(labelAlpha(1, 3)).toBe(0);
  });

  it("fits the dots in the view with room around them, and never zooms in past its limit", () => {
    const pts = [{ x: -100, y: -50, r: 5 }, { x: 100, y: 50, r: 5 }];
    const t = fitTransform(pts, 800, 600, 40, 8);
    for (const p of pts) {
      const sx = p.x * t.k + t.x, sy = p.y * t.k + t.y;
      expect(sx).toBeGreaterThanOrEqual(40 - 1e-9); expect(sx).toBeLessThanOrEqual(760 + 1e-9);
      expect(sy).toBeGreaterThanOrEqual(40 - 1e-9); expect(sy).toBeLessThanOrEqual(560 + 1e-9);
    }
    expect(fitTransform([{ x: 3, y: 4 }], 800, 600, 40, 2).k).toBe(2);
    expect(fitTransform([], 800, 600)).toEqual({ k: 1, x: 400, y: 300 });
  });

  it("goes with an arrow key to the nearest dot that way, or nowhere", () => {
    const p = (id: string, x: number, y: number) => ({ id, x, y });
    const pts = [p("o", 0, 0), p("r", 50, 5), p("far-r", 200, 0), p("u", 0, -40), p("d", 3, 80), p("diag", 40, 60)];
    expect(nextDot(pts, pts[0]!, "right")!.id).toBe("r");
    expect(nextDot(pts, pts[0]!, "up")!.id).toBe("u");
    expect(nextDot(pts, pts[0]!, "down")!.id).toBe("d");
    expect(nextDot(pts, pts[0]!, "left")).toBe(null);
    expect(nextDot([pts[0]!], pts[0]!, "right")).toBe(null);
  });
});

// ---- 500 notes -------------------------------------------------------------------------------------------------------

describe("500 notes", () => {
  const FOLDERS = ["Decisions", "Workflows", "Standards", "Lessons", "Clients", "Projects", "Team Lead", "Agents/QA"];
  const many = Array.from({ length: 500 }, (_, i) => {
    const body = [`[[Note ${(i * 7 + 1) % 500}]]`, `[[Note ${(i * 13 + 5) % 500}]]`, i % 9 === 0 ? `[[Missing ${i % 40}]]` : "", i % 5 === 0 ? "KADE-1" : "",
      i % 11 === 0 ? "@backend" : "", i % 6 === 0 ? `#t${i % 4}` : ""].join(" ");
    return note(`${FOLDERS[i % FOLDERS.length]}/Note ${i}`, body);
  });

  it("builds the graph and every view of it quickly", () => {
    const t0 = performance.now();
    const g = buildGraph({ notes: many, ...SOURCES });
    const all = byId(many);
    graphView(g, settings({ tags: true }), all);
    graphView(g, settings({ query: "path:Lessons/ note" }), all);
    graphView(g, settings({ depth: 5 }), all, { centre: g.nodes[0]!.id });
    for (const n of g.nodes.slice(0, 50)) connections(g, n.id);
    const ms = performance.now() - t0;
    expect(g.nodes.filter((n) => n.kind === "note")).toHaveLength(500);
    expect(g.links.length).toBeGreaterThan(900);
    expect(ms).toBeLessThan(400);
  });
});
