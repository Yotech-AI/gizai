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
notes whose path starts with it, and with a `/` at the end only that folder's (`path:"Agents/QA/"` leaves out
`Agents/QA 2/`); `tag:rust` keeps the notes with that tag (the `tags` property or a `#tag` in the text).
Title matches come first.

### Safety

A save with an obvious secret in it is refused, with what to do instead (write where the secret is kept): a private key
block (`-----BEGIN … PRIVATE KEY-----`) or an API token (`sk-…`, `ghp_…` and the other GitHub tokens, `AKIA…`, `xoxb-…`,
`AIza…`, `glpat-…`). A `learned` line with a secret is left out and the run's log says so.

## The Team Lead

The Team Lead's tools: `memory_list`, `memory_search`, `memory_read`, `memory_write` (a whole note, with the version it
read; a new path makes the note), `memory_append` (under a heading, without rewriting the note) and `memory_move` (move
or copy, rewriting links). `memory_read`, `memory_append` and `memory_move` find a note by its path, its title (as a
wikilink does) or its id. It sees every scope, and the activity feed shows it as the author. Once a chat answer has used
a tool from outside Gizai (another MCP server, the web, the browser), `memory_write`, `memory_append` and `memory_move`
are refused for the rest of that answer, like its other tools that act: it proposes the note, and saves it once you
confirm in a new message.

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

## Agents ask the Team Lead first (GA-70)

When a task agent ends its run with `needs_decision`, the Team Lead looks at the question before you
(`gizai_core::questions`, the app's `ask_lead`):

1. **Who gets it.** The Team Lead takes it when the step is on (Settings → Runs → Ask the Team Lead first, on by default),
   agents aren't paused in Settings, the Team Lead (the agent with Chat on) is active, runs on a Claude Code CLI and is
   under its monthly budget, the asking agent isn't the Team Lead and its CLI can continue a session, and the limits allow
   it: one try per question, two per card, and the question right after a Team Lead answer goes to you (no loops; the
   board check leaves such a question to you as well, and the Runs tab says which limit sent it). Otherwise, and for a
   `run_for_me` request, a gate's hold (QA bounces, an answer the role can't give) or a failed push,
   the card goes to the Inbox as before.
2. **Meanwhile.** The card stays on hold `needs_decision` but is "with the Team Lead": not in the Inbox (`tasks::needs_you`
   and the UI's `needsYou`), no notification, and the board check leaves it out. The card's Hold row says With the Team
   Lead.
3. **The Team Lead's run on the question** (trigger `question`, role lead, no card of its own, so the card's latest run
   stays the agent's): a fresh session with the question, the card, its description, acceptance criteria and last
   comments, and its Memory block. It only reads: memory, the card and the project's docs through the gizai tools (the
   tools that change things are refused), and its copies of the code with Read, Glob and Grep. Its rules: always leave
   money, scope, deadlines, messages to clients, security, deleting and anything it can't find in memory or on the card to
   you. It ends with a result line:

   ```
   GIZAI_RESULT: {"outcome":"answered","answer":"…","memory":{"path":"Standards/Exports","text":"…"}}
   GIZAI_RESULT: {"outcome":"escalated","reason":"…","options":["…","…"],"advice":"…"}
   ```

4. **Answered:** the answer is kept in memory as the Team Lead's, with the card's identifier (so it links to the card): as
   a dated line in the shared note the result names, else in `Decisions/<project name>` (made with `type: decision` and
   `project:`, so the agents on that project's cards get it). Then the agent's session continues with the answer as the
   Team Lead's Continue note (GA-31), which is also its comment on the card. A start that only has to wait (Runs at once
   full, the agent paused or over its budget) is tried again every 30 seconds for ten minutes, with the card still with
   the Team Lead.
5. **Escalated**, and also a failed or timed-out run, one without its result line, or an answer the agent can't be
   continued with: the Team Lead's comment "Needs <you>: why", the options and its advice, and the hold's reason becomes
   "Team Lead escalated to you: why", in the Inbox (it notifies as a new hold). A question still with the Team Lead when
   Gizai stops goes to you at the next start. A later Continue doesn't count the Team Lead's comment as an answer. When the
   Team Lead can't run here at all (no gizai-mcp helper, its CLI not found), the question goes to you as before, without
   a comment, and the Runs tab says why.
6. **Your answer.** When the agent starts again on a card whose question the Team Lead escalated, the comments people
   wrote since are kept in `Decisions/<project name>` as the Team Lead's, with the card, once per question.

The question's record is kept under `lead` in the asking run's `outcome_json` (state `asking`, `answering`, `answered`,
`escalated`, `dropped` when you took the card over first, `skipped`, or `limit`), and the Team Lead's run is stored with trigger `approval` and
read as `question`: no new columns. The Runs tab shows on the asking run who answered, the answer or the reason, and what
the Team Lead's look cost; that cost counts toward the Team Lead's budget. The Team Lead's agent page lists its runs on
questions with their cards.

## Switches

- **Use memory** in an agent's form (on by default): off, its runs get no Memory section and its `learned` lines are not
  kept (for the Team Lead: no notes in chat and board checks either).
- **Settings → Runs → Use memory** (on by default): off for every agent. The Team Lead's memory tools still work.
- **Settings → Runs → Ask the Team Lead first** (on by default): off, an agent's question puts its card in the Inbox at once.

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

The Memory page (GA-68), the graph (GA-69), an export to a folder for Obsidian, deleting notes, a tool for task agents to
ask in the middle of a run, semantic search, a review pass.
Notes an agent kept in its CLI's own memory (like Claude Code's memory folder) are not moved over.
