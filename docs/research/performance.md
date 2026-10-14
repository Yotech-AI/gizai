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
| UI | React 19.3.0, TypeScript 6.0.3, built by Vite 8.3.3 (Rolldown 1.2.12), `build.target` `safari16`, one entry | `src/`, `vite.config.ts:10` |
| Bridge | 187 `#[tauri::command]` functions; events `rows-changed`, `runs-changed`, `run-event`, `chat-event`, `chat-changed`, `update-changed`, `editor-updated`, `terminals-changed` | `src-tauri/src/lib.rs:279-302`, `:575-620`; the UI side only in `src/api.ts` |
| Core | `gizai-core` (SQLite through rusqlite 0.40.2, SQLite bundled), `gizai-agents` (processes, git, worktrees, terminals), `gizai-mcp` | `crates/` |
| Data | one database `gizai.db` in the data folder, WAL with `synchronous` NORMAL, **one connection behind one `Mutex`**; run and chat logs as files next to it | `crates/gizai-core/src/db.rs:12-18`, `:162-165` |
| Tests | `cargo test`, `npm test` (Vitest), headless UI tests in `cage` driven by self-test probes | `scripts/ui-test.sh`, `scripts/smoke-cage.sh`, `scripts/shot-cage.sh`, `src/selftest.ts`, `crates/gizai-core/examples/prep_*.rs` |
| Release | `npm run tauri build -- --no-bundle`; release profile `lto = "thin"`, `codegen-units = 4`, `strip = true` | `Cargo.toml:24-27`, `docs/RELEASING.md` |

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
   sidebar and the route's page; the default route is the task list (`src/router.ts:35`, an unknown page falls back to
   `tasks`).

**What the performance work touches:** all four layers. The costs below sit in the bridge (which thread a command runs on,
how much comes back, how often), the database (one connection, queries), the UI (what loads at start, how much renders) and
the webview (how WebKitGTK draws).

## 7. The benchmark harness

One entry point, `scripts/perf.sh`, built by the first proposed card (P1) and used by every later one: each card runs it on
`main` and on its branch, on the same machine with nothing else running, and puts both tables in its pull request. It reuses
what exists: the headless `cage` start of `scripts/smoke-cage.sh`, the self-test report (`GIZAI_SELFTEST`,
`src-tauri/src/lib.rs:103-110`), the frame timing of the Changes and terminal probes (`src/selftest.ts:2804-2826`,
`:3089-3100`), the Memory graph probe's numbers (`src/selftest.ts:1988-2005`: load, frames a second while it lays out,
settle time, a frame's work) and the data makers' pattern (`crates/gizai-core/examples/prep_*.rs`).

**Its five parts:**

1. **The data, `crates/gizai-core/examples/prep_perf.rs`** (new; usage like the others:
   `cargo run --release -p gizai-core --example prep_perf -- <data dir> <fake claude>`). On a copy of the demo data:
   500 cards in project KADE over every column (each with a 1 to 3 KB description, up to three labels and three comments);
   2,000 finished runs over 90 days with tokens and cost (Usage, Inbox, `check_board`); one chat of 1,000 messages (every
   tenth with a tool call) among 60 chats; 300 Memory notes that link each other (`prep_graph`'s pattern); a finished run on
   KADE-1 whose log has 5,000 events; agents paused and Claude Code pointed at the fake. It prints the ids the probe needs on
   one line, as `prep_graph` does.
2. **The probe, `GIZAI_SELFTEST_MODE=perf:<scenario>`** in `src/selftest.ts`, reported as `perf` in the self-test JSON.
   It changes nothing in the app: it wraps `window.__TAURI_INTERNALS__.invoke` (what `@tauri-apps/api`'s `invoke` calls)
   to count each command's calls, answer size (JSON length) and time, counts the events heard, and times frames with
   `requestAnimationFrame` (frames a second, the longest frame).

   | Scenario | Route and data | Numbers |
   |---|---|---|
   | `start` | the default route | `performance.timeOrigin`; the navigation entry's `responseStart` and `domContentLoadedEventEnd`; `app_info` answered; first card drawn; calls and bytes in the first 5 s |
   | `board` | `#/board`, 500 cards | all columns drawn (ms); 3 s scrolling each column (fps, longest frame); a drag to the next column (ms to drawn; calls after the drop) |
   | `chat` | the 1,000-message chat | first screen (ms); scroll to the top (fps, longest frame); 50 characters typed in the composer (longest frame) |
   | `memory` | `#/memory`, 300 notes; `#/memory/graph` | tree drawn (ms); search typing (longest frame); the graph probe's load, fps, settle and frame numbers |
   | `runlog` | the card with the 5,000-event run; then a fake run streaming 50 events a second for 20 s | log drawn (ms); scroll (fps, longest frame); live: fps, longest frame, calls and events counted |
   | `idle` | `#/board`, 60 s, nothing happening | calls and bytes, events heard |
   | `editor` | the Editor on a made git repository of 20,000 files | tree drawn (ms); a 5,000-line file opened (ms); calls a minute while it sits open |
   | `lead` | the chat, with the fake Team Lead streaming a 20 KB answer | fps and longest frame while it streams; events heard |

3. **The script, `scripts/perf.sh [scenario …]`.** Builds when stale (`gz_app`, `scripts/app-ready.sh:20-27`), makes the
   data once, then per scenario starts `target/release/gizai` in the headless `cage` exactly as `scripts/smoke-cage.sh:16-22`
   does, with the probe's mode. It notes the launch time (`date +%s%3N`), so start to board is the page's `timeOrigin` plus
   the probe's first-card time minus the launch. Every 500 ms it reads, for the PIDs it started only (the app, and its
   children found with `pgrep -P` as `scripts/shot-cage.sh:25` does), `VmRSS` from `/proc/<pid>/status` and
   `utime + stime` from `/proc/<pid>/stat`: memory per process and CPU % over the scenario, at rest and under load. Each
   scenario runs five times (the first after a fresh data copy counts as cold, the other four as warm); it prints the
   medians as a table and writes `.devdata/perf/<commit>.json`. `scripts/perf.sh --compare a.json b.json` prints two
   results side by side with the change in %. Optional: `--webkit-env 'WEBKIT_DISABLE_COMPOSITING_MODE=1'` runs the same
   with an environment variable, to compare WebKitGTK's drawing modes.
4. **The query bench, `crates/gizai-core/examples/bench_queries.rs`** (new): on the `prep_perf` data, without a window,
   times each hot core function 30 times (the task list with each filter, the team, the usage summary, the big chat's
   messages, the Memory list and search, the board check's queries, a card's activity) and prints the median and p95 in
   µs as JSON lines: `cargo run --release -p gizai-core --example bench_queries -- <data dir>`.
5. **The bundle report, `scripts/bundle-size.mjs`** (new): after `npm run build`, lists `dist/assets/*` with bytes and
   gzip bytes (Node's `zlib`), and marks what `dist/index.html` loads at start (its script and `modulepreload` links)
   apart from the chunks loaded later.

**What it can't show** **[I]**: the headless cage draws with wlroots' `gles2` renderer on the same GPU
(`WLR_RENDERER=gles2`, `WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128`, `scripts/smoke-cage.sh:18`) in a 1280 × 720 output, not
in Hyprland on your screen, so its numbers are for comparing before and after, not the ones you feel. For your own Gizai,
the read-only line in section 3 gives its memory and CPU.
