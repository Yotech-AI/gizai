# Gizai: terminals in the sidebar and a panel in the Code Editor (research, 2026-10-12)

Status: research only, no code. Written by the R&D Agent for GA-124 on 2026-10-12, against `main` at `1dd5a08`, merged into this branch (Gizai 0.8.0, with all of GA-91's Code Editor cards: PRs #96, #97, #102 and #103, merged that day). `docs/research/code-editor.md` is the model and is cited for what it already established.
Legend: **[V]** = verified today, in the code (file and line), with the command named (`git`, `gh`, `npm view`, `npm ls`, `rg`), or in the primary source named (its link and the date read are in §8). **[I]** = my judgement, or a fact I could not check in a primary source (the reason is given).
Limits: the run may not start node, cargo builds or a browser, so nothing was measured or tried in a webview; what rests on a measurement is marked [I] and given to a card to measure.

---

## 1. Summary

- **It can be built well on Linux, macOS and Windows, and one terminal can show in both places.** Gizai has no terminal parts yet; everything below is new, next to the Editor of GA-91's cards.
- **Backend:** `portable-pty` 0.9.0 (WezTerm's, MIT) is one pseudo-terminal API over Linux, macOS and Windows' ConPTY. Gizai starts your shell (`$SHELL`, a login shell on macOS; PowerShell or cmd on Windows) with your environment in a folder the Editor may open. Closing ends the shell and everything it started, as Stop ends a run (SIGHUP and SIGKILL on Unix, a Job Object on Windows). The output goes to the window as bytes over a Tauri `Channel` per terminal, batched, with acknowledgements so a flood can't freeze the window.
- **Window:** xterm.js 6 (VS Code's terminal) with the fit, WebGL, search, web-links and unicode11 addons. WebGL is used where the webview has hardware WebGL2 (WebView2, WKWebView since Safari 15, WebKitGTK since 2.40), and the DOM renderer everywhere else. The terminal uses the app's mono font and new `term-*` colour tokens, light and dark. The security policy stays as it is; one new right (reading the clipboard's text) makes Ctrl+Shift+V paste on Linux.
- **One terminal in two places:** one list of sessions in the backend, and one xterm.js view per session in the window. The view lives as long as the session and moves between the Terminal page and the Editor's panel, as VS Code moves a terminal between its panel and editor area. A terminal shows in one place at a time: the place that asked last.
- **Screens:** a folding Terminal section above Editor, with a row per open terminal (any number of them). A Terminal page with the folder's terminals as tabs. A Terminal button between Fetch and Open in, which opens a panel at 30% of the Editor, with a drag handle, a remembered height and Ctrl+\`.
- **Safety:** no agent or Team Lead tool reaches a terminal. Nothing from a terminal goes to disk, Memory or a card, and a terminal starts only where the Editor may open a folder. **Free**, with the Code Editor.
- **Recommended:** option A (§4). **Cards (§6):** TD (design) and TB (backend) first, then TF0 (the view), then TF1 (sidebar and page) and TF2 (the Editor's panel) at the same time. **Open questions (§7):** 11, each with a recommendation.

---

## 2. The project now

### 2.1 What Gizai does for its users

Gizai is a local desktop app for one developer and a team of coding agents: clients, projects and cards on a board, agents (Claude Code and other CLIs) that work each card headless in its own git worktree, a Team Lead in Chat, Memory, Usage and Settings (README.md; `docs/research/code-editor.md` §2.1 describes it in full). [V] Since 12 Oct 2026 it also has the **Code Editor** of GA-91's cards: a folding Editor section in the sidebar listing the folders you opened, and an Editor page with a file tree, tabs, CodeMirror, search and Open in (PR #96, `src/pages/EditorPage.tsx`, `src/components/editor/`). [V `gh pr list`] The rest of GA-91's cards were merged the same day: a card's Changes view (F1, PR #97), the Editor's Behind main bar with **Fetch** and **Update** (F3, PR #103) and a person's card in the Editor (F4, PR #102). [V `git log --merges`] **Gizai has no terminal of its own yet**: no pseudo-terminal crate, no xterm.js, nothing in `Cargo.lock` or `package-lock.json` (`rg -i "pty|xterm|conpty|vt100|alacritty"`). Its only "terminal" is a run's **Open in terminal** button, which copies `cd '<worktree>' && claude --resume <id>` to the clipboard for your own terminal app (`src/components/RunPanel.tsx:124`, `src/lib/runs.ts:28-31`). [V]

### 2.2 How it is built

| Part | What it is | Where |
|---|---|---|
| Desktop shell | Tauri 2.12 (locked 2.12.1, wry 0.57.0; `Cargo.lock:3977-3978`, `:5463-5464`), crate `gizai`, Rust 2024 edition. A thin adapter: commands, app state, background loops, events. One window, `main`, 1440 × 900 (at least 900 × 600) (`src-tauri/tauri.conf.json:15`). Closing it only hides it; Quit is in the tray, Settings and Cmd+Q (`src-tauri/src/lib.rs:622-637`, `:334`, `:345-360`). | `src-tauri/`; `AppState` `lib.rs:36-71`; commands registered `lib.rs:567-610` (the Editor's at `:605-609`); events from `ui_notifier` `lib.rs:274-296` |
| Core | Data and rules, never Tauri (CLAUDE.md:36-38). SQLite. The folders the Editor may open: `crates/gizai-core/src/editor.rs` (`folders` `:103-148`, `find` `:150-168`, the type `EditorFolder` `:45-64`), with the refusals of `folders.rs` (key folders `:45-57`, `refusal_in` `:199-228`). | `crates/gizai-core/src/` |
| Agents | Everything that starts programs: git, worktrees, the coding CLIs, and the process trees: `os::command` (`crates/gizai-agents/src/os.rs:19-42`), `os::find_in` (`:64-91`), `spawn_tree` and `spawn_tree_tokio` (`:251-287`, `:290-316`) and `Tree::end` (`:204-227`). The Editor's files, search and git (`crates/gizai-agents/src/editor.rs`, its path gate `:226-337`). | `crates/gizai-agents/src/` |
| MCP | The `gizai-mcp` shim and the Team Lead's 46 tools (dispatch `src-tauri/src/tools/mod.rs:160-208`, catalog `:371-479`). | `crates/gizai-mcp/`, `src-tauri/src/tools/` |
| Screens | React 19.3, TypeScript 6, Vite 8. A hash router (`src/router.ts`; `href` `:49-58`, `parseHash` `:31-47`); pages are one chain of conditions in `src/App.tsx:173-189`, so **leaving a page unmounts it** (the Editor at `:186`); only Settings keeps its tabs mounted (`SettingsPage.tsx:35-36`). `src/api.ts` holds the commands (`invoke<T>("snake_name", { camelArgs })`) and the event listeners (`listen<P>(…)`). | `src/pages/`, `src/components/`, `src/lib/` |
| Sidebar | Projects (`Sidebar.tsx:71-78`), **Editor** (`:80`, `EditorSection` `:179-246`), Agents (`:82-94`), Memory (`:96-97`). `FoldSection` (`:119-139`) draws a heading with an optional `+` and a caret, and unmounts its rows while folded (`:136`). Folded sections are kept in `localStorage` `gizai.sidebar.folded`, type `Fold = "editor" \| "agents" \| "memory"` (`src/lib/sidebar.ts:6-8`). | `src/components/Sidebar.tsx`, `src/lib/sidebar.ts` |
| The Editor | `#/editor/<folder>`; the folders you opened (at most 8) and each folder's tabs are kept in `localStorage` (`gizai.editor.opened`, `gizai.editor.tabs`; `src/lib/editor.ts:10-11`, `:59-87`). The page: the top bar (`EditorPage.tsx:559-566`), whose actions (`:562-565`) are **Fetch** (`:563`, when the folder has a main branch to compare with) and **Open in** (`:564`). Under it is one bar for the folder's state: an agent at work, your own card's bar (F4), another's card, or Behind main (F3) (`:569-603`). Then `.ed` (`:605`) holds the tree (`aside.ed-side` `:606-626`) and `.ed-main` (`:628`): tabs (`:629-660`), the file bar, the code (`.ed-code` `:666-679`) and the status bar (`.ed-status` `:681-696`). Its keys sit on `window` and don't look at where the key was pressed (`:516-529`). | `src/pages/EditorPage.tsx`, `src/components/editor/`, `src/styles/components.css:539-757` |
| Design system | Tokens in `design/tokens.json` → `src/styles/tokens.css` (dark `:2`, light `[data-theme="light"]` `:76`); D1's `syn-*` colours (`tokens.css:54-67`, light `:116-120`); D2's sizes `--size-tree` 260px, `--size-tree-row` 28px, `--size-tab` 36px, `--size-statusbar` 24px (`:152-155`); `--font-mono: "Atkinson Hyperlegible Mono", ui-monospace, monospace` (`:157`). Teal `--live` only for an agent at work, magenta `--needs` only for "needs you", no teal or magenta in code (`design/README.md:35-38`, `:50-51`). Lucide icons; the concept "terminal" is the icon `terminal` (`design/README.md:145`). **No terminal (ANSI) colours yet.** The mono font is the one picked in Settings → Appearance: Atkinson Hyperlegible Mono (the default), JetBrains Mono, Geist Mono or Hack (`src/styles/appearance.css:11-15`). The bundled Atkinson, JetBrains and Geist files are cut to Latin ranges (`src/styles/fonts.css:2-5`, `:21-26`), so they have **no box-drawing characters** (U+2500–257F); Hack is a web subset of its own (`:27-28`), whose characters weren't checked. | `design/`, `src/styles/` |
| Appearance | Theme, fonts and sizes are attributes and CSS variables on `<html>` (`src/lib/appearance.ts:85-97`); a change fires no DOM event, only a listener set inside the module (`:124-137`, `useAppearance` `:151-160`). The `t` key toggles the theme (`src/App.tsx:43`). | `src/lib/appearance.ts`, `src/styles/appearance.css` |
| Security | Content security policy: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src ipc: http://ipc.localhost; …` (`tauri.conf.json:18`). The window may use `core:default`, `dialog:allow-open` and the opener's URL permissions (`src-tauri/capabilities/default.json:8-13`). No file system or shell plugin: every file and program goes through a Rust command. | |
| Data on disk | `~/.local/share/gizai` on Linux (`%APPDATA%\Gizai`, `~/Library/Application Support/Gizai`; `lib.rs:119-144`; `GIZAI_DATA_DIR` picks another): the database, `runs/` (run logs), `worktrees/`, `code/`, `chat/`, `lead/`. | |

### 2.3 How processes, output and quitting work today

- **Starting and ending.** An agent's CLI starts with `os::spawn_tree_tokio` (`crates/gizai-agents/src/process.rs:186`): on Unix in a process group of its own (`process_group(0)`, `os.rs:293`; the std `spawn_tree` does the same at `:255`), on Windows suspended inside a Job Object that ends everything in it when its handle closes (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, `os.rs:341-364`, `:366-402`). **Stop** sends SIGINT, SIGTERM after 5 s and SIGKILL after 10 s to the group; on Windows it ends the job at once (`crates/gizai-agents/src/process.rs:243-262`; `docs/PLATFORMS.md:21`). After any exit, what is left gets SIGTERM, then SIGKILL after 3 s (`process.rs:263-276`). [V]
- **PATH.** On Linux the launcher starts Gizai through `bash -lc`, so Gizai has your login shell's PATH (`install.sh:319-321`), and `runs::command_path` adds the login PATH's folders again per command (`src-tauri/src/runs.rs:773-793`). On macOS Gizai takes the login shell's PATH at start (`$SHELL -l -i -c`, at most 5 s; `src-tauri/src/shell_path.rs:6-18`, `:32-87`; `main.rs:24-25`). Windows apps get the full PATH. A run gets Gizai's environment plus its own variables, handed over at `process.rs:180`: `TMPDIR` in the worktree (`worktree.rs:280-283`), `CLAUDE_CODE_DISABLE_AUTO_MEMORY` (`crates/gizai-agents/src/claude.rs:7`, `:58-60`), …. [V]
- **Output to the window.** A run's output is sent as `run-event`s, the events parsed from each line (`process.rs:82`), sent with `app.emit` to the whole window, without batching (`process.rs:205-214`, `runs.rs:1114-1123`, `lib.rs:285`); the page filters it by run (`RunPanel.tsx:58-74`). **No code uses Tauri's `Channel` yet** (`rg -w Channel`). [V]
- **Quitting.** The first quit while agents work stops them first (at most 12 s), then exits; on exit Gizai ends WebKitGTK's page process and what is left of the agents (`src-tauri/src/quit.rs:25`, `:35-60`; `lib.rs:617-621`, `:643-650`). Since GA-122, closing or quitting with unsaved text in the doc or Memory editor waits for the page to save it first (`src-tauri/src/unsaved.rs:1-10`). Settings → Quit asks in the button itself when agents work ("Stop them and quit?", `src/components/QuitSettings.tsx:20-30`); the tray's Quit, Cmd+Q and signals don't ask. SIGTERM, SIGINT and SIGHUP quit the same way (`quit.rs:78-104`). [V]

### 2.4 What the terminal touches

| Area | Today | For the terminal |
|---|---|---|
| Sidebar (`Sidebar.tsx`, `sidebar.ts`) | Projects, Editor, Agents, Memory; `Fold` has `"editor"`, `"agents"`, `"memory"` | A folding **Terminal** section between Projects and Editor (`:78`–`:80`); `"terminal"` in `Fold` |
| Router, App | `#/editor/<folder>`; one page at a time | `#/terminal/<id>`; the terminal's view must outlive the page that shows it (§5.3) |
| Editor page | Top bar actions: Open in (Fetch comes with its Behind main card); code above the status bar | **Terminal** button before Open in (after Fetch once it is there); a bottom panel between `.ed-code` and `.ed-status`; Ctrl+\` |
| Editor's folder rules | `editor::find` checks every Editor call (`gizai-core/src/editor.rs:150-168`) | the same check for a terminal's start folder |
| Processes (`os.rs`) | `spawn_tree` with pipes; process groups or Job Objects | a pseudo-terminal needs its own way to start (§3.2 Q1), still ended as a whole |
| Events and IPC | `app.emit` per line; no `Channel` | a `Channel` per terminal for its bytes, batched, with flow control |
| Quit (`quit.rs`) | stops runs and chat answers | ends every terminal too |
| Design tokens | `syn-*`, no terminal colours, mono font without box drawing | 16 terminal colours and a cursor and selection colour, dark and light; a fallback font for box drawing |
| Free or paid (`docs/PAID-FEATURES.md`) | "the code editor and a card's changes" is free (`:36`, table `:46`) | the terminal goes with it |

What the terminal must not touch: the agents' runs and their logs (`<data>/runs`), Memory, cards, and the MCP tools. [V for what exists; I for the rule]

---

## 3. The feature

### 3.1 What it should do

From the card (Jefsev, 12 Oct 2026):

1. **A Terminal section in the sidebar, above Editor.** The terminals you open are rows under Terminal, as the Editor's folders are rows under Editor. You can close them, so there may be none, 3 or 10: no fixed number.
2. **A click on a terminal's row** opens that terminal in the main panel, to run commands in folders, use git and so on.
3. **A Terminal button in the Code Editor's top bar, between Fetch and Open in.** It opens a terminal panel at the bottom of the Editor, in the root of the folder the Editor shows, like PhpStorm and VS Code. The panel starts at 30% of the Editor's height and can be dragged higher or lower.
4. **The same terminal in both places:** one list of sessions and one terminal component, so a terminal opened in the Editor's panel is also a row under Terminal and can be opened on its own page, and the other way round.

Not in this card: building it, and a terminal for agents.

### 3.2 The eight questions

**Q1. The pseudo-terminal in Rust: which crate, which shell, which environment, resizing, and how the output reaches the window without flooding it.**

Answer:

- **The crate: `portable-pty` 0.9.0**, the pseudo-terminal layer of the WezTerm terminal. It is MIT; 0.9.0 was published on 2025-02-11; it has 20.4 million downloads, 11.7 million of them in the last 90 days [V crates.io]. It is one API on the three systems: `native_pty_system().openpty(size)` gives a master and a slave; `slave.spawn_command(CommandBuilder)` starts the program; `master.try_clone_reader()` and `master.take_writer()` read and write (one writer per terminal); `master.resize(size)`; `master.process_group_leader()` gives the foreground process group on Unix [V docs.rs and the 0.9.0 source].
  - *Linux and macOS:* the program gets the slave as stdin, stdout and stderr. Before it runs, portable-pty resets SIGCHLD, SIGHUP, SIGINT, SIGQUIT, SIGTERM and SIGALRM to their defaults and clears the signal mask. It also calls `setsid()`, makes the pseudo-terminal the controlling terminal (`TIOCSCTTY`) and closes the descriptors the program shouldn't inherit. `resize` is `TIOCSWINSZ` [V `src/unix.rs`]. Dropping the writer sends a newline and the end-of-file character (Ctrl+D) to the shell [V `src/unix.rs`], so TB closes a terminal on purpose (Q3) and never by just dropping it. [I]
  - *Windows:* ConPTY. portable-pty uses a `conpty.dll` and `OpenConsole.exe` shipped next to the app when there are any, and otherwise `CreatePseudoConsole` from kernel32, which needs Windows 10 October 2018 (1809) or newer. It passes the flags `INHERIT_CURSOR`, `RESIZE_QUIRK` and `WIN32_INPUT_MODE`; `resize` is `ResizePseudoConsole`; the program starts with `CreateProcessW` (`EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT`, not suspended) [V `src/win/psuedocon.rs`]. With `INHERIT_CURSOR` the console first asks the terminal for the cursor's position (`ESC [6n`) and waits for the answer, which xterm.js gives through `onData`. So input must be wired before the first output is drawn, and TF0's view attaches at once [I]. Gizai uses kernel32's ConPTY and ships no `conpty.dll`: that would be one more binary to ship and keep up to date [I].
  - *Maintenance, the main risk:* nothing has been released since February 2025, but the crate's folder in WezTerm's repository still gets fixes (commits on 2026-06-07, 2026-08-25 and 2026-09-12) [V GitHub API]. One of them matters here. Before #7709 (2026-06-07, not released), `Child::kill()` on Windows read `TerminateProcess`'s result the wrong way round: it reported failure when it worked and success when it failed [V the commit]. TB doesn't use `kill()` on Windows; it ends the terminal's Job Object (Q3). TB pins `portable-pty = "=0.9.0"`. If a later fix is needed, the way out is a git dependency on a WezTerm commit, or a copy of the MIT crate [I]. Its dependencies include `nix` 0.28 (Gizai locks 0.29.0, `Cargo.lock:2391-2392`, so there would be two), `serial2`, `filedescriptor`, and on Windows `winapi` and `winreg` [V crates.io, the dependency list of 0.9.0].
  - *The other choices* are in §4: `tauri-plugin-pty` (the window starts any program; output by polling), `alacritty_terminal` (a whole terminal emulator in Rust), or `nix`/`rustix` with a hand-written ConPTY.
- **The shell** [I, unless marked]:
  - *Linux and macOS:* `$SHELL`, else your entry in the password database, else `/bin/sh`. That is what portable-pty's `get_shell` does [V `src/cmdbuilder.rs`]. On macOS it is a **login shell** (`-l`), as Terminal.app does, so `/etc/zprofile` and `~/.zprofile` set up PATH. portable-pty's "default program" runs a login shell on every Unix, with argv[0] `-zsh` and the home folder as its folder [V `src/cmdbuilder.rs`]. On Linux an **interactive shell that isn't a login shell**, as GNOME Terminal, Konsole and VS Code start it, so `~/.bashrc` or `~/.zshrc` runs and `~/.profile` doesn't run again (Gizai already started from a login shell, below). TB builds the command itself (`CommandBuilder::new(shell)`, `cwd` the start folder, `-l` on macOS) rather than `new_default_prog`, so one code path sets the folder.
  - *Windows:* PowerShell 7 (`pwsh.exe`) when it is on PATH, else Windows PowerShell (`powershell.exe`, part of Windows 10 and 11), else `%ComSpec%` (cmd.exe). PowerShell starts with `-NoLogo`. portable-pty's own default on Windows is `%ComSpec%`, which is cmd [V `src/cmdbuilder.rs`], so TB names the program itself. Git Bash, WSL and a Shell setting are left out (open question 8).
- **The environment: your own, as Gizai has it.**
  - *Linux:* the launcher starts Gizai through `bash -lc` (`install.sh:320`), so Gizai has your login environment. [V]
  - *macOS:* Gizai adds your login shell's PATH when it starts (`src-tauri/src/shell_path.rs:8-18`). [V]
  - *Windows:* portable-pty starts from Gizai's environment, reads the system and user variables from the registry again and joins their PATHs, so a PATH changed after Gizai started is there too [V `src/cmdbuilder.rs`, `get_base_env`].
  - *Then the shell* runs your own startup files, as in any terminal. `runs::command_path` is not used: it adds the login PATH for programs Gizai starts without a shell (`src-tauri/src/runs.rs:773-793`), and a terminal's shell builds its own PATH. [V the function; I the rule]
  - *What TB adds:* `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=gizai` and `TERM_PROGRAM_VERSION`. On macOS it also sets `LANG=en_US.UTF-8` when none of `LANG`, `LC_ALL` and `LC_CTYPE` is set: an app started from the Dock has none, and programs then print `?` for non-ASCII. It removes `GIZAI_*` variables, which are meant for Gizai itself (such as `GIZAI_DATA_DIR`, `src-tauri/src/lib.rs:119-125`). [I]
- **Resizing:** the window measures the place the terminal shows in (the fit addon works out columns and rows from the element and the font's cell size) and calls `terminal_resize`, and TB calls `master.resize` [V the APIs]. Only the place that shows the terminal sends sizes (Q4), and a hidden terminal keeps its last size. During a drag TF2 sends the size at most every 50 ms and once when the drag ends, because each resize makes full-screen programs redraw. [I]
- **Output to the window: a Tauri 2 `Channel` per terminal, not events.**
  - *Why a Channel:* Tauri's docs say the event system "is not designed for low latency or high throughput situations", and channels "are designed to be fast and deliver ordered data", which Tauri itself uses for "child process output" [V v2.tauri.app, "Calling the Frontend from Rust"].
  - *How the bytes travel:* `Channel::send` takes `InvokeResponseBody::Raw(Vec<u8>)` or a `tauri::ipc::Response`, and the bytes reach JavaScript as an `ArrayBuffer`. Tauri puts up to 1,024 raw bytes in the message itself and makes the window fetch anything larger (`plugin:__TAURI_CHANNEL__|fetch`), so fewer, larger messages cost less [V `ipc/channel.rs`, `ipc/mod.rs` at `tauri-v2.12.1`]. It sends bytes, not text: one UTF-8 character can be split over two reads, and xterm.js decodes bytes itself. [I]
  - *Batching:* each terminal has a reading thread (blocking reads of up to 64 KB). What has arrived is sent at most every 8 ms, about 120 messages a second per terminal. [I]
  - *Flow control, so nothing floods:* xterm.js parses 5–35 MB/s, a program can write gigabytes a second, and xterm.js throws away what is past its 50 MB write buffer [V xterm.js's flow control guide]. So the window acknowledges every 100,000 bytes that xterm.js has written (its `write` callback). TB stops reading a terminal while more than 500,000 bytes are unacknowledged, and starts again at 200,000. These are the guide's numbers for a connection with acknowledgements, and its advice is that the limit stay at 500K or less, so keystrokes stay quick [V the guide]. While TB doesn't read, the pseudo-terminal's buffer fills up and the program waits in its `write`, as it would in any terminal. [I]
  - *What TB keeps:* the last 256 KB of each terminal's output, in memory only, so a reloaded window can draw the screen again (§5.3). [I]

Recommendation: `portable-pty` 0.9.0, pinned; the shells and environment above; a `Channel` of raw bytes per terminal, with acknowledgements. Card TB builds it.

**Q2. The terminal in the window: xterm.js, its addons, the renderer in each webview, speed, copy and paste, links, fonts, colours and the security policy.**

Answer:

- **xterm.js 6.0.0** (`@xterm/xterm`, MIT, published 2025-12-22; 6.1.0 is in beta) [V `npm view`]. It is the terminal of VS Code, which keeps one xterm.js instance per terminal and moves it between its panel and its editor area (Q4) [V `terminalInstance.ts`]. Its minified module is 345 KB (`lib/xterm.mjs`) and its stylesheet 7 KB (`css/xterm.css`) [V unpkg]. Version 6.0 removed the canvas renderer ("we recommend using either the DOM renderer or WebGL"), took over VS Code's scroll bar and dropped `windowsMode` [V the 6.0.0 release notes].
- **Addons**, all MIT and from xterm.js's own repository [V `npm view`, today]:

| Addon | Version | What for |
|---|---|---|
| `@xterm/addon-fit` | 0.11.0 | the columns and rows that fit the element |
| `@xterm/addon-webgl` | 0.19.0 | the WebGL2 renderer (126 KB minified, `lib/addon-webgl.mjs` [V unpkg]) |
| `@xterm/addon-search` | 0.16.0 | Find in the scrollback |
| `@xterm/addon-web-links` | 0.12.0 | http(s) links in the output, opened with Ctrl+click |
| `@xterm/addon-unicode11` | 0.9.0 | Unicode 11 character widths, so emoji take two cells |

  Not used: `@xterm/addon-canvas` 0.7.0, which still requires xterm.js `^5.0.0`; the clipboard addon (OSC 52), which lets programs in the terminal use your clipboard; and the image, ligatures and serialize addons. [V the canvas addon's peer dependency; I the choice]
- **The renderer per webview.** xterm.js 6 has two renderers. The DOM renderer is the default: it draws text as HTML, with the font for every character. The WebGL2 addon draws on a canvas from a texture atlas; it is the fast one, and the one VS Code uses when it can (`terminal.integrated.gpuAcceleration`, default `auto`) [V the release notes; VS Code's `terminalConfiguration.ts`]. Per webview:
  - *WebView2 (Windows):* Chromium, as under VS Code. WebGL2 works on a GPU. With Chromium's software WebGL (a virtual machine, a blocked driver) characters can go missing. So, as Proxmox does for its xterm.js console, WebGL is only used when `getContext("webgl2", { failIfMajorPerformanceCaveat: true })` gives a context [V the Proxmox patch, 2023-10-25].
  - *WKWebView (macOS):* WebGL2 since Safari 15 (2021), on Metal [V WebKit's blog]. The WebKit of the macOS versions Gizai supports has it [I: Gizai's oldest macOS isn't written down].
  - *WebKitGTK (Linux):* WebGL2 since WebKitGTK 2.40 (March 2023), through ANGLE [V webkitgtk.org]. Whether it is fast depends on the GPU driver. A known WebKit bug draws green artifacts on a *transparent* canvas; it was reported from a Tauri app with xterm.js on Linux and is still open [V bugs.webkit.org 264268]. So Gizai keeps xterm.js's `allowTransparency` off (the default) and the terminal's background opaque.
  - *Context limits:* a page keeps only a limited number of WebGL contexts alive and drops the oldest. WebKit keeps at most 16 and recycles the oldest (`maxActiveContexts = 16`) [V WebKit's `WebGLRenderingContextBase.cpp`]; Chromium's limit is 16 too [I: from forum reports, not Chromium's source]. With "any number" of terminals, the view on screen loads the WebGL addon and a view that leaves the screen disposes of it. At most one or two contexts are then in use. [I]
  - *Recommendation:* WebGL when a hardware WebGL2 context is available, else the DOM renderer. When the WebGL context is lost (a GPU reset, the system sleeping), dispose of the addon and go on with the DOM renderer, as the addon's README advises (`onContextLoss`) [V the README]. TF0 measures `yes`, `cat` of a 50 MB file and htop with both renderers on the three systems before this is settled (I: nothing was measured in this run).
- **Speed with a lot of output:** Q1's flow control keeps at most about 500 KB waiting in xterm.js, whose parser runs on the window's main thread. A terminal that isn't on screen still parses what arrives, so its scrollback stays whole, but it draws nothing [I: xterm.js pauses drawing for an element that isn't visible; its render service wasn't re-read in this run]. **Scrollback costs memory:** xterm.js keeps 3 × 32 bits per cell (`CELL_SIZE = 3`, `BufferLine.ts`) [V], so 5,000 lines of 120 columns are about 7 MB per terminal when full, about 70 MB for ten. Q3 recommends 5,000 lines anyway: VS Code keeps 1,000 [V `terminalConfiguration.ts`], which is short for a build log.
- **Copy and paste.** Selecting with the mouse works as in any terminal. Copy (the keys in §5.2) takes xterm.js's `getSelection()` and writes it with `navigator.clipboard.writeText`, as Gizai already does elsewhere (`src/pages/EditorPage.tsx:510`). [V Gizai; I design]
  - *Paste that works already:* on macOS (Cmd+V) and Windows (Ctrl+V), and with Shift+Insert on Linux, the webview's own paste reaches xterm.js's hidden text field, and xterm.js pastes it. [I]
  - *Ctrl+Shift+V on Linux:* the webview has no paste behind this key, so the window must read the clipboard itself. In WebKit, `navigator.clipboard.readText()` of text copied in another app first shows WebKit's own "Paste" prompt [V WebKit's layout test `clipboard-read-text-from-platform`]. So TF0 reads it through Tauri's clipboard plugin (`@tauri-apps/plugin-clipboard-manager` 2.4.2, MIT OR Apache-2.0 [V `npm view`]), with the single permission `clipboard-manager:allow-read-text`. Then `term.paste(text)` sends it, with bracketed paste when the shell asks for it [V `xterm.d.ts`: `paste` "performing the necessary transformations for pasted text"]. [I design]
- **Links:** the web-links addon, with Gizai's own handler: Ctrl+click (Cmd+click) opens only `http:` and `https:` links, with the opener the app already allows. OSC 8 links (a program marks text as a link) go through xterm.js's `linkHandler` option. Without a handler, xterm.js uses the browser's `confirm` "with a strongly worded warning" [V `xterm.d.ts`], so Gizai sets one, with the same rules. [I design]
- **Fonts:** the app's mono font (`--font-mono`, `src/styles/tokens.css:157`) at the code size. xterm.js measures the font's cell when it opens, so TF0 waits for `document.fonts.load(…)` first and fits again when Settings → Appearance changes the font or the size [I].
  - *Box drawing:* the bundled mono fonts are cut to Latin ranges (`src/styles/fonts.css:2-5`, `:21-26`; §2.2), so box-drawing characters (`tree`, htop, `git log --graph`) aren't in them. In the DOM renderer they come from a fallback font, which can misalign them. With WebGL, xterm.js draws box drawing and block elements itself (`customGlyphs`, which "doesn't work with the DOM renderer which renders all characters using the font" [V `xterm.d.ts`]). So: `customGlyphs: true` and `rescaleOverlappingGlyphs: true`, and TD decides on a fallback for the DOM renderer (for example a box-drawing subset of a free mono font). [I]
- **Colours, light and dark:** xterm.js's `ITheme` has foreground, background, cursor, cursorAccent, three selection colours, the scroll bar colours and the 16 ANSI colours [V `xterm.d.ts`]. TD defines them as `term-*` tokens (§5.4). Gizai's theme switch fires no DOM event, only the listeners of `appearance.ts` (`src/lib/appearance.ts:124-137`, `useAppearance` `:151-160`). So the store subscribes there and sets `term.options.theme` on every view [I]. `minimumContrastRatio` is 4.5, VS Code's default [V `terminalConfiguration.ts`], so a program's dark blue stays readable on the dark background.
- **The security policy stays as it is.** xterm.js needs no `eval` and no remote code, so `script-src 'self'` is enough [I]. Its DOM renderer adds two `<style>` elements without a nonce [V `DomRenderer.ts`], which `style-src 'self' 'unsafe-inline'` allows (`src-tauri/tauri.conf.json:18`). Tauri adds a nonce to `style-src`, which would switch `'unsafe-inline'` off, only when the page itself has `<style>` elements or style hashes [V `crates/tauri/src/manager/mod.rs` at `tauri-v2.12.1`]. Gizai's `index.html` has none [V]. The channel's fetches go over `ipc:`, which `connect-src` allows. TF0's UI probe checks the DOM renderer's colours under the real policy. [I]

Recommendation: xterm.js 6 with the fit, webgl, search, web-links and unicode11 addons; WebGL with the DOM renderer as the fallback; paste on Linux through the clipboard plugin. Card TF0 builds it.

**Q3. Sessions: how many, closing, leaving the page, quitting, scrollback, many open.**

Answer and recommendation:

- **Any number at once**, each a row under Terminal (§5.1). The backend keeps them in a list in `AppState` (TB), in the order opened; the window keeps the same list (`terminal_list`, `terminals-changed`). A terminal costs a shell process, two threads in Gizai (reading and writing) and its scrollback in the window. [I]
- **Closing ends the shell and everything it started, as Stop ends a run**, in two steps, like the clean-up after a run ends (SIGTERM, then SIGKILL after 3 s, `crates/gizai-agents/src/process.rs:263-276`; Stop itself is `:249-261`) [V Stop; I the steps]:
  - *Linux and macOS:* SIGHUP to the terminal's foreground process group and to the shell's own group, then closing the pseudo-terminal (the kernel then sends SIGHUP to the shell's session, as when a terminal window closes). After 3 s, SIGKILL to every process still attached to the terminal: on Linux the processes whose `/proc/<pid>/stat` names it as their controlling terminal (`tty_nr`, field 7 [V proc_pid_stat(5)]), on macOS those `sysctl` lists for it with `KERN_PROC_TTY` (in `libc` 0.2.190, the version Gizai locks [V docs.rs, `Cargo.lock:2179-2180`]). That reaches background jobs and `nohup` programs too, which live in other process groups of the same terminal and which `kill(-group)` alone would miss. A program that left the terminal on purpose (`setsid`, a daemon) keeps running, as it would after closing any terminal window. Gizai's own `spawn_tree` can't be used as it is: it puts the child in a new process group (`os.rs:255`), and a terminal's shell must lead a new session instead, which `portable-pty` does itself (§3.2 Q1). [V `os.rs`; I the rest]
  - *Windows:* right after the shell starts, TB puts it in a **Job Object** with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, the same kind `spawn_tree` uses for agents (`os.rs:341-364`), so everything the shell starts is in the job. Close ends the job (`TerminateJobObject`, as `Tree::end`, `os.rs:222-226`) and closes the pseudo-console. The shell runs for a few microseconds before it is in the job, too short to start anything. [V `os.rs`; I the gap]
- **They keep running when you leave the page or hide the panel.** The backend reads every terminal all the time, and the window writes what arrives into that terminal's one xterm.js view, which lives for the terminal's life outside the pages (§5.3). Pages unmount when you leave them (`src/App.tsx:173-189`), so the view can't live inside one. A view that isn't on screen still takes in output and acknowledges it, so no program stalls because you looked away. [V the unmount; I design]
- **When Gizai quits,** every terminal ends, with the steps above, in `quit.rs`'s exit path next to the agents (`end_agents`, `quit.rs:52-60`). Closing the window only hides it (`src-tauri/src/lib.rs:622-637`), so terminals keep running then, like agents. If Gizai crashes, Linux and macOS close its pseudo-terminals with the process (SIGHUP to each shell) and Windows ends the jobs when their last handle goes, so no shell is left behind. Nothing is restored at the next start (question 6). [V quit and window; I crash behaviour]
- **Scrollback: 5,000 lines** per terminal, in memory only (VS Code's default is 1,000 [V `terminalConfiguration.ts`]); Clear empties it. §3.2 Q2 gives what that costs.
- **Very many open:** no limit and no dialog. From the 20th, New terminal says "20 terminals are open: each keeps its shell running until you close it." The folded heading always says how many are open. [I]

**Q4. One terminal in two places: can a terminal be one and the same on its page and in the Editor's panel?**

Answer: **yes**, with one list of sessions and one terminal view per session, moved to wherever it is shown. [I design, on the facts below]

- **One list.** The backend owns the sessions (TB), and the window mirrors the list (`terminal_list`, `terminals-changed`). The sidebar shows every session as a row. The Editor's panel shows the sessions whose start folder is the folder the Editor shows (`TerminalSession.folder`), and so does the Terminal page's tab row. So a terminal opened in the Editor's panel is a row under Terminal, and one opened from the sidebar in `gizai` is a tab in gizai's Editor panel. [I]
- **One view per session.** For each session the window makes **one** xterm.js `Terminal` when the session appears, and disposes of it when the session closes. It is attached to the session's output once, so it takes in everything the shell prints, wherever you are.
  - Each place that can show a terminal (the page, the panel) is an empty element. Showing a session there moves the view's element into it, fits it to the place and tells the backend the new size. Leaving the place takes the element out again, and the view lives on in memory.
  - Moving the element keeps all of xterm.js's state (buffer, scrollback, cursor, selection), because nothing is rebuilt. §5.3 has the code's shape.
  - VS Code does exactly this. When a terminal moves between its panel and its editor area, `attachToElement` appends the same terminal's element to the new container, calls xterm.js's `open` on its existing element "to pick up any changes to the window", and refreshes. `detachFromElement` only removes the element [V `terminalInstance.ts`].
- **Why not a view per place:** two xterm.js views of one session would each need the whole output from the start to draw the same screen. Both would also answer the program's questions (cursor position, colours), so the shell would get every answer twice. And a pseudo-terminal has one size, so two places of different sizes would fight over it. tmux, which does let several clients show one session, has to choose whose size wins (`window-size`) [I: tmux's manual not read in this run]. One moving view avoids all of that.
- **Two places at once** can't happen in today's app: one page shows at a time (`src/App.tsx:173-189`), and the Terminal page and the Editor (with its panel) are different pages. A second window or a split could make it possible later. Then **the place that asked last shows the terminal**, and the other shows a quiet placeholder with "Show here". VS Code follows the same rule: a terminal is in the panel or in the editor area, never in both, and moves between them [V VS Code's docs, "Terminal Basics"].
- **Size:** the pseudo-terminal always has the size of the place it shows in now. Moving it sends a resize, and full-screen programs redraw. A hidden terminal keeps its last size. [I]

Recommendation: build it this way, with the store and the view in TF0, so that TF1 (sidebar and page) and TF2 (the Editor's panel) use the same view and can be built at the same time.

**Q5. Where a terminal starts, and how each row is named.**

Answer and recommendation:

- **From the Editor's panel:** the root of the folder the Editor shows (its `EditorFolder.id`).
- **From the sidebar's `+`:** a folder you pick in the Editor's folder picker (Projects, In ~/Herd, Cards), with the folder the main panel shows on top.
- **A card's worktree:** in the picker's Cards group, and from the Editor when it shows one. Refused while an agent works on that card, as the Editor makes that worktree read-only (`gizai-core/src/editor.rs:136-140`), with the same sentence ("QA Agent is working on GA-118: …"). (Open question 3.)
- **The same folder rules as the Editor:** `terminal_open` asks `editor::find` (`crates/gizai-core/src/editor.rs:150-168`), so only projects' linked folders, the folders directly in the Editor folders (`~/Herd`), and card worktrees; never `/`, your home folder, the folders with keys and logins (`~/.ssh`, `~/.gnupg`, `~/.aws`, … `folders.rs:45-57`) or Gizai's data folder apart from card worktrees (`folders.rs:199-228`). A link that leads out is checked where it leads (`editor.rs:170-183`). [V]
- **Names:** the folder's own name ("gizai"), then "gizai 2", "gizai 3" for more in the same folder (the lowest free number); a card's worktree shows the card's identifier with its title faint after it, as the Editor's card rows do (`Sidebar.tsx:237`). Rename (F2, a double click on a tab) sets any name up to 40 characters; an empty one gives the default back. The row's right side shows the program running now, never the shell's own title escape sequence, which any program can set. [V the Editor's rows; I the rest]

**Q6. Safety.**

Answer: as §5.6 says. In short: no agent or Team Lead tool can type into a terminal or read it (none exists, and TB adds none); nothing from a terminal is saved to disk, Memory or a card; scrollback lives in memory only and goes with the terminal; links open only on Ctrl+click and only for http(s); no program in a terminal can read or set the clipboard (no OSC 52), and the window reads it only when you paste. Recommendation: make each of these an acceptance criterion of TB or TF0 (§6), so they are tested, not just intended. [V for what exists; I for the rules]

**Q7. The screens.**

Answer: §5.1 (sidebar section, New terminal, the Terminal page with its tabs, the Editor's panel with its drag handle, one terminal in two places) and §5.2 (keys), in D2's style. Recommendation: TD draws them as a page in the repository, as D2 did, before TF0 to TF2 are built.

**Q8. Free or paid.**

Answer: free. `docs/PAID-FEATURES.md` keeps the whole desktop app free and sells only hosted services; "the code editor and a card's changes" is on its free list (`:36`) and in the plans table's first feature row (`:46`, under Price). [V] A terminal runs entirely on your computer, so nothing in it could be a hosted service. Recommendation: TF1 writes "the terminal" next to the code editor in both places and in README.md.

---

## 4. Options

All three options draw the terminal with xterm.js (§3.2 Q2). It is the maintained terminal emulator for web pages, and VS Code's. They differ in what starts and keeps the shells, and in how the output reaches the window.

### Option A (recommended): Gizai's own terminal sessions on `portable-pty`, a `Channel` per terminal

- **What it takes:** a terminal module in `gizai-agents` (sessions, reading, writing, resizing, ending with everything a shell started). It comes with Tauri commands that check the folder, and one event. The window gets one xterm.js view per session, moved between places (§5.3).
- **Libraries:**
  - `portable-pty` 0.9.0: MIT, from WezTerm, 20 million downloads; released in February 2025, with fixes on WezTerm's main branch since (§3.2 Q1).
  - `@xterm/xterm` 6.0.0 and five addons, all MIT: about 470 KB minified for xterm.js and the WebGL addon [V unpkg]. They go in a chunk of their own that loads with the first terminal, through a dynamic `import()` [I].
  - Tauri's clipboard plugin, for paste on Linux.
- **Risks:**
  - portable-pty's slow releases. Mitigation: pin it, and take a git dependency or a copy if a fix is needed.
  - ConPTY's ways: the cursor-position question when it starts, and redraws after a resize.
  - WebGL in WebKitGTK, which depends on the GPU driver. Mitigation: the DOM renderer as the fallback.
  - The window's main thread when output floods. Mitigation: the acknowledgements.
  
  All are measurable in TB's and TF0's tests. [I]
- **Size:** TB about 1,500–2,500 lines with its tests; TF0 about 1,000–1,500; TF1 and TF2 about 800–1,500 each. Merged card PRs here have a median of 964 changed lines [I, from the R&D notes on this repository].

### Option B: `tauri-plugin-pty`, with the window driving the terminals

- **What it takes:** the plugin in Rust and its npm package; the window starts the shell, writes, resizes and kills through the plugin's commands, and reads by calling `read` in a loop.
- **Libraries:** `tauri-plugin-pty` 0.3.1 (MIT; built on portable-pty; 50 thousand downloads; first published December 2023, last July 2026; its README says "Developing!") and `tauri-pty` 0.3.1 [V crates.io, `npm view`, its `lib.rs` 0.3.1].
- **Risks:**
  - Its `spawn` takes any program, arguments, folder and environment from the window, with no allow-list [V `lib.rs`]. Today the window can't start a program of its own choosing: Gizai has no shell plugin, and every program it starts is chosen in Rust (`src-tauri/capabilities/default.json:8-13`).
  - Output comes by polling, 4,096 bytes per `read`, with no flow control [V].
  - `kill` ends the shell only, through portable-pty's `ChildKiller`, so on Windows it hits the inverted `kill()` result of Q1, and nothing it started is ended.
  - Closing it to the window and adding the folder rules, closing and flow control means writing option A on top of it.
- **Size:** a few hundred lines at first; then the security and closing work of option A comes back. [I]

### Option C: Gizai's own sessions, with the screen kept in Rust (`alacritty_terminal`)

- **What it takes:** Alacritty's terminal library parses the output into a screen grid in Rust, as Zed does for its native terminal [I]. The window draws that grid, either through a renderer of Gizai's own or by turning grid updates back into escape sequences for xterm.js.
- **Libraries:** `alacritty_terminal` 0.26.0 (Apache-2.0, April 2026, 2 million downloads) [V crates.io]; it includes its own pseudo-terminal code.
- **What it would give:** the backend knows the screen, so a reloaded window, or two windows showing one terminal at once, get the exact screen.
- **Risks:** with its own renderer, Gizai would rebuild selection, find, links, input encoding, screen readers and speed, which xterm.js already has. With xterm.js on top, the screen is parsed twice. Either way it is the largest option, and the only thing it gives is what §5.3's 256 KB replay already gives closely enough. [I]
- **Size:** about two to three times option A's TB and TF0. [I]

**Recommended: option A.** It keeps every program start in Rust behind the Editor's folder rules, and ends a terminal with everything it started, as Gizai ends agents. It streams output with flow control, and it uses the pseudo-terminal layer of WezTerm and the terminal of VS Code. One terminal in two places works by moving one view (§3.2 Q4), so option A doesn't need option C's screen in Rust.

---

## 5. The design

All screens use the design system as it is, with D2's Editor (`design/code-editor/editor.html`, made by `gen-editor.mjs`) as the model: sidebar rows (`.nav-item`, the row's state in `.meta`), the top bar with breadcrumbs and actions (`.topbar`), the Editor's tabs (`.ed-tab`, `--size-tab` 36px), the folder picker (`src/components/editor/FolderPicker.tsx`), the right-click menu (`ContextMenu`, `Menus.tsx:17-53`), Lucide icons at 16px (`terminal` for a terminal, `design/README.md:145`), sentence case, and questions asked in one sentence under the row, never in a dialog (`gen-editor.mjs:680`). New are the terminal colours, the drag handle and the Terminal button; card TD designs them as a page in the repository for Jefsev to review. [V for the existing parts; I for the rest]

### 5.1 The screens

**Screen 1: the sidebar's Terminal section.** A folding **Terminal** section between Projects and Editor (after the Projects section, `src/components/Sidebar.tsx:72-79`, and before `EditorSection`, `:81`), drawn with `FoldSection` like Editor: `+` (New terminal) and the caret on its heading; `"terminal"` joins `Fold` (`src/lib/sidebar.ts:8`). [V where; I design]

```
│ PROJECTS                          + │
│  ● Gizai                            │
│  ● Kade portal                      │
│ TERMINAL                        + ⌄ │
│  >_ gizai                      npm  │
│  >_ gizai 2                         │
│  >_ GA-123 Fix the invoice total    │
│  >_ kade-portal              ended  │
│ EDITOR                          + ⌄ │
│  ▣ gizai                4 changed   │
│  ▣ kade-portal                      │
│ AGENTS                            ⌄ │
```

(`>_` and `▣` stand for the Lucide icons `terminal` and `folder-code`.)

- **Rows:** one per open terminal, any number, in the order they were opened (a new one at the bottom, so rows don't move under the mouse). Each is a link to `#/terminal/<id>`, highlighted (`.on`) while its page shows. The name (§3.2 Q5) and, on the right in `text-3`, the program running in it now (`npm`, `cargo`, `vim`), nothing while the shell waits for a command, or `ended` when the shell ended. Never teal or a pulse: those mean an agent at work (`design/README.md:36-37`).
- **No × on the row** (D2's rule: no button inside a row that is a link, `gen-editor.mjs:552`). Closing is in the row's menu, a middle click, and the × on the terminal's tab.
- **Right click** (or the context menu key, Shift+F10): Rename (F2), Show in the Editor (opens its folder in the Editor with this terminal in the panel), Copy folder path, Close. Close on a terminal with a program running asks under the row: "npm is still running in gizai 2. Close it and end npm? [Close] [Cancel]".
- **Folded:** the heading shows how many are open, "3 open", as the Editor's folded heading says how many folders are behind, "2 behind" (`src/components/Sidebar.tsx:220`, `:238`; D2's `design/code-editor/gen-editor.mjs:61`).
- **None open:** one row, "New terminal", as the Editor's "Open a folder" row.

**Screen 2: New terminal.** `+` opens the Editor's folder picker (`FolderPicker.tsx`) titled "New terminal in": the same groups (Projects, In ~/Herd, Cards) and search box, with the folder the main panel shows now (the Editor's folder, or the current terminal's folder) on top. Enter starts it and opens its page. Ctrl+Shift+\` starts one at once in the folder you are in, without the picker (§5.2).

**Screen 3: the Terminal page** (`#/terminal/<id>`).

```
┌ Terminal / gizai 2 ──────────── ~/Herd/gizai · zsh · 132 × 38 ───── [Show in Editor] [⋯] ┐
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ >_ gizai  npm  ×    >_ gizai 2  ×    +                                                    │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ jefsev@arch gizai (main) $ git status                                                     │
│ On branch main                                                                            │
│ Your branch is up to date with 'origin/main'.                                             │
│ jefsev@arch gizai (main) $ █                                                              │
│                                                                                           │
│                                                                                           │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

- **Top bar:** Terminal / *name* (double-click or F2 renames it in place); then the folder (`~` form, mono), the shell and the size in `text-3`; actions **Show in Editor** and **⋯** (Rename, Find, Clear, Copy folder path, Close).
- **Tabs:** the terminals of **this terminal's folder**, the same tabs its Editor panel shows (Screen 4), drawn as the Editor's file tabs (`.ed-tab`): `terminal` icon, name, the running program, ×; `+` starts another one in the same folder. A middle click or × closes (asking first when a program runs); a double click renames. Ctrl+PgDn and Ctrl+PgUp step through them. (Open question 1 has the other way: every terminal as a tab.)
- **The terminal** fills the rest, with 8px padding, the mono font at the code size, and the terminal colours (§5.4).
- **Find** (Ctrl+Shift+F; Cmd+F on macOS): a small bar at the top right, like the Editor's find panel (`FindPanel.tsx`): the query, Match case, Words, Regular expression, "3 of 12", previous and next, through `@xterm/addon-search`.
- **Ended:** `exit` (exit code 0) closes the terminal, as VS Code does [I]. Any other end keeps it, read-only, with a bar: "The shell ended with exit code 1. [Start again] [Close]". "Couldn't start zsh in ~/Herd/gizai: <why>" when it never started.

**Screen 4: the Editor's terminal panel.**

```
┌ Editor / gizai ──── ⑂ main · 4 changed ─────────────────── [Fetch] [>_ Terminal] [Open in ▾] ┐
├────────────────────────┬────────────────────────────────────────────────────────────────────┤
│ Files | Search      ⇤  │ Sidebar.tsx ● ×   worktree.rs ×                                     │
│ [Go to file…  Ctrl P]  │  40  const agents = (team.data?.members ?? [])…                     │
│ ▾ src                  │  41  const notes = useData(() => memoryNotes()…                     │
│   ▾ components       M │  42  …                                              (70%)            │
│       Sidebar.tsx    M ├══════════════════════════════ ═══ ═══════════════════════════════════┤ ← drag handle
│   ▸ lib                │ >_ gizai  npm ×   >_ gizai 2 ×   +          [⤢ Own page] [⌄ Hide]    │
│ ▸ crates               │ $ npm run dev                                                       │
│                        │   VITE v8 ready in 312 ms                           (30%)            │
│                        │   ➜  Local: http://localhost:5173/                                   │
│                        ├────────────────────────────────────────────────────────────────────┤
│                        │ src/components/Sidebar.tsx · Modified · Ln 42, Col 7 · TSX · Saved  │
└────────────────────────┴────────────────────────────────────────────────────────────────────┘
```

- **The Terminal button** (`btn ghost sm`, Lucide `terminal`, "Terminal") sits in the top bar's actions **between Fetch and Open in** (`src/pages/EditorPage.tsx:562-565`: after Fetch, `:563`, and before Open in, `:564`). Fetch shows only for a folder with a main branch to compare with; without it the Terminal button comes first. It is pressed (`aria-pressed`) while the panel shows; its tooltip says "Show the terminal (Ctrl+\`)" or "Hide the terminal (Ctrl+\`)". [V where; I design]
- **What it does:** shows the panel. When the folder has no terminal yet, it starts one in the folder's root, as PhpStorm ("the root directory of the current project") and VS Code ("the folder that is opened in the Explorer") do [V their docs]. The terminal gets the focus.
- **Where:** in the code column (`.ed-main`), between the code (`.ed-code`, `EditorPage.tsx:666-679`) and the file's status bar (`:681-696`), so the file tree keeps its full height, as in VS Code's default layout [I]. (The status bar then sits under the panel, at the Editor's bottom edge as in VS Code and PhpStorm; TD may keep it with the code instead.)
- **Height:** the first time **30% of the column** (tabs, code and panel together); the **drag handle** is the panel's top border: a 1px line with a 6px grab area, `row-resize` cursor, `role="separator"` with `aria-orientation="horizontal"` and the height in `aria-valuenow`; ↑ and ↓ move it by 5% when it has focus, a double click sets it back to 30%. The panel keeps at least its tab row and 4 lines (about 120px), the code at least 120px. Gizai remembers the height as a share of the column (not pixels), once for all folders, in `localStorage` (`gizai.editor.panel`), so it stays 30% when the window changes size. [I]
- **Its own tabs:** this folder's terminals (the same as on the Terminal page), `+`, ×, the running program; on the right **Own page** (opens `#/terminal/<id>`) and **Hide** (Lucide `chevron-down`, Ctrl+\`). Hiding keeps every terminal running; closing the last tab hides the panel. Whether the panel was open is remembered per folder, like the folder's file tabs (`gizai.editor.tabs`, `src/lib/editor.ts:59-87`).
- **A card's worktree** follows the Editor's own rules (`src/pages/EditorPage.tsx:146-152`). [V the rules; I the terminal's part]
  - *While an agent works on it* (`folder.readOnly`): the button is off, with the folder's reason ("QA Agent is working on GA-118: a terminal there waits until its run ends"), and TB refuses it too. A terminal opened before the run started stays, under the Editor's bar.
  - *Another person's card* (F4): the Editor shows it read-only until Edit anyway (`:585-599`). The button is off until you press Edit anyway, because a shell can change files just as an edit can.
  - *Your own card:* the button works as in any folder.
  - (Open question 3.)

**Screen 5: one terminal, two places.** Only one page shows at a time (`src/App.tsx:173-189`), so the Terminal page and the Editor's panel are never on screen together. A terminal moves with you: open gizai 2 from the sidebar and the page has it; go back to the Editor and the panel has it again, with everything on it, its scrollback, what runs in it and its selection. The terminal takes the size of where it is (120 × 12 in the panel, 132 × 38 on the page), and a full-screen program (vim, htop) redraws at the new size. When two places want it at once, which a second window or a split could bring later, **the place that asked last shows it** and the other shows a quiet placeholder: "gizai 2 is on its own page. [Show here]". VS Code works the same way: a terminal is in one place at a time and moves between the panel and the editor area [V VS Code's docs]. §5.3 says how.

### 5.2 Keys

| Key | Where | What it does |
|---|---|---|
| Ctrl+\` (all three systems) | the Editor, also from inside its terminal | shows or hides the terminal panel, as VS Code [V its docs] |
| Ctrl+Shift+\` | the Editor; the Terminal page | a new terminal in the folder: a new tab in the panel, or on the page |
| Ctrl+Shift+C, Ctrl+Shift+V (Linux); Ctrl+C with a selection, Ctrl+V (Windows); Cmd+C, Cmd+V (macOS) | in a terminal | copy and paste, as VS Code on each system [V its docs]; without a selection Ctrl+C is the program's interrupt |
| Shift+Insert (Linux, Windows) | in a terminal | paste |
| Ctrl+Shift+F (Cmd+F on macOS) | in a terminal | find in its scrollback |
| Ctrl+PgDn, Ctrl+PgUp | in a terminal | the next and previous terminal tab |
| Ctrl+click (Cmd+click on macOS) | on a link | opens an http or https link in the browser [I] |
| every other key | in a terminal | goes to the shell: Ctrl+W, Ctrl+P, Ctrl+B, Ctrl+K, Ctrl+R and Escape do what the shell or the program wants, not what the Editor or Gizai would (`src/pages/EditorPage.tsx:514-530`, `src/App.tsx:35-48`) |

The key for Ctrl+\` is read by where it is (`KeyboardEvent.code === "Backquote"`, the key left of 1), so it works on keyboards where that key has another character. [I]

**How the keys get where they should.** xterm.js reads keys in a hidden text field of its own. Gizai's single-letter keys (`n`, `t`, `d`) already skip text fields (`src/App.tsx:41`). Ctrl+K (`:39`) and the Editor's Ctrl keys (`EditorPage.tsx:516-527`) listen on `window`, though, so TF0 makes sure that a key xterm.js sends to the shell never reaches them [I: xterm.js stops the keys it handles; TF0's probe checks it]. The other way round, the keys of this table that Gizai keeps (Ctrl+\`, Ctrl+Shift+\`, Ctrl+PgDn and Ctrl+PgUp, find, copy and paste) are let through xterm.js with `attachCustomKeyEventHandler`, which runs "before keys are processed" [V `xterm.d.ts`].

### 5.3 One view per terminal: the code's shape

The store lives in a module, `src/lib/terminals.ts` (TF0), started once by `App`, not by a page, because pages unmount when you leave them (`src/App.tsx:173-189`). [V the unmount; I the rest]

```ts
// src/lib/terminals.ts (TF0): one view per session, for the session's whole life
type View = { term: Terminal; fit: FitAddon; host: HTMLDivElement; webgl: WebglAddon | null; place: HTMLElement | null };
const views = new Map<string, View>();

function create(s: TerminalSession) {              // when terminal_list or terminals-changed brings a new session
  const host = document.createElement("div");      // the view's own element: this is what moves between places
  const term = new Terminal({ scrollback: 5000, customGlyphs: true, rescaleOverlappingGlyphs: true,
                              minimumContrastRatio: 4.5, fontFamily: monoFont(), theme: themeFromTokens() });
  const fit = new FitAddon();
  term.loadAddon(fit); /* + unicode11, search, web-links with Gizai's handler */
  term.open(host);                                 // once, before any place shows it
  term.onData((d) => terminalInput(s.id, d));
  term.onBinary((d) => terminalInput(s.id, d, true));
  let written = 0;                                 // flow control, §3.2 Q1
  terminalAttach(s.id, (bytes) => term.write(bytes, () => {
    written += bytes.length;
    if (written >= 100_000) { terminalAck(s.id, written); written = 0; }
  }));
  views.set(s.id, { term, fit, host, webgl: null, place: null });
}

export function show(id: string, place: HTMLElement) {  // a place (the page, the panel) wants the terminal
  const v = views.get(id); if (!v) return;
  if (v.place && v.place !== place) left(v.place, id);  // the other place shows "Show here" (§5.1, Screen 5)
  place.appendChild(v.host);                       // moves the element: buffer, scrollback, cursor and selection stay
  v.place = place;
  v.term.open(v.host);                             // as VS Code's attachToElement does; cheap when already open
  if (!v.webgl && hardwareWebgl2()) {              // WebGL only while on screen (§3.2 Q2)
    v.webgl = new WebglAddon();
    v.webgl.onContextLoss(() => { v.webgl?.dispose(); v.webgl = null; });  // falls back to the DOM renderer
    v.term.loadAddon(v.webgl);
  }
  v.fit.fit();
  terminalResize(id, v.term.cols, v.term.rows);
}

export function hide(id: string, place: HTMLElement) {  // the place goes: page left, panel hidden, another tab
  const v = views.get(id);
  if (!v || v.place !== place) return;             // another place has it already
  v.webgl?.dispose(); v.webgl = null;              // frees the WebGL context; the buffer stays
  v.host.remove(); v.place = null;                 // still takes in output and acknowledges it
}
// when a session closes: views.get(id)?.term.dispose(); views.delete(id)
```

```tsx
// src/components/terminal/TerminalView.tsx (TF0): a place, used by the Terminal page (TF1) and the Editor's panel (TF2)
export function TerminalView({ id }: { id: string }) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = ref.current!;
    show(id, el);
    const ro = new ResizeObserver(() => refit(id));  // fit, then terminal_resize at most every 50 ms
    ro.observe(el);
    return () => { ro.disconnect(); hide(id, el); };
  }, [id]);
  return <div ref={ref} className="term-place" />;
}
```

- **A reloaded window** (a reload in development, or WebKitGTK's page process restarting) rebuilds the store from `terminal_list`. Each view attaches again, and TB first sends the last 256 KB it kept, so the screen and recent output come back. Full-screen programs redraw at the next resize. [I]
- **An ended shell** keeps its view with `disableStdin`, until the terminal is closed (§5.1, Screen 3). [I]
- **Themes and fonts:** the store listens to `appearance.ts` (§3.2 Q2) and sets `term.options.theme` and `fontFamily` and `fontSize` on every view; the view on screen fits again. [I]

### 5.4 Terminal colours and the font

- **Tokens:** TD defines `term-*` tokens in `design/tokens.json` for dark and light: `term-fg`, `term-bg`, `term-cursor`, `term-cursor-text`, `term-selection`, `term-selection-inactive`, and the 16 colours `term-black` … `term-bright-white`. TF0 generates them into `src/styles/tokens.css` with `scripts/tokens-css.ts`, as every token is generated (`tokens.css:1`). [V the generator; I the names]
- **Into xterm.js:** xterm.js takes colours as strings, not CSS variables. So the store reads each token with `getComputedStyle(document.documentElement).getPropertyValue("--term-red")` when it starts and after each theme change. A token written as `var(--…)` (as `--st-review: var(--needs)`, `tokens.css:34`) arrives resolved. [V the token style; I the rest]
- **The background** is the code's background, so the Editor's panel and the code above it are one surface; the panel's top border is the drag handle. [I]
- **The rules of `design/README.md`:** teal (`--live`) only for an agent at work and magenta (`--needs`) only for "needs you" (`design/README.md:35-38`). A program's cyan and magenta are its own output, not Gizai's states, so TD picks `term-cyan` and `term-magenta` clearly apart from `--live` and `--needs` (TD's acceptance criteria). [V the rule; I the reading]
- **The font:** `--font-mono` at the code size (`.t-code`, 12.5px on 20px lines, `tokens.css:171`): xterm.js gets `fontSize: 12.5` and `lineHeight: 1.6`. It follows Settings → Appearance (`src/styles/appearance.css:11-15`); box drawing as in §3.2 Q2. [V the sizes; I the mapping]

### 5.5 The data, the commands and the files

- **Data:** no database table and nothing on disk. TB keeps the sessions in memory in `AppState` (the shell, the reader and writer threads, the 256 KB of recent output, the counters for acknowledgements). The window keeps the views (§5.3). `localStorage` gets only the layout: `"terminal"` in `gizai.sidebar.folded`; the panel's height share in `gizai.editor.panel`; and whether the panel was open, per folder, with the folder's tabs (`gizai.editor.tabs`, `src/lib/editor.ts:59-87`). It never gets a terminal's name, folder or output. [V the keys today; I the new ones]
- **Commands and the event:** contract CT (§6), word for word.
- **Files** [I]:

| Card | New | Changed |
|---|---|---|
| TD | `design/terminal/gen-terminal.mjs`, `design/terminal/terminal.html` | `design/README.md` (the terminal colours' rule) |
| TB | `crates/gizai-agents/src/terminal.rs`, `src-tauri/src/terminal.rs`, `crates/gizai-agents/tests/terminal_test.rs`, a flow test in `src-tauri/tests/` | `crates/gizai-agents/Cargo.toml` (`portable-pty`), `crates/gizai-agents/src/os.rs` (the shell, the Job Object, the processes on a terminal), `src-tauri/src/lib.rs` (`AppState.terminals`, the commands, the clipboard plugin), `src-tauri/src/quit.rs`, `src-tauri/capabilities/default.json`, `src-tauri/tests/tools_test.rs`, `src/api.ts`, `src/types.ts`, `package.json`, `docs/PLATFORMS.md` |
| TF0 | `src/lib/terminals.ts` (+ test), `src/components/terminal/TerminalView.tsx`, `src/styles/terminal.css`, `src/pages/TerminalPage.tsx` | `package.json` (`@xterm/*`), `design/tokens.json`, `src/styles/tokens.css`, `src/router.ts`, `src/App.tsx`, `src/selftest.ts`, `scripts/ui-test.sh` |
| TF1 | `src/components/terminal/TerminalTabs.tsx` | `src/components/Sidebar.tsx`, `src/lib/sidebar.ts`, `src/pages/TerminalPage.tsx`, `docs/PAID-FEATURES.md`, `README.md`, `src/components/RunPanel.tsx` (open question 9) |
| TF2 | `src/components/editor/TerminalPanel.tsx` (+ test) | `src/pages/EditorPage.tsx`, `src/lib/editor.ts`, `src/styles/components.css` |

### 5.6 Security and privacy

- **It runs as you, and says so by being yours only.** A terminal is a shell with your rights, like any terminal app. Gizai's job is that nothing else drives it or reads it. [I]
- **Agents and the Team Lead can't reach it.** The Team Lead's 46 tools read the database and the list of live runs, never in-memory state or logs (`src-tauri/src/tools/mod.rs:160-208`, `:371-479`; `src-tauri/src/tools/read.rs`), and task runs get no Gizai tools at all (`src-tauri/src/runs.rs:975`). [V] TB adds no tool and no MCP entry. It also adds a test that fails when a tool in the catalog reaches a terminal: today's tests check the catalog's names and schemas, not what the tools do (`src-tauri/tests/tools_test.rs:38-59`), so a terminal tool would pass them. The terminal commands are Tauri commands. Their only caller is the window, which runs only Gizai's own code: the capability names the `main` window (`src-tauri/capabilities/default.json:5-13`), and the security policy loads no remote script. [V for today; I for the rule]
- **Nothing from a terminal is kept on disk.** Its scrollback is xterm.js's buffer in the window's memory, and TB keeps at most 256 KB per terminal in Gizai's memory for a view that attaches again; both go with the terminal. No log file, no database row, no Memory note, nothing on a card. This matters beyond privacy: agents' runs have `cat`, `rg` and `tail` among their default commands and run as you (`crates/gizai-core/src/seed.rs:245-250`; the folder limits apply to the file tools, not to commands, `folders.rs:1-2`), so a terminal log under `<data>` would be readable by any run. [V the agents' commands; I the rule]
- **Only the window's own terminals.** `terminal_input` writes only to a terminal the window opened, by its id; there is no command that runs a command line. A link is opened only on Ctrl+click (Cmd+click), only `http:` and `https:`, through the opener the app already allows (`opener:allow-open-url`, `capabilities/default.json:11-12`). No program in the terminal can read the clipboard: the clipboard addon (OSC 52) isn't used. Paste uses bracketed paste, so a pasted line isn't run before Enter in shells that support it (xterm.js does this itself, §3.2 Q2). [I design; V the capability]
- **Where it starts.** Only in a folder the Editor may open, checked by `editor::find` on every `terminal_open` (§3.2 Q5), so never in `~/.ssh` or Gizai's data folder, and not in a card's worktree while an agent works there. This decides where Gizai starts a shell; it doesn't fence the shell in. [V the check; I the rule]
- **One new right for the window:** reading the clipboard's text (`clipboard-manager:allow-read-text`), used only to paste into a terminal with Ctrl+Shift+V on Linux (§3.2 Q2). No shell or file plugin, and the same content security policy. [I]

### 5.7 How it is tested

- **Rust** (TDD, CLAUDE.md:39-45): `crates/gizai-agents/tests/terminal_test.rs`. Behind `#[cfg(unix)]`: a session with `/bin/sh` in a temporary folder echoes what it is sent; `stty size` follows `resize`; `TERM`, `COLORTERM` and `TERM_PROGRAM` are set and `GIZAI_*` aren't; `busy` names `sleep` while it runs; close ends a foreground `sleep 1000`, a `sleep 1000 &` and a `nohup sleep 1000 &` within 3 s (their pids are gone); 50 MB of output arrives in order through a slow fake listener, reading pausing and going on; the shell's own `exit 3` gives `ended.code == 3`. Behind `#[cfg(windows)]` (CI's Windows runner): `cmd.exe /c echo hi` through ConPTY; close ends `ping -t 127.0.0.1` started from the shell (the Job Object). Everywhere: a folder the Editor refuses is refused with its sentence; nothing appears under the data folder. `src-tauri/tests/` flow test: open, attach, input, close, and quit ending what is left.
- **TypeScript** (`npm test`, in Node without a DOM, `vitest.config.ts:2`): the store (names "gizai", "gizai 2", a card's identifier, rename back to the default, the order of rows), the panel's height share and its limits, where Ctrl+\` and the copy keys go per system, the acknowledgement counter, and the `"terminal"` fold.
- **UI** (`scripts/ui-test.sh`, headless in `cage`, `src/selftest.ts`): open a terminal, type `echo gizai-probe`, find it in the terminal's buffer; leave and come back, it is still there; the Editor's button opens the panel at 30% and `pwd` prints the folder; drag the handle, reload, the height stays; Ctrl+K and Ctrl+W in the terminal don't open the palette or close a file; `yes` for 10 s keeps the window drawing (frames counted) and Ctrl+C stops it. Screenshots with `scripts/shot-cage.sh` for the design review.
- **Linux, macOS, Windows:** CI builds and tests all three (`.github/workflows/ci.yml`); the shell differs per system (bash or zsh, zsh on macOS, PowerShell on Windows), so tests that type commands use `/bin/sh` or `cmd.exe` explicitly.

### 5.8 What is left out

A terminal for agents (not this card); splitting a terminal into panes; terminals restored after a restart; a Shell setting, Git Bash and WSL; shell integration (marking each command and its exit code, as VS Code's injected scripts do); opening `src/lib/x.ts:12` in the Editor from a click in the terminal; the clipboard addon and images (sixel); "Open in terminal" for your own terminal app; and a second window.

---

## 6. Proposed cards

Five cards, each one pull request that fits in one agent run (the Editor's F2, PR #96, was 5,201 added lines in 35 files [V `gh pr view 96`], built as one card). "Builds on" names the cards that must be merged first. The contract between the backend card and the frontend cards is below the list, word for word.

| # | Card | Starts in | Role | Builds on |
|---|---|---|---|---|
| TD | Design: the terminal, in the sidebar, on its page and in the Editor | Design | design | – |
| TB | Terminals in the backend: your shell in a pseudo-terminal on Linux, macOS and Windows | To do | backend | – |
| TF0 | The terminal view: xterm.js in Gizai's colours, one view per terminal | To do | frontend | TD, TB |
| TF1 | Terminals in the sidebar and on their own page | To do | frontend | TF0 |
| TF2 | The terminal panel in the Editor | To do | frontend | TF0 |

At the same time: **TD and TB** first (the design and the backend need nothing else); then **TF0**; then **TF1 and TF2** together (both build on TF0's view and store, and touch different files: the sidebar and the Terminal page, or the Editor page). "Show in the Editor" on a terminal's row needs TF2's panel, so whichever of TF1 and TF2 merges last wires it.

### TD. Design: the terminal, in the sidebar, on its page and in the Editor

- **Column, role:** Design, design.
- **Builds:** the design of §5 as a page in the repository, made like D2's (`design/terminal/terminal.html` from `design/terminal/gen-terminal.mjs`, drawn with the app's own stylesheets, every screen in dark and then light, a Specs section and an "Every text" table): the sidebar's Terminal section (open, folded with "3 open", none open, a row with a running program, an ended row, the row menu, the close question under a row), New terminal (the folder picker), the Terminal page with its tabs (find, ended, couldn't start, the two-places placeholder), the Editor with its panel (closed, at 30%, dragged higher, the drag handle's hover, focus and drag states, the Terminal button pressed, and off for a folder an agent works on); and the terminal's colours.
- **Acceptance criteria:**
  - [ ] Every screen and state of §5.1 in dark and light, with the app's components and D2's Editor around the panel.
  - [ ] Terminal colours for dark and light: the 16 colours programs ask for (black, red, green, yellow, blue, magenta, cyan, white and their bright forms), plus foreground, background, cursor and selection, as tokens (`term-*`); each colour's contrast on the terminal background listed; cyan and magenta clearly apart from `--live` and `--needs`; a sample of `git status`, `git log --oneline --graph`, `ls --color`, a failing test run and htop in both themes.
  - [ ] The panel, its drag handle, the tabs and the Terminal button specified (sizes in tokens; the 30% start and the limits; hover, focus, pressed, dragging).
  - [ ] Every text written out in sentence case, and the keys of §5.2.
  - [ ] Jefsev approved it on the card.

### TB. Terminals in the backend

- **Column, role:** To do, backend.
- **Builds:** `crates/gizai-agents/src/terminal.rs` (new): the sessions, on `portable-pty` 0.9 (§4); your shell and environment (§3.2 Q1); reading on a thread per terminal, batched, with flow control and 256 KB kept in memory for a view that attaches again; writing on a thread of its own; resize; the program running now; ending a terminal and everything it started (§3.2 Q3). `crates/gizai-agents/src/os.rs`: the user's shell; on Windows the shell's Job Object; on Unix the processes still attached to a terminal. `src-tauri/src/terminal.rs` (new) with the commands and the event of contract CT, `AppState.terminals`, registered in `lib.rs`; every terminal ended at quit (`quit.rs`). The start folder is checked with `editor::find` (`crates/gizai-core/src/editor.rs:150-168`). `src/api.ts` and `src/types.ts` as in CT. `docs/PLATFORMS.md`: a row for the terminal (shells, ConPTY, how it ends). Tauri's clipboard plugin (`tauri-plugin-clipboard-manager`, `@tauri-apps/plugin-clipboard-manager`), registered in `lib.rs`, with only `clipboard-manager:allow-read-text` added to `src-tauri/capabilities/default.json`, for TF0's paste on Linux. No database change, and the same security policy.
- **Acceptance criteria:**
  - [ ] `terminal_open` starts your shell in one of the Editor's folders and refuses any other folder with the Editor's own sentence; it refuses a card's worktree while an agent works on it.
  - [ ] On Linux and macOS the shell is `$SHELL` (else your entry in the password database, else `/bin/sh`), a login shell on macOS; on Windows `pwsh.exe` when it is found, else `powershell.exe`, else `cmd.exe`. The environment has `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=gizai` and `TERM_PROGRAM_VERSION`, and no `GIZAI_*` variables (tested).
  - [ ] Output reaches the attached channel as bytes, in order, at most one message per terminal every 8 ms; 50 MB of output arrives complete, while reading pauses above 500,000 bytes not yet acknowledged and goes on at 200,000 (tested with a fake listener that acknowledges slowly).
  - [ ] Input, a paste of 1 MB and resize work (`stty size` on Unix and `$Host.UI.RawUI.WindowSize` on Windows report the new size).
  - [ ] `terminal_close` ends the shell, a program in the foreground (`sleep 1000`), a background job (`sleep 1000 &`) and a `nohup` one within 3 s on Linux and macOS, and on Windows the shell and its children (`ping -t`), through the Job Object.
  - [ ] Every terminal ends when Gizai quits, and when it is killed (Linux: closing the pseudo-terminal; Windows: the job), tested with a scratch process.
  - [ ] `busy` names the program in the foreground within a second; `terminals-changed` comes on open, rename, busy, end and close.
  - [ ] Nothing a terminal shows or is sent is written to disk, to the database, a log, Memory or a card; no MCP tool reaches terminals (a new test in `src-tauri/tests/tools_test.rs` checks the catalog).
  - [ ] `cargo test --workspace`, `npm test` and `npm run build` pass, and CI passes on Linux, macOS and Windows.

### TF0. The terminal view: xterm.js in Gizai's colours, one view per terminal

- **Column, role:** To do, frontend.
- **Builds:** TD's `term-*` tokens in `design/tokens.json` and `src/styles/tokens.css` (with `scripts/tokens-css.ts`) and their rule in `design/README.md`; the packages of §4 (`@xterm/xterm` and its addons); `src/lib/terminals.ts` (the store of sessions from `terminalList` and `onTerminalsChanged`, and one xterm.js `Terminal` per session for its whole life, attached once to its output, moved between places, §5.3); `src/components/terminal/TerminalView.tsx` (places the view, fits it, sends the size, recolours it on a theme change, the keys of §5.2, copy and paste (on Linux, Ctrl+Shift+V reads the clipboard with the plugin's `readText`, §3.2 Q2), links, find, acknowledging what xterm.js wrote; WebGL on screen with the DOM renderer as fallback); `src/styles/terminal.css` (xterm's own stylesheet, imported, and Gizai's); `src/pages/TerminalPage.tsx` with `#/terminal` (New terminal, with the folder picker) and `#/terminal/<id>` (the terminal under its top bar), and the route in `src/router.ts`; a UI probe in `src/selftest.ts` and `scripts/ui-test.sh`.
- **Acceptance criteria:**
  - [ ] A terminal opens in a folder from `#/terminal`; typing `echo gizai-probe` shows it (UI probe in `cage`).
  - [ ] Leaving the page and coming back shows the same terminal, with what it printed meanwhile and its scrollback (5,000 lines).
  - [ ] Colours come from the tokens; `t` and Settings → Appearance recolour and refont open terminals at once; box-drawing characters line up (`tree`, htop).
  - [ ] Copy, paste and find with the keys of §5.2 on each system; Ctrl+Shift+V on Linux pastes through the clipboard plugin without WebKit's paste prompt; Ctrl+click opens an http(s) link with the opener, nothing else opens.
  - [ ] WebGL draws the terminal where a hardware WebGL2 context exists; with WebGL off (a probe forces it) or its context lost, the DOM renderer draws the same screen; only the terminals on screen hold a WebGL context.
  - [ ] In a terminal, Ctrl+K, Ctrl+W, Ctrl+P, Ctrl+B, `n`, `t` and `d` reach the shell and do nothing in Gizai (probe).
  - [ ] `yes` running for 10 s doesn't freeze the window, and Ctrl+C stops it within a second (probe).
  - [ ] The security policy is unchanged; `npm test`, `npm run build` and `scripts/ui-test.sh` pass.

### TF1. Terminals in the sidebar and on their own page

- **Column, role:** To do, frontend.
- **Builds:** the Terminal section in the sidebar above Editor (`"terminal"` in `Fold`), its rows, New terminal, the row menu (Rename, Show in the Editor, Copy folder path, Close), middle click, the close question; the Terminal page's tabs (the folder's terminals, `+`, ×, rename, Ctrl+PgDn and Ctrl+PgUp), its top bar and ⋯ menu, the ended and couldn't-start states with Start again; the note at 20 terminals; "the terminal" in `docs/PAID-FEATURES.md`'s free list and table, and in README.md.
- **Acceptance criteria:**
  - [ ] Ten terminals open at once show as ten rows under Terminal, above Editor; a click opens each on its page; closing them leaves "New terminal".
  - [ ] Rename (F2 on the row, double click on a tab) shows the new name on the row, the tab and the page; an empty name gives the default back.
  - [ ] Close on a terminal running a program asks under the row first; Close ends the shell and the row goes.
  - [ ] `exit` closes the terminal; a shell that ends with another code shows why, with Start again.
  - [ ] The section folds like Editor and remembers it; folded it says how many are open.
  - [ ] `npm test`, `npm run build` and `scripts/ui-test.sh` pass.

### TF2. The terminal panel in the Editor

- **Column, role:** To do, frontend.
- **Builds:** the Terminal button between Fetch and Open in; `src/components/editor/TerminalPanel.tsx` between the code and the status bar, with its tabs (the folder's terminals), Own page and Hide; the drag handle; the height as a share in `localStorage` and whether the panel shows per folder; Ctrl+\` and Ctrl+Shift+\`; the Editor's keys (`src/pages/EditorPage.tsx:514-530`) left alone while a terminal has the focus; Own page and Show in the Editor moving the one view.
- **Acceptance criteria:**
  - [ ] The button opens the panel at 30% of the column with a terminal in the folder's root (`pwd` prints it, probe); pressed again or Ctrl+\`, it hides, and the shell keeps running.
  - [ ] Dragging the handle changes the height between the limits; a double click gives 30% back; the height is the same after a reload and in another folder.
  - [ ] A terminal opened in the panel is a row under Terminal and opens on its own page with the same screen; one opened from the sidebar in the same folder is a tab in the panel.
  - [ ] With the focus in the terminal, Ctrl+W, Ctrl+P and Ctrl+B go to the shell and Ctrl+Shift+F finds in the terminal, none of them reaching the Editor; Ctrl+\` still hides the panel.
  - [ ] For a folder an agent works on, the button is off and says why; on another person's card it is off until Edit anyway.
  - [ ] `npm test`, `npm run build` and `scripts/ui-test.sh` pass.

### Contract CT: TB → TF0, TF1, TF2 (terminals)

Rust: `src-tauri/src/terminal.rs`, one `#[tauri::command]` per function below, with the same names in snake case and the arguments in snake case; registered in `lib.rs`; types with `#[serde(rename_all = "camelCase")]`. `folder` is always an `EditorFolder.id` (`editorFolders()`), checked with `editor::find` on every `terminal_open`.

```rust
#[tauri::command] pub fn terminal_list(st: State<'_, AppState>) -> Vec<TerminalSession>
#[tauri::command] pub async fn terminal_open(st: State<'_, AppState>, folder: String, cols: u16, rows: u16,
                                             name: Option<String>) -> Result<TerminalSession, String>
#[tauri::command] pub fn terminal_attach(st: State<'_, AppState>, id: String,
                                         output: tauri::ipc::Channel<tauri::ipc::Response>) -> Result<(), String>
#[tauri::command] pub fn terminal_input(st: State<'_, AppState>, id: String, data: String, binary: bool) -> Result<(), String>
#[tauri::command] pub fn terminal_resize(st: State<'_, AppState>, id: String, cols: u16, rows: u16) -> Result<(), String>
#[tauri::command] pub fn terminal_ack(st: State<'_, AppState>, id: String, bytes: u32) -> Result<(), String>
#[tauri::command] pub fn terminal_rename(st: State<'_, AppState>, id: String, name: String) -> Result<TerminalSession, String>
#[tauri::command] pub async fn terminal_close(st: State<'_, AppState>, id: String) -> Result<(), String>
// event "terminals-changed", payload Vec<TerminalSession>: after open, rename, close, a shell's end, and a change of `busy`
```

`src/api.ts` (with `import { Channel, invoke } from "@tauri-apps/api/core"`):

```ts
/** Every open terminal, in the order they were opened. */
export const terminalList = () => invoke<T.TerminalSession[]>("terminal_list");
/** Starts your shell in one of the Editor's folders (an EditorFolder id), at this size. Refuses, with the Editor's reason,
 *  a folder the Editor can't open or a card's worktree an agent is working on. `name` null: the folder's name, numbered. */
export const terminalOpen = (folder: string, cols: number, rows: number, name: string | null) =>
  invoke<T.TerminalSession>("terminal_open", { folder, cols, rows, name });
/** Sends the terminal's output to `onBytes`: first what it kept in memory (at most 256 KB), then everything new, in order.
 *  One listener per terminal: attaching again replaces the last one. */
export function terminalAttach(id: string, onBytes: (bytes: Uint8Array) => void): Promise<void> {
  const output = new Channel<ArrayBuffer>((b) => onBytes(new Uint8Array(b)));
  return invoke<void>("terminal_attach", { id, output });
}
/** What you typed or pasted (xterm's onData), or with `binary` xterm's onBinary: one byte per character. */
export const terminalInput = (id: string, data: string, binary = false) => invoke<void>("terminal_input", { id, data, binary });
export const terminalResize = (id: string, cols: number, rows: number) => invoke<void>("terminal_resize", { id, cols, rows });
/** xterm.js has written this many more bytes (its write callback; send it every 100,000 bytes or so). Above 500,000 bytes
 *  not yet acknowledged the backend stops reading until it is back at 200,000, and the program waits, as in any terminal. */
export const terminalAck = (id: string, bytes: number) => invoke<void>("terminal_ack", { id, bytes });
/** An empty name gives the default back. */
export const terminalRename = (id: string, name: string) => invoke<T.TerminalSession>("terminal_rename", { id, name });
/** Ends the shell and everything it started; resolves when they are gone (at most about 3 s). */
export const terminalClose = (id: string) => invoke<void>("terminal_close", { id });
export const onTerminalsChanged = (cb: (list: T.TerminalSession[]) => void): Promise<UnlistenFn> =>
  listen<T.TerminalSession[]>("terminals-changed", (m) => cb(m.payload));
```

`src/types.ts`:

```ts
export type TerminalSession = {
  id: string;                  // for as long as Gizai runs
  name: string;                // "gizai", "gizai 2", "GA-123", or the name given with Rename
  folder: string;              // the EditorFolder id it started in: which Editor panel shows it
  folderPath: string;          // as shown, ~ for the home folder
  taskId: string | null;       // when it started in a card's worktree
  shell: string;               // "zsh", "bash", "fish", "sh", "pwsh", "powershell", "cmd"
  busy: string | null;         // the program running in it now ("npm"); null while the shell waits for a command
  ended: TerminalEnd | null;   // null while the shell runs
  cols: number;
  rows: number;
  openedAt: number;            // ms since 1970
};
export type TerminalEnd = {
  code: number | null;         // the shell's exit code; null when a signal ended it
  at: number;                  // ms since 1970
  why: string | null;          // "Couldn't start zsh: …", or null
};
```

Errors are plain sentences, like the rest of the app ("The Editor can't open ~/.ssh: …", "QA Agent is working on GA-118: a terminal there waits until its run ends").

Paste on Linux (TF0) uses `readText()` from `@tauri-apps/plugin-clipboard-manager`, which TB adds with its one permission (`clipboard-manager:allow-read-text`); there is no Gizai command for it.

---

## 7. Open questions for Jefsev

1. **The Terminal page's tabs.** Recommended: the terminals of the same folder, the same tabs as that folder's Editor panel, so the page and the panel are one set of tabs in two places, and `+` needs no picker. The other way: every open terminal as a tab, which repeats the sidebar's rows.
2. **Where a terminal may start.** Recommended: the folders the Editor may open (projects' folders, the folders in `~/Herd`, card worktrees), with the Editor's refusals, so never `~/.ssh` or Gizai's data folder. From there `cd` goes anywhere, as you: the rule decides where Gizai starts a shell, it doesn't fence it in. Should your home folder be a start folder too (most terminal apps start there)? Recommended: not in the first version.
3. **A card's worktree while an agent works on it, or another person's card.** Recommended: no new terminal while an agent works there, until the run ends, as the Editor makes it read-only; one opened before stays, with the Editor's bar. On another person's card (F4), a terminal only after Edit anyway, as for editing. Or allow both with a warning?
4. **When the shell ends by itself.** Recommended: `exit` (code 0) closes the terminal; any other end keeps it with the reason and Start again, as VS Code does.
5. **Closing a terminal that runs a program.** Recommended: ask once, under the row or tab ("npm is still running in gizai 2. Close it and end npm?"). VS Code can ask for terminals with child processes, by default only for those in its editor area (`terminal.integrated.confirmOnKill`: `editor`); Gizai asks in both places, because a terminal moves between them. Closing one that only waits for a command doesn't ask.
6. **Quitting Gizai with terminals open.** Recommended: they end with Gizai (closing the window keeps Gizai and its terminals running, as now). Settings → Quit's question counts busy terminals with the agents ("1 agent and 2 terminals are at work: stop them and quit?"); the tray's Quit and Cmd+Q don't ask, as today. Nothing is restored at the next start: no old output on disk, and no shells started for you.
7. **How much a terminal keeps.** Recommended: 5,000 lines of scrollback each, in memory only (VS Code keeps 1,000). No limit on how many terminals; from 20 open, New terminal says that each keeps a shell running until you close it.
8. **The shell on Windows.** Recommended: PowerShell 7 when installed, else Windows PowerShell, else cmd; Git Bash, WSL and a Shell setting later, when asked. (Gizai keeps WSL out for the agents' CLIs and shell commands, and already finds Git Bash for a worktree's prepare commands (`os::git_bash`, `crates/gizai-agents/src/os.rs:149`), so a later Shell setting can offer Git Bash cheaply: `docs/PLATFORMS.md:7`, `:31`.)
9. **A run's "Open in terminal".** It copies `cd '<worktree>' && claude --resume <id>` for your own terminal app (`RunPanel.tsx:124`). Recommended: rename it to "Copy resume command" in TF1, so it isn't mistaken for Gizai's terminal; a later card may open a Gizai terminal in the worktree with the command typed, not run.
10. **Free.** Recommended: the terminal is free, with the code editor, in `docs/PAID-FEATURES.md` (§3.2 Q8).
11. **Order.** Recommended: TD and TB at once, then TF0, then TF1 and TF2 at once.

---

## 8. Sources

**The repository**, read on 2026-10-12 in this card's worktree and at `origin/main` `a70fd15` (`git show origin/main:<path>` for the files PR #97 changed): every `path:line` above. Commands: `git log`, `git diff --stat HEAD...origin/main`, `rg`, `gh pr list`, `gh pr view 96 --json additions,changedFiles`. Packages: `npm view @xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-webgl`, `@xterm/addon-search`, `@xterm/addon-web-links`, `@xterm/addon-unicode11`, `@xterm/addon-canvas`, `tauri-pty` and `@tauri-apps/plugin-clipboard-manager`, all on 2026-10-12.

**Outside sources**, all read on 2026-10-12:

| What | Link |
|---|---|
| portable-pty on crates.io: versions, dates, licence, downloads | https://crates.io/api/v1/crates/portable-pty |
| portable-pty 0.9.0's dependencies | https://crates.io/api/v1/crates/portable-pty/0.9.0/dependencies |
| portable-pty 0.9.0, `CommandBuilder` | https://docs.rs/portable-pty/0.9.0/portable_pty/cmdbuilder/struct.CommandBuilder.html |
| portable-pty 0.9.0 source: `cmdbuilder.rs`, `unix.rs`, `win/psuedocon.rs` | https://docs.rs/crate/portable-pty/0.9.0/source/src/cmdbuilder.rs, https://docs.rs/crate/portable-pty/0.9.0/source/src/unix.rs, https://docs.rs/crate/portable-pty/0.9.0/source/src/win/psuedocon.rs |
| Commits to WezTerm's `pty/` folder | https://api.github.com/repos/wezterm/wezterm/commits?path=pty |
| WezTerm #7709, "pty: windows: fix kill()" | https://github.com/wezterm/wezterm/commit/8afe0ad |
| tauri-plugin-pty on crates.io, and its 0.3.1 source | https://crates.io/api/v1/crates/tauri-plugin-pty, https://docs.rs/crate/tauri-plugin-pty/0.3.1/source/src/lib.rs |
| alacritty_terminal on crates.io | https://crates.io/api/v1/crates/alacritty_terminal |
| Tauri 2, "Calling the Frontend from Rust" (channels and events) | https://v2.tauri.app/develop/calling-frontend/ |
| Tauri 2.12.1, `Channel` | https://docs.rs/tauri/2.12.1/tauri/ipc/struct.Channel.html |
| Tauri 2.12.1 source: channels, `IpcResponse`, the security policy's nonces | https://github.com/tauri-apps/tauri/blob/tauri-v2.12.1/crates/tauri/src/ipc/channel.rs, https://github.com/tauri-apps/tauri/blob/tauri-v2.12.1/crates/tauri/src/ipc/mod.rs, https://github.com/tauri-apps/tauri/blob/tauri-v2.12.1/crates/tauri/src/manager/mod.rs |
| xterm.js 6.0.0 release notes | https://github.com/xtermjs/xterm.js/releases/tag/6.0.0 |
| xterm.js 6.0.0 typings (`xterm.d.ts`) | https://unpkg.com/@xterm/xterm@6.0.0/typings/xterm.d.ts |
| xterm.js and the WebGL addon, file sizes | https://unpkg.com/@xterm/xterm@6.0.0/?meta, https://unpkg.com/@xterm/addon-webgl@0.19.0/?meta |
| xterm.js flow control guide | https://xtermjs.org/docs/guides/flowcontrol/ |
| xterm.js WebGL addon README (6.0.0) | https://github.com/xtermjs/xterm.js/blob/6.0.0/addons/addon-webgl/README.md |
| xterm.js `DomRenderer.ts` and `BufferLine.ts` (6.0.0) | https://github.com/xtermjs/xterm.js/blob/6.0.0/src/browser/renderer/dom/DomRenderer.ts, https://github.com/xtermjs/xterm.js/blob/6.0.0/src/common/buffer/BufferLine.ts |
| VS Code's terminal settings (`terminalConfiguration.ts`, main) | https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/terminal/common/terminalConfiguration.ts |
| VS Code's `terminalInstance.ts` (`attachToElement`, `detachFromElement`, main) | https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/terminal/browser/terminalInstance.ts |
| VS Code docs, "Terminal Basics" (updated 2026-10-07) | https://code.visualstudio.com/docs/terminal/basics |
| PhpStorm docs, "Terminal" | https://www.jetbrains.com/help/phpstorm/terminal-emulator.html |
| WebKitGTK 2.40.0: ANGLE and WebGL2 | https://webkitgtk.org/2023/03/17/webkitgtk2.40.0-released.html |
| WebKit, "New WebKit Features in Safari 15": WebGL2 | https://webkit.org/blog/11989/new-webkit-features-in-safari-15/ |
| WebKit bug 264268, green artifacts on a transparent canvas (Tauri, xterm.js) | https://bugs.webkit.org/show_bug.cgi?id=264268 |
| WebKit's limit of 16 WebGL contexts (`WebGLRenderingContextBase.cpp`, main) | https://github.com/WebKit/WebKit/blob/main/Source/WebCore/html/canvas/WebGLRenderingContextBase.cpp |
| WebKit's test: `readText` of the platform's text shows the paste UI | https://github.com/WebKit/WebKit/blob/main/LayoutTests/editing/async-clipboard/clipboard-read-text-from-platform-expected.txt |
| Proxmox, detecting hardware WebGL2 before xterm.js's WebGL addon (2023-10-25) | https://lists.proxmox.com/pipermail/pve-devel/2023-October/059631.html |
| proc_pid_stat(5): `tty_nr`, `tpgid` | https://man7.org/linux/man-pages/man5/proc_pid_stat.5.html |
| libc 0.2.190, `KERN_PROC_TTY` (Apple) | https://docs.rs/libc/0.2.190/x86_64-apple-darwin/libc/constant.KERN_PROC_TTY.html |

Not read in this run, so marked [I] where used: Chromium's source for its WebGL context limit, tmux's manual (`window-size`), xterm.js's render service (drawing paused while hidden), and the docs of GNOME Terminal, Konsole and Terminal.app on login shells.
