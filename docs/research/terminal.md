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
