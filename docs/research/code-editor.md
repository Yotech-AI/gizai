# Gizai: a card's changes, and an Editor for local folders (research, 2026-10-11)

Status: research only, no code. Written by the R&D Agent for GA-91 on 2026-10-11, against `main` at `f3541f0` (Gizai 0.7.1, database schema 16).
Legend: **[V]** = verified today, in the code (file and line) or with the command named (`git`, `gh`, `npm view`). **[I]** = my judgement, or a fact I know but could not re-check today.
Limits of this pass: web search and fetching web pages were refused in this run, and the run may not start node or a browser. So nothing was measured in a browser, outside facts come only from the npm registry (`npm view`) and from GitHub through `gh` (both read on 2026-10-11), and what only the libraries' websites document is marked [I] and listed in §7 and in the card summary as not checked. The speed of the diff components is therefore argued, not measured; card F1 measures it before the Changes view ships (§6).

---

## 1. Summary

1. Both parts fit how Gizai is built: git runs as a command, every card has its own worktree, CodeMirror 6 is already in the app, and the screens reach Rust only through `src/api.ts`. [V]
2. **A card's changes can always come from local git**, without GitHub's or Bitbucket's API: the worktree (uncommitted files included) against its merge base with main while it exists; the branch when only that is left; after a merge, the merge commit against its first parent. For PR #81 that gives exactly GitHub's diff (34 files, +1923 −112). `gh pr diff` is only a fallback. [V `git`, `gh`]
3. **Card diffs here are big.** Of the last 70 merged card pull requests, the median changes 964 lines in 20 files, a quarter change more than 2,108 lines, and the largest 15,131 lines in 245 files. The view must list files first, load each diff as it scrolls into sight, and fold unchanged code and lock files. [V `gh pr list`]
4. **Recommended: CodeMirror 6 for both** (`@codemirror/merge` for the diff, language packages and `@codemirror/search` for the editor), coloured with new syntax and diff tokens in the design system. Not Shiki-based diff components or Monaco: they bring their own colours and need WASM or workers that the app's content security policy blocks today. [V CSP; I]
5. **Changes:** a **Changes** tab at the top of the card, next to the card itself (Card | Changes +964 −112), full width: the file list on the left, every file's diff below each other on the right, removed lines red and added lines green, unchanged code folded. Each run in the Runs tab gets "Show changes" too. [I]
6. **Editor:** a folding **Editor** section in the sidebar above Agents, listing the folders you opened, and an Editor page: file tree, tabs, syntax colours, find and replace, search in the folder, Ctrl+S, each file's git status, outside changes, and a bar with behind, ahead and uncommitted, **Fetch** and **Update** (a fast-forward only, with `update_checkout`'s rules). [I]
7. **Blade is the one real gap:** no CodeMirror package for it exists, so we write a small Blade language on top of `@codemirror/lang-php`. [V registry; I effort]
8. **A person's card:** Open in editor makes the card's worktree as for an agent and pins the card to you, so agents leave it alone; Commit, Push and Hand over then move it on as a builder's `ready_for_testing` does. One rule in `qa_fail` changes. [V `workflow.rs:185`; I]
9. **Also add Open in VS Code, Zed or PhpStorm:** cheap, and better for big edits; the built-in editor stays basic. **Free:** nothing in `docs/PAID-FEATURES.md` changes; add the editor to "What stays free". [I]
10. **Eleven cards** (§6): two Design cards, then backend and frontend cards that can run two at a time.

---

## 2. The project now

### 2.1 What Gizai does for its users

Gizai is a local desktop app for one developer and a team of coding agents (README.md:3-5; docs/HANDOFF-2026-10-07.md:5-29). [V]

- **Work:** clients with contacts, projects (each linked to a local git repository, its *linked folder*, and optionally to GitHub, Bitbucket or another git URL), tasks on a board or in a list, with labels, priorities, acceptance criteria, files and docs, and an Inbox for "needs you" (README.md:125-126).
- **The board** is the card flow Backlog → To do → In progress → Testing → Review → Deploy → Done. Each column holds the agents that work its cards, is Auto (its agents take its cards by priority) or Manual (only Run starts one), and links to its next column; you set this up on Team → Workflow (README.md:133-134; `crates/gizai-core/src/columns.rs`, `workflow.rs`).
- **Agents** are coding CLIs (Claude Code, Codex, Gemini, others) with a role, model, effort, allowed commands, folders, tools and a budget. Each run is headless, in the card's own **git worktree** `<data>/worktrees/<KEY>/<identifier>` on the branch `gizai/<identifier>-<slug>` (`crates/gizai-agents/src/worktree.rs:84-120`; `src-tauri/src/runs.rs:693-717`). A new worktree is prepared first: paths copied from the linked folder, `composer install` or `npm ci` for what is missing, a setup command (`runs.rs:722-733`). A run ends with a `GIZAI_RESULT` line that moves the card (`workflow.rs:380-489`), and Gizai pushes the branch after every run (README.md:181).
- **QA and review:** the QA Agent tests cards in Testing; a pass moves them to Review, assigned to the person who reviews (`workflow.rs:345-351`, `:409-411`). Open pull request pushes the branch and opens the pull request with `gh` or Bitbucket's API (`src-tauri/src/pulls.rs:224-266`). The PR check asks every two minutes (`src-tauri/src/lib.rs:495-504`); a merge moves the card to Deploy or Done and removes its worktree, and its branch when the pull request has its latest commit (`pulls.rs:377-420`).
- **Runs:** live output, history with why each run ended, the commits it made (from `base_sha` to `head_sha`, `crates/gizai-core/src/model.rs:383-388`; `runs.rs:1154-1169`), Continue with a note, Run this for me, limits and budgets.
- **The Team Lead** answers in Chat with Gizai's own tools over MCP (`crates/gizai-mcp`, `src-tauri/src/tools/`), checks the board every 15 minutes when switched on, and reads its own read-only copies of the code at `<data>/code/<KEY>` (`src-tauri/src/code.rs:1-15`). When a linked folder's dependencies are behind main it offers to update it, and `update_checkout` does it: a fast-forward, then `composer install` or `npm ci`, never a merge, reset or the setup command (`code.rs:370-399`; `crates/gizai-agents/src/checkout.rs`).
- **Memory** (notes per agent and shared, the Memory page), **Usage** (tokens, API cost, subscription limits), **Settings** in tabs (General, Appearance, Notifications, Agents and runs, MCP servers, GitHub and Bitbucket; `src/router.ts:11`), desktop notifications, a tray icon, and updates from GitHub Releases (README.md:127-163).

### 2.2 How it is built

| Part | What it is | Where |
|---|---|---|
| Desktop shell | Tauri 2.12, crate `gizai` (Rust 2024 edition). A thin adapter: Tauri commands, the app state, background loops and events to the screen. | `src-tauri/`; state `src-tauri/src/lib.rs:31-62`; commands `src-tauri/src/commands.rs`, registered at `lib.rs:550-586`; events (`rows-changed`, `runs-changed`, `run-event`, `chat-event`, …) at `lib.rs:258-279`; plugins dialog and opener at `lib.rs:444-445` |
| Core | Data and rules, never Tauri (CLAUDE.md:36-38): tasks, projects, clients, team, columns, the workflow gates, runs, memory, settings, folder checks. SQLite through rusqlite (bundled). | `crates/gizai-core/src/`; database `db.rs` (schema 16, `db.rs:10`; migrations 0001–0016 listed at `db.rs:22-37`, files in `crates/gizai-core/migrations/`); settings are key–value JSON (`settings.rs:8`, `:18`) |
| Agents | Everything that starts programs: git (run as a command, `worktree.rs:51-59`; no git library), worktrees, preparing them, a linked folder's status and update (`checkout.rs`), the Team Lead's copies (`copies.rs`), `gh` (`github.rs`), Bitbucket's API (`bitbucket.rs`), pushes over SSH or HTTPS (`connection.rs`, `worktree.rs:370-417`), the coding CLIs, process trees (`os.rs`, `process.rs`). | `crates/gizai-agents/src/` |
| MCP | The MCP server protocol and the `gizai-mcp` shim Claude Code starts in a chat turn. | `crates/gizai-mcp/` |
| Screens | React 19, TypeScript 6, Vite 8. A tiny hash router (`src/router.ts`); pages switched in `src/App.tsx:158-172`; one file talks to Tauri (`src/api.ts:1`); data loads with `useData`, which reloads on every `rows-changed` (`src/lib/useData.ts:4-20`). Logic with tests in `src/lib/*.ts`. | `src/pages/`, `src/components/`, `src/lib/`, `src/types.ts` |
| Code editing today | CodeMirror 6 in `MarkdownEditor` only: `@codemirror/lang-markdown`, coloured with the app's CSS variables (`src/components/MarkdownEditor.tsx:20-41`, theme `:229-250`), Ctrl+S and Ctrl+Enter save (`:446-447`). Installed with it, but not yet used for code: `@codemirror/lang-javascript` (TS, TSX), `lang-html`, `lang-css`, `autocomplete`, `lint` (`node_modules/@codemirror/`). | `package.json:14-22` |
| Design system | Tokens in `design/tokens.json`, turned into `src/styles/tokens.css` by `scripts/tokens-css.ts`; components in `src/styles/components.css`; layout in `src/styles/app.css`; the rules in `design/README.md`; `design/gen-design-system.mjs` builds the design system pages. Dark first, blue accent, teal `--live` only for an agent at work, magenta `--needs` only for "needs you" (`design/README.md:34-45`). There are **no syntax or diff colours** yet (`src/styles/tokens.css:2-117`). | `design/`, `src/styles/` |
| Data on disk | `~/.local/share/gizai` on Linux (`%APPDATA%\Gizai`, `~/Library/Application Support/Gizai`): `gizai.db`, `runs/`, `chat/`, `backups/`, `worktrees/<KEY>/<identifier>`, `code/<KEY>`, `update/` (README.md:104-110; `lib.rs:108-133`). | |
| Security | Content security policy `default-src 'self'; script-src 'self'; …` with no `wasm-unsafe-eval` and no `worker-src` (`src-tauri/tauri.conf.json:18`). The window may only use `core:default`, `dialog:allow-open` and the opener's URL permissions (`src-tauri/capabilities/default.json`); there is no file system plugin: every file access is a Rust command. | |

### 2.3 How it is tested, built and released

- **Tests** (CLAUDE.md:39-45): `cargo test --workspace` (core, agents, MCP and `src-tauri/tests/*_flow_test.rs`, all with fake coding CLIs such as `crates/gizai-agents/tests/fake-claude.sh`); `npm test` (vitest, `src/lib/*.test.ts` and component tests); `npm run build`; `scripts/ui-test.sh` when the screens change: headless probes in `cage` driven by `src/selftest.ts`, against `.devdata/demo` made by `crates/gizai-core/examples/prep_*.rs`. Screenshots with `scripts/shot-cage.sh`. Never on Jeffrey's screen or data (CLAUDE.md:18-30).
- **CI** builds and tests every pull request and push to `main` on Linux, macOS and Windows: `npm ci`, `npm run build`, `npm test`, `cargo build`, `cargo test` (`.github/workflows/ci.yml:90-101`). Code for one system goes behind `cfg(...)`, programs start through `gizai_agents::os::command` (CLAUDE.md:47-50).
- **Release** (`docs/RELEASING.md`): bump the version in three places, merge `main` into `production` with a merge commit, tag `vX.Y.Z`, publish a GitHub Release; installed Gizais build it from source. A database change makes it a MINOR version (`RELEASING.md:21-27`).

### 2.4 What the two features touch

| Area | Today | For a card's changes | For the Editor |
|---|---|---|---|
| Task page `src/pages/TaskPage.tsx` | Header, title, description, criteria, attachments, Run and Pull request panels, then tabs Comments, Activity, Runs (`:77`, `:174-209`); Properties on the right, `]` toggles it (`:82-95`, `:212`); the column is at most 920 px wide (`src/styles/app.css:67`) | Card \| Changes tabs at the top; a full-width Changes view | Open in editor on a person's card |
| Runs tab `src/components/RunHistory.tsx:15-39` | Lists each run's commits (`runCommits`, `src/api.ts:160`) | Show changes of one run | |
| Sidebar `src/components/Sidebar.tsx` | Work, Projects, Agents, Memory; Agents and Memory fold (`FoldSection`, `:114-128`; folded state in `src/lib/sidebar.ts:5-6`) | | A folding Editor section above Agents (`Fold` gains `"editor"`) |
| Router `src/router.ts` | `task/<id>`; Memory already has a sub-view (`view: "graph"`, `:9`) | `#/task/<id>/changes` | `#/editor`, `#/editor/<folder>` |
| CodeMirror | Markdown only | `@codemirror/merge`, languages, a code theme | languages, `@codemirror/search`, Blade |
| Design system | No syntax or diff tokens | `diff-*` tokens | `syn-*` tokens, editor layout |
| Worktrees `worktree.rs` | `ensure`, `reuse`, `commits`, `uncommitted`, `remove_card_worktree` | diff of the worktree against main's merge base | open a card's worktree; a person's card |
| Runs `src-tauri/src/runs.rs` | `open_worktree` (`:693`), `prepare_worktree` (`:722`), `base_sha` (`:878-880`), `record_head` (`:1142`), `commits` (`:1154`) | per-run diff from `base_sha..head_sha` | the same worktree for a person |
| Pull requests `src-tauri/src/pulls.rs`, `crates/gizai-agents/src/github.rs` | `gh pr list --json number,url,state,isDraft,headRefOid,commits` (`github.rs:52`, `:169-175`); merge seen at `pulls.rs:386-398`; clean-up at `:408-420` | remember the merge commit (`mergeCommit`) | push for a person's card (`push_to`, `push_over_for`, `:146-160`) |
| Linked folder `checkout.rs`, `code.rs` | `status` (behind, dependencies; local only, `checkout.rs:59-76`), `plan` (`:144-196`), `update` (`:224-258`); `update_checkout` from chat (`code.rs:370-405`) | | Behind and Update in the Editor |
| Workflow `crates/gizai-core/src/workflow.rs` | The queue skips cards pinned to someone else, but a person as assignee doesn't stop it (`:185-188`); `qa_fail` gives the card back to its implementer (`:457-475`) | | A person's card: pin, hand over, QA fail |
| Settings | Key–value; `runs::Settings` holds what Settings shows (`src-tauri/src/runs.rs:165`) | | `editor_roots` (default `~/Herd`) |
| Folder checks `crates/gizai-core/src/folders.rs` | Refuses `/`, home, Gizai's data, key folders for agent folders (`:46-56`, `:189`) | | the same refusals for Editor roots |
| Opener | `tauri-plugin-opener` opens files and links (`commands.rs:318-328`) | | Open in VS Code, Zed, PhpStorm |

What the work must not touch: the Team Lead's copies (`<data>/code/<KEY>`), which Gizai resets at will (`crates/gizai-agents/src/copies.rs:37-43`), and Gizai's own data folder apart from card worktrees. [V]

---
