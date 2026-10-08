# Gizai Chat and the Team Lead: design

Date: 2026-10-06 (overnight work). Branch: `v0.3-chat` (from `v0.2-ui`).
Status: Jeffrey asked for this to be researched and built overnight ("a feature you can research and work on this night"), so the usual review gates are replaced by his standing rule: decide, record, keep going. Everything that is only his to decide is listed at the end.

## 1. What Jeffrey asked for

His words, condensed:

1. A **Chat** page, in the sidebar under WORK, above Tasks.
2. To chat, an agent must be set up **with Chat as its designation**: the **Agents Team Lead**. Without one, the Chat page says something like "To start chatting, make the Agents Team Lead". Its button goes to the agents page with the agent setup open and the Chat tool on.
3. On the agents page, an **org chart** like Paperclip's (CEO → CMO/CTO/COO → engineers), but for software development: one Team Lead, then dev teams, a design team, QA and so on.
4. The chat has **tools**: create and edit agents, clients, projects and tasks; add files; write docs; read the inbox when asked. "Like Chief on this Linux install": someone who prefers chat over the Kanban board and the inbox can run everything from the chat.

What I assume (not said):

- One chat agent per organisation. The Team Lead is the agent with Chat turned on.
- Chats are **threads**, as in Claude Desktop, so old conversations stay readable.
- The Team Lead is a manager, not a coder. In chat it uses Gizai's tools and may *read* the projects' code (its own copies, §5a), but it doesn't edit files or run commands. Code work goes to the developer agents as tasks.
- Changes the Team Lead makes are attributed to it in every activity feed ("Team Lead created KADE-14").

Success looks like:

- A fresh install shows the Chat entry. Chat explains how to set up the Team Lead, and one click opens the agent form with everything prefilled.
- "Add client Kade Logistics in Rotterdam and a project Kade portal with three tasks" in the chat creates exactly that. The chat shows a card per change, each linking to the new item, and the rest of the app updates live.
- "What needs me?" lists the inbox.
- The Team page shows the team as an org chart. Empty places in the usual software team are dashed placeholders, one click away from being filled.

## 2. Approaches considered

**A. Tools over MCP, served by the app over a local socket, reached through a tiny stdio shim** (chosen). Claude Code starts the shim (`gizai-mcp`) from a per-turn MCP config. The shim pipes bytes to a user-only unix socket where the running Gizai serves MCP. This is the research recommendation (RESEARCH.md §F, notes/F-agents-runs.md §4):

- The app stays the only writer of the database.
- Every write goes through the same core functions and the same "rows changed" events as the UI, so screens refresh by themselves.
- No TCP port and no web server.

**B. An MCP server that opens the SQLite file itself.** Simpler, but it bypasses the app:

- no live refresh (the app would have to poll);
- two writers;
- no access to the run manager (starting or stopping agent runs).

The research already rejected it.

**C. A `gizai` command-line tool the agent calls through Bash.** It needs Bash permission and shell-quoting of untrusted text. It has no tool schemas, and Claude would see shell commands instead of typed tools. Kept for later as a human/script client.

**MCP library:** hand-rolled JSON-RPC (`initialize`, `ping`, `tools/list`, `tools/call`) instead of the `rmcp` SDK.

- Why: four methods, no new dependencies, every line tested here, and full control of error text.
- Version handling: the server answers with the protocol version the client asks for. These methods have not changed since the first spec version.
- If we later need resources, prompts or elicitation, switch to `rmcp` (3.5, MIT/Apache).

## 3. Architecture

```
Chat page ──invoke──► chat.rs (src-tauri) ──spawn──► claude -p --resume … --mcp-config turn.json
    ▲                     │  ▲                                  │ stdio
    │ chat-event          │  │ stream-json (deltas, tools)      ▼
    └─────────────────────┘  └──────────────────────────  gizai-mcp (shim) ──► unix socket
                                                                              │
                         tools.rs (src-tauri) ◄── gizai-mcp::serve ◄──────────┘
                              │ gizai-core (clients, projects, tasks, team, docs, files, chat)
                              ▼
                           SQLite (single writer) ──► rows-changed ──► every open screen
```

New and changed units:

| Unit | Where | Job |
|---|---|---|
| `gizai-mcp` (lib) | `crates/gizai-mcp/src/lib.rs` | MCP over newline-delimited JSON-RPC: `serve(reader, writer, handler)`, `ToolDef`, the hello/token line. No Tauri, no gizai-core. |
| `gizai-mcp` (bin) | `crates/gizai-mcp/src/bin/gizai-mcp.rs` | The shim: connects to `$GIZAI_SOCKET`, sends `{"token": $GIZAI_TOKEN}`, then pipes stdin→socket and socket→stdout. std only. |
| chat stream parser | `crates/gizai-agents/src/chat_stream.rs` | stream-json → `ChatEvent` (text deltas, finished text blocks, tool calls with id and input, tool results, init, result). |
| process | `crates/gizai-agents/src/process.rs` | `spawn` becomes generic over the event type (task runs keep `RunEvent`). |
| claude argv | `crates/gizai-agents/src/claude.rs` | New optional flags: `--resume`, `--mcp-config` + strict, `--include-partial-messages`, `--restricted`, `--tools`, `--permission-prompts none`, `--add-dir`, `--no-session-persistence`. |
| chat store | `crates/gizai-core/src/chat.rs` | Threads, messages, chat runs, per-thread cost bookkeeping. |
| tokens | `crates/gizai-core/src/tokens.rs` | Mint, verify and revoke per-turn tokens (`api_tokens`, sha256 only). |
| inbox | `crates/gizai-core/src/tasks.rs` | `needs_you(db, you)`, the same rule as the UI's `needsYou`. |
| team | `crates/gizai-core/src/team.rs` | `chat_enabled` on agents (at most one), `chat_agent(db)`, roles `design` and `devops` with templates. |
| socket server | `src-tauri/src/mcp.rs` | Binds the socket and checks each connection's token. Serves `tools.rs` with the token's agent as the actor. |
| tools | `src-tauri/src/tools/` | The Gizai tool catalog, about 34 tools (§6). Each one resolves names (KADE-12, "Kade portal", "Backend Agent"), calls gizai-core, notifies the UI and wakes on-assign agents. |
| chat turns | `src-tauri/src/chat.rs` | Starts a turn (token, MCP config, argv), follows the stream, saves messages and tool cards, records the run, stops on request. |
| UI | `src/pages/ChatPage.tsx`, `src/components/chat/*`, `src/components/OrgChart.tsx`, `src/lib/chat.ts`, `src/lib/org.ts` | Chat page, setup state, org chart; pure helpers with tests. |

## 4. Data

Migration `0003_chat.sql`:

- `agent_configs.chat_enabled INTEGER NOT NULL DEFAULT 0`. Turning it on for one agent turns it off for every other agent in the same write, so there is never more than one chat agent. The UI says so ("Chat moves from X to this agent").
- `chat_threads.cost_usd_micros INTEGER NOT NULL DEFAULT 0`: the Claude session's cumulative cost after its last turn. From Claude Code 2.1.277 on (installed: 2.1.289), a resumed session reports **cumulative** cost and tokens, so a turn's own cost is `reported − previous`. The tokens get the same treatment through `chat_threads.input_tokens` and `output_tokens`.

The tables `chat_threads`, `chat_messages`, `api_tokens` and `runs.chat_thread_id` (trigger `chat`) already exist in 0001 and are used as designed:

- **One user message is one turn, and one turn is one `runs` row**: trigger `chat`, `task_id` NULL, `chat_thread_id` set, role `lead`. Chat spend therefore counts towards the agent's monthly budget, and the agent page shows chat turns in its stats and recent runs.
- **`chat_messages` rows are written as they happen:**
  - `user`: what Jeffrey typed;
  - `agent`: one row per finished text block;
  - `tool`: one row per tool call. `tool_name` holds the tool's name; `tool_json` holds `{input, result, isError}`, and the result is filled in when it arrives;
  - `system`: errors, "stopped", "interrupted".
- **The thread title** is the first message, cut at 60 characters.

## 5. A chat turn

1. **`send_chat(thread_id?, text)`**, checked in this order:
   - a chat agent exists, else: "Set up the Team Lead first";
   - it is active, else: "Team Lead is paused";
   - it is under its monthly budget;
   - Claude Code is found;
   - no other turn is running in this thread.

   It then creates the thread if needed, saves the user message, creates the run row and returns the thread id at once.
2. **Token:** 256 random bits from `/dev/urandom`, stored as its sha256 in `api_tokens`, scoped to `{"chat": thread, "run": run}`. It expires after 30 minutes and is revoked when the turn ends.
3. **MCP config:** `data_dir/chat/<run>.mcp.json`, mode 0600, deleted after the turn. Contents: `{"mcpServers":{"gizai":{"type":"stdio","command":"<gizai-mcp>","args":[],"env":{"GIZAI_SOCKET":…,"GIZAI_TOKEN":…}}}}`.
4. **argv** (prompt on stdin, as for task runs):

   ```
   -p --output-format stream-json --verbose --include-partial-messages
   --session-id <new uuid> | --resume <thread session>
   --restricted --tools Read,Glob,Grep --permission-mode manual --permission-prompts none
   --mcp-config <file> --strict-mcp-config --allowedTools mcp__gizai
   --append-system-prompt <Gizai chat prompt + the agent's instructions>
   [--model m] [--max-budget-usd n] [--add-dir <data dir>/code/<KEY> for each copy (§5a)]
   ```

   What these flags do:
   - **`--restricted`** (Claude Code 2.1.289) ignores user, project and local settings files. Jeffrey's plugins and hooks (superpowers, a SessionStart hook) therefore don't load in the chat. It also removes command-running tools and confines file tools to the working directories.
   - **`--tools`** leaves only read tools.
   - **`--permission-prompts none`** refuses anything that would ask.
   - **`--allowedTools mcp__gizai`** lets every Gizai tool run without asking.
   - **Working directory:** `data_dir/lead/` (its own folder; created on first use).
5. **Streaming:**
   - text deltas go to the UI as `chat-event {threadId, kind: "delta", text}` (not saved);
   - each finished text block becomes an `agent` message;
   - each `tool_use` becomes a `tool` message;
   - its `tool_result` completes that message.

   Every saved message emits `rows-changed: chat_messages`. Start and end emit `chat-changed`, so the sidebar and the page can show "working".
6. **End:**
   - The run row finishes as `succeeded`, `failed`, `cancelled` or `timed_out`, with the turn's own cost and tokens. The thread stores the new session id and the cumulative totals.
   - A failure adds a `system` message with the reason: Claude Code's error subtype, or the last stderr lines.
   - Caps per turn: 15 minutes and 60 tool calls.
7. **Resume fails** (the session file is gone; Claude Code exits before its `init` line): run the turn once more with a new session. The prompt then includes the last 20 messages of the thread as context, so the conversation carries on.
8. **Stop:** the Stop button sends SIGINT to the turn's process group, with the same escalation as task runs.

   **Quitting Gizai:** live turns stop like runs.

   **Start-up:** turns left running are marked interrupted by the existing recovery. Their claude process group is ended only when /proc shows it still leads its group and works in the lead folder.

## 5a. The Team Lead's copies of the code (GA-44)

The linked folders (`repo_path`) are Jeffrey's own checkouts. They are often on a feature branch and behind main, so the Team Lead doesn't read them. It reads its own copies instead.

- **What:** for every active project with a git repository, a detached worktree (`git worktree add --detach`, no branch) at `<data dir>/code/<KEY>`. It sits at the commit a new card of the project starts from: main fetched from the remote that matches the project's GitHub link (else from the link itself), or the local default branch when there is no link. One helper, `git::start_point`, picks that commit for card starts and copies alike. A copy shares the repository's objects and holds only tracked files: no `vendor/`, `node_modules/`, `target/` or `.env`. It is not under `worktrees/` (the card worktree features don't see it) and not under `lead/` (Claude Code would load its `CLAUDE.md`).
- **When:** before each turn, for all copies in parallel, one refresh per project at a time:
  - fetch, unless the project was fetched (or a fetch was tried) less than a minute ago;
  - if the commit moved, move the copy there and throw away anything changed in it; if it didn't, touch nothing.

  The turn waits at most 10 seconds. A slow or failed refresh leaves the last good copy and finishes in the background. At start-up the missing copies are made in the background.
- **Chat:** `--add-dir` gets the copies, never the linked folders. The system prompt names each copy's folder (`GA: <data dir>/code/GA`) and stays the same from turn to turn. The turn's prompt starts with one line in square brackets, which isn't saved as a chat message: the commit and date each copy shows, and which copy couldn't be refreshed and why.
- **Clean-up:** at start-up and before each turn, the copy of a project that is no longer active, has lost its repository or now points to another repository is removed: `git worktree remove --force`, then `git worktree prune`. Nothing outside `<data dir>/code/` is removed.
- **The linked folder:** each refresh also looks at the project's linked folder, locally and without a fetch: its branch, how many commits of main it lacks, and whether `vendor/` (for `composer.lock`) or `node_modules/` (for `package-lock.json`) is missing or its lock file differs from main's. Only dependencies that are behind make the folder outdated. The turn's line then gets a note, on a chat's first turn and again when it changes, that also says what an update would do. The Team Lead asks once, when the project comes up, and updates only after a yes in the chat (`update_checkout`, §6).
- **The update** (`update_checkout`, chat only, the project's linked folder only) changes nothing and says why when the folder has uncommitted changes to tracked files (named), a merge or rebase in progress, is on another branch and the call doesn't ask to switch, its default branch has commits main doesn't, it isn't a checkout of the project's repository, or an update of it is already running. Otherwise it answers "started" at once and, in the background: switches to the default branch when asked (the other branch stays as it is), fast-forwards to main as last fetched (`git merge --ff-only`, without git hooks), and runs `composer install` or `npm ci` where a dependency folder was behind, without prompts and within the preparation's time limit. It never runs the project's setup command. The end is posted in the chat as a system message (old and new commit, any switch, the installs, or why it stopped with the end of the output), and the next turn's line mentions it once. An update and a card's worktree preparation from the same folder wait for each other.

## 6. The Team Lead's tools

All names are served as `mcp__gizai__<name>`. References accept an id or a human name: a task by `KADE-12`, a project by key or name, a client or agent by name. A name that matches nothing, or matches more than one item, returns an error listing the candidates. Every write returns `{ok, …, link: {page, id, label}}`, so the chat can show a card that opens the item.

| Area | Read | Write |
|---|---|---|
| Overview | `get_overview` (counts, columns, inbox size, agents and who is working) | |
| Inbox | `read_inbox` | |
| Clients | `list_clients`, `get_client` (with contacts and projects) | `create_client`, `update_client`, `save_contact` |
| Projects | `list_projects`, `get_project` (with docs and task counts per column) | `create_project` (key suggested from the name when omitted), `update_project`, `update_checkout` (chat only: the linked folder to main, §5a) |
| Tasks | `list_tasks` (project, column, assignee, label, text, done included or not), `get_task` (comments, runs, files) | `create_task`, `update_task` (fields, labels, assignee, hold), `move_task` (by column name), `comment_on_task` |
| Agents | `list_agents`, `get_agent` | `create_agent`, `update_agent`, `set_agent_status`, `add_routing_rule` |
| Runs | | `start_agent_run` (an agent on a task, now), `stop_agent_run` |
| Docs | `list_docs`, `read_doc` | `create_doc`, `write_doc` (a new version) |
| Files | (part of `get_task`/`get_project`/`get_client`) | `attach_file` (a local path → a task, project or client) |
| People | `list_people`, `get_workflow` (columns, labels, rules) | `add_person` |

There is no delete tool in v1. Archiving a client and deleting rules, docs and files stay in the UI (§9).

Updates merge: `update_project` and `update_client` read the current row and change only the fields given. The core `update` functions take the whole row, and leaving a field out would otherwise clear it.

## 7. Interface

**Sidebar:** WORK starts with **Chat** (MessagesSquare icon). While the Team Lead is answering, it shows the teal "working" pulse. Teal still means only "an agent is working now".

**Chat page** (`#/chat`, `#/chat/<thread>`):

- **Top bar:** "Chat" and a Team Lead chip (avatar, name, model, state). Actions: New chat, and Settings (opens the agent drawer).
- **Left:** a 260px thread list, newest first. Each row shows the title and when it was last used, with a live dot on a thread that is working.
- **Centre:** the conversation, at most 760px wide.
  - Your messages sit on the right on a raised surface.
  - The Team Lead's messages sit on the left: avatar, name, time, then Markdown.
  - Tool calls are one compact line with a verb and a link ("Created task · KADE-14 Export invoices", "Read inbox · 3 items"). They expand to show the input and result. Failed calls are red, with the reason.
  - While working: the streamed text with a caret, and a teal "Working…" row naming the tool in use.
  - A new thread shows four suggestions to start from: "What needs my attention?", "Add a client", "Plan a project", "Set up my team".
- **Composer:** sticky at the bottom; Enter sends, Shift+Enter adds a line. Send turns into Stop while working.

**No Team Lead yet:** the Chat page shows one centred panel:

- the heading "Chat with your Team Lead" and two lines on what it can do;
- the button **Set up the Team Lead**. It goes to the Team page and opens the agent drawer prefilled: name "Team Lead", role Lead, Chat on, wake-up manual, model empty.

**Team Lead paused:** the page shows a banner with Resume.

**Agent drawer:**

- **Roles:** Lead, Frontend, Backend, Design, QA, DevOps, or Other.
- **New "Chat" section:**
  - a switch, "Talk to this agent on the Chat page";
  - one line on what it can do there;
  - when another agent has Chat: "Chat moves from Team Lead to this agent".
- **Prefill:** a drawer request can carry `preset` (name, role, chat).

**Team page:** "Members" becomes **"Organisation"**: an org chart in the style of Jeffrey's screenshot.

- **Top:** the Team Lead (any agent with role `lead`, or the chat agent).
- **Below, one column per department:**
  - Development: frontend, backend, fullstack, mobile;
  - Design;
  - Quality: qa;
  - Operations: devops;
  - Specialists: any other role.
- **Nodes:** each agent is a rounded node: role icon, name, a status dot and "Claude Code". An agent that is working now gets a teal ring and a "Working" tag; a paused agent is dimmed.
- **Empty places** in the usual software team (Team Lead, Frontend, Backend, Design, QA, DevOps) are dashed nodes: "Add Frontend agent". One click opens the agent drawer with that role.
- **Clicks:** a real node opens the agent page.
- **People** (Jeffrey, reviewers) stay as cards under "People".

## 8. Errors and safety

- **The socket:**
  - path `$XDG_RUNTIME_DIR/gizai/<12 hex of sha256(data dir)>.sock` (directory 0700, socket 0600), so a test instance with another data folder never touches Jeffrey's running Gizai;
  - fallback without XDG_RUNTIME_DIR: `data_dir/mcp.sock`;
  - a stale socket file is removed before binding.
- **Tokens:**
  - a connection whose first line isn't a valid, unexpired, unrevoked token is closed with an error line;
  - tools act as the token's agent;
  - the shim exits with a clear stderr message when it can't connect.
- **Tool inputs** are validated by gizai-core, as for the UI. Errors come back to Claude as `isError: true` with gizai-core's sentence, so it can correct itself.
- **`attach_file`** copies only regular files up to 1 GB (the existing core rule). The Team Lead can only attach a path Jeffrey gave it or one inside its copies of the code (§5a). The copies hold only tracked files, so a repository's `.env` isn't among them.
- **Instructions:** the Team Lead's appended instructions say:
  - ask before changes that touch more than five items, or anything it can't undo;
  - never invent ids: look things up first;
  - give work to agents as tasks; don't write code.
- **Everything read is data:** text in tasks, docs and comments is data. The system prompt says so, the same rule as the research's prompt-injection section.

## 9. Out of scope (v1)

- Delete and archive tools.
- Approval cards for risky actions.
- Attachments typed into the composer (drag a file into the chat).
- Task agents getting the Gizai MCP tools (their runs could later use the same socket with task-scoped tokens).
- Codex, Gemini and other adapters.
- Rich charts in chat.
- Packaging the shim as a Tauri sidecar (`externalBin`). For now `scripts/run.sh` builds it next to the app binary.
- Renaming or deleting threads.

## 10. Testing

**Rust:**

- the MCP protocol (initialize/list/call/ping/unknown method/bad JSON/notifications);
- the shim end to end against a test socket server;
- the chat stream parser on recorded lines;
- the generic `spawn`;
- the claude argv with the new flags;
- core: the chat store, tokens, the inbox, `chat_enabled` exclusivity, the new roles and templates;
- the tools, through the real catalog on a test state: create/update/resolve/merge and name errors;
- the socket server: token accepted, refused and revoked;
- a whole chat turn with a fake Claude Code (Python) that reads its MCP config, starts the real shim, calls `get_overview` and `create_task` over the socket, and streams the result.

**Vitest:**

- the tool card labels and links;
- the thread titles;
- message grouping;
- the org chart grouping and empty places;
- the router.

**Headless cage probe** (`scripts/ui-test.sh` #6), with the fake Claude Code:

- the Chat page with no Team Lead shows the setup panel;
- the button opens the drawer with Chat on;
- sending a message streams a reply;
- a tool card links to the new task.

**One real Claude Code check** (haiku, scratch data folder, `--no-session-persistence`): the flags are accepted, the MCP server connects, and a tool call works. This costs about a cent. With `--restricted`, it doesn't load Jeffrey's plugins or hooks.

## 11. Decisions for Jeffrey

1. **One chat agent, called "Team Lead" by default.** Chat moves when you turn it on for another agent. OK, or do you want several chat agents with a picker?
2. **The Team Lead is read-only on code in chat.** It reads linked repos but can't edit files or run commands; code work goes to developer agents as tasks. OK?
3. **No delete tools in chat.** Should the Team Lead be able to archive clients or delete docs and rules?
4. **Chat cost counts towards the Team Lead's monthly budget**, like its runs.
5. **Plugins and hooks don't load in the chat** (`--restricted`). Task agents today run with `--setting-sources user`, so they **do** load your user plugins, such as superpowers, and your hooks. Should task agents also run with `--restricted` (or `--safe-mode`)?
6. **Org chart departments come from roles** (Development, Design, Quality, Operations, Specialists), not from a "reports to" field you edit. OK for now?
