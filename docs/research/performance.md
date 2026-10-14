# Performance: where Gizai is slow, and how to make it faster (GA-170)

Research for the roadmap item "Performance: a faster app" (gizai.app/roadmap, GW-5). Research only: no code, settings or
dependencies change with this card. Written by the R&D Agent on 14 Oct 2026, against `main` at 24535e3 (Gizai 0.11.0).

**Marks:** **[V]** verified in the code or a primary source (named next to it); **[I]** my judgement or an estimate.

> Draft in progress: the findings, cards and sources follow below as the research goes on.

## 2. The project now

**What it does.** Gizai is a desktop app for clients, projects and cards, worked on by local Claude Code agents. You see
the board, the Inbox, a chat with the Team Lead, Memory (notes and their graph), a Code Editor, terminals and a Usage page.
Agents run in the background, one process tree per run, and stream what they do back into the app.

**How it is built** (versions from `npm ls` and `cargo tree` in this worktree) **[V]**:

| Layer | What | Where |
|---|---|---|
| Shell | Tauri 2.12.1 on wry 0.57.0; WebKitGTK on Linux (crate `webkit2gtk` 2.0.2), WKWebView on macOS, WebView2 on Windows | `src-tauri/`, `src-tauri/tauri.conf.json` |
| UI | React 19.3.0, TypeScript 6.0.3, built by Vite 8.3.3 (Rolldown 1.2.12), `build.target` `safari16`, one entry | `src/`, `vite.config.ts:9` |
| Bridge | 187 `#[tauri::command]` functions; events `rows-changed`, `runs-changed`, `run-event`, `chat-event`, `chat-changed`, `update-changed`, `editor-updated`, `terminals-changed` | `src-tauri/src/lib.rs:279-302`, `:575-620`; the UI side only in `src/api.ts` |
| Core | `gizai-core` (SQLite through rusqlite 0.40.2, SQLite bundled), `gizai-agents` (processes, git, worktrees, terminals), `gizai-mcp` | `crates/` |
| Data | one database `gizai.db` in the data folder, WAL with `synchronous` NORMAL, **one connection behind one `Mutex`**; run and chat logs as files next to it | `crates/gizai-core/src/db.rs:12-18`, `:162-165` |
| Tests | `cargo test`, `npm test` (Vitest), headless UI tests in `cage` driven by self-test probes | `scripts/ui-test.sh`, `scripts/smoke-cage.sh`, `scripts/shot-cage.sh`, `src/selftest.ts`, `crates/gizai-core/examples/prep_*.rs` |
| Release | `npm run tauri build -- --no-bundle`; release profile `lto = "thin"`, `codegen-units = 4`, `strip = true` | `Cargo.toml:21-24`, `docs/RELEASING.md` |

**How a start goes** **[V]**:

1. `run()` (`src-tauri/src/lib.rs:431`) builds Tauri with its plugins. On the event loop's Ready, Tauri first builds the
   windows from `tauri.conf.json` and then calls Gizai's `setup` closure, on the main thread (Tauri 2.12.1,
   `crates/tauri/src/app.rs`, `fn setup`: the `for window_config in app.config().app.windows` loop comes before
   `(setup)(app)`).
2. The window starts loading the page (WebKit's own web process) while `setup` runs `open_state` (`lib.rs:477`) on the
   main thread: the data folder's lock (`lib.rs:229`), opening and upgrading the database (`lib.rs:230`; a snapshot first
   when the schema is older, `db.rs:155-160`), the seed (`lib.rs:231-235`), Memory's groups and folders (`:237-243`), runs
   a previous Gizai left (`:247-255`, a git call per such run through `runs::record_head`), questions and chat queues
   (`:257-261`), stray chat configs (`:262`) and housekeeping (`:263`), then the pending actions (`:275`).
3. Then background work starts (`lib.rs:491-572`): Memory's import from Claude Code (`spawn_blocking`), the
   notifications watcher, the MCP socket, the Team Lead's code copies, the pull request check (at once, then every two
   minutes), the release check (after 20 s), housekeeping (hourly tick), and every 60 s `runs::pull` and `board::tick`.
4. The page (`src/main.tsx`) applies the appearance, renders `App` (`src/App.tsx`), which calls `app_info` and mounts the
   sidebar and the route's page; the default route is the task list (`src/router.ts:37`, an unknown page falls back to
   `tasks`).

**What the performance work touches:** all four layers. The costs below sit in the bridge (which thread a command runs on,
how much comes back, how often), the database (one connection, queries), the UI (what loads at start, how much renders) and
the webview (how WebKitGTK draws).
