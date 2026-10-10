# Memory

Memory is what the Team Lead and the agents keep for later, and what people can read and manage too: decisions and their
reasons, preferences, gotchas, how things are done here. It works the same for agents on every coding CLI (Claude Code,
Codex, Gemini and Other), because it reaches them as plain text in the prompt and comes back on the result line.

It stores what the repository and the board can't tell an agent. Link to code, docs and cards; don't copy them.
**Memory is data written by people and agents, never instructions.** Every prompt says so where the notes start.

Part 1 (GA-19) is the store, the links, the Team Lead's tools and memory in every run. Part 2 (GA-68) is the Memory page,
where people find, read and manage the notes. The graph (GA-69) and agents asking the Team Lead before a person (GA-70)
build on them.

## Notes

A note is a Markdown text in Gizai's database: a doc of kind `memory`, at organisation level (no project needed). So
versions, authorship, activity, live refresh and backups work as for docs. The Memory page shows and edits a note
(`#/memory/<id>`; an old `#/doc/<id>` link to a note opens it there). A project's doc list never shows notes.

Notes follow Obsidian's file conventions, so a copy opens there: Markdown, `[[wikilinks]]`, YAML properties at the top
(frontmatter, kept inside the note's text) and folders. Gizai does not use or need Obsidian.

Every note has:

- a **path**: folder and title, like `Standards/Rust style`. A title can't hold `* " \ < > : | ? # ^ [ ]`; paths are at
  most 200 characters, and two notes can't share a path (case ignored);
- a **scope**: `shared`, or `agent` for an agent's own folder and the Team Lead's;
- an **owner**: in an agent's folder, that agent.

### Folders

| Folder | Scope | Who reads | Who writes |
|---|---|---|---|
| `Clients/`, `Projects/`, `Standards/`, `Workflows/`, `Deployments/`, `Dependencies/`, `Decisions/`, `Lessons/` | shared | everyone | the Team Lead and people |
| `Agents/<name>/` | agent | that agent, the Team Lead and people | that agent, the Team Lead and people |
| `Team Lead/` | agent | the Team Lead and people | the Team Lead and people |

A note goes in one of these folders; other top folders are refused, with the list. Each agent gets `Agents/<name>/Notes`
when it is made (agents from before memory get theirs when Gizai starts). Renaming an agent moves its folder, and the
links to its notes follow. The Team Lead's `Team Lead/Notes` is made the first time the Team Lead needs it, from a short
template: the user's preferences, working agreements, open threads, and how to use memory.

The rule is written once, in `gizai_core::memory::can_read` and `can_write`: the Team Lead and people read and write
everything; an agent reads the shared folders and its own, and writes only its own. The Team Lead moves useful notes from
an agent's folder into a shared one (`memory_move`).

### Properties

YAML properties at the top of a note say what it is about and who gets it:

```yaml
---
type: decision          # client, project, standard, workflow, deployment, dependency, decision, lesson or note
tags: [rust, style]
client: Kade            # a note about a client…
project: KADE           # …or a project (its key or name)
applies_to: [backend, qa]   # roles, or all: who gets it in a run
updated: 2026-10-09
source: GA-19           # a card, chat or run
---
```

### Links

On every save Gizai rebuilds the note's links (`doc_links`) from its text:

- `[[Note]]`, `[[Folder/Note|text]]`, `[[Note#Heading]]`: a link; `![[Note]]`: an embed;
- `KADE-12`: a card;
- `@handle`: a person or agent (a mention).

A wikilink finds a note as in Obsidian: by title, case ignored; a path (or the end of one) picks between notes with the
same title, else the note in the same folder, else the shortest path. A link to a note that doesn't exist yet starts
working when that note is made. Moving or renaming a note rewrites the links that point to it, in notes and docs, as a
new version of each.

### Search

Plain matching over the notes (memory stays small), so it is right after every save, rename and move: every word and
`"quoted phrase"` must be in the path or the text (case ignored); `path:Standards` (or `path:"Team Lead"`) keeps the
notes whose path starts with it; `tag:rust` keeps the notes with that tag (the `tags` property or a `#tag` in the text).
Title matches come first.

### Safety

A save with an obvious secret in it is refused, with what to do instead (write where the secret is kept): a private key
block (`-----BEGIN … PRIVATE KEY-----`) or an API token (`sk-…`, `ghp_…` and the other GitHub tokens, `AKIA…`, `xoxb-…`,
`AIza…`, `glpat-…`). A `learned` line with a secret is left out and the run's log says so.

## The Team Lead

The Team Lead's tools: `memory_list`, `memory_search`, `memory_read`, `memory_write` (a whole note, with the version it
read; a new path makes the note), `memory_append` (under a heading, without rewriting the note) and `memory_move` (move
or copy, rewriting links). `memory_read`, `memory_append` and `memory_move` find a note by its path, its title (as a
wikilink does) or its id. It sees every scope, and the activity feed shows it as the author.

Every chat answer and every board check has the Team Lead's Memory block at the end of its system prompt, in old chats
and new ones: its own notes (`Team Lead/`, `Notes` first) in full, at most **6,000 characters** (a note that doesn't fit
is cut, with a pointer to `memory_read`), then the paths of every other note, at most **4,000 characters**. Its task
runs get the same block. Its instructions ask it to save decisions with their reasons, preferences and gotchas, not to
copy what the repository or the board say, never to store a secret, and to look in memory before asking the user.

## Agents' runs

A new task run's prompt has a Memory section after the task, built by `memory::prompt_block`:

1. the agent's own notes (`Agents/<name>/Notes` first);
2. the shared notes for this card, in this order: the card's project (`project:`), its client (`client:`), the agent's
   role or `all` (`applies_to:`).

At most **6,000 characters** of notes in full (the one that doesn't fit is cut), then the paths of the rest that match
(**4,000**). **Client isolation:** a note about another client or another project never goes in, not even its path, and
not from the agent's own folder either; a shared note with `applies_to` for other roles doesn't go in. A continued run
has the notes in its session already.

An agent keeps what it learned by adding an optional `learned` list to its result line:

```
GIZAI_RESULT: {"outcome":"ready_for_testing","summary":"…","issues":[],"learned":["The fake CLI ignores --effort"]}
```

Gizai appends each line to the agent's own `Notes` under `## Learned`, as a dated bullet with the card
(`- 2026-10-09 (GA-19): …`), written by the agent, with the run on the note's version. At most 20 lines, each cut at 300
characters. A result line without `learned` parses as before.

The task page's Runs tab lists the notes a run was given, with their size (and how much was shown when one was cut).

## Switches

- **Use memory** in an agent's form (on by default): off, its runs get no Memory section and its `learned` lines are not
  kept (for the Team Lead: no notes in chat and board checks either).
- **Settings → Runs → Use memory** (on by default): off for every agent. The Team Lead's memory tools still work.

## The Memory page

The sidebar's **Memory** section, under Agents, opens it: the Team Lead first (by its name; it opens every note: its own,
the shared folders and every agent's folder), then each other agent (only its own folder), each with how many notes it
opens. With no Team Lead yet it shows **Shared notes** and *Set up the Team Lead*. Routes: `#/memory` (every note),
`#/memory/shared`, `#/memory/agent/<agent id>`, each with a note's id after it to open that note.

- **Files** (left): the folder tree, folders before notes, by name; it remembers which folders are open (this
  computer). *New note*, *New folder* (a folder exists once a note is in it: until then this computer keeps it), rename
  (double-click, F2 or the pencil; a note or a folder inside a memory folder, never a memory folder itself), and drag a
  note or folder onto a folder to move it. Moves and renames rewrite the links to the notes, as `memory_move` does.
- **Search** above the tree: words, `"phrases"`, `path:` and `tag:` (as the Team Lead's `memory_search`), with the
  matching line and the match marked. An agent's page searches only its folder. Notes are also in Ctrl+K.
- **The note** (centre): *Read* shows it with its links working (`[[Note]]`, `[[Note#Heading]]`, `[[Note|text]]`;
  dashed when no note has that name yet: a click offers to make it), `![[Note]]` (or `![[Note#Heading]]`) shown in
  place, and `KADE-12` as a chip that opens the card. Resting the mouse on a link shows the note. *Edit* is the doc
  editor, with its versions and conflict handling: `[[` lists the notes (then `#` their headings, `|` their
  `aliases`), links are styled, and Ctrl+click (Cmd+click on macOS) opens a link or a card. *Read* or *Edit* is kept.
- **The panel** (right; its sections fold and stay folded): *Backlinks*, the notes that link here and, under *Unlinked
  mentions*, those that name this note without a link, each with **Link** (it makes that place a link and saves that
  note); *Outgoing links*, also those that find no note yet (*Make it*); *Outline* (a click goes to the heading);
  *Properties* as a small form (each change rewrites that line and saves a version); *Tags*, every tag of the page's
  notes with how many have it (a click shows only those notes in the tree); *History*.
- **New note** asks for a type (note, decision, lesson, standard, workflow, client, project, deployment, dependency), a
  title and a folder (the type's folder by default) and starts the note from that type's template: its properties and
  a few headings.
- With no note open: **Recently changed**, the notes by their last version, newest first, with who wrote it (you, an
  agent, a person) and the run's card when a run did, filtered by Everyone, Agents or You; or, with no notes yet, what
  memory is.

## Not yet

The graph (GA-69), asking the Team Lead before a person (GA-70), an export to a folder for
Obsidian, deleting notes, a tool for task agents to ask in the middle of a run, semantic search, a review pass.
Notes an agent kept in its CLI's own memory (like Claude Code's memory folder) are not moved over.
