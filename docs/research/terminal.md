# Gizai: terminals in the sidebar and a panel in the Code Editor (research, 2026-10-12)

Status: research only, no code. Written by the R&D Agent for GA-124 on 2026-10-12, against `main` at `5bd06a4` (Gizai 0.8.0, with the Editor of GA-91's cards: PR #96 merged that day). `docs/research/code-editor.md` is the model and is cited for what it already established.
Legend: **[V]** = verified today, in the code (file and line), with the command named (`git`, `gh`, `npm view`, `npm ls`, `rg`), or in the primary source named (its link and the date read are in §8). **[I]** = my judgement, or a fact I could not check in a primary source (the reason is given).
Limits: the run may not start node, cargo builds or a browser, so nothing was measured or tried in a webview; what rests on a measurement is marked [I] and given to a card to measure.

(Work in progress: the summary, the questions, the options, the design, the cards and the sources follow.)

---

## 2. The project now

### 2.1 What Gizai does for its users

Gizai is a local desktop app for one developer and a team of coding agents: clients, projects and cards on a board, agents (Claude Code and other CLIs) that work each card headless in its own git worktree, a Team Lead in Chat, Memory, Usage and Settings (README.md; `docs/research/code-editor.md` §2.1 describes it in full). [V] Since 12 Oct 2026 it also has the **Code Editor** of GA-91's cards: a folding Editor section in the sidebar listing the folders you opened, and an Editor page with a file tree, tabs, CodeMirror, search and Open in (PR #96, `src/pages/EditorPage.tsx`, `src/components/editor/`). [V `gh pr list`] A card's Changes view (PR #97) is in review; the Editor's Behind main bar with **Fetch** and **Update** (its backend is merged, `src/api.ts:233-241`) and a person's card in the Editor are cards still to build. [V `gh pr list --state open`; I for which cards remain] **Gizai has no terminal of its own yet**: no pseudo-terminal crate, no xterm.js, nothing in `Cargo.lock` or `package-lock.json` (`rg -i "pty|xterm|conpty|vt100|alacritty"`). Its only "terminal" is a run's **Open in terminal** button, which copies `cd '<worktree>' && claude --resume <id>` to the clipboard for your own terminal app (`src/components/RunPanel.tsx:124`, `src/lib/runs.ts:28-31`). [V]

### 2.2 How it is built

| Part | What it is | Where |
|---|---|---|
| Desktop shell | Tauri 2.12 (locked 2.12.1, wry 0.57.0; `Cargo.lock:3977-3978`, `:5463-5464`), crate `gizai`, Rust 2024 edition. A thin adapter: commands, app state, background loops, events. One window, `main`, 1440 × 900 (at least 900 × 600) (`src-tauri/tauri.conf.json:15`). Closing it only hides it; Quit is in the tray, Settings and Cmd+Q (`src-tauri/src/lib.rs:612-624`, `:326`, `:333-352`). | `src-tauri/`; `AppState` `lib.rs:33-69`; commands registered `lib.rs:559-601` (the Editor's at `:596-600`); events from `ui_notifier` `lib.rs:266-288` |
| Core | Data and rules, never Tauri (CLAUDE.md:36-38). SQLite. The folders the Editor may open: `crates/gizai-core/src/editor.rs` (`folders` `:103-148`, `find` `:150-168`, the type `EditorFolder` `:45-64`), with the refusals of `folders.rs` (key folders `:45-57`, `refusal_in` `:199-228`). | `crates/gizai-core/src/` |
| Agents | Everything that starts programs: git, worktrees, the coding CLIs, and the process trees: `os::command` (`crates/gizai-agents/src/os.rs:19-42`), `os::find_in` (`:64-91`), `spawn_tree` (`:251-287`) and `Tree::end` (`:204-227`). The Editor's files, search and git (`crates/gizai-agents/src/editor.rs`, its path gate `:264-331`). | `crates/gizai-agents/src/` |
| MCP | The `gizai-mcp` shim and the Team Lead's 46 tools (dispatch `src-tauri/src/tools/mod.rs:136-183`, catalog `:341-449`). | `crates/gizai-mcp/`, `src-tauri/src/tools/` |
| Screens | React 19.3, TypeScript 6, Vite 8. A hash router (`src/router.ts`; `href` `:48`, the parse `:30-41`); pages are one chain of conditions in `src/App.tsx:159-175`, so **leaving a page unmounts it** (the Editor at `:172`); only Settings keeps its tabs mounted (`SettingsPage.tsx:35-36`). `src/api.ts` holds the commands (`invoke<T>("snake_name", { camelArgs })`) and the event listeners (`listen<P>(…)`). | `src/pages/`, `src/components/`, `src/lib/` |
| Sidebar | Projects (`Sidebar.tsx:71-78`), **Editor** (`:80`, `EditorSection` `:179-246`), Agents (`:82-94`), Memory (`:96-97`). `FoldSection` (`:119-139`) draws a heading with an optional `+` and a caret, and unmounts its rows while folded (`:136`). Folded sections are kept in `localStorage` `gizai.sidebar.folded`, type `Fold = "editor" \| "agents" \| "memory"` (`src/lib/sidebar.ts:6-8`). | `src/components/Sidebar.tsx`, `src/lib/sidebar.ts` |
| The Editor | `#/editor/<folder>`; the folders you opened (at most 8) and each folder's tabs are kept in `localStorage` (`gizai.editor.opened`, `gizai.editor.tabs`; `src/lib/editor.ts:10-11`, `:59-87`). The page: top bar (`EditorPage.tsx:451-460`, its actions `:457-459` hold only Open in today), a folder bar, then `.ed` with the tree (`aside.ed-side` `:489-509`) and `.ed-main` (`:511`): tabs, file bar, the code (`.ed-code` `:549-562`) and the status bar (`.ed-status` `:564-579`). Its keys sit on `window` and don't look at where the key was pressed (`:407-423`). | `src/pages/EditorPage.tsx`, `src/components/editor/`, `src/styles/components.css:539-757` |
| Design system | Tokens in `design/tokens.json` → `src/styles/tokens.css` (dark `:2`, light `[data-theme="light"]` `:76`); D1's `syn-*` colours (`tokens.css:54-67`, light `:116-120`); D2's sizes `--size-tree` 260px, `--size-tree-row` 28px, `--size-tab` 36px, `--size-statusbar` 24px (`:152-155`); `--font-mono: "Atkinson Hyperlegible Mono", ui-monospace, monospace` (`:157`). Teal `--live` only for an agent at work, magenta `--needs` only for "needs you", no teal or magenta in code (`design/README.md:35-38`, `:50-51`). Lucide icons; the concept "terminal" is the icon `terminal` (`design/README.md:145`). **No terminal (ANSI) colours yet.** The bundled mono font files are cut to Latin ranges (`src/styles/fonts.css:2-5`, `:21-26`): they have **no box-drawing characters** (U+2500–257F). | `design/`, `src/styles/` |
| Appearance | Theme, fonts and sizes are attributes and CSS variables on `<html>` (`src/lib/appearance.ts:85-97`); a change fires no DOM event, only a listener set inside the module (`:124-137`, `useAppearance` `:151-160`). The `t` key toggles the theme (`App.tsx:42`). | `src/lib/appearance.ts`, `src/styles/appearance.css` |
| Security | Content security policy: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src ipc: http://ipc.localhost; …` (`tauri.conf.json:18`). The window may use `core:default`, `dialog:allow-open` and the opener's URL permissions (`src-tauri/capabilities/default.json:8-13`). No file system or shell plugin: every file and program goes through a Rust command. | |
| Data on disk | `~/.local/share/gizai` on Linux (`%APPDATA%\Gizai`, `~/Library/Application Support/Gizai`; `lib.rs:115-140`): the database, `runs/` (run logs), `worktrees/`, `code/`, `chat/`, `lead/`. | |

### 2.3 How processes, output and quitting work today

- **Starting and ending.** An agent's CLI starts with `os::spawn_tree`: on Unix in a process group of its own (`process_group(0)`, `os.rs:255`), on Windows suspended inside a Job Object that ends everything in it when its handle closes (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, `os.rs:341-364`, `:366-402`). **Stop** sends SIGINT, SIGTERM after 5 s and SIGKILL after 10 s to the group; on Windows it ends the job at once (`crates/gizai-agents/src/process.rs:243-262`; `docs/PLATFORMS.md:21`). After any exit, what is left gets SIGTERM, then SIGKILL after 3 s (`process.rs:263-276`). [V]
- **PATH.** On Linux the launcher starts Gizai through `bash -lc`, so Gizai has your login shell's PATH (`install.sh:319-321`), and `runs::command_path` adds the login PATH's folders again per command (`src-tauri/src/runs.rs:773-793`). On macOS Gizai takes the login shell's PATH at start (`$SHELL -l -i -c`, at most 5 s; `src-tauri/src/shell_path.rs:6-18`, `:32-87`; `main.rs:24-25`). Windows apps get the full PATH. A run gets Gizai's environment plus its own variables (`TMPDIR` in the worktree, `CLAUDE_CODE_DISABLE_AUTO_MEMORY`, …; `process.rs:180`, `worktree.rs:280-283`). [V]
- **Output to the window.** A run's output is one `run-event` per parsed line, sent with `app.emit` to the whole window, without batching (`process.rs:205-214`, `runs.rs:1114-1123`, `lib.rs:277`); the page filters it by run (`RunPanel.tsx:58-74`). **No code uses Tauri's `Channel` yet** (`rg -w Channel`). [V]
- **Quitting.** The first quit while agents work stops them first (at most 12 s), then exits; on exit Gizai ends WebKitGTK's page process and what is left of the agents (`src-tauri/src/quit.rs:25`, `:35-60`; `lib.rs:607-611`, `:630-637`). Settings → Quit asks in the button itself when agents work ("Stop them and quit?", `src/components/QuitSettings.tsx:20-30`); the tray's Quit, Cmd+Q and signals don't ask. SIGTERM, SIGINT and SIGHUP quit the same way (`quit.rs:78-104`). [V]

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

(Q1, Q2 and Q4 follow below Q8.)

**Q3. Sessions: how many, closing, leaving the page, quitting, scrollback, many open.**

Answer and recommendation:

- **Any number at once**, each a row under Terminal (§5.1). The backend keeps them in a list in `AppState` (TB), in the order opened; the window keeps the same list (`terminal_list`, `terminals-changed`). A terminal costs a shell process, two threads in Gizai (reading and writing) and its scrollback in the window. [I]
- **Closing ends the shell and everything it started, as Stop ends a run**, in two steps like Stop's (`process.rs:243-276`) [V Stop; I the steps]:
  - *Linux and macOS:* SIGHUP to the terminal's foreground process group and to the shell's own group, then closing the pseudo-terminal (the kernel then sends SIGHUP to the shell's session, as when a terminal window closes). After 3 s, SIGKILL to every process still attached to the terminal: on Linux the processes whose `/proc/<pid>/stat` names it as their controlling terminal (`tty_nr`, field 7 [V proc_pid_stat(5)]), on macOS those `sysctl` lists for it with `KERN_PROC_TTY` (in `libc` 0.2.190, the version Gizai locks [V docs.rs, `Cargo.lock:2179-2180`]). That reaches background jobs and `nohup` programs too, which live in other process groups of the same terminal and which `kill(-group)` alone would miss. A program that left the terminal on purpose (`setsid`, a daemon) keeps running, as it would after closing any terminal window. Gizai's own `spawn_tree` can't be used as it is: it puts the child in a new process group (`os.rs:255`), and a terminal's shell must lead a new session instead, which `portable-pty` does itself (§3.2 Q1). [V `os.rs`; I the rest]
  - *Windows:* right after the shell starts, TB puts it in a **Job Object** with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, the same kind `spawn_tree` uses for agents (`os.rs:341-364`), so everything the shell starts is in the job. Close ends the job (`TerminateJobObject`, as `Tree::end`, `os.rs:222-226`) and closes the pseudo-console. The shell runs for a few microseconds before it is in the job, too short to start anything. [V `os.rs`; I the gap]
- **They keep running when you leave the page or hide the panel.** The backend reads every terminal all the time, and the window writes what arrives into that terminal's one xterm.js view, which lives for the terminal's life outside the pages (§5.3). Pages unmount when you leave them (`App.tsx:159-175`), so the view can't live inside one. A view that isn't on screen still takes in output and acknowledges it, so no program stalls because you looked away. [V the unmount; I design]
- **When Gizai quits,** every terminal ends, with the steps above, in `quit.rs`'s exit path next to the agents (`end_agents`, `quit.rs:52-60`). Closing the window only hides it (`lib.rs:612-624`), so terminals keep running then, like agents. If Gizai crashes, Linux and macOS close its pseudo-terminals with the process (SIGHUP to each shell) and Windows ends the jobs when their last handle goes, so no shell is left behind. Nothing is restored at the next start (question 6). [V quit and window; I crash behaviour]
- **Scrollback: 5,000 lines** per terminal, in memory only (VS Code's default is 1,000 [V `terminalConfiguration.ts`]); Clear empties it. §3.2 Q2 gives what that costs.
- **Very many open:** no limit and no dialog. From the 20th, New terminal says "20 terminals are open: each keeps its shell running until you close it." The folded heading always says how many are open. [I]

**Q5. Where a terminal starts, and how each row is named.**

Answer and recommendation:

- **From the Editor's panel:** the root of the folder the Editor shows (its `EditorFolder.id`).
- **From the sidebar's `+`:** a folder you pick in the Editor's folder picker (Projects, In ~/Herd, Cards), with the folder the main panel shows on top.
- **A card's worktree:** in the picker's Cards group, and from the Editor when it shows one. Refused while an agent works on that card, as the Editor makes that worktree read-only (`gizai-core/src/editor.rs:136-140`), with the same sentence ("QA Agent is working on GA-118: …"). (Open question 3.)
- **The same folder rules as the Editor:** `terminal_open` asks `editor::find` (`crates/gizai-core/src/editor.rs:150-168`), so only projects' linked folders, the folders directly in the Editor folders (`~/Herd`), and card worktrees; never `/`, your home folder, the folders with keys and logins (`~/.ssh`, `~/.gnupg`, `~/.aws`, … `folders.rs:45-57`) or Gizai's data folder apart from card worktrees (`folders.rs:199-228`). A link that leads out is checked where it leads (`editor.rs:170-183`). [V]
- **Names:** the folder's own name ("gizai"), then "gizai 2", "gizai 3" for more in the same folder (the lowest free number); a card's worktree shows the card's identifier with its title faint after it, as the Editor's card rows do (`Sidebar.tsx:237`). Rename (F2, a double click on a tab) sets any name up to 40 characters; an empty one gives the default back. The row's right side shows the program running now, never the shell's own title escape sequence, which any program can set. [V the Editor's rows; I the rest]

**Q6. Safety.**

Answer: as §5.6 says. In short: no agent or Team Lead tool can type into a terminal or read it (none exists, and TB adds none); nothing from a terminal is saved to disk, Memory or a card; scrollback lives in memory only and goes with the terminal; links open only on Ctrl+click and only for http(s); no program can read the clipboard. Recommendation: make each of these an acceptance criterion of TB or TF0 (§6), so they are tested, not just intended. [V for what exists; I for the rules]

**Q7. The screens.**

Answer: §5.1 (sidebar section, New terminal, the Terminal page with its tabs, the Editor's panel with its drag handle, one terminal in two places) and §5.2 (keys), in D2's style. Recommendation: TD draws them as a page in the repository, as D2 did, before TF0 to TF2 are built.

**Q8. Free or paid.**

Answer: free. `docs/PAID-FEATURES.md` keeps the whole desktop app free and sells only hosted services; "the code editor and a card's changes" is on its free list (`:36`) and in the plans table's first row (`:46`). [V] A terminal runs entirely on your computer, so nothing in it could be a hosted service. Recommendation: TF1 writes "the terminal" next to the code editor in both places and in README.md.

---

## 5. The design

All screens use the design system as it is, with D2's Editor (`design/code-editor/editor.html`, made by `gen-editor.mjs`) as the model: sidebar rows (`.nav-item`, the row's state in `.meta`), the top bar with breadcrumbs and actions (`.topbar`), the Editor's tabs (`.ed-tab`, `--size-tab` 36px), the folder picker (`src/components/editor/FolderPicker.tsx`), the right-click menu (`ContextMenu`, `Menus.tsx:17-53`), Lucide icons at 16px (`terminal` for a terminal, `design/README.md:145`), sentence case, and questions asked in one sentence under the row, never in a dialog (`gen-editor.mjs:680`). New are the terminal colours, the drag handle and the Terminal button; card TD designs them as a page in the repository for Jefsev to review. [V for the existing parts; I for the rest]

### 5.1 The screens

**Screen 1: the sidebar's Terminal section.** A folding **Terminal** section between Projects and Editor (`Sidebar.tsx:78`–`:80`), drawn with `FoldSection` like Editor: `+` (New terminal) and the caret on its heading; `"terminal"` joins `Fold` (`src/lib/sidebar.ts:8`). [V where; I design]

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
- **Folded:** the heading shows how many are open, "3 open", like the Editor's "2 behind".
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

- **The Terminal button** (`btn ghost sm`, Lucide `terminal`, "Terminal") sits in the top bar's actions **between Fetch and Open in** (`EditorPage.tsx:457-459`; Fetch arrives with the Editor's Behind main card, so until then it is first). It is pressed (`aria-pressed`) while the panel shows; its tooltip says "Show the terminal (Ctrl+\`)" or "Hide the terminal (Ctrl+\`)". [V where; I design]
- **What it does:** shows the panel. When the folder has no terminal yet, it starts one in the folder's root, as PhpStorm ("the root directory of the current project") and VS Code ("the folder that is opened in the Explorer") do [V their docs]. The terminal gets the focus.
- **Where:** in the code column (`.ed-main`), between the code (`.ed-code`, `EditorPage.tsx:549-562`) and the file's status bar (`:564-579`), so the file tree keeps its full height, as in VS Code's default layout [I]. (The status bar then sits under the panel, at the Editor's bottom edge as in VS Code and PhpStorm; TD may keep it with the code instead.)
- **Height:** the first time **30% of the column** (tabs, code and panel together); the **drag handle** is the panel's top border: a 1px line with a 6px grab area, `row-resize` cursor, `role="separator"` with `aria-orientation="horizontal"` and the height in `aria-valuenow`; ↑ and ↓ move it by 5% when it has focus, a double click sets it back to 30%. The panel keeps at least its tab row and 4 lines (about 120px), the code at least 120px. Gizai remembers the height as a share of the column (not pixels), once for all folders, in `localStorage` (`gizai.editor.panel`), so it stays 30% when the window changes size. [I]
- **Its own tabs:** this folder's terminals (the same as on the Terminal page), `+`, ×, the running program; on the right **Own page** (opens `#/terminal/<id>`) and **Hide** (Lucide `chevron-down`, Ctrl+\`). Hiding keeps every terminal running; closing the last tab hides the panel. Whether the panel was open is remembered per folder, like the folder's file tabs (`gizai.editor.tabs`, `src/lib/editor.ts:59-87`).
- **A card's worktree an agent works on** (the Editor's read-only folder, `folder.readOnly`): the button is off, with the folder's reason ("QA Agent is working on GA-118: a terminal there waits until its run ends"); a terminal opened before the run started stays, with the Editor's read-only bar above. (Open question 3.)

**Screen 5: one terminal, two places.** Only one page shows at a time (`App.tsx:159-175`), so the Terminal page and the Editor's panel are never on screen together. A terminal moves with you: open gizai 2 from the sidebar and the page has it; go back to the Editor and the panel has it again, with everything on it, its scrollback, what runs in it and its selection. The terminal takes the size of where it is (120 × 12 in the panel, 132 × 38 on the page), and a full-screen program (vim, htop) redraws at the new size. When two places want it at once, which a second window or a split could bring later, **the place that asked last shows it** and the other shows a quiet placeholder: "gizai 2 is on its own page. [Show here]". VS Code works the same way: a terminal is in one place at a time and moves between the panel and the editor area [V VS Code's docs]. §5.3 says how.

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
| every other key | in a terminal | goes to the shell: Ctrl+W, Ctrl+P, Ctrl+B, Ctrl+K, Ctrl+R and Escape do what the shell or the program wants, not what the Editor or Gizai would (`EditorPage.tsx:407-423`, `App.tsx:36-43`) |

The key for Ctrl+\` is read by where it is (`KeyboardEvent.code === "Backquote"`, the key left of 1), so it works on keyboards where that key has another character. [I]

### 5.6 Security and privacy

- **It runs as you, and says so by being yours only.** A terminal is a shell with your rights, like any terminal app. Gizai's job is that nothing else drives it or reads it. [I]
- **Agents and the Team Lead can't reach it.** The Team Lead's 46 tools read the database and the list of live runs, never in-memory state or logs (`src-tauri/src/tools/mod.rs:136-183`, `:341-449`; `src-tauri/src/tools/read.rs`), and task runs get no Gizai tools at all (`src-tauri/src/runs.rs:975`). [V] TB adds no tool and no MCP entry; the tool catalog's test lists every tool, so a terminal tool would show there. The terminal commands are Tauri commands, which only the window can call (`capabilities/default.json:8-13`). [V for today; I for the rule]
- **Nothing from a terminal is kept on disk.** Its scrollback is xterm.js's buffer in the window's memory, and TB keeps at most 256 KB per terminal in Gizai's memory for a view that attaches again; both go with the terminal. No log file, no database row, no Memory note, nothing on a card. This matters beyond privacy: agents' runs have `cat`, `rg` and `tail` among their default commands and run as you (`crates/gizai-core/src/seed.rs:245-250`; the folder limits apply to the file tools, not to commands, `folders.rs:1-2`), so a terminal log under `<data>` would be readable by any run. [V the agents' commands; I the rule]
- **Only the window's own terminals.** `terminal_input` writes only to a terminal the window opened, by its id; there is no command that runs a command line. A link is opened only on Ctrl+click (Cmd+click), only `http:` and `https:`, through the opener the app already allows (`opener:allow-open-url`, `capabilities/default.json:11-12`). No program in the terminal can read the clipboard: the clipboard addon (OSC 52) isn't used. Paste uses bracketed paste, so a pasted line isn't run before Enter in shells that support it (xterm.js does this itself, §3.2 Q2). [I design; V the capability]
- **Where it starts.** Only in a folder the Editor may open, checked by `editor::find` on every `terminal_open` (§3.2 Q5), so never in `~/.ssh` or Gizai's data folder, and not in a card's worktree while an agent works there. This decides where Gizai starts a shell; it doesn't fence the shell in. [V the check; I the rule]
- **No new rights for the window**: no shell or file plugin, the same capabilities and content security policy (§3.2 Q2).

### 5.7 How it is tested

- **Rust** (TDD, CLAUDE.md:39-45): `crates/gizai-agents/tests/terminal_test.rs`. Behind `#[cfg(unix)]`: a session with `/bin/sh` in a temporary folder echoes what it is sent; `stty size` follows `resize`; `TERM`, `COLORTERM` and `TERM_PROGRAM` are set and `GIZAI_*` aren't; `busy` names `sleep` while it runs; close ends a foreground `sleep 1000`, a `sleep 1000 &` and a `nohup sleep 1000 &` within 3 s (their pids are gone); 50 MB of output arrives in order through a slow fake listener, reading pausing and going on; the shell's own `exit 3` gives `ended.code == 3`. Behind `#[cfg(windows)]` (CI's Windows runner): `cmd.exe /c echo hi` through ConPTY; close ends `ping -t 127.0.0.1` started from the shell (the Job Object). Everywhere: a folder the Editor refuses is refused with its sentence; nothing appears under the data folder. `src-tauri/tests/` flow test: open, attach, input, close, and quit ending what is left.
- **TypeScript** (`npm test`, in Node without a DOM, `vitest.config.ts:2`): the store (names "gizai", "gizai 2", a card's identifier, rename back to the default, the order of rows), the panel's height share and its limits, where Ctrl+\` and the copy keys go per system, the acknowledgement counter, and the `"terminal"` fold.
- **UI** (`scripts/ui-test.sh`, headless in `cage`, `src/selftest.ts`): open a terminal, type `echo gizai-probe`, find it in the terminal's buffer; leave and come back, it is still there; the Editor's button opens the panel at 30% and `pwd` prints the folder; drag the handle, reload, the height stays; Ctrl+K and Ctrl+W in the terminal don't open the palette or close a file; `yes` for 10 s keeps the window drawing (frames counted) and Ctrl+C stops it. Screenshots with `scripts/shot-cage.sh` for the design review.
- **Linux, macOS, Windows:** CI builds and tests all three (`.github/workflows/ci.yml`); the shell differs per system (bash or zsh, zsh on macOS, PowerShell on Windows), so tests that type commands use `/bin/sh` or `cmd.exe` explicitly.

### 5.8 What is left out

A terminal for agents (not this card); splitting a terminal into panes; terminals restored after a restart; a Shell setting, Git Bash and WSL; shell integration (marking each command and its exit code, as VS Code's injected scripts do); opening `src/lib/x.ts:12` in the Editor from a click in the terminal; the clipboard addon and images (sixel); "Open in terminal" for your own terminal app; and a second window.

---

## 6. Proposed cards

Five cards, each one pull request that fits in one agent run (the Editor's F2, PR #96, was 5,201 added lines in 35 files in one run [V `gh pr list`]). "Builds on" names the cards that must be merged first. The contract between the backend card and the frontend cards is below the list, word for word.

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
- **Builds:** `crates/gizai-agents/src/terminal.rs` (new): the sessions, on `portable-pty` 0.9 (§4); your shell and environment (§3.2 Q1); reading on a thread per terminal, batched, with flow control and 256 KB kept in memory for a view that attaches again; writing on a thread of its own; resize; the program running now; ending a terminal and everything it started (§3.2 Q3). `crates/gizai-agents/src/os.rs`: the user's shell; on Windows the shell's Job Object; on Unix the processes still attached to a terminal. `src-tauri/src/terminal.rs` (new) with the commands and the event of contract CT, `AppState.terminals`, registered in `lib.rs`; every terminal ended at quit (`quit.rs`). The start folder is checked with `editor::find` (`crates/gizai-core/src/editor.rs:150-168`). `src/api.ts` and `src/types.ts` as in CT. `docs/PLATFORMS.md`: a row for the terminal (shells, ConPTY, how it ends). No database change, no new Tauri plugin, the same security policy and capabilities.
- **Acceptance criteria:**
  - [ ] `terminal_open` starts your shell in one of the Editor's folders and refuses any other folder with the Editor's own sentence; it refuses a card's worktree while an agent works on it.
  - [ ] On Linux and macOS the shell is `$SHELL` (else your entry in the password database, else `/bin/sh`), a login shell on macOS; on Windows `pwsh.exe` when it is found, else `powershell.exe`, else `cmd.exe`. The environment has `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=gizai` and `TERM_PROGRAM_VERSION`, and no `GIZAI_*` variables (tested).
  - [ ] Output reaches the attached channel as bytes, in order, at most one message per terminal every 8 ms; 50 MB of output arrives complete, while reading pauses above 1 MB not yet acknowledged and goes on below 256 KB (tested with a fake listener that acknowledges slowly).
  - [ ] Input, a paste of 1 MB and resize work (`stty size` on Unix and `$Host.UI.RawUI.WindowSize` on Windows report the new size).
  - [ ] `terminal_close` ends the shell, a program in the foreground (`sleep 1000`), a background job (`sleep 1000 &`) and a `nohup` one within 3 s on Linux and macOS, and on Windows the shell and its children (`ping -t`), through the Job Object.
  - [ ] Every terminal ends when Gizai quits, and when it is killed (Linux: closing the pseudo-terminal; Windows: the job), tested with a scratch process.
  - [ ] `busy` names the program in the foreground within a second; `terminals-changed` comes on open, rename, busy, end and close.
  - [ ] Nothing a terminal shows or is sent is written to disk, to the database, a log, Memory or a card; no MCP tool reaches terminals (the catalog test lists none).
  - [ ] `cargo test --workspace`, `npm test` and `npm run build` pass, and CI passes on Linux, macOS and Windows.

### TF0. The terminal view: xterm.js in Gizai's colours, one view per terminal

- **Column, role:** To do, frontend.
- **Builds:** TD's `term-*` tokens in `design/tokens.json` and `src/styles/tokens.css` (with `scripts/tokens-css.ts`) and their rule in `design/README.md`; the packages of §4 (`@xterm/xterm` and its addons); `src/lib/terminals.ts` (the store of sessions from `terminalList` and `onTerminalsChanged`, and one xterm.js `Terminal` per session for its whole life, attached once to its output, moved between places, §5.3); `src/components/terminal/TerminalView.tsx` (places the view, fits it, sends the size, recolours it on a theme change, the keys of §5.2, copy and paste, links, find, acknowledging what it drew); `src/styles/terminal.css` (xterm's own stylesheet, imported, and Gizai's); `src/pages/TerminalPage.tsx` with `#/terminal` (New terminal, with the folder picker) and `#/terminal/<id>` (the terminal under its top bar), and the route in `src/router.ts`; a UI probe in `src/selftest.ts` and `scripts/ui-test.sh`.
- **Acceptance criteria:**
  - [ ] A terminal opens in a folder from `#/terminal`; typing `echo gizai-probe` shows it (UI probe in `cage`).
  - [ ] Leaving the page and coming back shows the same terminal, with what it printed meanwhile and its scrollback (5,000 lines).
  - [ ] Colours come from the tokens; `t` and Settings → Appearance recolour and refont open terminals at once; box-drawing characters line up (`tree`, htop).
  - [ ] Copy, paste and find with the keys of §5.2 on each system; Ctrl+click opens an http(s) link with the opener, nothing else opens.
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
- **Builds:** the Terminal button between Fetch and Open in; `src/components/editor/TerminalPanel.tsx` between the code and the status bar, with its tabs (the folder's terminals), Own page and Hide; the drag handle; the height as a share in `localStorage` and whether the panel shows per folder; Ctrl+\` and Ctrl+Shift+\`; the Editor's keys (`EditorPage.tsx:407-423`) left alone while a terminal has the focus; Own page and Show in the Editor moving the one view.
- **Acceptance criteria:**
  - [ ] The button opens the panel at 30% of the column with a terminal in the folder's root (`pwd` prints it, probe); pressed again or Ctrl+\`, it hides, and the shell keeps running.
  - [ ] Dragging the handle changes the height between the limits; a double click gives 30% back; the height is the same after a reload and in another folder.
  - [ ] A terminal opened in the panel is a row under Terminal and opens on its own page with the same screen; one opened from the sidebar in the same folder is a tab in the panel.
  - [ ] With the focus in the terminal, Ctrl+W, Ctrl+P, Ctrl+B and Ctrl+Shift+F go to the shell, not to the Editor; Ctrl+\` still hides the panel.
  - [ ] For a folder an agent works on, the button is off and says why.
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
/** The view has drawn this many more bytes (send it every 64 KB or so). Above 1 MB not yet drawn, the backend stops reading
 *  until it is back under 256 KB, and the program waits, as in any terminal. */
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

---

## 7. Open questions for Jefsev

1. **The Terminal page's tabs.** Recommended: the terminals of the same folder, the same tabs as that folder's Editor panel, so the page and the panel are one set of tabs in two places, and `+` needs no picker. The other way: every open terminal as a tab, which repeats the sidebar's rows.
2. **Where a terminal may start.** Recommended: the folders the Editor may open (projects' folders, the folders in `~/Herd`, card worktrees), with the Editor's refusals, so never `~/.ssh` or Gizai's data folder. From there `cd` goes anywhere, as you: the rule decides where Gizai starts a shell, it doesn't fence it in. Should your home folder be a start folder too (most terminal apps start there)? Recommended: not in the first version.
3. **A card's worktree while an agent works on it.** Recommended: no new terminal there until the run ends, as the Editor makes it read-only; one opened before stays, with the Editor's bar. Or allow it with a warning?
4. **When the shell ends by itself.** Recommended: `exit` (code 0) closes the terminal; any other end keeps it with the reason and Start again, as VS Code does.
5. **Closing a terminal that runs a program.** Recommended: ask once, under the row or tab ("npm is still running in gizai 2. Close it and end npm?"), as VS Code asks by default for terminals with child processes. Closing one that only waits for a command doesn't ask.
6. **Quitting Gizai with terminals open.** Recommended: they end with Gizai (closing the window keeps Gizai and its terminals running, as now). Settings → Quit's question counts busy terminals with the agents ("1 agent and 2 terminals are at work: stop them and quit?"); the tray's Quit and Cmd+Q don't ask, as today. Nothing is restored at the next start: no old output on disk, and no shells started for you.
7. **How much a terminal keeps.** Recommended: 5,000 lines of scrollback each, in memory only (VS Code keeps 1,000). No limit on how many terminals; from 20 open, New terminal says that each keeps a shell running until you close it.
8. **The shell on Windows.** Recommended: PowerShell 7 when installed, else Windows PowerShell, else cmd; Git Bash, WSL and a Shell setting later, when asked. (Gizai keeps WSL out for the agents' CLIs and shell commands: `docs/PLATFORMS.md:7`, `:31`.)
9. **A run's "Open in terminal".** It copies `cd '<worktree>' && claude --resume <id>` for your own terminal app (`RunPanel.tsx:124`). Recommended: rename it to "Copy resume command" in TF1, so it isn't mistaken for Gizai's terminal; a later card may open a Gizai terminal in the worktree with the command typed, not run.
10. **Free.** Recommended: the terminal is free, with the code editor, in `docs/PAID-FEATURES.md` (§3.2 Q8).
11. **Order.** Recommended: TD and TB at once, then TF0, then TF1 and TF2 at once.
