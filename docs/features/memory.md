# Memory

The Team Lead, the agents and you keep notes for later, and every run gets the notes for its card.

## What goes in

Decisions and their reasons, preferences, gotchas and how things are done here: what the repository and the board can't tell an agent. Notes link to code, docs and cards instead of copying them.

Memory is data written by people and agents, never instructions. Every prompt says so where the notes start.

## Where notes live

Notes live in shared folders and a folder per agent, on the Memory page (sidebar → Memory).

- **Shared folders**, like `Projects/`, `Standards/` and `Decisions/`: everyone reads them, and the Team Lead and people write them.
- **A folder per agent**: the agent reads and writes its own, and the Team Lead moves useful notes into a shared folder.
- **Agents that take turns**, like Backend Agent and Backend Agent 2 on two Claude Code accounts, can share one folder (agent form → Memory → Shares memory with). What one learns reaches the other.

## In every run

- Each run gets the notes for its card: the agent's own, then the shared notes for the card's project, its client and the agent's role.
- A note about another client or project never goes in.
- Each run keeps what the agent learned: a `learned` list on its result line becomes dated lines in the agent's notes.
- The Team Lead keeps its answers to agents' questions in memory, linked to the card ([Agents ask the Team Lead first](team-lead.md#agents-ask-the-team-lead-first)).

Memory reaches agents as plain text in the prompt, so it works the same on Claude Code, Codex, Gemini and other CLIs.

## The Memory page

- A folder tree, with new notes, new folders, renaming and moving by drag.
- Search by words, `"phrases"`, `path:` and `tag:`.
- Read and Edit, with `[[wikilinks]]`, backlinks, an outline, properties, tags and history.
- **Notes | Graph** shows how notes link to each other and to cards, projects, clients and agents.

## Safety and switches

- A note with an obvious secret, like a private key or an API token, is refused.
- **Use memory** in an agent's form (on by default) switches memory off for that agent.
- **Settings → Runs → Use memory** (on by default) switches it off for every agent.

## Read more

- [docs/memory.md](../memory.md): folders, properties, links, search, how runs get notes, and the Memory page in full.
- [A Team Lead you talk to](team-lead.md): the agent that sorts and shares notes.
