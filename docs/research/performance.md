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
  note and every chat. On the Memory graph it is about fourteen, among them every note with its text
  (`src/pages/MemoryPage.tsx:50`, `memoryNotes(true)`), every card, open or closed, only to know their ids (`:51`), and the
  graph's own cards, projects, clients, people and team (`src/components/memory/MemoryGraph.tsx:21-25`).
- **Where they run.** Each of those is a plain `fn` command (`src-tauri/src/commands.rs:82` `list_projects`, `:97`
  `list_tasks`, `:163` `get_team`, `:227` `memory_notes`, `:576` `list_chat_threads`), so Tauri runs it on the main thread,
  the one that also runs the window ("Commands without the async keyword are executed on the main thread unless defined
  with #[tauri::command(async)]", and "Asynchronous commands are preferred in Tauri to perform heavy work in a manner that
  doesn't result in UI freezes or slowdowns": Tauri's Calling Rust page **[V]**); and each takes the database's only connection (`crates/gizai-core/src/db.rs:13`,
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

## 5. Compared with what Tauri, WebKitGTK, React, Vite and SQLite recommend

Each line: the advice (quoted from a primary source, read on 14 Oct 2026, links in section 10), what Gizai does, and the gap.

| Advice | Gizai today | Gap |
|---|---|---|
| **Tauri:** "Commands without the async keyword are executed on the main thread"; "Asynchronous commands are preferred in Tauri to perform heavy work in a manner that doesn't result in UI freezes or slowdowns" (Calling Rust; Tauri's macro does the same: a plain `fn` gets the blocking execution context, `tauri-macros/src/command/wrapper.rs`) | 112 of 187 commands are plain `fn`, every list and search among them (F1) | **Large** (P3) |
| **Tauri:** "The event system is not designed for low latency or high throughput situations"; "Channels are designed to be fast and deliver ordered data" (Calling the frontend) | Terminals use a channel with 8 ms batching; run and chat streams send one JSON event per stream event (`src-tauri/src/lib.rs:290-295`) | **Medium** (P5) |
| **Tauri:** large answers are slow as JSON; "To return array buffers in an optimized way, use tauri::ipc::Response" | Lists come back whole, as JSON, after every write (F1) | Medium; fixed mostly by P2 (fewer reloads), not by a new format |
| **WebKitGTK:** hardware acceleration is the default, and in 2.54 "now always used"; the DMA-BUF renderer is the only accelerated one since 2.44 ("The X11 and WPE renderers have been removed in favor of the DMA-BUF one"); 2.54's Skia compositor improves MotionMark "by around 36%… while keeping the GPU markedly less busy" | Gizai sets no WebKit variable (FW) | None. Keep it so; `WEBKIT_DISABLE_DMABUF_RENDERER=1` would turn acceleration off altogether (WebKit's `AcceleratedBackingStore.cpp` leaves no transport mode when it is set, so `canUseHardwareAcceleration()` is false; my reading of the source **[I]**) |
| **Tauri's Linux graphics guide:** most NVIDIA problems come from "the WebKitGTK DMABUF renderer requesting buffer formats the NVIDIA driver does not provide"; `__NV_DISABLE_EXPLICIT_SYNC=1` fixes "Error 71" "without a performance cost"; the other switches cost speed; "Only ship an unconditional override like this if you have verified your app is affected" | The launcher's only graphics setting is Mesa's EGL on machines without NVIDIA (`install.sh:316-317`) | None for this PC; question 6 for NVIDIA machines. One third-party report (xnmp/tauri-explorer#1057, secondary) measured scrolling at 44 to 68 ms a frame on 2.54.1 with an RTX 3070, and 17 ms with `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` |
| **WebKitGTK memory:** the cache model `WEBKIT_CACHE_MODEL_DOCUMENT_VIEWER` would "Disable the cache completely, which substantially reduces memory usage" (the default is the web browser model); `MemoryPressureSettings` sets the web process's memory limit ("the system's RAM size with a maximum of 3GB" by default) | Neither is set (`rg` finds no cache model or memory pressure setting) | Small, worth one measured try: Gizai's pages come from inside the app, so a web cache buys little **[I]** (P9) |
| **WebView2 (Windows):** "Batch messages wherever possible, because frequent message passing can increase CPU usage"; set `MemoryUsageTargetLevel` to Low "on inactive WebViews" (wry has `set_memory_usage_level` since 0.35.0) | Unbatched run and chat events; no memory level when the window is closed to the tray | Medium on Windows; P5 batches for all three systems |
| **macOS (WKWebView):** each web view's page runs in its own WebContent process, and since macOS Sonoma "all drawing… occurs in the 'GPU Process'" | Nothing macOS-specific in drawing; a login shell runs once at start for the PATH (`src-tauri/src/shell_path.rs:8-18`) | None known |
| **React:** "lazy lets you defer loading component's code until it is rendered for the first time"; "memo is only valuable when your component re-renders often with the same exact props, and its re-rendering logic is expensive" (it is, for Markdown); `useDeferredValue` keeps typing responsive with a slow list; for long lists "we recommend using a technique known as 'windowing'" (the old docs; react.dev has no newer page) | No `lazy`; `MarkdownView`, `MessageGroup`, `Stream` and `Column` aren't memoised; only `DataTable` virtualises | **Large** for streaming (P4), medium for start (P6) and long lists (P7) |
| **React Compiler 1.0** (7 Oct 2025): "a build-time tool that optimizes your React app through automatic memoization" | Not used (`@vitejs/plugin-react` 5.2.0) | Option for later; P4's hand-made memoisation is smaller and easier to check |
| **Vite 8 / Rolldown:** dynamic `import()` splits code ("we don't put imported code together with the importers"); the object form of `manualChunks` is gone, and chunks are grouped with `build.rolldownOptions.output.codeSplitting` | Two dynamic imports (terminals, languages); no chunk groups; the chunk warning raised to 2,000 KB (`vite.config.ts:10`) | Medium (P6): `lazy` pages split by themselves; no chunk groups needed |
| **SQLite:** WAL: "readers do not block writers and a writer does not block readers", but only between connections ("there is no isolation between operations… within the same database connection"); `synchronous=NORMAL` is "safe from corruption" in WAL; long-lived connections should run `PRAGMA optimize=0x10002` when opened and then "once per day or once per hour" | One connection behind one `Mutex` for all reads and writes (`db.rs:13`, `:204-222`); `synchronous` NORMAL, as advised (`db.rs:164`); no `PRAGMA optimize` anywhere | **Large** for the lock (P3); small for `optimize` (P8) |

## 6. Options

Three ways to go about "a faster app". None needs a new library: `@tanstack/react-virtual` (MIT, 3.14.13 here, 3.14.19
on npm, 63,728 bytes unpacked; `npm view`, 14 Oct 2026) is already a dependency (`src/components/DataTable.tsx`), and the
rest is Tauri, React and SQLite as they are.

| | A. Measure, then fix by cost (recommended) | B. Rework the data flow in one go | C. Quick fixes, no harness |
|---|---|---|---|
| **What** | The harness first (P1), then one card per finding, biggest first, each proving its gain with the harness | A client-side store fed by row-level change notices (the changed ids with each notice, patched into the store), every command async with a pool of read connections, lists paged by the backend | Memoise the hot components, lazy pages, async commands and a table filter in `useData`, straight away |
| **Size** | About ten cards of one pull request each; P1 is the biggest | Three to five large cards that touch every page at once | Two or three cards |
| **Gain** | Each finding's gain, shown in numbers before and after | The largest in the long run, and what a future sync needs (`changes` is meant as its log, `crates/gizai-core/src/db.rs:1-2`) | Most of A's gains for F1 to F3 |
| **Risk** | Low: small steps, each checked; the order can change when the numbers come | High: one change across the app, hard to test in steps, in the way of every card in flight | Unproven: no numbers to show the gain or catch a step back, and the roadmap's "faster" can't be shown |
| **Libraries** | none new | possibly TanStack Query (MIT) for the store, or Gizai's own | none new |

**Recommendation: A.** The biggest costs (F1, F2) have small, local fixes; the harness turns "feels faster" into numbers
the roadmap can show, and it guards the gains afterwards. B's direction (say what changed, reload only that) is kept in the
small: P2 filters by table first, and row ids can follow if P1's numbers ask for it.

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

## 8. Proposed cards

In order. Each is one pull request for one agent run, starts in **To do** (none needs a screen designed first: they change
how screens work, not how they look), and measures its gain with P1's harness on `main` and on its branch, both tables in
the pull request. The expected gains are my estimates **[I]** until P1's baseline is in.

**Order and what can run at the same time:** P1a, then P1b. After P1b, two lanes side by side: frontend P2 → P4 → P7 and
P6 → P9; backend P3 → P5 and P8 → P10. P2 and P9 both change `Sidebar.tsx`, so P9 waits for P2; P3 and P8 both touch
`db.rs`, so P8 waits for P3.

### P1a. Benchmark data and query bench (backend)
- **Builds:** `crates/gizai-core/examples/prep_perf.rs` (the data in section 7, part 1) and
  `crates/gizai-core/examples/bench_queries.rs` (part 4); two markers in the fake CLIs: `FAKE_STREAM` in
  `crates/gizai-agents/tests/fake-claude.sh` (a run that writes text, tool-use and tool-result events at a set rate, e.g.
  `FAKE_STREAM 50x20` for 50 a second for 20 s) and `FAKE_CHAT_STREAM` in `fake-claude-chat.py` (a Team Lead answer of
  about 20 KB, streamed in small pieces).
- **Acceptance:**
  - [ ] `prep_perf` makes the data on a copy of the demo data and prints the ids the probe needs on one line.
  - [ ] `bench_queries` prints one JSON line per query: name, runs, median and p95 in µs.
  - [ ] A test covers each new fake marker; `cargo test --workspace` passes.
- **Builds on:** nothing. **Gain:** none by itself; it is the measuring stick.

### P1b. Benchmark harness: the probe and `scripts/perf.sh` (frontend)
- **Builds:** the `perf:<scenario>` probe in `src/selftest.ts` (start, board, chat, memory, runlog, idle, editor, lead),
  its `invoke` counter, `scripts/perf.sh` with its `/proc` sampling and `--compare`, and `scripts/bundle-size.mjs` (section
  7, parts 2, 3 and 5). Fills section 3's second table of this document with today's medians: the baseline.
- **Acceptance:**
  - [ ] `scripts/perf.sh` runs every scenario five times headless, prints the medians and writes `.devdata/perf/<commit>.json`.
  - [ ] `scripts/perf.sh --compare a.json b.json` prints both with the change in %.
  - [ ] `node scripts/bundle-size.mjs` lists each file with bytes and gzip bytes, start files apart.
  - [ ] Outside a `perf:` mode nothing in the app changes; `npm test`, `npm run build` and `scripts/ui-test.sh` pass.
  - [ ] The baseline table is committed.
- **Builds on:** P1a.

### P2. Reload only what a write changed (frontend)
- **Builds:** `useData` learns which tables a fetch reads (one map in one place, e.g. `src/lib/tables.ts`; a fetch it
  doesn't know reloads on any write, as today) and reloads only for those (`src/lib/useData.ts:15-20`); notices that arrive
  within one frame cause one reload; the sidebar and the page share one fetch of the same list (the cards, the team, the
  projects: `Sidebar.tsx:42-45`, `TasksPage.tsx:103-106`). The tables come from the notices the backend sends today:
  `settings`, `tasks`, `docs`, `workflow_states`, `actors`, `teams`, `team`, `labels`, `files`, `contacts`, `clients`,
  `projects`, `comments`, `chat_threads`, `chat_messages`, `runs`, `subscription_limits`, `pending_actions`
  (`src-tauri/src/commands.rs`, `runs.rs`, `chat.rs`, `tools/`).
- **Acceptance:**
  - [ ] A `settings` notice reloads nothing on the board; a `tasks` notice reloads the card list once, not twice.
  - [ ] Ten notices within 50 ms reload once.
  - [ ] Unit tests for the table map and the coalescing; `npm test` passes.
- **Expected gain:** calls per write on the board from about 8 to at most 2, on the Memory graph from about 14 to at most 3.
- **Measure:** P1 `runlog` (a fake run writing): calls per `rows-changed`, and the longest frame; before and after.
- **Builds on:** P1b.

### P3. No command on the main thread; reads beside writes (backend)
- **Builds:** the 112 plain `fn` commands (`src-tauri/src/commands.rs`, `terminal.rs`, `lib.rs`, `unsaved.rs`, `tour.rs`)
  run off the main thread: `async fn` that runs its body in `tauri::async_runtime::spawn_blocking`, or
  `#[tauri::command(async)]` for the ones that only read memory. `Db` (`crates/gizai-core/src/db.rs:12-18`) gets two
  read-only connections for `read`, so reads go on while the one writer writes (WAL allows it between connections).
- **Acceptance:**
  - [ ] No `#[tauri::command]` plain `fn` does database or file work (a test or a lint lists them).
  - [ ] A test: a read finishes while another thread holds a write transaction open.
  - [ ] `cargo test --workspace` and `scripts/ui-test.sh` pass.
- **Expected gain:** the window never waits for SQL, a file or the database lock; reads stop queueing behind an agent's
  writes.
- **Measure:** P1 `runlog` and `board` (drag while a fake run writes): the longest frame; `bench_queries` with a writer
  running: read p95.
- **Builds on:** P1b.

### P4. Stream without drawing the past again (frontend)
- **Builds:** `MarkdownView` memoised on its text, with its plugin list and components made once
  (`src/components/MarkdownView.tsx:26-46`); `MessageGroup` memoised (`src/components/chat/ChatMessage.tsx:75`) and
  `groupMessages` kept until the messages change (`ChatThread.tsx:233`), so a streamed piece re-renders only the draft;
  following the bottom once a frame instead of a layout effect per piece (`ChatThread.tsx:153-156`); the run panel's
  one-second clock in its own small component, and `Stream` memoised (`src/components/RunPanel.tsx:22`, `:90-91`);
  `mergeEvents` appends without sorting when the next event is the next `seq` (`src/lib/runs.ts:4-8`).
- **Acceptance:**
  - [ ] While the fake Team Lead streams, past messages don't render again (a test counts renders).
  - [ ] While a run is live, the log doesn't render again on the clock's tick.
  - [ ] `npm test` and `scripts/ui-test.sh` pass.
- **Expected gain:** per streamed piece, from re-parsing every message's Markdown to parsing only the draft; the longest
  frame while streaming no longer grows with the chat's length.
- **Measure:** P1 `lead` (the 1,000-message chat) and `runlog`: frames a second and the longest frame.
- **Builds on:** P1b; can run beside P5.

### P5. Batch the run and chat streams (backend)
- **Builds:** run and chat events go to the window at most once every 50 ms per run or chat, all events since the last
  send in one message, as the terminal does with 8 ms (`crates/gizai-agents/src/terminal.rs:38`); `run_events` returns only
  what a panel lacks. `src/api.ts`'s `onRunEvent` and `onChatEvent` unpack the batches, so the components don't change.
- **Contract (word for word):**
  - Event `run-events` (replaces `run-event`): `{ runId: string; events: { seq: number; event: RunEvent }[] }`, in `seq`
    order, at most every 50 ms per run.
  - Event `chat-events` (replaces `chat-event`): `{ threadId: string; events: ChatEvent[] }`, at most every 50 ms per chat;
    `delta` events of one block that follow each other are joined into one.
  - Command `run_events(runId: string, afterSeq: number | null) -> SeqEvent[]`: the events with `seq > afterSeq` (all when
    `null`), at most the last 500.
  - `onRunEvent(cb)` and `onChatEvent(cb)` in `src/api.ts` keep their signatures and call `cb` once per event of a batch.
- **Acceptance:**
  - [ ] A fake run streaming 200 events a second makes at most 20 `run-events` messages a second, and loses or doubles none.
  - [ ] `cargo test --workspace`, `npm test` and `scripts/ui-test.sh` pass.
- **Expected gain:** messages to the window during a run from one per stream event to at most 20 a second; React renders
  follow (React batches the updates of one message).
- **Measure:** P1 `runlog` and `lead`: events heard a second, CPU % of the app and its web process.
- **Builds on:** P1b, and P3 (both change `src-tauri/src/runs.rs`).

### P6. Load a page when it opens; the self-test only in test runs (frontend)
- **Builds:** `React.lazy` for the pages that aren't the start page (`src/App.tsx:10-25`), each with a light fallback; the
  self-test loaded with `import()` only when `app_info` says it is a test run (`src/App.tsx:27`, used at `:77-167`).
- **Acceptance:**
  - [ ] `scripts/bundle-size.mjs`: the start files hold no self-test code and are smaller than the baseline's by the
    agreed amount.
  - [ ] Every page still opens; `scripts/ui-test.sh` and the tour capture pass.
- **Expected gain:** the start files lose the self-test (about a fifth of the app's own code) and the pages that aren't the
  board; the first card shows sooner by their parse and run time.
- **Measure:** `scripts/bundle-size.mjs` start bytes; P1 `start`: `domContentLoadedEventEnd` and the first card, median of
  four warm starts.
- **Builds on:** P1b.

### P7. Draw only what is in sight in long lists (frontend)
- **Builds:** the chat thread, a run's log and the Memory note list drawn with `@tanstack/react-virtual` (as
  `src/components/DataTable.tsx` already is), with measured heights, keeping the chat at the bottom while you are there.
- **Acceptance:**
  - [ ] The 1,000-message chat has at most about 60 messages in the page at once.
  - [ ] Opening it takes about as long as a 50-message chat.
  - [ ] `scripts/ui-test.sh` passes (the chat, Memory and run tests).
- **Expected gain:** a long chat opens and scrolls like a short one, and uses less of the web process's memory.
- **Measure:** P1 `chat`: first screen in ms, nodes in the page, the web process's RSS.
- **Builds on:** P4. **Risk:** the window's own find (Ctrl+F) can't see messages out of sight; the Chat Archive's search
  still finds them.

### P8. Database: indexes and queries (backend)
- **Builds:** migration `0018`: `CREATE INDEX chat_messages_thread ON chat_messages(thread_id, created_at)` and
  `CREATE INDEX comments_task ON comments(task_id, created_at)` (neither table has an index today:
  `crates/gizai-core/migrations/0001_init.sql:285-291`, `:350-359`); `PRAGMA optimize=0x10002` when the database opens and
  with the daily housekeeping (`crates/gizai-core/src/db.rs:153-168`, `src-tauri/src/lib.rs:544-562`).
- **Acceptance:**
  - [ ] A test reads `EXPLAIN QUERY PLAN` for a chat's messages and a card's comments and finds the index, not a `SCAN`.
  - [ ] `bench_queries` shows the gain; `cargo test --workspace` passes.
- **Expected gain:** opening a chat or a card no longer scans every message or comment ever written.
- **Measure:** `bench_queries` before and after.
- **Builds on:** P1a and P3.

### P9. Quiet when nothing happens (frontend)
- **Builds:** the sidebar's 30-second check of open Editor folders (`src/components/Sidebar.tsx:294`) and the Editor's
  10-second and 3-second checks (`src/pages/EditorPage.tsx:194`, `:248`, `:389-394`) run only while the window is in
  sight and in use, and once when it comes back; first check that `document.hidden` turns true on Hyprland when the window
  is closed to the tray or on another workspace, and else use Tauri's window focus.
- **Acceptance:**
  - [ ] With the window hidden, P1's `idle` scenario counts no Editor or git calls.
  - [ ] Coming back shows fresh git marks within a second.
- **Expected gain:** no git processes and no reloads while you aren't looking.
- **Measure:** P1 `idle` and `editor`: calls a minute, CPU % at rest.
- **Builds on:** P2 (both change `Sidebar.tsx`).

### P10. Small backend costs (backend)
- **Builds:** the login PATH read once and kept (refreshed when Settings → Coding CLIs looks again), instead of a login
  shell per call (`src-tauri/src/runs.rs:785-797`); WebKitGTK's cache model set to document viewer on Linux, kept only if
  P1 shows less memory and no slower page; the start's phases (lock, database, seed, recovery, housekeeping) timed and
  written to the log once, so a slow start says where it went (`src-tauri/src/lib.rs:227-276`).
- **Acceptance:**
  - [ ] A test: two calls of the PATH lookup start one shell.
  - [ ] The start's phase times appear in the log; `cargo test --workspace` passes.
- **Expected gain:** one shell start less per run, terminal and Editor action; maybe tens of MB less for the web process
  **[I]**.
- **Measure:** P1 `runlog` and `start`; the web process's RSS in `idle`.
- **Builds on:** P5 (both change `src-tauri/src/runs.rs`).

## 9. Open questions

1. **A baseline on your machine now, or with P1?** Two things you can run in seconds give today's numbers before any card:
   the bundle (`source scripts/env.sh && npm run build` in this worktree; `dist/` is ignored by git) and your Gizai's memory
   (the `ps` line in section 3, once at rest and once while an agent works). *Recommendation:* let P1 take all of it, in the
   cage, so every later card compares against the same setup; run the `ps` line once if you want a feel for today's memory.
2. **What should feel faster first?** Staying smooth while agents and the Team Lead work (F1, F2), the start (F3), or long
   chats and big boards? *Recommendation:* smooth while agents work: it is paid all day, and its fixes are the smallest.
3. **Budgets.** Should the harness fail a card that makes things slower, with fixed limits (for example a warm start to the
   board under 1.5 s, the longest frame under 50 ms while a run streams, at most two calls per write)? *Recommendation:* yes,
   but set the limits from P1's baseline, not before.
4. **Long chats.** Load only a chat's last 200 messages, with "Show older"? *Recommendation:* not yet: draw only what is in
   sight first (P7), and page only if P1 shows that loading 1,000 messages is itself slow.
5. **The self-test out of the app's start.** It would load only in test runs (`GIZAI_SELFTEST` set), as a separate file
   inside the app. *Recommendation:* yes; the UI tests keep working, and only test runs load it.
6. **NVIDIA.** Does this PC have an NVIDIA GPU next to its main one (the launcher's `lspci` check, `install.sh:316`)? It
   decides whether WebKitGTK's DMA-BUF path is worth a look. *Recommendation:* tell me or the P1 card; no change before.
7. **macOS and Windows.** Measure there too? *Recommendation:* not now: the harness is Linux-only (cage), and the fixes are
   the same on all three; CI's runners are too noisy for frame numbers.

## 10. Sources

All read on 14 Oct 2026. P: primary (official docs, source code, release notes, registry data); S: secondary.

**Tauri and wry**
- P: Calling Rust from the frontend (async commands, returning data): https://v2.tauri.app/develop/calling-rust/
- P: Calling the frontend from Rust (events and channels): https://v2.tauri.app/develop/calling-frontend/
- P: Tauri 2.12.1's `setup` (windows first, then the setup closure): https://raw.githubusercontent.com/tauri-apps/tauri/tauri-v2.12.1/crates/tauri/src/app.rs
- P: The command macro's execution context: https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-macros/src/command/wrapper.rs
- P: Linux graphics (NVIDIA, DMA-BUF, compositing switches): https://v2.tauri.app/develop/debug/linux-graphics/
- P: Debugging (the inspector in release builds): https://v2.tauri.app/develop/debug/
- P: Window config (background throttling per system): https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.WindowConfig.html
- P: wry 0.35.0 (`set_memory_usage_level` on Windows): https://v2.tauri.app/release/wry/v0.35.0/
- S: tauri#3988 "Bad performance on linux": https://github.com/tauri-apps/tauri/issues/3988
- S: tauri-explorer#1057 (scroll timings on WebKitGTK 2.54.1 with NVIDIA): https://github.com/xnmp/tauri-explorer/issues/1057

**WebKitGTK and WebKit**
- P: WebKitGTK 2.54 highlights: https://webkitgtk.org/2026/09/16/webkitgtk-2.54-highlights.html
- P: WebKitGTK 2.54.0 release news: https://webkitgtk.org/releases/webkitgtk-2.54.0.tar.xz.news
- P: WebKitGTK 2.52 highlights: https://webkitgtk.org/2026/03/18/webkitgtk-2.52-highlights.html
- P: WebKitGTK 2.50 highlights: https://webkitgtk.org/2025/11/26/webkitgtk-2.50.html
- P: WebKitGTK 2.48 highlights: https://webkitgtk.org/2025/04/08/webkitgtk-2.48.html
- P: WebKitGTK 2.46 highlights: https://webkitgtk.org/2024/10/04/webkitgtk-2.46.html
- P: WebKitGTK 2.44 highlights: https://webkitgtk.org/2024/03/27/webkigit-2.44.html
- P: WebKitGTK 2.42.0 released (the DMA-BUF renderer): https://webkitgtk.org/2023/09/15/webkitgtk2.42.0-released.html
- P: `AcceleratedBackingStore.cpp` (`WEBKIT_DISABLE_DMABUF_RENDERER`): https://raw.githubusercontent.com/WebKit/WebKit/main/Source/WebKit/UIProcess/gtk/AcceleratedBackingStore.cpp
- P: `WebProcessGLib.cpp` (`WEBKIT_SKIA_ENABLE_CPU_RENDERING`): https://raw.githubusercontent.com/WebKit/WebKit/main/Source/WebKit/WebProcess/glib/WebProcessGLib.cpp
- P: Hardware acceleration policy: https://webkitgtk.org/reference/webkit2gtk/stable/property.Settings.hardware-acceleration-policy.html
- P: Cache model: https://webkitgtk.org/reference/webkit2gtk/stable/enum.CacheModel.html
- P: Memory pressure settings: https://webkitgtk.org/reference/webkit2gtk/stable/method.MemoryPressureSettings.set_memory_limit.html
- P: WebKit's environment variables (last edited 2022): https://trac.webkit.org/wiki/EnvironmentVariables
- P: Web Inspector Timelines (CPU and memory): https://webkit.org/web-inspector/timelines-tab/
- P: WebKit's process architecture: https://docs.webkit.org/Deep%20Dive/Architecture/WebKit2.html
- P: WebKit features in Safari 17.0 (the GPU process on macOS Sonoma): https://webkit.org/blog/14445/webkit-features-in-safari-17-0/
- P: Arch's `webkit2gtk-4.1` package data: https://archlinux.org/packages/search/json/?name=webkit2gtk-4.1

**WebView2**
- P: Performance best practices: https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance
- P: Process model: https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model
- P: `MemoryUsageTargetLevel`: https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2.memoryusagetargetlevel

**React, Vite and Rolldown**
- P: `lazy`: https://react.dev/reference/react/lazy
- P: `memo`: https://react.dev/reference/react/memo
- P: `useDeferredValue`: https://react.dev/reference/react/useDeferredValue
- P: Optimizing performance, windowing (legacy docs): https://legacy.reactjs.org/docs/optimizing-performance.html
- P: React Compiler 1.0: https://react.dev/blog/2025/10/07/react-compiler-1
- P: `StrictMode`: https://react.dev/reference/react/StrictMode
- P: `Profiler`: https://react.dev/reference/react/Profiler
- P: Vite 8 migration (`manualChunks`): https://vite.dev/guide/migration
- P: Vite build guide (`codeSplitting`): https://vite.dev/guide/build
- P: Vite build options (`build.target`): https://vite.dev/config/build-options
- P: Rolldown `codeSplitting`: https://rolldown.rs/reference/OutputOptions.codeSplitting
- P: Rolldown automatic code splitting: https://rolldown.rs/in-depth/automatic-code-splitting

**SQLite**
- P: Write-ahead logging: https://www.sqlite.org/wal.html
- P: Isolation: https://www.sqlite.org/isolation.html
- P: `PRAGMA synchronous`: https://www.sqlite.org/pragma.html#pragma_synchronous
- P: `PRAGMA optimize`: https://www.sqlite.org/pragma.html#pragma_optimize
- P: `EXPLAIN QUERY PLAN`: https://www.sqlite.org/eqp.html

**npm**
- P: `@tanstack/react-virtual` (3.14.19, MIT, 13 Oct 2026): https://registry.npmjs.org/@tanstack/react-virtual/latest
- P: `react-window` (2.3.4, MIT, 10 Oct 2026): https://registry.npmjs.org/react-window/latest
