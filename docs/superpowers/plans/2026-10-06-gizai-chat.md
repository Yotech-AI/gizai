# Gizai Chat and Team Lead Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Chat page where Jeffrey talks to a Team Lead agent (Claude Code) that runs Gizai through ~34 MCP tools, plus an org chart on the Team page.

**Architecture:** The running app serves MCP (hand-rolled JSON-RPC) on a user-only unix socket; Claude Code reaches it through a tiny stdio shim (`gizai-mcp`) named in a per-turn MCP config. Each chat message is one `claude -p --resume` turn, recorded as a `runs` row (trigger `chat`), streamed to the UI and saved as `chat_messages`. Tools call gizai-core as the Team Lead and fire the same `rows-changed` events as the UI.

**Tech Stack:** Rust 2024 (tokio, rusqlite, serde_json, sha2), Tauri 2.12, React 19 + TS 6, vitest 4, Claude Code 2.1.289, Python 3 (test fake only).

**Spec:** `docs/superpowers/specs/2026-10-06-gizai-chat-design.md`

## Global Constraints

- Source `scripts/env.sh` before cargo/npm (caches stay in the repo). No sudo, no global installs.
- Core logic in `crates/` never imports Tauri; `src-tauri/` adapts; the UI talks only through `src/api.ts`.
- TDD; before claiming done: `cargo test --workspace`, `npm test`, `npm run build`, `scripts/ui-test.sh` (headless cage only).
- Never kill by name; only PIDs/process groups we started. Never run the app on Jeffrey's screen.
- Local git only; branch `v0.3-chat`; commits end with the two attribution lines.
- Design system: dark first, one blue accent; teal `--live` only for "an agent is working now"; magenta `--needs` only for "needs you"; drawers ≥ 1024px.
- Chat turns: `--restricted --tools Read,Glob,Grep --permission-mode manual --permission-prompts none --strict-mcp-config --mcp-config <0600 file> --allowedTools mcp__gizai`; caps 15 min / 60 tool calls.
- Socket: `$XDG_RUNTIME_DIR/gizai/<first 12 hex of sha256(data dir)>.sock` (dir 0700, socket 0600), fallback `data_dir/mcp.sock`.
- At most one agent has `chat_enabled`; turning it on elsewhere turns it off here.
- Copy is plain, sentence case, no exclamation marks.

## Review Focus

1. A chat message sent while the previous turn in that thread is still running must be refused with a clear sentence, not start a second claude on the same session (Task 6 test `a_second_message_while_working_is_refused`).
2. `update_client` / `update_project` given one field must keep every other field (core updates take whole rows) (Task 4 tests `update_client_changes_only_the_given_fields`, `update_project_keeps_repo_and_colour`).
3. A test Gizai (cage, other data dir) must never remove or bind the running app's socket (Task 5 test `socket_path_depends_on_the_data_dir`).
4. Names that match several items (two projects called "Portal") must return the candidates, never pick one silently (Task 4 test `ambiguous_names_list_the_candidates`).
5. A turn whose claude never reaches `init` after `--resume` must retry once with a fresh session carrying recent context, and only once (Task 6 test `a_lost_session_starts_again_with_the_recent_messages`).

---

### Task 1: `gizai-mcp` crate — MCP protocol and the stdio shim

**Files:**
- Create: `crates/gizai-mcp/Cargo.toml`, `crates/gizai-mcp/src/lib.rs`, `crates/gizai-mcp/src/bin/gizai-mcp.rs`
- Create: `crates/gizai-mcp/tests/protocol_test.rs`, `crates/gizai-mcp/tests/shim_test.rs`
- Modify: `Cargo.toml` (workspace member; tokio feature `net`)

**Interfaces:**
- Produces:
  - `pub struct ToolDef { pub name: String, pub description: String, pub input_schema: serde_json::Value, pub read_only: bool }`
  - `pub trait Tools: Send + Sync { fn list(&self) -> Vec<ToolDef>; fn call(&self, name: &str, args: Value) -> impl Future<Output = Result<Value, String>> + Send; }`
  - `pub async fn handle<T: Tools>(tools: &T, msg: &Value) -> Option<Value>` (None for notifications)
  - `pub async fn serve<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin, T: Tools>(reader: R, writer: W, tools: &T) -> std::io::Result<()>`
  - `pub fn hello_line(token: &str) -> String` → `{"token":"…"}\n`; `pub fn parse_hello(line: &str) -> Option<String>`
  - binary `gizai-mcp`: env `GIZAI_SOCKET`, `GIZAI_TOKEN`; exit 1 with `gizai-mcp: …` on stderr when it can't connect.

- [ ] **Step 1: Write the failing protocol tests** (`tests/protocol_test.rs`): a `Fake` Tools with tools `echo` (returns args) and `boom` (Err("it broke")).
  - `initialize_echoes_the_clients_protocol_version` → result.protocolVersion == "2025-11-25", serverInfo.name == "gizai", capabilities.tools present.
  - `initialize_without_a_version_gets_the_default` → "2025-06-18".
  - `tools_list_has_schemas_and_read_only_hints` → tools[0].inputSchema.type == "object", annotations.readOnlyHint.
  - `tools_call_returns_text_content` → result.content[0].text parses to the args; isError false.
  - `a_failing_tool_is_an_error_result_not_a_protocol_error` → result.isError true, text "it broke".
  - `unknown_tool_is_an_error_result` → isError, text contains "unknown tool".
  - `notifications_get_no_answer` → handle(`notifications/initialized`) is None.
  - `unknown_method_is_method_not_found` → error.code -32601.
  - `ping_answers_empty` → result == {}.
  - `serve_reads_lines_and_answers_each_request` → over `tokio::io::duplex`, two requests + a notification + a broken line → three responses, the broken one `error.code == -32700`, id null.
  - `hello_round_trips` → parse_hello(hello_line("abc")) == Some("abc"); parse_hello("nope") == None.
- [ ] **Step 2: Run** `cargo test -p gizai-mcp` → FAIL (crate missing).
- [ ] **Step 3: Implement** `lib.rs` (≈150 lines): JSON-RPC 2.0 dispatch on `method`; `initialize` → `{protocolVersion: params.protocolVersion or DEFAULT_PROTOCOL, capabilities: {tools: {listChanged: false}}, serverInfo: {name: "gizai", version}, instructions}`; `tools/list` → `{tools: [{name, description, inputSchema, annotations: {readOnlyHint}}]}`; `tools/call` → `{content: [{type: "text", text}], isError}` where text is compact JSON (string values unquoted); messages without `id` are notifications; `serve` reads lines, skips blanks, writes one line per answer and flushes.
- [ ] **Step 4: Write the failing shim test** (`tests/shim_test.rs`): a std `UnixListener` in a thread at a temp path accepts one connection, reads the hello line (asserts token), reads one request line, writes `{"jsonrpc":"2.0","id":1,"result":{}}\n`; the test runs `env!("CARGO_BIN_EXE_gizai-mcp")` with the env, writes the request to stdin, closes stdin, and asserts stdout is the reply. Second test: no socket → exit code 1, stderr starts with `gizai-mcp:`.
- [ ] **Step 5: Implement the shim** (std only): connect, write hello, thread copying socket→stdout with flush per read, main copying stdin→socket, `shutdown(Write)` at stdin EOF, join.
- [ ] **Step 6: Run** `cargo test -p gizai-mcp` → all pass.
- [ ] **Step 7: Commit** `feat(mcp): MCP protocol over JSON lines and the gizai-mcp stdio shim`.

### Task 2: `gizai-agents` — chat flags, chat stream parser, generic spawn

**Files:**
- Modify: `crates/gizai-agents/src/claude.rs`, `src/process.rs`, `src/lib.rs`
- Create: `crates/gizai-agents/src/chat_stream.rs`, `tests/chat_stream_test.rs`, `tests/fixtures/chat-ok.jsonl`
- Modify: `tests/parse_test.rs`, `tests/process_test.rs`; `src-tauri/src/runs.rs` (ClaudeArgs `..Default::default()`, `spawn::<RunEvent>`)

**Interfaces:**
- Produces:
  - `ClaudeArgs` derives `Default`; new fields `resume: bool`, `mcp_config: Option<PathBuf>`, `partial_messages: bool`, `restricted: bool`, `tools: Option<Vec<String>>`, `permission_prompts_none: bool`, `add_dirs: Vec<String>`, `no_session_persistence: bool`.
  - `pub enum ChatEvent { Init { session_id, model, mcp_status: Option<String> }, BlockStart, Delta { text }, Text { text }, ToolUse { id, name, input: Value }, ToolResult { tool_use_id, is_error, text }, Result { is_error, subtype, text, cost_usd: Option<f64>, input_tokens, output_tokens, num_turns }, Other { raw_type } }` (serde tag `kind`, snake_case)
  - `pub fn chat_stream::parse_line(line: &str) -> Vec<ChatEvent>`
  - `pub trait StreamEvent: Clone + Send + 'static { fn parse(line: &str) -> Vec<Self>; fn is_tool_call(&self) -> bool; fn other(raw: String) -> Self; }` implemented by `RunEvent` and `ChatEvent`
  - `pub fn process::spawn<E: StreamEvent>(args, cwd, log_path, caps) -> Result<RunHandle<E>, AgentError>`; `RunHandle<E = RunEvent>`

- [ ] **Step 1: Failing tests.** In `parse_test.rs`:
  - `argv_for_a_chat_turn` → contains, in order of appearance, `--include-partial-messages`, `--resume <id>` (and no `--session-id`), `--restricted` (and no `--setting-sources`), `--mcp-config <path>`, `--tools Read,Glob,Grep`, `--permission-prompts none`, `--add-dir /a /b`, `--no-session-persistence`, `--allowedTools mcp__gizai` last.
  - `argv_for_a_task_run_is_unchanged` → the old flags (`--session-id`, `--setting-sources user`), none of the new ones.

  In `chat_stream_test.rs`, over the fixture:
  - init (session id, `mcp_status == Some("connected")` for server `gizai`);
  - BlockStart then two Deltas;
  - Text;
  - ToolUse (id `toolu_1`, name `mcp__gizai__create_task`, input object);
  - ToolResult (tool_use_id, text from the content array);
  - Result (cost 0.0123, tokens summed with cache).

  Unknown and broken lines give Other; thinking deltas are ignored (no Delta).
- [ ] **Step 2: Run** `cargo test -p gizai-agents` → FAIL.
- [ ] **Step 3: Implement** argv branches, `chat_stream.rs` (reuse the `cut`/`result_text` ideas; `stream_event` → `content_block_start` with `content_block.type == "text"` → BlockStart; `content_block_delta` with `delta.type == "text_delta"` → Delta; other stream events → nothing), `StreamEvent` impls, generic `spawn`.
- [ ] **Step 4: Run** `cargo test -p gizai-agents` and `cargo test -p gizai` (runs still pass) → PASS.
- [ ] **Step 5: Commit** `feat(agents): chat turn flags, chat stream parser, spawn over any event type`.

### Task 3: `gizai-core` — chat store, tokens, inbox, chat agent, new roles

**Files:**
- Create: `crates/gizai-core/migrations/0003_chat.sql`, `src/chat.rs`, `src/tokens.rs`, `tests/chat_test.rs`
- Modify: `src/db.rs` (migration, SCHEMA_VERSION 3), `src/lib.rs`, `src/team.rs`, `src/model.rs` (AgentInput.chat_enabled), `src/tasks.rs` (needs_you), `src/runs.rs` (create_chat), `src/seed.rs` (templates), `tests/role_template_test.rs`

**Interfaces:**
- Produces:
  - `team::Member.chat_enabled: bool`; `AgentInput.chat_enabled: Option<bool>` (None = off on create, unchanged on update); `team::chat_agent(db) -> Result<Option<Member>>`; `team::ROLES: [&str; 6] = ["lead","frontend","backend","design","qa","devops"]`
  - `tasks::needs_you(db, you_id) -> Result<Vec<Task>>`
  - `tokens::mint(db, actor_id, scope: Value, ttl_ms) -> Result<String>`, `tokens::verify(db, token) -> Result<Option<TokenGrant>>` (`TokenGrant { actor_id, scope }`), `tokens::revoke(db, token) -> Result<()>`
  - `chat::{ChatThread, ChatMessage}`; `chat::create_thread(db, you, agent_id, first_text) -> Result<String>`; `list_threads`, `get_thread`, `messages(db, thread)`, `add_message(db, NewMessage) -> Result<ChatMessage>`, `set_tool_result(db, message_id, text, is_error) -> Result<ChatMessage>`, `record_session(db, thread, session_id, totals: Totals)`, `reset_session(db, thread)`, `turn_cost(prev: Totals, now: Totals) -> Totals`, `title_from(text) -> String`
  - `runs::create_chat(db, agent_id, thread_id, session_id, cwd, log_path) -> Result<String>`

- [ ] **Step 1: Failing tests** (`chat_test.rs` and additions):
  - `turning_chat_on_for_one_agent_turns_it_off_for_the_others`.
  - `chat_agent_is_none_until_one_has_chat`.
  - `needs_you_lists_held_cards_and_review_cards_assigned_to_you` (not done, not others' review).
  - `a_minted_token_verifies_until_revoked_or_expired` (ttl 0 → None).
  - `tokens_are_stored_hashed` (raw token not in `api_tokens`).
  - `threads_list_newest_first_with_titles_cut_at_60`.
  - `messages_keep_insert_order_and_tool_results_merge_into_tool_json`.
  - `turn_cost_subtracts_the_previous_cumulative_totals_and_restarts_after_a_reset`.
  - `a_chat_run_has_no_task_and_finishes_like_any_run`.
  - Templates: `design_and_devops_have_templates`; lead template mentions the Chat page and still ends with the result line.
- [ ] **Step 2: Run** `cargo test -p gizai-core` → FAIL.
- [ ] **Step 3: Implement.**
  - **0003 SQL:**

    ```sql
    ALTER TABLE agent_configs ADD COLUMN chat_enabled INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE chat_threads ADD COLUMN cost_usd_micros INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE chat_threads ADD COLUMN input_tokens INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE chat_threads ADD COLUMN output_tokens INTEGER NOT NULL DEFAULT 0;
    ```
  - **Messages** are ordered by `rowid`.
  - **Tokens:** 32 bytes from `/dev/urandom`, hex; `token_sha256`; `scopes_json` holds the scope; `expires_at`.
- [ ] **Step 4: Run** `cargo test -p gizai-core` → PASS.
- [ ] **Step 5: Commit** `feat(core): chat threads and messages, turn tokens, inbox, one chat agent, design and devops roles`.

### Task 4: Gizai tools (`src-tauri/src/tools/`)

**Files:**
- Create: `src-tauri/src/tools/mod.rs` (catalog, dispatch, `Notify` effects), `tools/schema.rs`, `tools/resolve.rs`, `tools/read.rs`, `tools/write.rs`, `src-tauri/tests/tools_test.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod tools`), `crates/gizai-core/src/projects.rs` (`suggest_key`)

**Interfaces:**
- Consumes: Task 1 `ToolDef`, Task 3 `chat_agent`, `needs_you`.
- Produces:
  - `tools::catalog() -> Vec<ToolDef>`
  - `tools::call(st: &AppState, actor: &str, name: &str, args: Value) -> Result<Value, String>` (async)
  - `pub struct GizaiTools { pub st: AppState, pub actor: String }` implementing `gizai_mcp::Tools`
  - `projects::suggest_key(name) -> String` (same rule as `src/lib/projectKey.ts`)
  - Tool names, exactly: `get_overview, read_inbox, list_clients, get_client, create_client, update_client, save_contact, list_projects, get_project, create_project, update_project, list_tasks, get_task, create_task, update_task, move_task, comment_on_task, list_agents, get_agent, create_agent, update_agent, set_agent_status, add_routing_rule, start_agent_run, stop_agent_run, list_docs, read_doc, create_doc, write_doc, attach_file, list_people, add_person, get_workflow`
  - Every write returns `{ "ok": true, …, "link": { "page": "task"|"project"|"client"|"agent"|"doc", "id", "label" } }`

- [ ] **Step 1: Failing tests** (`tools_test.rs`, on `gizai_lib::test_state` + an agent "Team Lead" with chat):
  - `the_catalog_has_unique_names_and_object_schemas`.
  - `create_client_then_find_it_by_name`.
  - `update_client_changes_only_the_given_fields`.
  - `create_project_suggests_a_key_and_links_the_client_by_name`.
  - `update_project_keeps_repo_and_colour`.
  - `create_task_by_project_key_with_column_labels_and_assignee_names`.
  - `get_task_by_identifier_includes_comments`.
  - `move_task_by_column_name`.
  - `update_task_sets_labels_and_clears_a_hold`.
  - `ambiguous_names_list_the_candidates`.
  - `unknown_names_say_what_exists`.
  - `read_inbox_lists_held_cards`.
  - `create_and_update_an_agent_and_pause_it`.
  - `docs_can_be_created_written_and_read`.
  - `attach_file_copies_a_local_file_onto_a_task`.
  - `writes_are_attributed_to_the_team_lead` (task activity actor name).
  - `list_tasks_filters_by_column_and_text`.
- [ ] **Step 2: Run** `cargo test -p gizai --test tools_test` → FAIL.
- [ ] **Step 3: Implement.**
  - **Resolvers:** id exact first; tasks by identifier (case-insensitive); projects by key or name; clients, agents and people by name (case-insensitive, then unique prefix). Ambiguous → `"\"Portal\" matches 2 projects: Kade portal (KADE), Fiets portal (GFP). Use the key or id."`. Unknown → `"No project called X. Projects: …"` (at most 10).
  - **Effects:** each write calls `(st.notify)(Note::RowsChanged(table))` and, for task writes, `runs::dispatch` in a spawned task.
  - **Merges:** updates of clients and projects merge onto the current row.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(tools): the Team Lead's Gizai tools (clients, projects, tasks, agents, docs, files, inbox)`.

### Task 5: MCP socket server (`src-tauri/src/mcp.rs`)

**Files:**
- Create: `src-tauri/src/mcp.rs`, `src-tauri/tests/mcp_socket_test.rs`
- Modify: `src-tauri/src/lib.rs` (AppState `mcp_socket: PathBuf`; `open_state` computes it)

**Interfaces:**
- Produces:
  - `mcp::socket_path(data_dir: &Path) -> PathBuf`
  - `mcp::start(st: &AppState) -> std::io::Result<tokio::task::JoinHandle<()>>` (binds, chmods, accepts; each connection: first line must be a valid hello with a live token, else one `{"error":"…"}` line and close; then `gizai_mcp::serve` with `GizaiTools { actor: grant.actor_id }`)
  - `mcp::shim_bin() -> Option<PathBuf>` (env `GIZAI_MCP_BIN`, else next to the current exe)

- [ ] **Step 1: Failing tests:**
  - `socket_path_depends_on_the_data_dir` (two dirs → two paths, both under the XDG dir when set);
  - `a_valid_token_gets_tools`: mint, connect, hello, initialize, tools/list (34 tools), tools/call get_overview ok;
  - `a_bad_or_revoked_token_is_refused`;
  - `a_stale_socket_file_is_replaced`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** with `tokio::net::UnixListener`. Remove the old file only if it is a socket, and create the parent dir with mode 0700.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(app): MCP socket server with per-turn tokens`.

### Task 6: Chat turns (`src-tauri/src/chat.rs`) and commands

**Files:**
- Create: `src-tauri/src/chat.rs`, `crates/gizai-agents/tests/fake-claude-chat.py` (executable), `src-tauri/tests/chat_flow_test.rs`
- Modify: `src-tauri/src/lib.rs` (AppState `chat: Arc<chat::ChatManager>`; `Note::ChatEvent`, `Note::ChatChanged`; notifier events `chat-event`, `chat-changed`; start the socket in `run()`; quit stops chat turns; startup ends orphan chat process groups by `cwd`), `src-tauri/src/commands.rs` (`list_chat_threads`, `chat_messages`, `send_chat`, `stop_chat`, `chat_live`, `chat_agent`)

**Interfaces:**
- Consumes: Tasks 1–5.
- Produces:
  - `chat::send(st, thread_id: Option<String>, text: String, bin_override: Option<String>) -> Result<(String, JoinHandle<TurnSummary>), String>`
  - `chat::stop(st, thread_id)`, `chat::live(st) -> Vec<ChatStatus>` (`{threadId, runId, draft, tool}`), `chat::stop_all(st, wait)`
  - `TurnSummary { run_id, status, error }`
  - UI events: `chat-event` `{threadId, kind: "delta"|"block"|"tool"|"message", text?, name?, message?}`; `chat-changed`; `rows-changed {table: "chat_messages"|"chat_threads"}`

- [ ] **Step 1: Failing tests** (`chat_flow_test.rs`, fake = `fake-claude-chat.py`, shim from `target/debug/gizai-mcp`, building it when missing):
  - `a_chat_turn_runs_tools_and_saves_the_conversation`: "create task Chat probe task in KADE".
    - KADE gets the task.
    - Messages are user, agent "Sure, on it.", a tool `create_task` with a result link, then the final agent text.
    - The run row is `chat`, `succeeded`, cost 10000 µ$.
    - The thread has a session.
  - `the_second_turn_resumes_the_session_and_counts_only_its_own_cost`: second run cost 10000 (cumulative 0.02 − 0.01).
  - `a_lost_session_starts_again_with_the_recent_messages`: prompt `FAKE_LOST_SESSION`.
    - The first attempt ends before init.
    - The retry gets `--session-id` and a prompt containing the earlier messages.
    - Exactly one retry; the thread's session is replaced.
  - `chat_needs_an_active_chat_agent`: no agent → "Set up the Team Lead first"; paused → "Team Lead is paused".
  - `a_second_message_while_working_is_refused` (FAKE_CHAT_HANG, then send again → "still answering"; then stop).
  - `stopping_a_turn_cancels_it_and_says_so` (system message "Stopped.", run cancelled).
  - `a_crashing_claude_leaves_a_system_message_with_its_error`.
  - `the_turn_token_is_revoked_afterwards`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** per spec §5. Details:
  - **System prompt:** a fixed Gizai chat text, then `## Your instructions` + the agent's instructions.
  - **Working dir:** `data_dir/lead`.
  - **MCP config:** `data_dir/chat/<run>.mcp.json` (0600, removed at the end).
  - **Add-dirs:** the repos of active projects that exist on disk.
  - **Persistence:** `GIZAI_CHAT_NO_PERSIST=1` adds `--no-session-persistence` (probes only).
  - **Caps:** 15 min, 60 tool calls.
  - **Retry rule:** resumed, no Init event, not stopped → retry once with a fresh session and the last 20 messages as context.
- [ ] **Step 4: Run** `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit** `feat(app): chat turns with Claude Code, resume, stop and live events`.

### Task 7: Chat UI

**Files:**
- Create: `src/lib/chat.ts`, `src/lib/chat.test.ts`, `src/pages/ChatPage.tsx`, `src/components/chat/ChatThread.tsx`, `src/components/chat/ChatMessage.tsx`, `src/components/chat/useChat.ts`
- Modify: `src/types.ts`, `src/api.ts`, `src/router.ts` (+test), `src/App.tsx`, `src/components/Sidebar.tsx`, `src/components/CommandPalette.tsx`, `src/styles/components.css` (chat components), `src/styles/app.css` (page layout)

**Interfaces:**
- Consumes: Task 6 commands and events.
- Produces:
  - `chat.ts`: `toolName(raw)`, `toolCard(msg) -> { verb, label?, href?, state: "running"|"ok"|"error", detail? }`, `SUGGESTIONS`, `groupMessages(msgs)` (consecutive agent rows share one header)
  - Route `chat` with optional id

- [ ] **Step 1: Failing vitest:**
  - `toolCard` maps `mcp__gizai__create_task` with a result link → verb "Created task", label "KADE-14 Export", href `#/task/<id>`, state ok;
  - a running tool (no result) → state running;
  - an error → state error, detail = text;
  - read tools → "Read the inbox", "Looked at the overview";
  - an unknown non-Gizai tool → its raw name ("Read" → "Read a file");
  - `groupMessages` starts a new group when the role changes or 5 minutes pass;
  - router parses `#/chat` and `#/chat/abc`.
- [ ] **Step 2: Run** `npm test` → FAIL.
- [ ] **Step 3: Implement.**
  - The page as spec §7 describes.
  - `useChat(threadId)`: loads messages, listens to `chat-event` (deltas into a draft, `message` upserts and clears the draft on agent messages), loads `chat_live` on mount.
  - Send: `sendChat`, then go to `#/chat/<id>`.
  - Composer: Enter sends, Shift+Enter adds a line; Stop while working.
  - Thread list.
  - Setup panel when `chatAgent()` is null.
  - Paused banner with Resume.
  - Sidebar Chat item above Tasks, with a live tag while any turn runs.
  - A palette entry.
- [ ] **Step 4: Run** `npm test && npm run build` → PASS.
- [ ] **Step 5: Commit** `feat(ui): Chat page, Team Lead setup state, sidebar entry`.

### Task 8: Agent form (roles, Chat switch, presets) and agent page bits

**Files:**
- Modify: `src/components/AgentForm.tsx`, `src/lib/drawers.tsx`, `src/components/Avatar.tsx` (design → Palette, devops → Container), `src/pages/AgentPage.tsx` ("Chat" trigger label, "Open chat" action when chat is on), `src/lib/agents.ts` (+test: `AgentPreset` → draft)

**Interfaces:**
- Produces:
  - `DrawerReq` agent kind gains `preset?: AgentPreset`
  - `export type AgentPreset = { name?: string; role?: string; chat?: boolean }`
  - `draftFrom(m, preset?)`

- [ ] **Step 1: Failing vitest** (`agents.test.ts`):
  - `draftFrom(null, {name: "Team Lead", role: "lead", chat: true})` → `chat` true, the role and the name;
  - `inputFrom` sends `chatEnabled`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.**
  - Roles: Lead, Frontend, Backend, Design, QA, DevOps, Other.
  - Chat FormSection with `.check` switch `#a-chat`.
  - When another agent has chat: "Chat moves from X to this agent".
  - The Chat page's setup button calls `go({page:"team"})` and opens `{kind:"agent", teamId, preset:{name:"Team Lead", role:"lead", chat:true}}`.
- [ ] **Step 4: Run** `npm test && npm run build` → PASS.
- [ ] **Step 5: Commit** `feat(ui): agent form roles, Chat switch and presets`.

### Task 9: Org chart on the Team page

**Files:**
- Create: `src/lib/org.ts`, `src/lib/org.test.ts`, `src/components/OrgChart.tsx`
- Modify: `src/pages/TeamPage.tsx` (Organisation + People sections), `src/styles/components.css` (org chart)

**Interfaces:**
- Produces:
  - `buildOrg(members: Member[]): { leads: OrgNode[]; departments: { key; name; nodes: OrgNode[] }[] }`
  - `OrgNode = { kind: "agent"; member: Member } | { kind: "ghost"; role: string; name: string }`

- [ ] **Step 1: Failing vitest:**
  - no agents → a ghost lead and ghosts in Development (frontend, backend), Design, Quality and Operations;
  - agents fill their places;
  - an `api` role goes to "Specialists";
  - a chat agent with role backend counts as a lead;
  - people are ignored.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** the chart as CSS tree connectors:
  - the lead row centred, a horizontal bar, department columns with headings, nodes as rounded pills (`.org-node`, `.org-node.live` teal ring + "Working", `.org-node.paused` dimmed, `.org-node.ghost` dashed);
  - the container scrolls sideways when wide.
- [ ] **Step 4: Run** `npm test && npm run build` → PASS.
- [ ] **Step 5: Commit** `feat(ui): org chart on the Team page`.

### Task 10: Headless chat probe, prep data, scripts

**Files:**
- Create: `crates/gizai-core/examples/prep_chat.rs`
- Modify: `src/selftest.ts` (`chatProbe`), `src/App.tsx` (route `chat` + mode `chat` runs the probe), `scripts/ui-test.sh` (#6), `scripts/run.sh` (build the shim; rebuild check includes it), `README.md`

- [ ] **Step 1:** Write `chatProbe`:
  - setup panel visible;
  - click → drawer has name "Team Lead" and `#a-chat` checked;
  - save;
  - go to `#/chat`;
  - type "create task Chat probe task in KADE" and press Enter;
  - wait ≤ 15 s for `.tool-card a[href^="#/task/"]` and `.chat-msg.agent`;
  - `listTasks` has the task.
- [ ] **Step 2:** `prep_chat` sets `claude_bin` to the fake chat script (no agents added).
- [ ] **Step 3:** `ui-test.sh` #6:
  - copy the demo data and run `prep_chat`;
  - `ROUTE=chat MODE=chat scripts/smoke-cage.sh`;
  - `smoke-cage.sh` passes `GIZAI_MCP_BIN=$GZ/target/release/gizai-mcp`.
- [ ] **Step 4: Run** `cargo build --release -p gizai-mcp && npm run tauri build -- --no-bundle && scripts/ui-test.sh` → all 6 OK.
- [ ] **Step 5:** Screenshots (`scripts/shot-cage.sh`) of Chat (setup, a conversation), Team (org chart, empty and filled), agent drawer (Chat on); fix what looks wrong.
- [ ] **Step 6: Commit** `test(ui): headless chat probe; run.sh builds the MCP shim`.

### Task 11: One real Claude Code check

**Files:**
- Create: `src-tauri/examples/chat_probe.rs` (scratch data dir, real `claude`, model haiku, `GIZAI_CHAT_NO_PERSIST=1`, sends "Use get_overview and tell me how many clients there are", prints the messages and the run row)

- [ ] **Step 1:** Run it once. Expected:
  - init shows `gizai` connected;
  - one `get_overview` tool card;
  - an agent answer;
  - the run is `succeeded`.
- [ ] **Step 2:** Fix any flag or format mismatch (with a test for each fix), re-run.
- [ ] **Step 3: Commit** `test: real Claude Code chat check (example, run by hand)`.

### Task 12: Docs, design system, memory

- [ ] Write `docs/v0.3-notes.md`: what was built, the rulings, the deferred minors, and "Decisions for Jeffrey" (spec §11).
- [ ] README: Chat section.
- [ ] Sync the design system artifact:
  - `bundle.css` = `components.css`;
  - new components ChatMessage, ToolCard, ChatSetup, OrgChart (README + preview);
  - Sidebar preview with Chat;
  - the index `lastChange`.
- [ ] Update memory `gizai-build-status.md`.
- [ ] Commit `docs: v0.3 notes`.

## Added overnight (Jeffrey, 2026-10-06 22:xx: "finish this, see what can be improved and run tests yourself, plan the paid features and how to implement them, write a proper GitHub README with install steps (maybe a CLI install command), and a summary for the morning")

### Task 13: Test and improve pass
- [ ] Run every suite, the cage probes and screenshots of every page; fix what's broken (each with a failing test first); list what's left.

### Task 14: Paid features plan
- [ ] `docs/PAID-FEATURES.md`: which features are paid (open core, MIT core stays free), why, how each is built (licence keys offline, a sync service, team seats…), the order and rough effort. Grounded in RESEARCH.md and Paperclip's model.

### Task 15: GitHub README and install command
- [ ] `README.md` for GitHub: what Gizai is, screenshots, requirements (Linux, Claude Code), install (one-line `install.sh`, from source), first steps, Chat, agents, safety, development, licence.
- [ ] `scripts/install.sh`: checks dependencies, builds from source (or later downloads a release), installs `gizai` + `gizai-mcp` into `~/.local/bin` and a desktop entry; `--uninstall`. Tested in a scratch HOME.

### Task 16: Morning summary
- [ ] `docs/MORNING-2026-10-07.md` (and a short chat reply): what was built, how to try it, test results, decisions for Jeffrey.
