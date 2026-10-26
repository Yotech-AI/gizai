# Team templates: format 1

A team template is a team's setup in one file: its columns, its agents (their settings, instructions and allowed commands), the labels and the organisation chart's branches (GA-226). You save a team as a template, share the file, and start a new team from it, or from a built-in template. This page describes the file as format 1, field by field, and what Gizai does when it saves one and when it makes a team from one. A community page may come later and will use the same file.

The code is `gizai_core::templates`; the app's commands are in `src-tauri/src/commands.rs`, and the screens (GA-227) call them through `src/api.ts`.

## The file

A template is a JSON file in UTF-8, named `<team name>.gizai-team.json`, like `Software team.gizai-team.json`. Gizai writes it indented, with its fields in the order below. It reads a file of at most 2 MB, with or without a byte order mark.

**Never in the file:** people (you, your reviewers), an agent's folders, MCP server settings (commands, addresses, environment and header values, keys and sign-ins), a CLI's id, program or account, the agents' status, clients, projects, cards, memory notes, runs and spend. Archived agents are left out; paused ones go in as they are.

```json
{
  "format": "gizai-team",
  "format_version": 1,
  "app_version": "0.13.0",
  "name": "Software team",
  "description_md": null,
  "color": null,
  "branches": [{ "key": "dev", "name": "Development", "roles": ["frontend", "backend", "fullstack", "mobile"] }],
  "labels": [{ "name": "frontend", "color": "#7b9bff" }],
  "columns": [
    { "name": "Backlog", "kind": "backlog", "auto": false, "next": null, "wip_limit": null, "color": null, "agents": [] },
    { "name": "To do", "kind": "ready", "auto": true, "next": "In progress", "wip_limit": null, "color": null, "agents": ["Backend Agent", "Frontend Agent"] }
  ],
  "agents": [{
    "name": "Backend Agent", "role": "backend", "title": null,
    "cli": "claude_code", "model": null, "effort": null, "permission_mode": "acceptEdits",
    "instructions_md": "You are the Backend Agent …", "allowed_tools": ["Bash(git status:*)"],
    "cards_at_once": 1, "monthly_budget_usd": null, "chat": false, "board_check_minutes": null,
    "use_memory": true, "shares_memory_with": null,
    "web_search": false, "web_fetch": false, "fetch_domains": [], "builtin_tools": [], "slash_commands": false,
    "browser": false, "insecure_certs": false, "screenshots": false,
    "mcp_servers": [{ "name": "figma", "tools_off": [] }]
  }]
}
```

Names link the file together: a column's `next` and `agents`, and an agent's `shares_memory_with`, are names within the file. Names are compared ignoring case and the spaces around them.

### The team

| Field | What it holds |
|---|---|
| `format` | always `"gizai-team"` |
| `format_version` | `1`. A Gizai that reads a higher one refuses the file: "This template needs a newer version of the app." |
| `app_version` | the Gizai version that wrote it, like `"0.13.0"` |
| `name` | the team's name |
| `description_md` | the team's description, or `null` |
| `color` | the team's colour, or `null` |
| `branches` | the organisation chart's branches, in order: each a `key` (its role key, like `dev`), a `name` and the `roles` whose agents it holds. Left out: the default four (Design, Development, Quality, Operations). |
| `labels` | every label, with its `color` (a hex colour like `#7b9bff`, or `null`). Labels belong to the whole organisation, not to one team. |
| `columns` | the board's columns, in board order (below) |
| `agents` | the team's agents (below) |

### A column

| Field | What it holds |
|---|---|
| `name` | its name, unique in the file |
| `kind` | its category, one of `backlog`, `ready` (like To do), `in_progress`, `testing`, `review`, `deploy`, `done` and `cancelled` (`team::CATEGORIES`) |
| `auto` | `true`: the agents on it pick up its cards by themselves. `false`: Manual, only Run starts one. |
| `next` | the name of the column its cards go to next, or `null` |
| `wip_limit` | at most this many cards in it, or `null` |
| `color` | its colour, or `null` |
| `agents` | the names of the agents on it, in order. Backlog, Review, Done and Cancelled columns take none. |

### An agent

| Field | What it holds |
|---|---|
| `name` | its name, unique in the file |
| `role` | its role key: `lead`, `frontend`, `backend`, `design`, `qa`, `devops` or another |
| `title` | its title, or `null` |
| `cli` | the kind of CLI it runs on: `claude_code`, `codex`, `gemini` or `other`. Never the CLI's id, program or account. |
| `model` | its model, or `null` for the CLI's default |
| `effort` | its effort level (Claude Code's `--effort`, Codex's reasoning effort), or `null` |
| `permission_mode` | its permission mode, like `acceptEdits` (Claude Code), `workspace-write` (Codex) or `auto_edit` (Gemini); `""` for an Other CLI |
| `instructions_md` | its instructions, at most 100,000 characters |
| `allowed_tools` | its allowed commands, like `Bash(git status:*)` |
| `cards_at_once` | the cards it works on at once, 1 to 10 |
| `monthly_budget_usd` | its monthly budget in dollars (`25` or `25.5`), or `null` for none |
| `chat` | it answers on the Chat page (the Team Lead) |
| `board_check_minutes` | its board check runs every this many minutes (5 to 1440), or `null` for off |
| `use_memory` | its runs get a Memory section and its learned lines are kept |
| `shares_memory_with` | the name of the agent whose memory folder it shares, or `null` for its own folder |
| `web_search`, `web_fetch` | it may search the web, and fetch web pages |
| `fetch_domains` | it fetches pages only from these domains, like `docs.rs`; none: any page |
| `builtin_tools` | the CLI's other tools switched on for it, by name |
| `slash_commands` | slash commands and skills (Claude Code only) |
| `browser` | the built-in browser (Chrome DevTools MCP) is on |
| `insecure_certs` | the browser accepts self-signed and expired certificates |
| `screenshots` | it takes screenshots of what it built (with the browser on). Left out: its role's default (on for frontend, design and qa). |
| `mcp_servers` | the MCP servers switched on for it, by their `name` in Settings → MCP, with the tools switched off of each (`tools_off`). Never their settings. The built-in browser isn't here: it is `browser`. |

The browser's switched-off tools aren't in format 1: an agent made from a template has all of the browser's tools.

## Saving a team

Team → Export saves the team in format 1 to the path the person picks (`save_team_template`). Before that, `check_team_export` lists each place in the file that looks private, with the agent it is in (or none, for the team's own fields), the field and what it found:

| Kind | What is found |
|---|---|
| `email` | an email address, like `jef@example.com` (not a git remote's `git@github.com:owner/repo`) |
| `path` | a local path: `/home/…`, `/Users/…`, `C:\…` (any drive) or `~/…` |
| `url` | a URL with a query string (`https://x.com/a?token=1`), or with a user and password (`https://me:secret@host`, `postgres://me:secret@localhost/db`) |
| `secret` | anything that looks like a key or token: `sk-…`, `ghp_…` (and `gho_`, `ghu_`, `ghs_`, `ghr_`), `github_pat_…`, `glpat-…`, `xox…-…`, `AKIA…`, a private key's `-----BEGIN … PRIVATE KEY-----`, or a run of 32 or more hex characters, or of base64 characters that looks random (capitals, small letters and digits, mixed) |

Gizai looks in the team's name, description, branches, labels and column names, and in each agent's name, title, model, instructions, allowed commands, fetch domains, built-in tools and MCP servers. Export never changes the text to hide what it finds: the person decides.

## Built-in templates

`team_templates` lists them. For now there is one:

| Key | Name | Description |
|---|---|---|
| `software` | Software team | The five agents a new install gets, and the seven usual columns. |

It is made when it is asked for, from the seed (`seed::DEFAULT_COLUMNS`, `DEFAULT_LABELS`, `DEFAULT_AGENTS`, `role_template` and `role_tools`), with each agent on the columns a new agent of its role lands on (`columns::role_columns`, which `place_new_agent` uses too). So it never drifts from a new install's first team. Its agents run on Claude Code with acceptEdits.

## Reading a template: the check

`preview_team_template` reads a template (a built-in's key, the path of a file, or the file's text) and says what an import would make. It writes nothing.

It refuses a template a team can't be made from, saying why:

- the file can't be read, isn't JSON (in UTF-8), or its `format` isn't `gizai-team`;
- its `format_version` is above 1: "This template needs a newer version of the app."
- the file is over 2 MB, it has no name or no columns, more than 50 agents or 30 columns, or an agent's instructions are over 100,000 characters;
- a column's `kind` is unknown, or a column or agent has no name;
- two columns or two agents have the same name (ignoring case);
- a `next` or a column's agent isn't in the file, or a column goes next to itself;
- a backlog, review, done or cancelled column has agents or is Auto;
- an Auto column has no `next`.

Otherwise its plan (`TemplatePlan`) says what the new team gets:

- **The team's name** is the file's, with a number when a team has it already: "Software team 2".
- **The agents' names** are the file's, except where an agent has the name already (ignoring case; agents of every team and archived ones count, as their memory folders do): that agent gets the team's name in brackets, "Backend Agent (Software team)", with a number when that is taken too. `fileName` keeps the file's name.
- **The CLI:** each agent goes on the first CLI of its kind in Settings → Coding CLIs. When there is none, it goes on the built-in Claude Code with acceptEdits and no model or effort, noted: "No Codex CLI is set up: Backend Agent runs on Claude Code."
- **The permission mode** is kept when that CLI offers it (`clis::permission_modes`), else it is the CLI's first mode, noted. Claude Code's `bypassPermissions`, Codex's `danger-full-access` and Gemini's `yolo` are never taken: they become the CLI's first mode, noted.
- **The effort** is kept when the CLI offers it, else empty.
- **The allowed commands** are kept. `extraTools` lists the ones that aren't in the role's default list (`seed::role_tools`).
- **MCP servers** are matched by name to Settings → MCP, ignoring case. A match is switched on for the agent, with its tools switched off as the file has them. One that isn't there is in `missingServers` and stays off. An agent that doesn't run on Claude Code gets no MCP servers and no browser, noted, as the agent form allows them on Claude Code only.
- **Chat:** at most one agent has it, on Claude Code. When an agent already has Chat, the file's Chat agent comes without Chat and without the board check, noted: "Your Team Lead stays Master Chief." The same goes, with a note, for a second Chat agent in the file and for one that doesn't run on Claude Code.
- **Shares memory with:** a name that isn't in the file gives the agent its own folder, noted. So do a Team Lead on either side (it keeps its notes in Team Lead/) and a circle. A chain ends at its first agent with a folder of its own.
- **Labels** that aren't there yet (ignoring case) are added and listed in `labelsAdded`; the ones there keep their colour. A label of over 40 characters isn't added, and one without a hex colour is added without one, noted.
- **Active:** agents from a file come in paused (`active` false); a built-in's are active.
- Other settings out of range are noted and set: cards at once to 1 to 10, a board check outside 5 to 1440 minutes off, a negative budget none, a fetch domain or tool name that can't be used left out (with no fetch domain left, fetching pages is off).

## Making a team from a template

`import_team_template` makes what the plan says, with the name passed in, in one write: a failure leaves nothing behind.

- The team: its name, description, colour and branches, with you as its reviewer, as a new team gets (`team::add_team`).
- The columns in order, with their kind, Auto, next column, WIP limit and colour.
- The labels the plan adds.
- The agents, each made as `team::add_agent` makes one (its actor, settings, team membership and memory folder), on the file's columns in the file's order. A shared memory folder is set once all the agents are there.

It returns the new team's id. Existing teams, agents, labels' colours and the Chat agent stay as they are. Nothing starts by itself: the new team has no cards until a project uses it (GA-225).

## Newer formats

A Gizai reads format 1 and refuses a file whose `format_version` is higher. Within format 1, a field Gizai doesn't know is passed over, and a missing field takes its default (`false`, `null` or none; `cli` `claude_code`, `cards_at_once` 1, `use_memory` true, the default branches). So a later Gizai can add a field to format 1 that this one passes over, and a hand-written file needs only the fields it sets, besides these: `format`, `format_version` and `name`, each column's `name` and `kind`, each agent's `name` and `role`, each label's and MCP server's `name`, and each branch's `key`, `name` and `roles`.
