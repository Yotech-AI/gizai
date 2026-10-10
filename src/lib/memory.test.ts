// GA-68: the Memory page's pure parts (src/lib/memory.ts): the folder tree and which notes a page shows, wikilinks and
// the note each finds (gizai_core::memory's rules, docs/memory.md), backlinks and unlinked mentions, properties, tags and
// the outline, New note's templates, the [[ autocomplete and the search's highlights.
import { describe, expect, it } from "vitest";
import {
  agentFolder, AGENTS, backlinks, buildTree, findFolder, findWikiTrigger, foldersTo, frontmatterBlock, hasTag, highlight, inScope, isNoteFolder,
  isOwnFolder, LEAD, leadOf, linkedNote, linkMention, linkTarget, memoryScope, moveTarget, newNotePath, NOTE_TYPES, noteCount, notesIn,
  noteTemplate, outgoing, outline, properties, property, propertyLine, rebase, resolve, scopedQuery, scopeRoots, searchWords, section, setProperty,
  SHARED_FOLDERS, tagCounts, tags, TASK_REF, titleProblem, today, TYPE_FOLDER, TYPE_HINT, TYPE_NAME, unlinkedMentions, WIKI_ROWS, wikilinks,
  wikiRows, withoutFrontmatter, type NoteType, type WikiRow,
} from "./memory";
import type { MemoryNote } from "../types";

/** A note at `path` (its id is `id:<path>`; in an agent's or the Team Lead's folder its scope is `agent`). */
const note = (path: string, bodyMd = "", more: Partial<MemoryNote> = {}): MemoryNote => ({
  id: `id:${path}`, path, scope: /^(agents|team lead)\//i.test(path) ? "agent" : "shared", ownerId: null, bodyMd, currentVersion: 1,
  updatedAt: 0, updatedBy: null, chars: bodyMd.length, ...more,
});
const paths = (ns: readonly { path: string }[]) => ns.map((n) => n.path);

// ---- The tree -------------------------------------------------------------------------------------------------------

describe("buildTree: the file explorer's folders and notes", () => {
  const notes = [
    note("Standards/Rust style"),
    note("Lessons/b note"),
    note("Lessons/A note"),
    note("Lessons/Sub/Deep"),
    note("lessons/sub/Deeper"),
    note("Agents/Backend/Notes"),
    note("Team Lead/Notes"),
    note("Clients/Kade"),
    note("Decisions/Use SQLite"),
    note("Loose"),
  ];
  const tree = buildTree(notes, [...scopeRoots({ kind: "all" }), "Projects/Empty", "  ", "/Workflows/Draft/"]);

  it("puts the Team Lead's folder first and Agents last at the top, the shared folders by name between them", () => {
    expect(tree.folders.map((f) => f.name)).toEqual([
      "Team Lead", "Clients", "Decisions", "Dependencies", "Deployments", "Lessons", "Projects", "Standards", "Workflows", "Agents",
    ]);
  });
  it("keeps folders and notes apart, folders and notes each by name (case ignored)", () => {
    const lessons = findFolder(tree, "Lessons")!;
    expect(lessons.folders.map((f) => f.name)).toEqual(["Sub"]);
    expect(paths(lessons.notes)).toEqual(["Lessons/A note", "Lessons/b note"]);
    expect(paths(tree.notes)).toEqual(["Loose"]);
  });
  it("sorts folders inside a folder by name only: Agents or Team Lead there are no top folders", () => {
    const t = buildTree([note("Lessons/Zeta/x"), note("Lessons/Agents/x"), note("Lessons/Team Lead/x"), note("Lessons/alpha/x")]);
    expect(findFolder(t, "Lessons")!.folders.map((f) => f.name)).toEqual(["Agents", "alpha", "Team Lead", "Zeta"]);
  });
  it("merges folders whose names differ only in case, under the first spelling", () => {
    const lessons = tree.folders.filter((f) => f.name.toLowerCase() === "lessons");
    expect(lessons).toHaveLength(1);
    const sub = findFolder(tree, "Lessons/Sub")!;
    expect(sub.path).toBe("Lessons/Sub");
    expect(paths(sub.notes)).toEqual(["Lessons/Sub/Deep", "lessons/sub/Deeper"]);
  });
  it("shows the folders it is given also while they hold no note (blank ones left out, slashes trimmed)", () => {
    const projects = findFolder(tree, "Projects")!;
    expect(projects.folders.map((f) => f.path)).toEqual(["Projects/Empty"]);
    expect(projects.count).toBe(0);
    expect(findFolder(tree, "Projects/Empty")!.notes).toEqual([]);
    expect(findFolder(tree, "Workflows")!.folders.map((f) => f.path)).toEqual(["Workflows/Draft"]);
    expect(tree.folders.some((f) => f.name.trim() === "")).toBe(false);
  });
  it("counts a folder's notes with those in its sub-folders", () => {
    expect(findFolder(tree, "Lessons")!.count).toBe(4);
    expect(findFolder(tree, "Lessons/Sub")!.count).toBe(2);
    expect(findFolder(tree, "Agents")!.count).toBe(1);
    expect(findFolder(tree, "Dependencies")!.count).toBe(0);
    expect(tree.count).toBe(notes.length);
  });
  it("without folders given, shows only the folders that hold notes", () => {
    expect(buildTree([note("Lessons/X")]).folders.map((f) => f.name)).toEqual(["Lessons"]);
    expect(buildTree([]).count).toBe(0);
  });

  it("findFolder finds a folder by path, case ignored; the root for '', null for none", () => {
    expect(findFolder(tree, "")).toBe(tree);
    expect(findFolder(tree, "lessons/SUB")).toBe(findFolder(tree, "Lessons/Sub"));
    expect(findFolder(tree, "Lessons/Sub")!.name).toBe("Sub");
    expect(findFolder(tree, "Lessons/Nope")).toBeNull();
    expect(findFolder(tree, "Nope")).toBeNull();
  });
  it("notesIn lists every note in a folder and the folders inside it", () => {
    expect(paths(notesIn(findFolder(tree, "Lessons")!)).sort()).toEqual(["Lessons/A note", "Lessons/Sub/Deep", "Lessons/b note", "lessons/sub/Deeper"]);
    expect(notesIn(tree)).toHaveLength(notes.length);
    expect(notesIn(findFolder(tree, "Projects")!)).toEqual([]);
  });
});

describe("foldersTo, moveTarget and rebase", () => {
  it("foldersTo lists the folders from the top down", () => {
    expect(foldersTo("Agents/Backend/Sub")).toEqual(["Agents", "Agents/Backend", "Agents/Backend/Sub"]);
    expect(foldersTo("Lessons")).toEqual(["Lessons"]);
    expect(foldersTo("")).toEqual([]);
    expect(foldersTo("/Lessons//Sub/")).toEqual(["Lessons", "Lessons/Sub"]);
  });
  it("moveTarget: a note lands in the folder under its own title, at the top with no folder", () => {
    expect(moveTarget({ kind: "note", path: "Lessons/X" }, "Decisions")).toBe("Decisions/X");
    expect(moveTarget({ kind: "note", path: "Lessons/X" }, "Decisions/Sub")).toBe("Decisions/Sub/X");
    expect(moveTarget({ kind: "note", path: "Lessons/Sub/X" }, "")).toBe("X");
  });
  it("moveTarget: a move that changes nothing is null (case ignored)", () => {
    expect(moveTarget({ kind: "note", path: "Lessons/X" }, "Lessons")).toBeNull();
    expect(moveTarget({ kind: "note", path: "Lessons/X" }, "lessons")).toBeNull();
    expect(moveTarget({ kind: "folder", path: "Lessons/Sub" }, "Lessons")).toBeNull();
    expect(moveTarget({ kind: "note", path: "X" }, "")).toBeNull();
  });
  it("moveTarget: a folder can't go into itself or a folder inside it", () => {
    const sub = { kind: "folder" as const, path: "Lessons/Sub" };
    expect(moveTarget(sub, "Lessons/Sub")).toBeNull();
    expect(moveTarget(sub, "Lessons/Sub/Deep")).toBeNull();
    expect(moveTarget(sub, "lessons/sub/deep/deeper")).toBeNull();
    expect(moveTarget(sub, "Lessons/Subway")).toBe("Lessons/Subway/Sub");
    expect(moveTarget(sub, "Decisions")).toBe("Decisions/Sub");
    expect(moveTarget(sub, "")).toBe("Sub");
  });
  it("moveTarget: a note may go into a folder named like it (only folders can't go into themselves)", () => {
    expect(moveTarget({ kind: "note", path: "Lessons/Sub" }, "Lessons/Sub")).toBe("Lessons/Sub/Sub");
  });
  it("rebase moves what is inside a folder to its new place, case ignored, and leaves the rest", () => {
    expect(rebase("Lessons/Sub/Deep", "Lessons/Sub", "Decisions/Sub")).toBe("Decisions/Sub/Deep");
    expect(rebase("lessons/sub/A/B", "Lessons/Sub", "Decisions/Renamed")).toBe("Decisions/Renamed/A/B");
    expect(rebase("Lessons/Subway/X", "Lessons/Sub", "Y")).toBe("Lessons/Subway/X");
    expect(rebase("Lessons/Sub", "Lessons/Sub", "Y")).toBe("Lessons/Sub");
    expect(rebase("Decisions/X", "Lessons/Sub", "Y")).toBe("Decisions/X");
  });
});

describe("isNoteFolder and isOwnFolder", () => {
  it("a note goes in a shared folder, an agent's folder or the Team Lead's, or a folder inside one", () => {
    for (const f of [...SHARED_FOLDERS, "lessons", "Lessons/Sub", "Team Lead", "team lead/Sub", "Agents/Backend", "agents/backend/Sub"])
      expect(isNoteFolder(f), f).toBe(true);
    for (const f of ["", "Agents", "Agents/", "Other", "Other/Lessons", "/Lessons"]) expect(isNoteFolder(f), f).toBe(false);
  });
  it("only a folder inside a memory folder is the user's own (renamed, moved), never a memory folder itself", () => {
    for (const f of ["Lessons/Sub", "Lessons/Sub/Deep", "Team Lead/Sub", "Agents/Backend/Sub"]) expect(isOwnFolder(f), f).toBe(true);
    for (const f of ["Lessons", "Team Lead", "Agents", "Agents/Backend", "Other/Sub", ""]) expect(isOwnFolder(f), f).toBe(false);
  });
});

describe("titleProblem", () => {
  it("accepts a plain title, spaces around it ignored", () => {
    expect(titleProblem("Rust style")).toBeNull();
    expect(titleProblem("  Rust style  ")).toBeNull();
    expect(titleProblem("Use SQLite (v3), not Postgres!")).toBeNull();
  });
  it("asks for a name for an empty one, . or ..", () => {
    for (const t of ["", "   ", ".", "..", " .. "]) expect(titleProblem(t), JSON.stringify(t)).toBe("Give it a name.");
  });
  it("names each character a title can't hold: * \" \\ / < > : | ? # ^ [ ]", () => {
    for (const ch of ["*", '"', "\\", "/", "<", ">", ":", "|", "?", "#", "^", "[", "]"]) {
      const p = titleProblem(`a${ch}b`);
      expect(p, ch).not.toBeNull();
      expect(p!.startsWith(`A name can't hold ${ch}:`), p!).toBe(true);
    }
  });
});

describe("agentFolder: an agent's name as its folder's name (gizai_core::memory::folder_name)", () => {
  it("keeps a plain name", () => {
    expect(agentFolder("Backend Agent")).toBe("Backend Agent");
    expect(agentFolder("  QA  ")).toBe("QA");
  });
  it("makes what a name can't hold a dash", () => {
    expect(agentFolder("QA/Tester")).toBe("QA-Tester");
    expect(agentFolder("a:b*c?d")).toBe("a-b-c-d");
    expect(agentFolder("[Lead] #1")).toBe("-Lead- -1");
    expect(agentFolder("Tab\tName")).toBe("Tab-Name");
    expect(agentFolder("Del\u007fName")).toBe("Del-Name");
  });
  it("makes every control character a dash, as Rust's char::is_control does (also U+0080 to U+009F)", () => {
    expect(agentFolder("Bot\u0085One")).toBe("Bot-One");
    expect(agentFolder("Bot\u009bOne")).toBe("Bot-One");
  });
  it("drops dots at either end; 'Agent' when nothing is left", () => {
    expect(agentFolder("..hidden..")).toBe("hidden");
    expect(agentFolder(" . a . ")).toBe("a");
    expect(agentFolder("")).toBe("Agent");
    expect(agentFolder("...")).toBe("Agent");
    expect(agentFolder("   ")).toBe("Agent");
  });
});

// ---- Which notes a page shows ---------------------------------------------------------------------------------------

describe("scopes: which notes a Memory page shows", () => {
  const backend = memoryScope("a1", "Backend");
  const notes = [
    note("Team Lead/Notes", "", { ownerId: "lead" }),
    note("Agents/Backend/Notes", "", { ownerId: "a1" }),
    note("Agents/Backend/Sub/X", "", { ownerId: "a1" }),
    note("Agents/Backend Two/Notes", "", { ownerId: "a2" }),
    note("Lessons/X"),
    note("Clients/Kade"),
  ];

  it("memoryScope: no scope is everything, 'shared' the shared folders, else an agent by id with its folder", () => {
    expect(memoryScope(undefined)).toEqual({ kind: "all" });
    expect(memoryScope("")).toEqual({ kind: "all" });
    expect(memoryScope("shared")).toEqual({ kind: "shared" });
    expect(backend).toEqual({ kind: "agent", agentId: "a1", folder: "Agents/Backend" });
    expect(memoryScope("a3", "QA/Tester")).toEqual({ kind: "agent", agentId: "a3", folder: "Agents/QA-Tester" });
    expect(memoryScope("a3", null)).toEqual({ kind: "agent", agentId: "a3", folder: "Agents/Agent" });
  });
  it("inScope: everything for all, the shared notes for shared", () => {
    expect(notes.every((n) => inScope(n, { kind: "all" }))).toBe(true);
    expect(paths(notes.filter((n) => inScope(n, { kind: "shared" })))).toEqual(["Lessons/X", "Clients/Kade"]);
  });
  it("inScope: an agent's own notes, by owner or by its folder (case ignored), not a folder that only starts like it", () => {
    expect(paths(notes.filter((n) => inScope(n, backend)))).toEqual(["Agents/Backend/Notes", "Agents/Backend/Sub/X"]);
    expect(inScope(note("agents/backend/Old"), backend)).toBe(true);
    expect(inScope(note("Agents/Backend Two/X"), backend)).toBe(false);
    expect(inScope(note("Agents/Renamed/X", "", { ownerId: "a1" }), backend)).toBe(true);
    expect(inScope(note("Lessons/Y", "", { ownerId: "a2" }), backend)).toBe(false);
  });
  it("scopeRoots: the folders a scope always shows", () => {
    expect(scopeRoots({ kind: "all" })).toEqual([LEAD, ...SHARED_FOLDERS, AGENTS]);
    expect(scopeRoots({ kind: "shared" })).toEqual([...SHARED_FOLDERS]);
    expect(scopeRoots(backend)).toEqual(["Agents/Backend"]);
    scopeRoots({ kind: "shared" }).push("X");
    expect(SHARED_FOLDERS).toHaveLength(8);
  });
  it("leadOf: the lead that answers in Chat, else the first lead, else null", () => {
    const a = { name: "a", isLead: false, chatEnabled: true };
    const b = { name: "b", isLead: true, chatEnabled: false };
    const c = { name: "c", isLead: true, chatEnabled: true };
    const d = { name: "d", isLead: true, chatEnabled: false };
    expect(leadOf([a, b, c])).toBe(c);
    expect(leadOf([a, b, d])).toBe(b);
    expect(leadOf([a])).toBeNull();
    expect(leadOf([])).toBeNull();
  });
  it("noteCount: the Team Lead counts every note, another agent only its own folder's, shared the shared ones", () => {
    expect(noteCount(notes, memoryScope(undefined))).toBe(6);
    expect(noteCount(notes, backend)).toBe(2);
    expect(noteCount(notes, memoryScope("a2", "Backend Two"))).toBe(1);
    expect(noteCount(notes, memoryScope("a9", "Nobody"))).toBe(0);
    expect(noteCount(notes, memoryScope("shared"))).toBe(2);
  });
});

// ---- Wikilinks ------------------------------------------------------------------------------------------------------

describe("wikilinks", () => {
  const spans = (body: string, local = false) => wikilinks(body, local).map((l) => body.slice(l.start, l.end));

  it("finds a plain link, with where it is", () => {
    expect(wikilinks("See [[Note]].")).toEqual([{ target: "Note", embed: false, start: 4, end: 12 }]);
  });
  it("splits Folder/Note, #heading and |alias", () => {
    expect(wikilinks("[[Folder/Note|text]]")).toEqual([{ target: "Folder/Note", alias: "text", embed: false, start: 0, end: 20 }]);
    expect(wikilinks("[[Note#Heading]]")).toEqual([{ target: "Note", heading: "Heading", embed: false, start: 0, end: 16 }]);
    expect(wikilinks("[[ Note # Why | the reason ]]")).toEqual([{ target: "Note", heading: "Why", alias: "the reason", embed: false, start: 0, end: 29 }]);
  });
  it("marks an embed, its start on the !", () => {
    const body = "x ![[Pic]] y";
    expect(wikilinks(body)).toEqual([{ target: "Pic", embed: true, start: 2, end: 10 }]);
    expect(spans(body)).toEqual(["![[Pic]]"]);
    expect(wikilinks("![[Pic#Part]]")).toEqual([{ target: "Pic", heading: "Part", embed: true, start: 0, end: 13 }]);
  });
  it("takes .md and slashes at either end off the note part", () => {
    expect(wikilinks("[[Note.md]]")[0]!.target).toBe("Note");
    expect(wikilinks("[[ /Lessons/Note.md |x]]")[0]!.target).toBe("Lessons/Note");
    expect(wikilinks("[[/Lessons/Note/]]")[0]!.target).toBe("Lessons/Note");
  });
  it("leaves out a link to a heading of the same note, unless local", () => {
    expect(wikilinks("[[#Heading]]")).toEqual([]);
    expect(wikilinks("[[#Heading|there]]", true)).toEqual([{ target: "", heading: "Heading", alias: "there", embed: false, start: 0, end: 18 }]);
    expect(wikilinks("[[]] [[ ]] [[#]]", true)).toEqual([]);
  });
  it("finds every link in order", () => {
    expect(spans("[[A]] and [[B|b]], ![[C]]")).toEqual(["[[A]]", "[[B|b]]", "![[C]]"]);
  });
  it("ignores a link across lines, and the outer one of nested [[", () => {
    expect(spans("[[a\nb]] [[C]]")).toEqual(["[[C]]"]);
    expect(wikilinks("[[a [[B]] c]]").map((l) => l.target)).toEqual(["B"]);
    expect(wikilinks("[[open")).toEqual([]);
  });
});

describe("resolve, linkedNote and linkTarget: the note a link finds", () => {
  const notes = [
    note("Standards/Rust style"),
    note("Lessons/Rust style"),
    note("Agents/Backend/Rust style"),
    note("Decisions/Use SQLite"),
    note("Lessons/X"),
    note("Clients/X"),
    note("Projects/A/B"),
    note("Projects/Z/A/B"),
    note("Lessons/Notes"),
    note("Agents/Q/Lessons/Notes"),
  ];
  const at = (path: string) => notes.findIndex((n) => n.path === path);

  it("finds by title, case ignored", () => {
    expect(resolve(notes, "use sqlite", "")).toBe(at("Decisions/Use SQLite"));
    expect(resolve(notes, " USE SQLITE.md ", "Lessons")).toBe(at("Decisions/Use SQLite"));
  });
  it("of several with the title, prefers the linking note's folder, else the shortest path, then by name", () => {
    expect(resolve(notes, "Rust style", "Standards")).toBe(at("Standards/Rust style"));
    expect(resolve(notes, "rust style", "agents/backend")).toBe(at("Agents/Backend/Rust style"));
    expect(resolve(notes, "Rust style", "Clients")).toBe(at("Lessons/Rust style"));
    expect(resolve(notes, "x", "")).toBe(at("Clients/X"));
    expect(resolve(notes, "x", "Lessons")).toBe(at("Lessons/X"));
  });
  it("finds by path or the end of one, which picks between notes with the same title", () => {
    expect(resolve(notes, "Standards/Rust style", "Lessons")).toBe(at("Standards/Rust style"));
    expect(resolve(notes, "/standards/rust style.md/", "")).toBe(at("Standards/Rust style"));
    expect(resolve(notes, "Backend/Rust style", "")).toBe(at("Agents/Backend/Rust style"));
    expect(resolve(notes, "A/B", "")).toBe(at("Projects/A/B"));
    expect(resolve(notes, "A/B", "Projects/Z/A")).toBe(at("Projects/Z/A/B"));
  });
  it("prefers a whole path over the end of a longer one, also from that one's folder", () => {
    expect(resolve(notes, "Lessons/Notes", "Agents/Q/Lessons")).toBe(at("Lessons/Notes"));
  });
  it("finds nothing (-1) for an unknown title or path, part of a word, or nothing", () => {
    expect(resolve(notes, "Nope", "")).toBe(-1);
    expect(resolve(notes, "ust style", "")).toBe(-1);
    expect(resolve(notes, "ckend/Rust style", "")).toBe(-1);
    expect(resolve(notes, "Other/Rust style", "")).toBe(-1);
    expect(resolve(notes, "", "")).toBe(-1);
    expect(resolve(notes, " / ", "")).toBe(-1);
    expect(resolve([], "Rust style", "")).toBe(-1);
  });
  it("linkedNote finds from the linking note's folder, or with none from the top", () => {
    expect(linkedNote(notes, "Rust style", note("Standards/Other"))!.path).toBe("Standards/Rust style");
    expect(linkedNote(notes, "Rust style", null)!.path).toBe("Lessons/Rust style");
    expect(linkedNote(notes, "Nope", null)).toBeNull();
  });
  it("linkTarget: the title when it finds the note from that folder, else the path", () => {
    const std = notes[at("Standards/Rust style")]!;
    const les = notes[at("Lessons/Rust style")]!;
    expect(linkTarget(notes, les, "Clients")).toBe("Rust style");
    expect(linkTarget(notes, std, "Clients")).toBe("Standards/Rust style");
    expect(linkTarget(notes, std, "Standards")).toBe("Rust style");
    expect(linkTarget(notes, notes[at("Decisions/Use SQLite")]!, "")).toBe("Use SQLite");
  });
  it("newNotePath: the link's path with a folder, else next to the linking note, else the fallback", () => {
    expect(newNotePath("Decisions/Use Postgres", "Lessons")).toBe("Decisions/Use Postgres");
    expect(newNotePath("/Clients/Kade/", "Lessons")).toBe("Clients/Kade");
    expect(newNotePath("New idea", "Decisions")).toBe("Decisions/New idea");
    expect(newNotePath(" New idea ", "")).toBe("Lessons/New idea");
    expect(newNotePath("New idea", "", "Team Lead")).toBe("Team Lead/New idea");
  });
});

describe("TASK_REF: card references like KADE-12", () => {
  const refs = (s: string) => [...s.matchAll(TASK_REF)].map((m) => m[0]);
  it("finds a key, a dash and a number", () => {
    expect(refs("See KADE-12 and GA-3.")).toEqual(["KADE-12", "GA-3"]);
    expect(refs("(KADE-12), A1-5: KADE-12-3")).toEqual(["KADE-12", "A1-5", "KADE-12"]);
  });
  it("not inside a word, nor lower case, nor a key longer than ten", () => {
    expect(refs("xKADE-12")).toEqual([]);
    expect(refs("KADE-12a")).toEqual([]);
    expect(refs("_KADE-12 -KADE-12 KADE-12_ 3KADE-12")).toEqual([]);
    expect(refs("kade-12 Kade-12 KADE- KADE-x")).toEqual([]);
    expect(refs("ABCDEFGHIJ-1 ABCDEFGHIJK-1")).toEqual(["ABCDEFGHIJ-1"]);
  });
});

// ---- Backlinks, unlinked mentions, outgoing -------------------------------------------------------------------------

describe("backlinks", () => {
  const target = note("Lessons/Rust style", "Links to itself: [[Rust style]]");
  const a = note("Lessons/A", "Read [[Rust style]] first.\nMore.");
  const b = note("Standards/B", "See [[Rust style]] here.");
  const stdRust = note("Standards/Rust style", "The other one.");
  const c = note("Clients/C", "x\n![[Lessons/Rust style#Why]] y\nand [[rust style|again]]");
  const d = note("Clients/D", "[[Nope]] [[#Rust style]]");
  const notes = [target, a, b, stdRust, c, d];

  it("lists the links in other notes that find the note, each with its line and where it is", () => {
    const got = backlinks(notes, target);
    expect(got.map((m) => ({ path: m.note.path, line: m.line, link: m.note.bodyMd.slice(m.start, m.end) }))).toEqual([
      { path: "Lessons/A", line: "Read [[Rust style]] first.", link: "[[Rust style]]" },
      { path: "Clients/C", line: "![[Lessons/Rust style#Why]] y", link: "![[Lessons/Rust style#Why]]" },
      { path: "Clients/C", line: "and [[rust style|again]]", link: "[[rust style|again]]" },
    ]);
    expect(got[0]).toMatchObject({ start: 5, end: 19 });
  });
  it("leaves out the note itself and links that find another note with that title", () => {
    expect(backlinks(notes, stdRust).map((m) => m.note.path)).toEqual(["Standards/B"]);
    expect(backlinks(notes, d)).toEqual([]);
  });
});

describe("unlinkedMentions", () => {
  const target = note("Lessons/Rust style", "Rust style is mine.");
  const m1 = note("Clients/M1", "We follow rust style here.\nAlso Rust Style again.");
  const m2 = note("Clients/M2", "trust styles and rust styles and rust_style");
  const m3 = note("Clients/M3", "---\ntitle: Rust style\n---\nbody");
  const m4 = note("Clients/M4", "```\nRust style\n```\n`Rust style` [[Rust style]] [[Other|Rust style]] [Rust style](https://x) <a title=\"Rust style\">");
  const m5 = note("Clients/M5", "(Rust style). éRust style");
  const notes = [target, m1, m2, m3, m4, m5];

  it("finds the title as whole words, case ignored, one entry per place, with its line", () => {
    const got = unlinkedMentions(notes, target);
    expect(got.map((m) => ({ path: m.note.path, line: m.line, said: m.note.bodyMd.slice(m.start, m.end) }))).toEqual([
      { path: "Clients/M1", line: "We follow rust style here.", said: "rust style" },
      { path: "Clients/M1", line: "Also Rust Style again.", said: "Rust Style" },
      { path: "Clients/M5", line: "(Rust style). éRust style", said: "Rust style" },
    ]);
    expect(got[0]).toMatchObject({ start: 10, end: 20 });
  });
  it("not in properties, code, inline code, links or tags, nor in the note itself", () => {
    const got = unlinkedMentions(notes, target).map((m) => m.note.path);
    for (const p of ["Clients/M2", "Clients/M3", "Clients/M4", "Lessons/Rust style"]) expect(got).not.toContain(p);
  });
  it("finds nothing for a one-character title", () => {
    expect(unlinkedMentions([note("Lessons/X"), note("Clients/Y", "X marks the spot")], note("Lessons/X"))).toEqual([]);
  });
  it("linkMention makes the place a link: [[Title]] when it reads as the title, else [[Title|what it says]]", () => {
    const body = "We follow rust style here.";
    expect(linkMention(body, 10, 20, notes, target, "Clients")).toBe("We follow [[Rust style|rust style]] here.");
    expect(linkMention("Use Rust style.", 4, 14, notes, target, "Clients")).toBe("Use [[Rust style]].");
  });
  it("linkMention links by path when the title alone finds another note from that folder", () => {
    const both = [...notes, note("Clients/Rust style")];
    expect(linkMention("Use Rust style.", 4, 14, both, target, "Clients")).toBe("Use [[Lessons/Rust style|Rust style]].");
  });
});

describe("outgoing", () => {
  it("lists each link once with the note it finds, and those that find none with note null", () => {
    const la = note("Lessons/A");
    const lb = note("Lessons/B");
    const from = note("Lessons/X");
    const body = "[[A]] [[a]] [[Lessons/A]] ![[A#Why]] [[Nope]] [[nope|n]] [[B]] [[Other]] [[#Local]]";
    const got = outgoing(body, [la, lb, from], from);
    expect(got.map((o) => [o.link.target, o.note?.path ?? null])).toEqual([["A", "Lessons/A"], ["Nope", null], ["B", "Lessons/B"], ["Other", null]]);
    expect(got[0]!.link).toMatchObject({ start: 0, end: 5, embed: false });
  });
});

// ---- Properties, tags, outline --------------------------------------------------------------------------------------

describe("frontmatterBlock", () => {
  it("finds the block between --- lines at the start, and where it ends", () => {
    const body = "---\na: 1\nb: 2\n---\nbody";
    const fm = frontmatterBlock(body)!;
    expect(fm.lines).toEqual(["a: 1", "b: 2"]);
    expect(body.slice(fm.end)).toBe("body");
    expect(fm.end).toBe(18);
  });
  it("takes ... as the closing line, trailing spaces, an empty block and a block at the end", () => {
    const dots = "---\na: 1\n...\nbody";
    expect(dots.slice(frontmatterBlock(dots)!.end)).toBe("body");
    const spaced = "---  \na: 1\n---  \nbody";
    expect(spaced.slice(frontmatterBlock(spaced)!.end)).toBe("body");
    const empty = "---\n---\nx";
    expect(frontmatterBlock(empty)).toEqual({ end: 8, lines: [] });
    const last = "---\na: 1\n---";
    expect(frontmatterBlock(last)).toEqual({ end: last.length, lines: ["a: 1"] });
  });
  it("is null when the note doesn't start with one or it isn't closed", () => {
    for (const b of ["", "# T\n---\na\n---", "---\na: 1\nno end", " ---\na\n---", "--- x\na\n---"]) expect(frontmatterBlock(b), b).toBeNull();
  });
});

describe("properties and property", () => {
  const body = [
    "---",
    "Type: decision",
    "tags: [rust, \"style\", 'x', ]",
    "aliases:",
    "  - First alias",
    "  - \"Second\"",
    "client: \"[[Kade]]\"",
    "project: [[KADE|Kade portal]]",
    "empty:",
    "  nested: ignored",
    "---",
    "# Title",
    "key: not a property",
  ].join("\n");

  it("reads keys in lower case, in order: inline lists, - item lists, quotes and [[ ]] taken off", () => {
    expect(properties(body)).toEqual([
      ["type", ["decision"]],
      ["tags", ["rust", "style", "x"]],
      ["aliases", ["First alias", "Second"]],
      ["client", ["Kade"]],
      ["project", ["KADE"]],
      ["empty", []],
    ]);
  });
  it("lets a later line of a key replace the earlier one, where the first was", () => {
    expect(properties("---\ntags: a\nother: 1\nTags: b\n---\n")).toEqual([["tags", ["b"]], ["other", ["1"]]]);
  });
  it("is empty without frontmatter; [] reads as no values", () => {
    expect(properties("# T\ntype: note")).toEqual([]);
    expect(properties("---\ntags: []\n---\n")).toEqual([["tags", []]]);
  });
  it("property: one key's values, the key's case ignored, [] when it isn't there", () => {
    expect(property(body, "TYPE")).toEqual(["decision"]);
    expect(property(body, "aliases")).toEqual(["First alias", "Second"]);
    expect(property(body, "missing")).toEqual([]);
    expect(property(body, "key")).toEqual([]);
  });
});

describe("propertyLine", () => {
  it("writes key: value, or key: [a, b] for a list property or several values", () => {
    expect(propertyLine("type", ["decision"])).toBe("type: decision");
    expect(propertyLine("tags", [])).toBe("tags: []");
    expect(propertyLine("tags", ["rust"])).toBe("tags: [rust]");
    expect(propertyLine("Aliases", ["One"])).toBe("Aliases: [One]");
    expect(propertyLine("applies_to", ["backend", "qa"])).toBe("applies_to: [backend, qa]");
    expect(propertyLine("client", ["a", "b"])).toBe("client: [a, b]");
    expect(propertyLine("project", [])).toBe("project:");
    expect(propertyLine("project", [" ", ""])).toBe("project:");
    expect(propertyLine("updated", ["2026-10-10"])).toBe("updated: 2026-10-10");
  });
  it("quotes a value YAML would read otherwise, and that quoted value reads back", () => {
    for (const v of ["a: b", "#12", "- y", "[[Kade]]", "x #y", "'q'"]) {
      const line = propertyLine("title", [v]);
      expect(line, v).toBe(`title: ${JSON.stringify(v)}`);
    }
    expect(property(`---\n${propertyLine("title", ["a: b"])}\n---\n`, "title")).toEqual(["a: b"]);
    expect(property(`---\n${propertyLine("client", ["[[Kade]]"])}\n---\n`, "client")).toEqual(["Kade"]);
  });
});

describe("setProperty", () => {
  const body = "---\ntype: note\ntags: [a]\n---\n# T\n\nbody: not a property\n---\n";

  it("rewrites the property's line where it is, keeping how its key is written", () => {
    expect(setProperty(body, "tags", ["a", "b"])).toBe("---\ntype: note\ntags: [a, b]\n---\n# T\n\nbody: not a property\n---\n");
    expect(setProperty(body, "Type", ["decision"])).toBe("---\ntype: decision\ntags: [a]\n---\n# T\n\nbody: not a property\n---\n");
    expect(setProperty("---\nTags: [a]\n---\nx", "tags", ["b"])).toBe("---\nTags: [b]\n---\nx");
  });
  it("adds a new property at the end of the frontmatter", () => {
    expect(setProperty(body, "source", ["GA-19"])).toBe("---\ntype: note\ntags: [a]\nsource: GA-19\n---\n# T\n\nbody: not a property\n---\n");
  });
  it("takes a property out with null, and leaves a note without it as it was", () => {
    expect(setProperty(body, "tags", null)).toBe("---\ntype: note\n---\n# T\n\nbody: not a property\n---\n");
    expect(setProperty(body, "missing", null)).toBe(body);
    expect(setProperty("---\na: 1\n---\nx", "a", null)).toBe("---\n---\nx");
  });
  it("replaces a - item list under the key, indented or not", () => {
    const list = "---\naliases:\n  - One\n  - Two\ntype: note\n---\nx";
    expect(setProperty(list, "aliases", ["Three"])).toBe("---\naliases: [Three]\ntype: note\n---\nx");
    expect(setProperty(list, "aliases", null)).toBe("---\ntype: note\n---\nx");
    expect(setProperty("---\naliases:\n- One\n-\ntype: note\n---\nx", "aliases", ["Three"])).toBe("---\naliases: [Three]\ntype: note\n---\nx");
  });
  it("starts a frontmatter in a note without one; null there changes nothing", () => {
    expect(setProperty("# T\n", "type", ["note"])).toBe("---\ntype: note\n---\n# T\n");
    expect(setProperty("# T\n", "type", null)).toBe("# T\n");
    expect(setProperty("", "tags", ["x"])).toBe("---\ntags: [x]\n---\n");
  });
  it("leaves the text after the frontmatter alone, and does nothing for an empty key", () => {
    const after = setProperty(body, "updated", ["2026-10-10"]);
    expect(withoutFrontmatter(after)).toBe(withoutFrontmatter(body));
    expect(after.endsWith("# T\n\nbody: not a property\n---\n")).toBe(true);
    expect(setProperty(body, "  ", ["x"])).toBe(body);
  });
  it("reads back what it wrote", () => {
    const md = setProperty(setProperty(body, "aliases", ["Rusty", "Style guide"]), "title", ["a: b"]);
    expect(property(md, "aliases")).toEqual(["Rusty", "Style guide"]);
    expect(property(md, "title")).toEqual(["a: b"]);
    expect(property(md, "tags")).toEqual(["a"]);
  });
});

describe("tags, tagCounts and hasTag", () => {
  it("takes the tags property and the #tags in the text, lower case, without #, each once", () => {
    const body = [
      "---",
      "tags: [Rust, '#Style', rust]",
      "---",
      "# Heading",
      "## Sub",
      "We use #rust and #Backend/API, see issue #123.",
      "Not a#b nor C#.",
      "(#paren) and",
      "#start-here",
    ].join("\n");
    expect(tags(body)).toEqual(["rust", "style", "backend/api", "paren", "start-here"]);
  });
  it("finds none in a heading, a number or a note without any", () => {
    expect(tags("# Title\n## Why\nIssue #42")).toEqual([]);
    expect(tags("")).toEqual([]);
  });
  it("tagCounts: how many notes have each tag, most used first, then by name", () => {
    const notes = [note("Lessons/A", "#rust #style #rust"), note("Lessons/B", "#rust #api"), note("Lessons/C", "#zeta #api #rust")];
    expect(tagCounts(notes)).toEqual([{ tag: "rust", count: 3 }, { tag: "api", count: 2 }, { tag: "style", count: 1 }, { tag: "zeta", count: 1 }]);
    expect(tagCounts([])).toEqual([]);
  });
  it("hasTag: the tag itself or one under it, case ignored", () => {
    expect(hasTag("#rust/style", "rust")).toBe(true);
    expect(hasTag("#rust/style", "RUST/Style")).toBe(true);
    expect(hasTag("---\ntags: [Rust]\n---\n", "rust")).toBe(true);
    expect(hasTag("#rustacean", "rust")).toBe(false);
    expect(hasTag("#rust", "rust/style")).toBe(false);
    expect(hasTag("no tags", "rust")).toBe(false);
  });
});

describe("outline, section and withoutFrontmatter", () => {
  const body = [
    "---",
    "title: x",
    "# not a heading",
    "---",
    "# Title",
    "Text",
    "## Why ##",
    "```js",
    "# not a heading either",
    "```",
    "### How",
    "~~~",
    "## nope",
    "~~~",
    "#nospace",
    "####### seven",
    "## ",
    "## Last",
  ].join("\n");

  it("lists the headings with level, text, where the line starts and its number, skipping frontmatter and code", () => {
    expect(outline(body)).toEqual([
      { level: 1, text: "Title", pos: body.indexOf("# Title"), line: 5 },
      { level: 2, text: "Why", pos: body.indexOf("## Why"), line: 7 },
      { level: 3, text: "How", pos: body.indexOf("### How"), line: 11 },
      { level: 2, text: "Last", pos: body.indexOf("## Last"), line: 18 },
    ]);
  });
  it("keeps a # inside a heading's text, and drops only the closing #s", () => {
    expect(outline("# C# tips\n## Title #\n# Tagged #tag").map((h) => h.text)).toEqual(["C# tips", "Title", "Tagged #tag"]);
  });
  it("section: the heading and what follows up to the next heading of its level or higher, case ignored", () => {
    const md = "# A\nintro\n## B\nb text\n### C\nc\n## D\nd\n# E\ne\n\n";
    expect(section(md, "b")).toBe("## B\nb text\n### C\nc");
    expect(section(md, "A")).toBe("# A\nintro\n## B\nb text\n### C\nc\n## D\nd");
    expect(section(md, " c ")).toBe("### C\nc");
    expect(section(md, "E")).toBe("# E\ne");
    expect(section(md, "nope")).toBeNull();
  });
  it("withoutFrontmatter: the text after the frontmatter, without the blank lines it left", () => {
    expect(withoutFrontmatter("---\na: 1\n---\n\n# T\nx")).toBe("# T\nx");
    expect(withoutFrontmatter("# T\nx")).toBe("# T\nx");
    expect(withoutFrontmatter("---\na: 1\n---\n")).toBe("");
  });
});

// ---- New note's templates -------------------------------------------------------------------------------------------

describe("New note: types and templates", () => {
  const DAY = "2026-10-10";
  const KEYS: Record<NoteType, string[]> = {
    note: ["type", "tags", "updated"],
    decision: ["type", "tags", "project", "updated", "source"],
    lesson: ["type", "tags", "applies_to", "updated", "source"],
    standard: ["type", "tags", "applies_to", "updated"],
    workflow: ["type", "tags", "applies_to", "updated"],
    client: ["type", "tags", "client", "updated"],
    project: ["type", "tags", "project", "client", "updated"],
    deployment: ["type", "tags", "project", "updated"],
    dependency: ["type", "tags", "project", "updated"],
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

  it("has the nine types gizai_core::memory::TYPES allows, each with a name, a hint and a folder", () => {
    expect([...NOTE_TYPES].sort()).toEqual(["client", "decision", "dependency", "deployment", "lesson", "note", "project", "standard", "workflow"]);
    for (const t of NOTE_TYPES) {
      expect(TYPE_NAME[t], t).toBeTruthy();
      expect(TYPE_HINT[t], t).toBeTruthy();
    }
  });
  it("puts each type but note in its own shared folder; note has none", () => {
    expect(TYPE_FOLDER.note).toBeNull();
    const folders = NOTE_TYPES.filter((t) => t !== "note").map((t) => TYPE_FOLDER[t]);
    for (const f of folders) expect((SHARED_FOLDERS as readonly (string | null)[]).includes(f), String(f)).toBe(true);
    expect(new Set(folders).size).toBe(folders.length);
    expect([...folders].sort()).toEqual([...SHARED_FOLDERS].sort());
  });
  for (const type of NOTE_TYPES) {
    it(`starts a ${type} with its properties, its title and its headings`, () => {
      const md = noteTemplate(type, "  Acme  ", DAY);
      expect(md.startsWith(`---\ntype: ${type}\n`)).toBe(true);
      const fm = frontmatterBlock(md);
      expect(fm).not.toBeNull();
      expect(property(md, "type")).toEqual([type]);
      expect(property(md, "updated")).toEqual([DAY]);
      expect(property(md, "tags")).toEqual([]);
      expect(tags(md)).toEqual([]);
      // The keys it wrote read back as the same keys, in order.
      expect(fm!.lines.map((l) => l.slice(0, l.indexOf(":")))).toEqual(KEYS[type]);
      expect(properties(md).map(([k]) => k)).toEqual(KEYS[type]);
      expect(withoutFrontmatter(md).startsWith("# Acme\n")).toBe(true);
      expect(md.split("\n")).toContain("# Acme");
      expect(outline(md).map((h) => [h.level, h.text])).toEqual([[1, "Acme"], ...HEADINGS[type].map((h) => [2, h])]);
      for (const h of HEADINGS[type]) expect(md.split("\n"), h).toContain(`## ${h}`);
    });
  }
  it("gives a client and a project their own title as a property", () => {
    expect(property(noteTemplate("client", " Kade ", DAY), "client")).toEqual(["Kade"]);
    const project = noteTemplate("project", "Kade portal", DAY);
    expect(property(project, "project")).toEqual(["Kade portal"]);
    expect(property(project, "client")).toEqual([]);
  });
  it("makes a standard and a workflow apply to all; a lesson to no role yet", () => {
    expect(property(noteTemplate("standard", "Rust style", DAY), "applies_to")).toEqual(["all"]);
    expect(property(noteTemplate("workflow", "Release", DAY), "applies_to")).toEqual(["all"]);
    expect(property(noteTemplate("lesson", "Flaky test", DAY), "applies_to")).toEqual([]);
  });
  it("writes a decision and a note exactly so", () => {
    expect(noteTemplate("decision", "Use SQLite", DAY)).toBe(
      "---\ntype: decision\ntags: []\nproject:\nupdated: 2026-10-10\nsource:\n---\n# Use SQLite\n\n## Decision\n\n## Why\n\n## Alternatives considered\n",
    );
    expect(noteTemplate("note", "Scratch", DAY)).toBe("---\ntype: note\ntags: []\nupdated: 2026-10-10\n---\n# Scratch\n\n");
  });
  it("today: the date as YYYY-MM-DD in local time", () => {
    expect(today(new Date(2026, 0, 5, 0, 0, 1))).toBe("2026-01-05");
    expect(today(new Date(2026, 11, 31, 23, 59, 59))).toBe("2026-12-31");
    expect(today()).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

// ---- The [[ autocomplete --------------------------------------------------------------------------------------------

describe("findWikiTrigger: the [[… before the cursor", () => {
  it("is null outside a [[, after its ]] or across a line", () => {
    expect(findWikiTrigger("no link here")).toBeNull();
    expect(findWikiTrigger("[[Done]] after")).toBeNull();
    expect(findWikiTrigger("[[a\nb")).toBeNull();
    expect(findWikiTrigger("[[a[b")).toBeNull();
    expect(findWikiTrigger(`[[${"x".repeat(121)}`)).toBeNull();
  });
  it("is a note's name right after [[, from where it starts (plus the offset)", () => {
    expect(findWikiTrigger("see [[")).toEqual({ from: 6, query: "", part: "note", note: "", embed: false });
    expect(findWikiTrigger("see [[Rust st")).toEqual({ from: 6, query: "Rust st", part: "note", note: "", embed: false });
    expect(findWikiTrigger("see [[Rust st", 100)).toEqual({ from: 106, query: "Rust st", part: "note", note: "", embed: false });
    expect(findWikiTrigger("[[A]] [[B")).toEqual({ from: 8, query: "B", part: "note", note: "", embed: false });
    expect(findWikiTrigger("[[a [[b")).toEqual({ from: 6, query: "b", part: "note", note: "", embed: false });
  });
  it("is a heading after #, of the note before it", () => {
    expect(findWikiTrigger("[[Rust style#Wh")).toEqual({ from: 13, query: "Wh", part: "heading", note: "Rust style", embed: false });
    expect(findWikiTrigger("[[#Wh", 10)).toEqual({ from: 13, query: "Wh", part: "heading", note: "", embed: false });
  });
  it("is an alias after |, of the note before it (its #heading left off)", () => {
    expect(findWikiTrigger("[[Rust style#Why|te")).toEqual({ from: 17, query: "te", part: "alias", note: "Rust style", embed: false });
    expect(findWikiTrigger("[[ Rust style |")).toEqual({ from: 15, query: "", part: "alias", note: "Rust style", embed: false });
  });
  it("knows an embed by its ![[", () => {
    expect(findWikiTrigger("x ![[Pi")).toEqual({ from: 5, query: "Pi", part: "note", note: "", embed: true });
  });
});

describe("wikiRows: what the [[ picker lists", () => {
  const cur = note("Lessons/Current", "# Current\n## Local heading\n## Other");
  const std = note("Standards/Rust style", "---\naliases: [Rusty, Style guide]\n---\n# Rust style\n## Why\n## Examples\n");
  const les = note("Lessons/Rust style", "# Rust style\n## Lesson heading\n");
  const trust = note("Decisions/Trust the compiler");
  const rusty = note("Projects/Rusty project/Notes");
  const leadNotes = note("Team Lead/Notes");
  const cafe = note("Clients/Café Kade");
  const notes = [std, les, trust, rusty, leadNotes, cafe, cur];
  const rows = (before: string, current: MemoryNote | null = cur) => wikiRows(findWikiTrigger(before)!, notes, current);
  const shown = (rs: WikiRow[]) => rs.map((r) => (r.type === "note" ? `${r.note.path} => ${r.insert}` : `${r.type}:${r.text} => ${r.insert}`));

  it("ranks a title that starts with the first word, then a title that has it, then a path; shorter paths first", () => {
    expect(shown(rows("[[rust"))).toEqual([
      "Lessons/Rust style => Rust style",
      "Standards/Rust style => Standards/Rust style",
      "Decisions/Trust the compiler => Trust the compiler",
      "Projects/Rusty project/Notes => Projects/Rusty project/Notes",
    ]);
  });
  it("needs every word, accents and case ignored", () => {
    expect(shown(rows("[[rust styl"))).toEqual(["Lessons/Rust style => Rust style", "Standards/Rust style => Standards/Rust style"]);
    expect(shown(rows("[[CAFE k"))).toEqual(["Clients/Café Kade => Café Kade"]);
  });
  it("lists every other note by path length when nothing is typed, never the current note", () => {
    expect(rows("[[").map((r) => (r.type === "note" ? r.note.path : ""))).toEqual([
      "Team Lead/Notes", "Clients/Café Kade", "Lessons/Rust style", "Standards/Rust style", "Decisions/Trust the compiler", "Projects/Rusty project/Notes",
    ]);
    expect(rows("[[cur")).toEqual([]);
    expect(rows("[[cur", null).map((r) => (r.type === "note" ? r.note.path : ""))).toEqual(["Lessons/Current"]);
  });
  it("inserts the title, or the path when the title finds another note from the current one's folder", () => {
    const fromStd = wikiRows(findWikiTrigger("[[rust style")!, notes, note("Standards/Other"));
    expect(shown(fromStd)).toEqual(["Lessons/Rust style => Lessons/Rust style", "Standards/Rust style => Rust style"]);
  });
  it("lists at most WIKI_ROWS notes", () => {
    const many = Array.from({ length: WIKI_ROWS + 10 }, (_, i) => note(`Lessons/Note ${i}`));
    expect(wikiRows(findWikiTrigger("[[note")!, many, null)).toHaveLength(WIKI_ROWS);
  });
  it("after #: the headings of the note the link finds, else the current note's", () => {
    expect(rows("[[Rust style#")).toEqual([
      { type: "heading", text: "Rust style", level: 1, insert: "Rust style" },
      { type: "heading", text: "Lesson heading", level: 2, insert: "Lesson heading" },
    ]);
    expect(rows("[[Standards/Rust style#ex")).toEqual([{ type: "heading", text: "Examples", level: 2, insert: "Examples" }]);
    expect(rows("[[#loc")).toEqual([{ type: "heading", text: "Local heading", level: 2, insert: "Local heading" }]);
    expect(rows("[[#", null)).toEqual([]);
    expect(rows("[[Nope#")).toEqual([]);
  });
  it("after |: the note's aliases", () => {
    expect(rows("[[Standards/Rust style|")).toEqual([
      { type: "alias", text: "Rusty", insert: "Rusty" },
      { type: "alias", text: "Style guide", insert: "Style guide" },
    ]);
    expect(rows("[[Standards/Rust style#Why|gui")).toEqual([{ type: "alias", text: "Style guide", insert: "Style guide" }]);
    expect(rows("[[Rust style|")).toEqual([]);
  });
});

// ---- Search ---------------------------------------------------------------------------------------------------------

describe("search: words, highlights and an agent's scope", () => {
  const marked = (parts: { text: string; hit: boolean }[]) => parts.map((p) => (p.hit ? `[${p.text}]` : p.text)).join("").replace(/\]\[/g, "");

  it("searchWords: words and \"phrases\" in lower case, path: and tag: left out", () => {
    expect(searchWords('Rust "Error handling" path:Standards tag:rust path:"Team Lead" STYLE')).toEqual(["rust", "error handling", "style"]);
    expect(searchWords("Tag:x PATH:y z")).toEqual(["z"]);
    expect(searchWords('"unclosed phrase')).toEqual(["unclosed phrase"]);
    expect(searchWords("")).toEqual([]);
    expect(searchWords('""   ')).toEqual([]);
  });
  it("highlight marks every place a word is, case ignored, keeping the text as it was", () => {
    const parts = highlight("Rust style guide, rust", ["rust", "STYLE"]);
    expect(marked(parts)).toBe("[Rust] [style] guide, [rust]");
    expect(parts.map((p) => p.text).join("")).toBe("Rust style guide, rust");
  });
  it("highlight joins overlapping and adjacent marks and drops one inside another", () => {
    const overlap = highlight("rust style", ["rust st", "style"]);
    expect(marked(overlap)).toBe("[rust style]");
    expect(overlap.map((p) => p.text).join("")).toBe("rust style");
    expect(marked(highlight("abcd", ["ab", "cd"]))).toBe("[abcd]");
    expect(marked(highlight("rustacean", ["rustacean", "st"]))).toBe("[rustacean]");
    expect(marked(highlight("aaaa", ["aa"]))).toBe("[aaaa]");
  });
  it("highlight with nothing to mark or no text", () => {
    expect(highlight("", ["x"])).toEqual([]);
    expect(highlight("", [])).toEqual([]);
    expect(highlight("text", [])).toEqual([{ text: "text", hit: false }]);
    expect(highlight("text", [""])).toEqual([{ text: "text", hit: false }]);
    expect(highlight("text", ["zz"])).toEqual([{ text: "text", hit: false }]);
  });
  it("highlight with searchWords marks the words, not a path:", () => {
    expect(marked(highlight("Team Lead notes", searchWords('path:"Team Lead" notes')))).toBe("Team Lead [notes]");
  });
  it("scopedQuery: an agent's page searches only its folder; the others as typed", () => {
    expect(scopedQuery("rust", { kind: "all" })).toBe("rust");
    expect(scopedQuery("rust", { kind: "shared" })).toBe("rust");
    const q = scopedQuery("rust", memoryScope("a1", "Backend"));
    expect(q).toBe('path:"Agents/Backend/" rust');
    expect(searchWords(q)).toEqual(["rust"]);
  });
});
