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

## 3. Measurements

**What this run could measure.** An R&D run may read files and run `git`, `rg`, `ls`, `wc`, `npm view`, `npm ls` and
`cargo tree`. It may not build, start Gizai, run the repository's scripts, or read outside this worktree and
`/home/jefsev/Herd` (so not `/proc` of a running Gizai). So the numbers below are the ones files and code give; the ones
that need a running Gizai are in the second table, with how the harness (section 7, card P1) takes them.

| What | Number | How it was taken |
|---|---|---|
| The last bundle built in the main checkout (7 Oct, 14:05: before the Memory graph, the terminals and the Editor's diff view) | **one JS file of 1,214,807 bytes** and one CSS file of 52,815 bytes; the fonts load on use (`font-display: swap`, `src/styles/fonts.css`) | `ls -la /home/jefsev/Herd/gizai/dist/assets`; `grep -o` shows CodeMirror (`cm-scroller`) and micromark in it, and no xterm.js (`xterm-screen`) and no d3-force (`forceManyBody`) yet |
| The release binary of that build | `gizai` 14,938,488 bytes, `gizai-mcp` 459,712 bytes | `ls -la /home/jefsev/Herd/gizai/target/release` (7 Oct, 14:07) |
| Today's libraries, as built files | xterm.js 344,970 bytes (minified) and its addons 202,924 (WebGL 126,558, search 39,031, unicode11 31,220, web links 3,148, fit 1,967): **547 KB that load only when a terminal opens**. React DOM's client 625,168 (not minified). CodeMirror and Lezer, 31 packages: 1,857,335 (not minified), of which `@codemirror/view` 493,290. d3-force with quadtree, dispatch and timer 17,427 (minified) | `wc -c node_modules/<package>/<file>` in this worktree (`npm ls`: xterm 6.0.0, React 19.3.0, CodeMirror view 6.43.13, d3-force 3.0.0) |
| The self-test inside the app's bundle | `src/selftest.ts`: **301,604 bytes** of TypeScript, 19% of the app's own 1,576,897 bytes (tests left out), imported by `src/App.tsx:27` and so loaded at every start | `wc -c` over `src/` |
| Commands and threads | 187 commands: **112 plain `fn`, which Tauri runs on the main thread**, 75 `async fn` on the async runtime, none `#[tauri::command(async)]` | `rg '#\[tauri::command' -A1 src-tauri/src` |
| Data hooks that reload after any write, mounted with the board | **9** (the sidebar 5, the task page 4), 2 of which load every card | `src/lib/useData.ts:15-20`, `src/components/Sidebar.tsx:42-52`, `src/pages/TasksPage.tsx:103-107` |
| Cards drawn on the board | at most 50 per column (Show more for the rest); Backlog, Done and Cancelled start as narrow rails without cards | `src/components/Board.tsx:3`, `:21`, `:67-73`, `:91-93` |
| A live run's log | the last 500 events, each text event drawn as Markdown | `src/lib/runs.ts:4-8`, `src/components/RunPanel.tsx:22-30` |

**What needs a running Gizai** (not taken in this run):

| What the card asks | Why not here | How it is taken (P1, section 7) |
|---|---|---|
| Start-up, cold and warm, launch to board | needs Gizai started | `scripts/perf.sh start`: launch time to the page's `timeOrigin`, then to the first card drawn; 1 cold and 4 warm starts; the setup part (`open_state`) shows as the wait before the page's `responseStart` |
| Memory (RSS) and CPU at rest, with a run, while the Team Lead answers | needs `/proc` of a running Gizai | `scripts/perf.sh idle runlog lead`: `VmRSS` and CPU % per process of the test Gizai it started. For **your own** Gizai, this read-only line gives memory per process now: `ps -o pid,rss,pcpu,etime,comm -p "$(pgrep -d, -x gizai),$(pgrep -d, -P "$(pgrep -x gizai)")"` (RSS in KiB; `pcpu` is the average since the process started, so for CPU now use `top -b -d 10 -n 2 -p <those pids>` and read the second block) |
| The UI with 500 cards, 1,000 messages, 300 notes, a big repository, a run's live log | needs the window | `scripts/perf.sh board chat memory editor runlog` on `prep_perf`'s data: time to drawn, frames a second and the longest frame while scrolling or streaming |
| `npm run build`'s chunk sizes today | needs a build | `npm run build && node scripts/bundle-size.mjs` |
| Backend calls and how much comes back | needs the window | the probe's `invoke` counter in every scenario (calls, bytes and ms per command) |

## 4. Findings, biggest cost first

The order weighs how often a cost is paid by how much it blocks what you see. It is my judgement **[I]** until P1 gives
numbers; each finding's code is verified **[V]**.

### F1. One write makes every screen load everything again, on the main thread

- **What happens.** `useData` reloads its fetch on **every** `rows-changed` event, whatever the table
  (`src/lib/useData.ts:15-20`; `src/api.ts:320-322`: "Fires after any write; screens refetch what they show"). On the
  board, nine of these hooks are mounted: the sidebar's `listProjects`, `listTasks`, `getTeam`, `memoryNotes` and
  `listChatThreads` (`src/components/Sidebar.tsx:42-52`) and the task page's `getTeam`, `listProjects`, `listUsers` and
  `listTasks` (`src/pages/TasksPage.tsx:103-106`). So one write anywhere (a comment, a setting, a Memory note an agent
  saves, a run's status) costs about eight full reloads: every card twice, the team twice, the projects twice, every Memory
  note and every chat.
- **Where they run.** Each of those is a plain `fn` command (`src-tauri/src/commands.rs:82` `list_projects`, `:97`
  `list_tasks`, `:163` `get_team`, `:227` `memory_notes`, `:576` `list_chat_threads`), so Tauri runs it on the main thread,
  the one that also runs the window; and each takes the database's only connection (`crates/gizai-core/src/db.rs:13`,
  `read` at `:204-207` takes the same `Mutex` as `write` at `:209-222`). A reload waits for any write in progress (an agent's
  run writing from another thread), and while it waits the window waits too.
- **Then the page.** The answers are parsed and the page renders again: the task list sorts, filters and groups every card
  (`TasksPage.tsx:131-136`), and the board's cards render again although `Card` is memoised (`Board.tsx:26`), because the
  page passes a new `onOpen` function each time (`TasksPage.tsx:221`) and `Board` builds a new `byId` map on every render
  (`Board.tsx:119`).
- **When it bites.** With an agent run going and while the Team Lead works, writes come every few seconds; each one costs
  the reload storm above. This is the most likely cause of "Gizai gets sluggish while agents work" **[I]**.

### F2. Streaming text re-renders, and re-parses, everything above it

- **The Team Lead's answer.** Each streamed piece (`delta`) updates the draft (`src/components/chat/useChat.ts:77-84`),
  which renders the whole thread again: `groupMessages` runs again in render (`src/components/chat/ChatThread.tsx:233`) and
  every `MessageGroup` (a plain function, not memoised, `src/components/chat/ChatMessage.tsx:75`) renders its messages again
  (`ChatThread.tsx:260-261`), each agent message through `MarkdownView` (`ChatMessage.tsx:97`). `MarkdownView` isn't
  memoised and passes a new `remarkPlugins` array and `components` object on each render
  (`src/components/MarkdownView.tsx:26-46`), so react-markdown parses every message's Markdown again, for every piece of
  text that streams in. Then a layout effect scrolls to the bottom on each piece (`ChatThread.tsx:153-156`), which makes
  WebKit lay out the whole thread at once. The cost grows with the chat's length: in a chat of 1,000 messages every
  streamed word re-parses up to 1,000 Markdown bodies **[I]**.
- **A live run's log.** `Stream` renders up to 500 events, text events through `MarkdownView`
  (`src/components/RunPanel.tsx:22-30`); it isn't memoised, and `RunPanel` re-renders every second while the run is live
  (`RunPanel.tsx:90-91`) and on each event (`RunPanel.tsx:66`, merged by `mergeEvents`, which copies, sorts and cuts the
  list each time, `src/lib/runs.ts:4-8`). So watching a live run re-parses up to 500 Markdown blocks every second, plus once
  per event. On the backend, each `run_events` call clones the live run's whole event list, or reads and parses the whole
  log file, on the main thread (`src-tauri/src/runs.rs:394-410`).
- **What the terminal shows is possible.** Terminals already stream the right way: raw bytes over a `tauri::ipc::Channel`,
  at most one message every 8 ms (`crates/gizai-agents/src/terminal.rs:38`, `:1022-1026`;
  `src-tauri/src/terminal.rs:73-74`).

### F3. Every page and the self-test load at start

- `src/App.tsx:5-27` imports every page and `src/selftest.ts` statically; there is no `React.lazy`, and Vite's chunk
  warning is raised to 2,000 KB on purpose ("Desktop app: one local bundle, so a large chunk costs nothing over the
  network", `vite.config.ts:9-10`). Over the network that is right, but WebKit still has to parse, compile and run every
  module's top level before the first render **[I]**.
- What is already split: xterm.js and its addons load when a terminal opens (`src/lib/terminals.ts:224-233`), and the
  Editor's language packs when a file of that language opens (`src/lib/codeLanguages.ts:48-112`). So the card's guess that
  xterm.js loads at start is wrong; CodeMirror's core does load at start (the Markdown editor imports
  `@codemirror/lang-markdown`, `src/components/MarkdownEditor.tsx:29`), and so do `@codemirror/merge` (through
  `src/selftest.ts:3`, `src/lib/changes.ts:6`, `src/components/changes/ChangesView.tsx:12`) and d3-force (the Memory graph).
- The self-test alone is 301,604 bytes of source that every start parses and never uses outside tests.

### FW. The webview: WebKitGTK already draws on the GPU; keep it that way

- **What Gizai sets.** The launcher only points EGL at Mesa when the machine has no NVIDIA GPU (`install.sh:316-317`) and
  starts Gizai through a login shell so agents find their tools (`install.sh:320`); neither it nor Gizai sets any WebKit
  variable (`rg WEBKIT_` finds none in `src-tauri/` or the scripts) **[V]**.
- **What WebKitGTK does now.** Arch's `webkit2gtk-4.1` is 2.54.1 (built 2 Oct 2026, archlinux.org's package data), so an
  up-to-date Omarchy runs 2.54 **[V]** (the version on this PC itself couldn't be read in this run). In 2.54 "the web
  process compositor now uses the Skia API instead of the venerable TextureMapper", compositing is damage-aware by default
  ("each draw the compositor issues is restricted to the damaged rectangles"), "Asynchronous scrolling is smoother", and
  "hardware acceleration is now always used, and the on-demand policy behaves like
  `WEBKIT_HARDWARE_ACCELERATION_POLICY_ALWAYS`" (WebKitGTK 2.54 highlights, webkitgtk.org) **[V]**. So there is no
  WebKit switch to flip for speed on Linux; the common Tauri workarounds `WEBKIT_DISABLE_COMPOSITING_MODE=1` and
  `WEBKIT_DISABLE_DMABUF_RENDERER=1` move drawing to slower paths and belong only on machines where the window stays blank
  **[I]**.
- **What still costs on the GPU path** **[I]**: anything that changes every frame makes WebKit composite every frame, also
  at rest: animations that never stop (the pulsing "is working" dot on a card, `Board.tsx:47`; the chat's caret and
  "Thinking" pulse, `ChatThread.tsx:239-240`), and full-window layers. Damage tracking in 2.54 limits the area redrawn, not
  the wake-ups. P1's `idle` scenario, with and without a live run, shows what they cost.
- **A login shell per action.** On Linux, `command_path()` starts `bash -lc` to read your login PATH each time it is called
  (`src-tauri/src/runs.rs:785-797`): at a run's preparation (`runs.rs:756`), a terminal's start
  (`src-tauri/src/terminal.rs:35`), the Editor's Open in and Update (`src-tauri/src/editor.rs:129`, `:293`), finding the
  coding CLIs (`src-tauri/src/clis.rs:86`, `:110`, `:141`, `:152`) and more. Its cost is your shell profile's start time,
  paid again each time; the launcher pays it once more at every start (`install.sh:320`). Small, but free to cache **[I]**.

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
