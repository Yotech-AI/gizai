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

## 3. The feature

### 3.1 What it should do

From the card (Jefsev, 10 and 11 October 2026):

1. **A card's changes.** On a card, see at a glance which files it changed and the changed lines, like Zed's Changes panel: a Changes tab and a tab per open file beside the conversation; "Since origin/main +6 −4" on top; each changed file under its path, removed lines red and added lines green, with line numbers; unchanged code folded, with arrows to open it. Easy to use comes first.
2. **An Editor in the sidebar, above Agents.** Open a local folder (a project in `~/Herd/*`) in a basic code editor with syntax colours. It shows when the folder is behind main on GitHub. A small card can then go to a person who does it quicker by hand.
3. It may be built before the website and offered there as a free feature, so Gizai suits every developer, also one who codes by hand next to the agents.

What "a person's card" means here [I]: a card assigned to a person, which that person builds in the card's own worktree with the Editor (or their own editor), then hands over to QA or to Review like an agent would.

### 3.2 The ten questions

#### A card's changes

**Q1. Where does the diff come from, in each column?**

Answer: always from local git, in this order. [V for the commands' sources; I for the order]

| Card | Changes come from | How (git as a command, like the rest of Gizai) | What they include |
|---|---|---|---|
| No branch yet (Backlog, To do before its first run) | nothing | `tasks.branch` is set by the first run (`crates/gizai-core/src/runs.rs:113`) [V] | "No changes yet: the first run makes this card's branch." |
| Its worktree is there (In progress, Testing, Review, a person's card) | the worktree against its **merge base** with main as last fetched | `git merge-base HEAD <main>`, then `git diff --numstat -z -M <base>` and `git ls-files --others --exclude-standard -z` for new files | commits **and** uncommitted changes; untracked files count as added |
| Worktree gone, branch still there (removed in Settings → Data, or kept after a merge) | the branch against its merge base | `git diff --numstat -z -M <base> refs/heads/<branch>` | commits only |
| Merged (Deploy, Done) | the **merge commit against its first parent** | `git diff -M <merge>^1 <merge>` | exactly what the pull request brought into main |
| Merged, but the merge commit is unknown or not on this disk | fetch main (as a card start does), else ask GitHub or Bitbucket | `gh pr diff <n>`; Bitbucket `GET /2.0/repositories/<ws>/<repo>/pullrequests/<id>/diff` [I] | a patch only, without the files' full text |
| One run | the commits it made | `git diff <base_sha> <head_sha>` (`model.rs:383-388`) | that run's commits only |

Notes:

- `<main>` is the ref card starts use: `refs/remotes/<remote>/<branch>` when the repository has a remote for the project's link, `refs/gizai/base/<branch>` when it hasn't, the local default branch without a link (`src-tauri/src/git.rs:65-72`, `worktree.rs:302-307`). On screen it reads **"Since origin/main"** (or "Since main"). [V]
- Against the merge base, not main's tip: otherwise every commit main got since the card started would show as removed lines. [I]
- `.gizai-tmp/` never shows: Gizai keeps it out of git status through `info/exclude` (`worktree.rs:204-247`). [V]
- **After a merge** the worktree and, when the pull request has its latest commit, the branch are gone (`pulls.rs:408-420`). This repository merges with merge commits (`f3541f0` has the parents `4d03026` and `e3b1d2c`), and the Team Lead's `merge_pull_request` always does (`github.rs:158-164`), so the card's commits stay reachable from main, and the merge commit against its first parent is the card's diff for good. For PR #81, `git diff f3541f0^1 f3541f0` and `git diff 4d03026...e3b1d2c` both give 34 files, +1923 −112, exactly GitHub's numbers (`gh pr view 81 --json additions,deletions,changedFiles`). [V] A squash merge has one parent, so the same "first parent → merge commit" works for it. [I]
- Gizai therefore needs **the merge commit's id**. `gh` gives it as `mergeCommit` (in `gh pr view --json`'s field list, together with `baseRefOid`, `headRefOid` and `files`). [V `gh`] Bitbucket gives `merge_commit.hash` on the pull request. [I] Save it on the card when the PR check sees the merge (`pulls.rs:386-398`, `crates/gizai-core/src/pulls.rs:122`), in a new column `tasks.merge_sha` (migration 0017; a MINOR release). Cards merged earlier get it looked up once, when their Changes view opens.
- `gh pr diff` is only the last resort: it needs the network and gh's login, works for GitHub only, gives a patch without the files around it (so no "show more lines"), and GitHub refuses very large diffs. [V `gh pr diff --help`; I for the size limit]
- **Per run: yes.** It is cheap (the two ids are saved already, `runs.rs:878-880`, `:1142-1149`) and answers "what did this run do?". It shows commits only; a run's own note already says how many uncommitted changes it left (README.md:181). [V]

Recommendation: one Rust function that picks the source in this order (worktree, branch, merge commit, GitHub or Bitbucket), returns the file list with counts at once, and each file's text before and after only when the screen asks for it (§5.3).

**Q2. The UX: a tab next to Comments, a panel beside the card, or a full-width view?**

Answer:

- *A tab next to Comments* puts the diff at the bottom of a long page, under the description, criteria, attachments and the Run and Pull request panels (`TaskPage.tsx:161-178`), in a column at most 920 px wide (`app.css:67`). Cramped, and far down. [V layout; I judgement]
- *A resizable panel beside the card, like Zed:* with the window at its usual 1440 px (`tauri.conf.json:15`), the sidebar (248 px) and Properties (320 px, `tokens.css:109-110`) leave about 870 px; split in two, each half gets about 430 px, which shows about 55 characters of code. Too narrow. [I, computed]
- *A full-width view* gives the code room: about 870 px with Properties open, about 1,190 px with it closed; minus a 260 px file list that leaves roughly 120 characters of code. [I, computed]

Recommendation: the full-width view, reached as a **tab at the top of the card**: **Card** and **Changes +964 −112**, the counts on the tab, so you see at a glance whether a card changed anything and how much. The address is `#/task/GA-91/changes`, so Back and links work.

- Opening Changes hides Properties (`]` still toggles it).
- Left, a **file list** (260 px, it can fold): each file under its folder with a status letter (A, M, D, R; with the word in its tooltip), +/− counts, and a dot for uncommitted changes. A filter box on top, "Since origin/main +964 −112 · 20 files" above it.
- Right, **all files below each other**, unified (one column), in the list's order; a click in the list scrolls there. Each file has a sticky header: path (old → new for a rename), +/−, a fold caret, and Open in editor while the worktree is there.
- **Unchanged code folds** to 3 lines around each change; the fold bar "⋯ 37 unchanged lines" has arrows to show 20 more lines above or below, and a click shows them all.
- **Big and odd files:** a binary file says "Binary file changed (12 KB → 14 KB)"; a file over 1 MB, or a diff over 3,000 lines, comes folded with Show; lock files (`package-lock.json`, `composer.lock`, `Cargo.lock`) and built files come folded; a rename without changes is one line; a deleted file is red and folded when long.
- Split side by side is left out of the first version: it needs about 1,400 px of room. [I]
- The Runs tab gets **Show changes** on each run, and the Pull request panel shows "+964 −112 · Show changes".

Screen by screen, with sketches: §5.1.

**Q3. Which component: CodeMirror's merge view, `@pierre/diffs` or `@git-diff-view/react`?**

Answer: compared in §4.2 on size, licence, upkeep, colours for PHP, Blade, TSX and Rust, Gizai's light and dark themes, speed on a 2,000-line diff and fit with the app's content security policy.

Recommendation: **`@codemirror/merge`**, its `unifiedMergeView`, read-only, with `collapseUnchanged`. [I]

- One highlighting system and one theme for the whole app: the same languages (and the same Blade work) and the same tokens as the Editor and the Markdown editor.
- MIT, small (183 KB unpacked), by CodeMirror's own maintainer, 37 releases in the 6.x line, the latest on 2026-06-09. [V `npm view`]
- No WebAssembly and no workers, so the content security policy stays as it is. [V CSP; I for what merge needs]
- Its costs: deleted lines show no old line number in the unified view [I]; every file needs both texts, which the backend sends; and every file is its own editor, so files mount only when they scroll near the screen.
- Speed: CodeMirror draws only the lines in sight [I], so a 2,000-line diff costs one diff computation and one screen. Not measured in this run; card F1 measures it in WebKitGTK and has a fallback if it is slow (§6).

**Q4. Comments on a line that land on the card: later or not?**

Answer: later, as its own small card after the Changes view (not planned in §6). It needs no database change: a comment on a line posts an ordinary card comment that starts with the place (`src/lib/pulls.ts:42 at 3f5c1a2`) and quotes the lines. QA and the next run read it, because every run gets the card's 10 latest comments (`runs.rs:882-887`). [V] The Changes view can later find these comments and show them at their line. [I]

Recommendation: later; keep the Changes view's line positions stable (path, side, line, commit) so it can be added without rework.

#### The Editor

**Q5. What may it open, and what does it hide or show read-only?**

Answer and recommendation: [I, with the refusals V]

- **May open:**
  - the projects' linked folders (`projects.repo_path`, `model.rs:87`);
  - any folder directly under the roots set in Settings → General → Editor folders (default `~/Herd`), so `~/Herd/kade-portal` opens without being a project first;
  - a card's worktree (`<data>/worktrees/<KEY>/<identifier>`), from the card.
- **Never:** `/`, the home folder itself, the folders that hold keys or logins and Gizai's data folder apart from card worktrees, with the refusals agent folders already use (`crates/gizai-core/src/folders.rs:46-56`, `:189`). Not the Team Lead's copies either (`<data>/code/`): Gizai throws away anything changed in them (`copies.rs:37-43`). A link that leads out of the opened folder is refused.
- **Card worktrees** are **read-only while an agent works on the card**, and editable when the card is yours (a person's card, Q8). On any other card's worktree the Editor says so and offers "Edit anyway" (your changes become part of the card, uncommitted, for its next run).
- **Hidden:** `.git/` (never shown or written).
- **Shown greyed, folded and read-only until you open them:** what git ignores, such as `node_modules/`, `vendor/`, `target/`, `dist/` and `storage/`; left out of search.
- **Big and binary files:** over 1 MB opens read-only, over 5 MB doesn't open ("Too big to show here: Open in VS Code"), a binary file (a NUL byte in its first 8 KB, git's own test) doesn't open. A file that isn't UTF-8 opens read-only.
- **`.env`** opens and saves like any file: it stays on this computer; it is ignored by git, so search leaves it out unless you ask.

**Q6. What does a first version need, and what waits?**

Answer and recommendation: [I]

| First version | How |
|---|---|
| File tree | One folder level per request, in the backend; folders open on click; git status letters and colours per file and on folders that hold changes |
| Tabs | Several files open, a dot for unsaved changes, middle click and Ctrl+W close, the open tabs kept per folder on this computer |
| Syntax colours | PHP, Blade, TypeScript/TSX, JavaScript, Rust, CSS, JSON, YAML, Markdown, SQL, HTML and `.env`, loaded on first use (§5.3) |
| Find and replace in a file | `@codemirror/search` (Ctrl+F, Ctrl+H, F3) |
| Search in the folder | Ctrl+Shift+F: `git grep` over tracked and untracked files that git doesn't ignore; at most 2,000 hits; a click opens the file at its line |
| Go to file | Ctrl+P over `git ls-files --cached --others --exclude-standard` |
| Save | Ctrl+S, as the Markdown editor (`MarkdownEditor.tsx:447`); keeps the file's line endings; refuses when the file changed on disk since you opened it, with Reload and Keep mine |
| Git status | per file in the tree and on tabs; the branch and behind, ahead and uncommitted in the status bar (Q7) |
| Outside changes | an agent, a terminal or another editor changed a file: an untouched tab reloads, a tab with your unsaved changes shows "Changed on disk: Reload · Keep mine"; the tree and git status follow |
| New file, rename, delete | in the tree's menu; delete asks first |
| Open in VS Code, Zed or PhpStorm | Q9 |

Waits for later: language servers (`@codemirror/lsp-client` exists, MIT, 6.3.0 [V `npm view`], but each server needs installing and running per language); a terminal; formatting (Prettier, Pint, rustfmt); git blame, staging and merge conflicts; image previews; settings for tab size and wrapping beyond the file's own `.editorconfig`.

**Q7. Behind main: how often to fetch, what to show, and an Update button.**

Answer and recommendation: [V for the reused functions; I for the timings]

- **Fetch** when a folder opens in the Editor, every 10 minutes while the Editor shows it, and on Fetch now; with the same code as a card start (`git::start_point` with a 60-second limit, `src-tauri/src/git.rs:57-72`, into the same ref, so card starts and the Editor agree). A folder that isn't a project's is fetched from its `origin` remote's default branch. A fetch never asks for a password (`worktree.rs:309-325`, `:624-643`).
- **Show** in the status bar, and in short on the folder's row in the sidebar: the branch; **behind** (commits main has that the folder doesn't), **ahead** (the folder's own commits), **uncommitted** files; and when the last fetch was. Words with the numbers ("3 behind main", "1 ahead", "2 changed"), arrows only as a short form in the sidebar.
- **Update** is shown when the folder is behind, and works only by `update_checkout`'s rules, through the same functions (`checkout::plan`, then `checkout::update`, `crates/gizai-agents/src/checkout.rs:144-258`): a fast-forward only; never over uncommitted changes in tracked files, nor during a merge or rebase; never a merge commit, reset or force; on the default branch only (no switching from the Editor); then `composer install` or `npm ci` when a lock file changed; never the setup command; and never at the same time as a card's worktree is prepared from that folder (`code.rs:346-351`). When the plan refuses, the button stays and says why (the plan's own sentence), so nothing happens by surprise.

**Q8. A person's card: Open in editor, then Commit, Push and Hand over.**

Answer: [V for today's rules; I for the design]

- Today a card can be assigned to a person (Properties → Assignee), but in an Auto column the agents on it **still take it**: "a person as assignee doesn't block" (`workflow.rs:161-163`, the query at `:184-189`). That is on purpose, so a card dragged back from Review is picked up again (docs/HANDOFF-2026-10-07.md:113). A card **pinned** to someone else, though, is left alone by every agent (`workflow.rs:185-188`). [V]
- **Open in editor** shows on a card assigned to you that no agent is working on. It:
  1. makes or takes over the card's worktree and prepares it exactly as for an agent (`runs.rs:693-717`, `:722-733`);
  2. pins the card to you (`tasks.pinned_actor_id`), so no agent takes it, and makes you its implementer;
  3. moves it from To do to In progress, as you;
  4. opens the worktree in the Editor, with a card bar on top.
- The card bar has:
  - **Commit…** (a message box, "GA-123: <title>" filled in; commits every change, as git sees it);
  - **Push** (the same push as after a run: `push_new_commits` to the project's remote, the same way, `worktree.rs:392-417`, `pulls.rs:146-160`);
  - **Hand over to Testing** (to Review when the card's Testing switch is off). It refuses uncommitted changes, pushes, then moves the card on as a builder's `ready_for_testing` does (`workflow.rs:433-447`), takes the pin off so QA takes it, and posts "Handed over by Jeffrey: 2 commits".
- **QA fails it:** `qa_fail` already gives the card back to its implementer (`workflow.rs:469`), but with the pin gone In progress's agents could take it. New rule: when the implementer is a person, `qa_fail` pins the card to them again.
- **On the board:** the card shows its person's avatar as now, plus a small "By hand" mark (Lucide `keyboard`) while it is pinned to a person; it is never shown as waiting for an agent. **No database change:** `pinned_actor_id`, `implementer_actor_id` and the activity log exist (`workflow.rs:71-80`). [V]
- The Changes view works on such a card as on any other: it shows your uncommitted changes too.

Recommendation: build it last (cards B4 and F4), on top of the Editor; it needs the Editor's folders, saving and git status.

**Q9. An alternative: Open in VS Code, Zed or PhpStorm.**

Answer: cheap and worth having next to a small built-in editor, not instead of it. [I]

- Gizai already has `tauri-plugin-opener` (`commands.rs:318-328`), and finds programs on PATH with `os::find_in` (`crates/gizai-agents/src/os.rs:67`), which handles Windows' `.cmd` shims; Gizai starts programs only through `os::command` (CLAUDE.md:49). [V]
- The editors' own command line tools take a folder: `code <folder>`, `zed <folder>`, `phpstorm <folder>`; on macOS also `open -a "<App>" <folder>`. [I]
- Show only the ones found, in a small **Open in** menu on the Editor's top bar, on a folder's row in the sidebar, and on a card with a worktree.
- The built-in editor is for a quick look and a small fix without leaving Gizai, and for reading an agent's worktree; a big piece of work is better in the developer's own editor, which Gizai then still follows (Changes, behind main, Hand over).

Recommendation: yes, in the first Editor cards (B2 and F2).

**Q10. Free on the website: what changes in the paid plans?**

Answer: nothing. `docs/PAID-FEATURES.md` keeps the whole desktop app free and sells only hosted services: encrypted backup, sync, team workspaces and a runner relay (`PAID-FEATURES.md:7-12`, `:30-38`). The editor and a card's changes run entirely on the user's computer, so they belong in the free app, like the rest. [V]

Recommendation: add "the code editor and a card's changes" to the list under "What stays free, forever" (`PAID-FEATURES.md:30-36`) and to "The desktop app, agents, chat" row of the plans table (`:45`), in the card that ships the Editor. For later, not now: in Team, line comments (Q4) and a person's cards would sync between teammates with the rest of the card. [I]

---
