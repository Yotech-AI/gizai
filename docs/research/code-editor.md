# Gizai: a card's changes, and an Editor for local folders (research, 2026-10-11)

Status: research only, no code. Written by the R&D Agent for GA-91 on 2026-10-11, in two passes. The first pass read `main` at `f3541f0`. The second pass, later the same day with web search and page fetches allowed, checked the outside facts in primary sources and re-checked every line reference against `main` at `9c32546` (Gizai 0.7.1, database schema 16).
Legend: **[V]** = verified today, in the code (file and line), with the command named (`git`, `gh`, `npm view`, `cargo tree`, `which`), or in the primary source named (its link and the date read are in §8). **[I]** = my judgement, or a fact I could not check in a primary source (the reason is given).
Limits: the run may not start node or a browser, so nothing was measured in a browser. No source gives the speed of any of the diff components on a 2,000-line diff (§4.2), so card F1 measures it before the Changes view ships. Bitbucket's API reference was too long to fetch, and Zed's documentation doesn't describe the Changes panel the card describes. What rests on those stays [I].

---

## 1. Summary

1. Both parts fit how Gizai is built: git runs as a command, every card has its own worktree, CodeMirror 6 is already in the app, and the screens reach Rust only through `src/api.ts`. [V]
2. **A card's changes can always come from local git**, without GitHub's or Bitbucket's API. While the worktree exists, it is compared (uncommitted files included) against its merge base with main. When only the branch is left, the branch is. After a merge, Gizai uses the range it saved at the merge in two hidden refs, which works for a merge commit, a squash and a rebase alike. For cards merged before that, it uses the merge commit against its first parent, which for PR #81 gives exactly GitHub's diff (34 files, +1923 −112). `gh pr diff` is only the last resort: it needs the network, and gives a patch without the files around it. [V `git`, `gh`, GitHub's docs]
3. **Card diffs here are big.** Of the 71 merged card pull requests (11 Oct), the median changes 951 lines in 19 files, a quarter change more than 2,108 lines (32 files), and the largest 15,131 lines in 245 files (#52). The view must list files first, load each diff as it scrolls into sight, and fold unchanged code and lock files. [V `gh pr list`]
4. **Recommended: CodeMirror 6 for both** (`@codemirror/merge` for the diff; language packages and `@codemirror/search` for the editor), coloured with new syntax and diff tokens in the design system, with one Blade language for both screens. The merge view's own diff gives up on precision in big files, so the backend sends git's hunks and the view uses them (`DiffConfig.override`, since 6.12.0). Not `@pierre/diffs` or `@git-diff-view`: they bring a second colouring system (Shiki or highlight.js) next to CodeMirror's, and Pierre's package is 7.5 MB and releases about ten times a month. Not Monaco: far heavier, no Blade, and a look of its own. [V packages, docs; I judgement]
5. **Changes:** a **Changes** tab at the top of the card, next to the card itself (Card | Changes +964 −112), full width: the file list on the left, every file's diff below each other on the right, removed lines red and added lines green, unchanged code folded. Each run in the Runs tab gets "Show changes" too. [I]
6. **Editor:** a folding **Editor** section in the sidebar above Agents, listing the folders you opened, and an Editor page: file tree, tabs, syntax colours, find and replace, search in the folder, Ctrl+S, each file's git status, outside changes, and a bar with behind, ahead and uncommitted, **Fetch** and **Update** (a fast-forward only, with `update_checkout`'s rules). [I]
7. **Blade is the one real gap:** no CodeMirror package for it exists, so we write a small Blade language on top of `@codemirror/lang-php`. [V registry; I effort]
8. **A person's card:** Open in editor makes the card's worktree as for an agent and pins the card to you, so agents leave it alone; Commit, Push and Hand over then move it on as a builder's `ready_for_testing` does. One rule in `qa_fail` changes. [V `workflow.rs:185`; I]
9. **Also add Open in VS Code, Zed or PhpStorm:** cheap, and better for big edits; the built-in editor stays basic. Zed's command is `zeditor` on Arch-based systems such as this computer, so Gizai looks for each editor under all its names. **Free:** nothing in `docs/PAID-FEATURES.md` changes; add the editor to "What stays free". [V `which`, the editors' docs; I]
10. **Eleven cards** (§6): two Design cards and nine build cards. Four can start at once (both designs, both first backends), and a card's changes can ship before the Editor.

### Changed in the second pass

1. **Merged cards (Q1, B1, C1, question 7).** GitHub's docs say that after a rebase merge the "merge commit" is only the last rebased commit, so "the merge commit against its first parent" would quietly show one commit of the card. At the merge, B1 now saves the card's range in two hidden refs, `refs/gizai/merged/<identifier>/base` and `/head`. That works for every merge method, offline, and with no database change: no migration 0017, and no MINOR release just for B1.
2. **The diff itself (Q3, §4, F1, C1).** The merge view computes its diff on the main thread. For big files it falls back to a rough diff: CodeMirror's maintainer confirms this for two 1,500-line files with scattered changes. So `card_file_diff` now also returns git's hunks, and F1 hands them to the view through `DiffConfig.override` (since 6.12.0). The view, the file list's +/− counts and GitHub then agree. `FileDiff` gains `hunks`.
3. **Folds and keys (Q2, §5.1, F1).** `collapseUnchanged` only opens a whole folded stretch at once, the package binds no keys, and `mergeControls` (accept and reject buttons) is on by default. F1 turns the buttons off, draws its own fold bar (↑ 20, ↓ 20, All) and binds the keys for `goToNextChunk` and `goToPreviousChunk`. Removed lines get no line numbers in the unified view. F1 adds their old numbers through the gutter's `widgetMarker`.
4. **`@pierre/diffs` and the security policy (§4).** Pierre uses Shiki's JavaScript regex engine by default (WebAssembly only on request) and runs workers the app creates itself. So the content security policy is no reason against it, as run 1 said. What is left against it: a second colouring system (Shiki themes, drawn in Shadow DOM), its size and its pace of releases. The recommendation stands.
5. **Hosts' diff limits (Q1).** Checked. GitHub's web view stops at 20,000 lines, 1 MB or 300 files a diff, and its files API lists at most 3,000 files. Bitbucket's view stops at 2,000 changed lines a file, 8,000 in all and 200 files. `gh pr diff` isn't held to GitHub's web limits: it returned a 27,708-line diff (1.58 MB) here in full. It does neutralise escape sequences unless told not to. Local git has none of these limits.
6. **Open in (Q9, B2, C2).** Zed's Linux packages may name it `zeditor` or `zedit`, and this computer has only `/usr/bin/zeditor`. PhpStorm is `phpstorm.sh`, `phpstorm` (Toolbox's script) or `phpstorm64.exe`, and it makes a folder that isn't a project yet into one by adding `.idea/`. B2 finds every name, and keeps `.idea/` out of a card's changes.
7. **`notify` (§5.3).** Checked: 8.2.0 is the stable release (CC0-1.0) and 9.0 is in release candidates; it uses inotify on Linux, FSEvents on macOS and ReadDirectoryChangesW on Windows; its docs warn about Linux's watch limits. Polling first stays.
8. **Zed (Q2).** Its docs show a branch diff against a base you pick and, since 1.15.0 (12 Aug 2026), a `git.diff_base` setting that compares against the merge base with the default branch: the base Q1 recommends. The Changes tab exactly as the card describes it isn't in Zed's docs, so the layout rests on the card.
9. **Line references.** Main got 7 commits since run 1 (GA-92, an agent's own push). The references into `src-tauri/src/pulls.rs`, `src-tauri/src/runs.rs` and `crates/gizai-agents/src/worktree.rs` moved and are corrected; the rest still hold.
10. **Everything else run 1 left as [I]** for the CodeMirror packages, the search keys, `@codemirror/lsp-client`, `@git-diff-view`, Monaco and the editors' command lines is now [V], or says why not.

---

## 2. The project now

### 2.1 What Gizai does for its users

Gizai is a local desktop app for one developer and a team of coding agents (README.md:3-5; docs/HANDOFF-2026-10-07.md:5-29). [V]

- **Work:** clients with contacts, projects (each linked to a local git repository, its *linked folder*, and optionally to GitHub, Bitbucket or another git URL), tasks on a board or in a list, with labels, priorities, acceptance criteria, files and docs, and an Inbox for "needs you" (README.md:125-126).
- **The board** is the card flow Backlog → To do → In progress → Testing → Review → Deploy → Done. Each column holds the agents that work its cards, is Auto (its agents take its cards by priority) or Manual (only Run starts one), and links to its next column; you set this up on Team → Workflow (README.md:133-134; `crates/gizai-core/src/columns.rs`, `workflow.rs`).
- **Agents** are coding CLIs (Claude Code, Codex, Gemini, others) with a role, model, effort, allowed commands, folders, tools and a budget. Each run is headless, in the card's own **git worktree** `<data>/worktrees/<KEY>/<identifier>` on the branch `gizai/<identifier>-<slug>` (`crates/gizai-agents/src/worktree.rs:84-120`; `src-tauri/src/runs.rs:693-717`). A new worktree is prepared first: paths copied from the linked folder, `composer install` or `npm ci` for what is missing, a setup command (`runs.rs:722-733`). A run ends with a `GIZAI_RESULT` line that moves the card (`workflow.rs:380-489`), and Gizai pushes the branch after every run (README.md:181).
- **QA and review:** the QA Agent tests cards in Testing; a pass moves them to Review, assigned to the person who reviews (`workflow.rs:345-351`, `:409-411`). Open pull request pushes the branch and opens the pull request with `gh` or Bitbucket's API (`src-tauri/src/pulls.rs:239-283`). The PR check asks every two minutes (`src-tauri/src/lib.rs:495-504`); a merge moves the card to Deploy or Done and removes its worktree, and its branch when the pull request has its latest commit (`pulls.rs:388-437`).
- **Runs:** live output, history with why each run ended, the commits it made (from `base_sha` to `head_sha`, `crates/gizai-core/src/model.rs:383-388`; `runs.rs:1170-1185`), Continue with a note, Run this for me, limits and budgets.
- **The Team Lead** answers in Chat with Gizai's own tools over MCP (`crates/gizai-mcp`, `src-tauri/src/tools/`), checks the board every 15 minutes when switched on, and reads its own read-only copies of the code at `<data>/code/<KEY>` (`src-tauri/src/code.rs:1-15`). When a linked folder's dependencies are behind main it offers to update it, and `update_checkout` does it: a fast-forward, then `composer install` or `npm ci`, never a merge, reset or the setup command (`code.rs:370-399`; `crates/gizai-agents/src/checkout.rs`).
- **Memory** (notes per agent and shared, the Memory page), **Usage** (tokens, API cost, subscription limits), **Settings** in tabs (General, Appearance, Notifications, Agents and runs, MCP servers, GitHub and Bitbucket; `src/router.ts:11`), desktop notifications, a tray icon, and updates from GitHub Releases (README.md:127-163).

### 2.2 How it is built

| Part | What it is | Where |
|---|---|---|
| Desktop shell | Tauri 2.12, crate `gizai` (Rust 2024 edition). A thin adapter: Tauri commands, the app state, background loops and events to the screen. | `src-tauri/`; state `src-tauri/src/lib.rs:31-62`; commands `src-tauri/src/commands.rs`, registered at `lib.rs:550-586`; events (`rows-changed`, `runs-changed`, `run-event`, `chat-event`, …) at `lib.rs:258-279`; plugins dialog and opener at `lib.rs:444-445` |
| Core | Data and rules, never Tauri (CLAUDE.md:36-38): tasks, projects, clients, team, columns, the workflow gates, runs, memory, settings, folder checks. SQLite through rusqlite (bundled). | `crates/gizai-core/src/`; database `db.rs` (schema 16, `db.rs:10`; migrations 0001–0016 listed at `db.rs:22-37`, files in `crates/gizai-core/migrations/`); settings are key–value JSON (`settings.rs:8`, `:18`) |
| Agents | Everything that starts programs: git (run as a command, `worktree.rs:51-59`; no git library), worktrees, preparing them, a linked folder's status and update (`checkout.rs`), the Team Lead's copies (`copies.rs`), `gh` (`github.rs`), Bitbucket's API (`bitbucket.rs`), pushes over SSH or HTTPS (`connection.rs`, `worktree.rs:389-446`), the coding CLIs, process trees (`os.rs`, `process.rs`). | `crates/gizai-agents/src/` |
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
| Runs `src-tauri/src/runs.rs` | `open_worktree` (`:693`), `prepare_worktree` (`:722`), `base_sha` (`:878-880`), `record_head` (`:1158`), `commits` (`:1170`) | per-run diff from `base_sha..head_sha` | the same worktree for a person |
| Pull requests `src-tauri/src/pulls.rs`, `crates/gizai-agents/src/github.rs` | `gh pr list --json number,url,state,isDraft,headRefOid,commits` (`github.rs:52`, `:169-175`); merge seen at `pulls.rs:401-405`, where the branch tip is read (`:402`); clean-up at `:420-437` | save the card's range at the merge, before the clean-up (two refs); `mergeCommit` for it | push for a person's card (`push_to`, `push_over_for`, `:147`, `:155`) |
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
| Merged (Deploy, Done) after B1 ships | the **range saved at the merge**: the branch tip the pull request contained, against its merge base with main just before the merge | `git diff --numstat -z -M refs/gizai/merged/<identifier>/base refs/gizai/merged/<identifier>/head` | the card's branch as its pull request showed it, after a merge commit, a squash or a rebase alike |
| Merged earlier, with a merge commit | the **merge commit against its first parent** | `git diff --numstat -z -M <merge>^1 <merge>` | what the pull request brought into main |
| Merged, and none of that on this disk | the pull request's diff from GitHub or Bitbucket | `gh pr diff <n> --color never --allow-escape-sequences` [V `gh pr diff --help`]; Bitbucket `GET /2.0/repositories/<ws>/<repo>/pullrequests/<id>/diff` [I] | a patch only, without the files' full text |
| One run | the commits it made | `git diff <base_sha> <head_sha>` (`model.rs:383-388`) | that run's commits only |

Notes:

- `<main>` is the ref card starts use: `refs/remotes/<remote>/<branch>` when the repository has a remote for the project's link, `refs/gizai/base/<branch>` when it hasn't, the local default branch without a link (`src-tauri/src/git.rs:65-72`, `worktree.rs:327-335`). On screen it reads **"Since origin/main"** (or "Since main"). [V]
- Against the merge base, not main's tip: otherwise every commit main got since the card started would show as removed lines. [I]
- `.gizai-tmp/` never shows: Gizai keeps it out of git status through `info/exclude` (`worktree.rs:204-247`). [V]
- **After a merge** the worktree and, when the pull request has its latest commit, the branch are gone (`pulls.rs:420-437`). What the merge leaves on main depends on how it was merged. GitHub's docs on `merge_commit_sha`: after a merge commit it is that merge commit, after a squash the squashed commit on the base branch, after a rebase "the commit that the base branch was updated to", which is only the last of the rebased commits. [V GitHub's REST docs] So "the merge commit against its first parent" is right after a merge commit (this repository's way: `f3541f0` has the parents `4d03026` and `e3b1d2c`; and Gizai's own merge, `gh pr merge --merge`, `github.rs:161-163`) and after a squash. After a rebase merge it would quietly show only the card's last commit. [V for the rules; I for the consequence] For PR #81, `git diff f3541f0^1 f3541f0` and `git diff 4d03026...e3b1d2c` both give 34 files, +1923 −112, exactly GitHub's numbers (`gh pr view 81 --json additions,deletions,changedFiles`). [V]
- **So Gizai saves the range at the merge.** The PR check reads the branch tip just before it cleans up (`pulls.rs:402`). B1 adds one step there, before `clean_up`. It fetches main as a card start does, then writes two refs in the linked repository with `git update-ref`: `refs/gizai/merged/<identifier>/head`, pointing at that tip, and `refs/gizai/merged/<identifier>/base`, pointing at `git merge-base <tip> <merge commit>^1`. After a merge commit or a squash, that first parent is main just before the merge. After a rebase it is the previous rebased commit, whose merge base with the tip is still where the card started, because rebased commits are copies and not ancestors of the tip. So one rule fits all three. [I, from how `git merge-base` works] The merge commit comes from `gh` (`mergeCommit` in `gh pr view --json`'s field list [V `gh`]) or from Bitbucket (`merge_commit.hash` on the pull request [I: Atlassian's API reference was too long to fetch]). When offline or without the merge commit, base is `git merge-base <tip> <main>` with main as last fetched, unless that gives the tip itself (main already has the merge); then nothing is saved and the next rows apply.
- **Why refs and not a database column:** a ref keeps the card's commits on this computer after its branch is deleted. After a squash or a rebase nothing else points to them, and `git gc` removes unreachable commits two weeks old (`gc.pruneExpire`, default `2.weeks.ago` [V git docs]). Gizai already keeps a ref of its own, `refs/gizai/base/<branch>` (`worktree.rs:327-335`) [V], and refs need no migration. The cost: tools that show every ref (`git log --all`, some git apps) show these too. [I] (Open question 7.)
- `gh pr diff` is only the last resort. It needs the network and gh's login, works for GitHub only, and gives a patch without the files around it (so no "show more lines"). By default it also neutralises terminal escape sequences in the text, so Gizai must pass `--allow-escape-sequences` to get the files as they are. [V `gh pr diff --help`]
- **The hosts' limits.** GitHub's web view shows at most 20,000 lines or 1 MB of diff, 500 KB a file and 300 files a pull request, and its files API lists at most 3,000 files. Bitbucket's view shows at most 2,000 changed lines or 100 KB a file, 8,000 lines in all and 200 files. [V GitHub's "Repository limits" and REST docs; Atlassian's "Limits for viewing content and diffs"] `gh pr diff` isn't held to GitHub's web limits. Today it returned the largest card's diff (#52: 245 files, 20,732 lines, 1.25 MB) and a release pull request's (#33: 27,708 lines, 1.58 MB) in full. The 300-file limit couldn't be tried: no pull request here has more than 271 files. [V `gh pr diff 52 | wc`, `gh pr diff 33 | wc`] Local git has none of these limits, which is one more reason to prefer it.
- **Per run: yes.** It is cheap (the two ids are saved already, `runs.rs:878-880`, `:1158-1165`) and answers "what did this run do?". It shows commits only; a run's own note already says how many uncommitted changes it left (README.md:181). [V]

Recommendation: one Rust function that picks the source in this order (worktree, branch, the range saved at the merge, the merge commit, GitHub or Bitbucket), returns the file list with counts at once, and each file's texts and git's hunks only when the screen asks for them (§5.3). The PR check saves the range at every merge from B1 on.

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
- **Folders git ignores** (`node_modules/`, `vendor/`, `target/`, `dist/`, `storage/`, …): shown greyed and folded, the files in them read-only, left out of search. An ignored file elsewhere, such as `.env`, opens and saves as usual.
- **Big and binary files:** over 1 MB opens read-only, over 5 MB doesn't open ("Too big to show here: Open in VS Code"), a binary file (a NUL byte in its first 8 KB, git's own test) doesn't open. A file that isn't UTF-8 opens read-only.
- **`.env`** opens and saves like any file: it stays on this computer; it is ignored by git, so search leaves it out unless you ask.

**Q6. What does a first version need, and what waits?**

Answer and recommendation: [I]

| First version | How |
|---|---|
| File tree | One folder level per request, in the backend; folders open on click; git status letters and colours per file and on folders that hold changes |
| Tabs | Several files open, a dot for unsaved changes, middle click and Ctrl+W close, the open tabs kept per folder on this computer |
| Syntax colours | PHP, Blade, TypeScript/TSX, JavaScript, Rust, CSS, JSON, YAML, Markdown, SQL, HTML and `.env`, loaded on first use (§5.3) |
| Find and replace in a file | `@codemirror/search`: Ctrl+F opens its panel with find and replace, F3 finds the next [I for its keys] |
| Search in the folder | Ctrl+Shift+F: `git grep` over tracked and untracked files that git doesn't ignore; at most 2,000 hits; a click opens the file at its line |
| Go to file | Ctrl+P over `git ls-files --cached --others --exclude-standard` |
| Save | Ctrl+S, as the Markdown editor (`MarkdownEditor.tsx:447`); keeps the file's line endings; refuses when the file changed on disk since you opened it, with Reload and Keep mine |
| Git status | per file in the tree and on tabs; the branch and behind, ahead and uncommitted in the status bar (Q7) |
| Outside changes | an agent, a terminal or another editor changed a file: an untouched tab reloads, a tab with your unsaved changes shows "Changed on disk: Reload · Keep mine"; the tree and git status follow |
| New file, rename, delete | in the tree's menu; delete asks first |
| Open in VS Code, Zed or PhpStorm | Q9 |

Waits for later: language servers (`@codemirror/lsp-client` exists, MIT, 6.3.0 [V `npm view`], but each server needs installing and running per language); a terminal; formatting (Prettier, Pint, rustfmt); git blame, staging and merge conflicts; image previews; settings for tab size and wrapping (the first version indents as the file already does).

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

## 4. Options

### 4.1 Three ways to build it

**A. CodeMirror 6 for both (recommended).** The Changes view shows each file with `@codemirror/merge`'s `unifiedMergeView` (read-only, `collapseUnchanged`); the Editor is CodeMirror with language packages and `@codemirror/search`. One code theme, made from new design tokens, colours both and the Markdown editor's code.

- *What it takes:* the packages below; a shared code theme and language map; a small Blade language; the Rust side (changes, files, status) as in §5.3.
- *Libraries* (all MIT, all by CodeMirror's maintainer; [V `npm view`, 2026-10-11]): `@codemirror/merge` 6.12.2 (183 KB unpacked, 37 releases of 6.x, latest 2026-06-09); `@codemirror/search` 6.7.2 (138 KB, 2026-08-31); `@codemirror/lang-php` 6.0.2 (13 KB, plus `@lezer/php` 389 KB; it depends on `lang-html`, which the app has); `@codemirror/lang-rust` 6.0.2 (9 KB, plus `@lezer/rust` 381 KB); `@codemirror/lang-json` 6.0.2 (11 KB); `@codemirror/lang-yaml` 6.1.3 (16 KB); `@codemirror/lang-sql` 6.10.0 (146 KB); `@codemirror/legacy-modes` 6.5.5 for `.env`, TOML and shell (1.96 MB unpacked, but each mode is its own small file). TypeScript, TSX, HTML, CSS and Markdown are installed already. Unpacked sizes count every build and the type files; what reaches the bundle is a fraction, and languages load on first use. [I]
- *Risks:* the Blade language is our own work (about a day, [I]); the unified view gives deleted lines no old line number [I]; one editor per changed file, so files must mount only when they come near the screen (245 files in the largest card); a very large file's diff is cut short by the diff's own limits (`diffConfig`'s `scanLimit` and `timeout`, [I]), which is why such files come folded.
- *Rough size:* 11 cards (§6), most of them medium.

**B. A ready-made diff view for Changes, CodeMirror for the Editor.** Changes uses `@pierre/diffs` (colours with Shiki: TextMate grammars, as VS Code) or `@git-diff-view/react` (highlight.js, or Shiki through an add-on), fed with `git diff`'s patch; for Blade both need Shiki.

- *What it takes:* the component, Shiki and its grammars; a Shiki theme written in Gizai's CSS variables; either a content security policy with `'wasm-unsafe-eval'` for Shiki's default regex engine (WebAssembly), or Shiki's JavaScript engine [I]; workers served from the app itself if the component uses them (the policy allows `'self'`, not `blob:`) [I].
- *Libraries:* `@pierre/diffs` 1.5.2, Apache-2.0, 7.47 MB unpacked in 860 files, depends on `shiki` 3 or 4, `diff` 9 and Pierre's theme packages, 107 versions since 2025-12-10, maintained by Pierre's team (pierre.co) [V `npm view`]; `@git-diff-view/react` 0.1.7, MIT, 1.31 MB unpacked plus `@git-diff-view/core` 1.40 MB, colours with highlight.js through lowlight (BSD-3-Clause) or with Shiki through `@git-diff-view/shiki` 0.1.7, one maintainer, last release 2026-07-13 [V `npm view`, README].
- *Risks:* two colouring systems in one app (Shiki in diffs, CodeMirror in the Editor), so the same file looks different in the two places unless both themes are kept in step by hand; the security policy change; Shiki's grammars are large (`@shikijs/langs` is 8.65 MB unpacked, [V]); `@pierre/diffs` moves fast (about ten versions a month), `@git-diff-view` is pre-1.0 with one maintainer.
- *Rough size:* the Changes view itself is a little smaller than in A (the component draws hunks, folds and both line numbers), but the theme, policy and Blade checks make up for it; the Editor still needs all of A's language work.

**C. Monaco for both.** VS Code's editor: a diff editor with "hide unchanged regions" and a full editor.

- *Libraries:* `monaco-editor` 0.57.0, MIT, 101.7 MB unpacked [V `npm view`]; usually wrapped with `@monaco-editor/react` 4.7.0 [V].
- *Risks:* several MB in the bundle and heavy in WebKitGTK [I]; its own theme engine with fixed colours rather than CSS variables, so light and dark and Settings → Appearance's fonts and sizes need wiring by hand [I]; web workers for its languages and diff [I]; PHP but no Blade [I]; it looks like VS Code, not like Gizai. The CodeMirror code the app ships today (about 1.4 MB of unminified ES modules, `node_modules/@codemirror/*/dist/index.js` and `@lezer/*`, [V `wc -c`]) would stay for Markdown.
- *Rough size:* like A, plus the integration work; no saving where it counts (Blade, theme).

### 4.2 The diff components compared

| | `@codemirror/merge` | `@pierre/diffs` | `@git-diff-view/react` | Monaco's diff editor |
|---|---|---|---|---|
| Version, licence [V] | 6.12.2, MIT | 1.5.2, Apache-2.0 | 0.1.7, MIT (lowlight BSD-3) | 0.57.0, MIT |
| Size, unpacked [V] | 183 KB; uses the CodeMirror the app has | 7.47 MB, plus Shiki (602 KB) and its grammars | 1.31 MB + core 1.40 MB, plus highlight.js or Shiki | 101.7 MB |
| Upkeep [V] | 37 releases of 6.x; latest 2026-06-09; CodeMirror's maintainer | 107 versions in 10 months; latest 2026-10-10; a company team | latest 2026-07-13; one maintainer; pre-1.0 | Microsoft; latest 2026-09-24 |
| Input | both texts of each file (backend sends them) | a patch, or both texts [I] | a patch (`hunks`) and optional texts [V README] | both texts |
| PHP | `@codemirror/lang-php` [V] | Shiki `php` [I] | highlight.js `php` [I] | built in [I] |
| **Blade** | **none: our own small language** [V no package] | Shiki `blade` [V `@shikijs/langs` exports `./blade`] | not in highlight.js [V]; Shiki through `@git-diff-view/shiki` [V] | none [I] |
| TSX | `lang-javascript` with JSX, installed [V] | Shiki `tsx` [I] | highlight.js handles TSX poorly [I]; Shiki ok | built in [I] |
| Rust | `@codemirror/lang-rust` [V] | Shiki `rust` [I] | highlight.js `rust` [I] | built in [I] |
| Gizai's light and dark themes | CSS variables in the theme, as `MarkdownEditor.tsx:229-250` already does: follows the `t` key and Appearance at once [V pattern] | a Shiki theme written with CSS variables [I] | its own `light`/`dark` CSS (`diffViewTheme`), restyled by hand [V README] | `defineTheme` with fixed colours, set again on every theme change [I] |
| Speed, 2,000-line diff | draws only the lines in sight; the diff is computed once per file, bounded by `scanLimit`/`timeout` [I: codemirror.net docs, not re-read] | virtualised, highlighting in a worker pool [I: diffs.com, not re-read] | "High performance with Web Worker support" [V README]; virtual scrolling not stated | draws only the lines in sight; diff in a worker [I] |
| Content security policy as it is | fits [I] | Shiki's WASM engine needs `'wasm-unsafe-eval'`; JS engine fits [I] | lowlight fits; Shiki as left [I] | workers to set up [I] |
| Old and new line numbers | new only in the unified view [I] | both [I] | both [I] | both [I] |
| Folded unchanged code | `collapseUnchanged` [I] | yes [I] | expand buttons per hunk [I] | "hide unchanged regions" [I] |
| Line comments later | a widget on a line (our code) [I] | annotations [I] | `renderWidgetLine`, `onAddWidgetClick` [V README] | view zones [I] |
| Same colours as the Editor | yes | no (Shiki) | no | yes if the Editor is Monaco |

Speed was not measured in this run (no browser or node allowed, web pages refused). Card F1 measures it where it counts, in the app's own WebKitGTK under `cage`: the 75th-percentile card here (2,108 changed lines, §1 item 3) and the largest one (15,131 lines in 245 files) must show their first screen within 300 ms and scroll without stalls. If CodeMirror misses that, F1 falls back to drawing the patch itself with `@tanstack/react-virtual` (in the app already, `package.json:24`) and CodeMirror's highlighter. [I targets]

### 4.3 Recommendation

**Option A.** It keeps one way of showing code across the Changes view, the Editor and the Markdown editor, with the design system's own colours in light and dark; it adds only small MIT packages from one well-kept source; it needs no change to the security policy; and the Blade language it lacks has to be written once, for both screens. B would draw the diff a little sooner but leaves two colour systems and a policy change; C is far heavier and looks like another app. [I]

---

## 5. The design

All screens use the design system as it is: the top bar with breadcrumbs and actions (`.topbar`, `components.css:129-135`), tabs (`.tab`, `:290-294`), rows like the sidebar's (`.nav-item`, 32 px, `:101-109`), the properties panel (`.props`, `:225`), mono text for machine values (`t-code`, `t-id`), sentence case, Lucide icons at 16 px (`design/README.md:6-23`, `:94-98`). New are the colours for code and diffs, the Changes view, and the Editor's tree, tabs and status bar; cards D1 and D2 design them as pages in the repository for Jefsev to review. [V for the existing parts]

### 5.1 The Changes view

**Screen 1: the card, with its Changes tab.** The task page gets two tabs at the top, under the top bar: **Card** (the page as it is now) and **Changes** with its counts (Lucide `file-diff`). The counts load with the card (`card_changes`, file list only); "No changes" when there are none; nothing when the card has no branch yet.

```
┌ Tasks / Gizai / GA-91 ─────────────────────────────── [Copy ID] [Open in ▾] [▸] ┐
│  Card   Changes +964 −112                                                       │
│  ───────                                                                        │
│  ◑ ▮▮▮ GA-91  ● Gizai                                                           │
│  Research for the code editor…                                                  │
│  (the card as now: description, criteria, attachments, run, pull request,       │
│   Comments · Activity · Runs)                                                   │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Screen 2: Changes.** Full width; Properties closes while it is open (`]` brings it back).

```
┌ Tasks / Gizai / GA-91 ─────────────────────────────── [Copy ID] [Open in ▾] [▸] ┐
│  Card   Changes +964 −112                                                       │
│         ─────────────────                                                       │
├─────────────────────────┬───────────────────────────────────────────────────────┤
│ Since origin/main       │ ▾ src/components/Sidebar.tsx         M  +12 −3  [Open]│
│ +964 −112 · 20 files    │  40   const agents = (team.data?.members ?? [])…      │
│ [Filter files…        ] │  41   const notes = useData(() => memoryNotes()…      │
│ ▾ src/components        │ ░ - const lead = leadOf(agents);              (red)   │
│   M Sidebar.tsx  +12 −3 │ ▓42 + const lead = leadOf(agents, editor);    (green) │
│   A EditorSection +240 •│  43   const threads = useData(() => …                 │
│ ▾ crates/gizai-agents   │   ⋯ 37 unchanged lines   [↑ 20] [↓ 20] [All]          │
│   A changes.rs     +180 │  81   return (                                        │
│   D old_diff.rs     −80 │ ▾ crates/gizai-agents/src/changes.rs   A  +180        │
│   R a.rs → b.rs         │   1 + //! A card's changes: …                         │
│ ▸ package-lock.json +412│ ▸ package-lock.json   M +412 −20   Lock file: folded │
└─────────────────────────┴───────────────────────────────────────────────────────┘
  • = uncommitted   A added · M modified · D deleted · R renamed
```

- The header line says where the changes come from when it isn't the worktree: "From the merge commit 3f5c1a2 on main", "From branch gizai/ga-12-… (its worktree was removed)". With uncommitted changes: "+964 −112, 3 files not committed yet".
- The file list scrolls on its own; the diffs scroll on their own; a file's header sticks while its diff is in sight.
- Keyboard: `n` and `p` go to the next and previous file, Alt+↓ and Alt+↑ to the next and previous change in a file (CodeMirror merge's chunk commands [I]), Escape returns to Card.
- A wide window (more than 1,400 px for the diffs) could later offer side by side; not in the first version.

**Screen 3: one run's changes.** The Runs tab's row gets **Show changes**; it opens the same view, limited to that run: "Run by Backend Agent, 10 Oct 14:02: 2 commits, +40 −3" with **All changes** to go back.

**States.** No branch yet: "No changes yet: the first run makes this card's branch." No changes: "This card's branch has no changes against origin/main." Merged without a known merge commit and no gh: "Gizai can't find this card's merge commit on main: fetch main, or log gh in (Settings → GitHub)." A failure says what went wrong and how to fix it, in the error banner (`design/README.md:15-17`).

**Colours.** New tokens in `design/tokens.json`, for dark and light, turned into CSS by `scripts/tokens-css.ts` [I values for D1 to set]:

| Token | Use |
|---|---|
| `--diff-add`, `--diff-del` | the + and − marks and line numbers of changed lines |
| `--diff-add-soft`, `--diff-del-soft` | the background of an added or removed line |
| `--diff-add-strong`, `--diff-del-strong` | the characters that changed inside a line |
| `--diff-fold` | the "⋯ 37 unchanged lines" bar |

Rules for D1: green and red of their own, near `--success` and `--danger` but separate tokens, because those mean run and form outcomes (`design/README.md:39`); never teal (`--live`) or magenta (`--needs`) (`:36-38`); every changed line also has its + or − mark and status letters have words, so nothing relies on colour alone (`:40-45`).

### 5.2 The Editor

**Screen 1: the sidebar.** A folding **Editor** section between Projects and Agents, folded and opened like Agents and Memory (GA-90: `FoldSection`, `Sidebar.tsx:114-128`; `"editor"` added to `Fold`, `src/lib/sidebar.ts:6`). Its rows are the folders you opened, newest first (at most 8); each with Lucide `folder-code`, the folder's name, and on the right how it stands: "3 behind" in `text-3`, "2 changed", or a card's identifier for a worktree. `+` on the heading opens the folder picker. Folded, the heading shows how many folders are behind.

```
│ PROJECTS                         + │
│  ● Gizai                           │
│  ● Kade portal                     │
│ EDITOR                         + ⌄ │
│  ▣ gizai               3 behind    │
│  ▣ kade-portal         2 changed   │
│  ▣ GA-123 (by hand)                │
│ AGENTS                           ⌄ │
│  ◆ Team Lead                       │
```

(▣ and ◆ stand for the Lucide icons `folder-code` and `crown`.)

**Screen 2: opening a folder.** A popover (like the item picker) with three groups: **Projects** (their linked folders), **In ~/Herd** (the folders under the roots that aren't projects yet), **Cards** (worktrees of cards that are yours or open). A search box filters all three. Settings → General → **Editor folders** sets the roots (default `~/Herd`); a folder outside them opens only through a project.

**Screen 3: the Editor page** (`#/editor/<folder>`). The top bar holds the breadcrumbs and how the folder stands; below it, the file tree on the left (260 px, can fold), the tabs and the editor on the right, a status bar at the bottom.

```
┌ Editor / gizai ──────── main · 3 behind · 1 ahead · 2 changed [Fetch] [Update] [Open in ▾] ┐
├────────────────────────┬────────────────────────────────────────────────────────────────────┤
│ [Go to file… Ctrl+P  ] │ Sidebar.tsx ● ×  worktree.rs ×  code-editor.md ×                   │
│ ▾ src                  │   1 // The sidebar (design system: Sidebar): quick actions, …       │
│   ▾ components       M │   2 // the company pages stay at the bottom, with the update …     │
│       Sidebar.tsx    M │   3 // data than the usual folder says so there. Agents and …      │
│       Tabs.tsx         │   4 import {                                                       │
│   ▸ lib                │   …                                                                │
│ ▸ crates               │                                                                    │
│ ▸ node_modules (ignored, read-only)                                                          │
├────────────────────────┴────────────────────────────────────────────────────────────────────┤
│ src/components/Sidebar.tsx · TypeScript JSX · LF · Ln 42, Col 7 · Not saved (Ctrl+S)         │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

- The tree's rows are the sidebar's rows at 28 px, the git status letter on the right (M, A, D, U for untracked) in its colour and with a tooltip; ignored folders greyed; a right click has New file, New folder, Rename, Delete (asks first), Copy path, Open in.
- Tabs: the file name (and its folder when two have the same name), a dot while unsaved, × or middle click closes; asks before closing an unsaved tab.
- The editor: `t-code` in the app's mono font (Settings → Appearance changes it, as everywhere), line numbers, the active line, matching brackets, folding, the search panel (Ctrl+F, with replace in the same panel).
- The status bar: path, language, line endings, the cursor, Saved or Not saved.

**Screen 4: search in the folder.** Ctrl+Shift+F swaps the tree for a search panel: the query, Match case, Words, Regular expression, the results grouped by file with the line and the match highlighted; a click opens the file at that line. "2,000 results shown; narrow the search for more."

**Screen 5: behind main.** When the folder is behind, a bar under the top bar: "3 commits behind origin/main, fetched 2 minutes ago. [Update] [Fetch]". When the plan refuses: "Update can't run: kade-portal has uncommitted changes in app/Models/Invoice.php. Commit or stash them first." (the plan's own sentence, `checkout.rs:161-170`). During the update the bar says what it does, and at the end what it did ("Moved main 3 commits to 319bf0b and ran npm ci").

**Screen 6: a person's card.** A card bar on top of the Editor, in place of the behind bar:

```
│ ◑ GA-123  Fix the invoice total · gizai/ga-123-fix-the-invoice-total · 2 files changed, 1 commit not pushed │
│                                             [Commit…] [Push] [Hand over to Testing ▸]                  │
```

On the card itself, **Open in editor** (Lucide `square-code`) sits in the top bar's actions for a card assigned to you; the board card shows Lucide `keyboard` with "By hand" while the card is pinned to a person.

**Screen 7: changed on disk.** A bar over the editor: "src/Invoice.php changed on disk. [Reload] [Keep mine]". Keep mine saves over it only after a second Ctrl+S.

**Colours for code.** New tokens for syntax, for dark and light, in `design/tokens.json` [I names; D1 sets the values]: `--syn-keyword`, `--syn-string`, `--syn-number`, `--syn-comment`, `--syn-function`, `--syn-type`, `--syn-variable`, `--syn-property`, `--syn-tag`, `--syn-attribute`, `--syn-operator`, `--syn-punctuation`, `--syn-meta` (`<?php`, Blade directives, attributes), `--syn-invalid`. One `HighlightStyle` maps Lezer's tags to them (`src/lib/codeTheme.ts`), as `MarkdownEditor.tsx:30-41` does for Markdown with the existing tokens. Rules: at least 4.5:1 on `--bg` and `--raised` in both themes; no teal or magenta hues (they mean an agent at work and "needs you"); taken from the decorative `c-*` family where possible (`tokens.css:39-46`).

### 5.3 Data, commands and the files that change

No new Tauri plugin and no change to the security policy or the window's permissions: every file and git access is a Rust command behind `src/api.ts`, like the rest of the app (CLAUDE.md:35-38). The commands, names and types are in §6 with the cards that make them, word for word.

| Where | What changes | Card |
|---|---|---|
| `crates/gizai-agents/src/changes.rs` (new) | a card's changes from git: the worktree against its merge base, a branch, a range; each file's texts before and after; parsing `--numstat -z` and `--name-status -z` (renames, binaries) | B1 |
| `crates/gizai-agents/src/github.rs`, `bitbucket.rs` | `mergeCommit` added to `FIELDS` (`github.rs:52`); Bitbucket's `merge_commit` read | B1 |
| `crates/gizai-core/migrations/0017_merge_commit.sql` (new), `db.rs`, `pulls.rs` | `tasks.merge_sha`, saved by `pulls::merged` | B1 |
| `src-tauri/src/changes.rs` (new), `lib.rs` | `card_changes`, `card_file_diff`, `run_changes` | B1 |
| `design/tokens.json`, `src/styles/tokens.css`, `design/README.md` | `diff-*` and `syn-*` tokens and their rules | D1, F0 |
| `src/lib/codeTheme.ts`, `src/lib/codeLanguages.ts`, `src/lib/blade.ts` (new) | the code theme, the extension → language map (loaded on first use), Blade | F0 |
| `src/components/changes/` (new), `src/lib/changes.ts` (new), `src/pages/TaskPage.tsx`, `src/router.ts`, `src/components/RunHistory.tsx`, `src/components/PullPanel.tsx` | the Changes view, the Card \| Changes tabs, `#/task/<id>/changes`, Show changes per run | F1 |
| `crates/gizai-core/src/editor.rs` (new), `settings` | which folders the Editor may open: projects' folders, roots (`editor_roots`, default `~/Herd`), card worktrees; the refusals of `folders.rs` | B2 |
| `crates/gizai-agents/src/editor.rs` (new) | list, read, write (atomic, keeps line endings), new, rename, delete, search (`git grep`), files (`git ls-files`), git status, file versions; the path guard; the external editors found | B2 |
| `src-tauri/src/editor.rs` (new), `lib.rs`, `runs.rs` (`Settings`) | the Editor's commands, including the file versions and git status the screen polls; `editorRoots` in Settings | B2 |
| `src/pages/EditorPage.tsx`, `src/components/editor/` (new), `src/lib/editor.ts` (new), `src/components/Sidebar.tsx`, `src/lib/sidebar.ts`, `src/pages/SettingsPage.tsx` | the Editor page, its tree, tabs, search and status bar; the sidebar section; Settings → General → Editor folders; Open in | F2 |
| `src-tauri/src/editor.rs`, `code.rs` | Fetch on open and every 10 minutes, Update with `checkout::plan` and `update`, under `folder_lock` | B3 |
| `src/components/editor/` | the behind bar, Update, the sidebar's "3 behind" | F3 |
| `crates/gizai-core/src/workflow.rs`, `tasks.rs`; `src-tauri/src/runs.rs`, `editor.rs` | a person's card: start by hand (worktree, pin, implementer, move), commit, push, hand over; `qa_fail` pins a person's card again | B4 |
| `src/pages/TaskPage.tsx`, `src/components/Board.tsx`, `src/components/editor/CardBar.tsx` | Open in editor, the card bar, "By hand" on the board | F4 |
| `crates/gizai-core/examples/prep_changes.rs` (new), `src/selftest.ts`, `scripts/ui-test.sh` | demo data with a repository, a card worktree and a merged card; UI probes | F1, F2 |
| `docs/PAID-FEATURES.md`, `README.md` | the editor stays free; the features list | F2 |

How outside changes reach the screen [I]: no file watcher in the first version. While the Editor shows a folder, the screen asks every 3 seconds for the versions (size and modification time) of the open files, and for the folder's git status every 10 seconds and when the window gets the focus; the tree reloads the folders that are open when the window gets the focus. That is simple, the same on Linux, macOS and Windows, and can't run into Linux's limit on watched folders in a Laravel project with a full `vendor/`. If it feels slow, a later card adds the `notify` crate (Rust, cross-platform watching) for the open folders only, sent to the screen as a Tauri event (`editor-changed`). [I; the `notify` crate's version and licence couldn't be checked today]

### 5.4 Security and privacy

- **One gate for every path.** The screen sends a folder (one `editor_folders` returned) and a path inside it, never an absolute path. Rust re-checks on every call that the folder is still allowed (§3.2 Q5), makes the path canonical, refuses `..`, any `.git` part and links that lead outside the folder, and refuses writes in read-only places (ignored folders, a live agent's worktree, files over 1 MB). The rules live in the core with tests, as the agent folder rules do (`folders.rs`). [I]
- **Safe writes.** Save writes a temporary file in the same folder and renames it over the file, keeping its permissions; it refuses when the file changed on disk since it was read. Line endings and a byte order mark stay as they were. [I]
- **Nothing leaves the computer.** Search, diffs and saving are local; the only network use is `git fetch` from the project's own remote, as card starts do, and `gh` or Bitbucket for a merge commit Gizai can't find. No file contents go to agents, logs or GitHub. [I]
- **No new rights for the window.** No file system plugin, the same capabilities and content security policy (`capabilities/default.json`, `tauri.conf.json:18`). [V today]
- **Update** stays as careful as `update_checkout`: fast-forward only, git hooks off (`checkout.rs:230`), never the setup command. A person's Commit runs the repository's own git hooks, as a commit in a terminal would. [V for update; I for commit]
- **Agents' work is safe:** a worktree is read-only while its agent runs, and a card pinned to a person is never taken by an agent (`workflow.rs:185-188`). [V rule; I design]

### 5.5 How it is tested

- **Rust** (TDD, as CLAUDE.md:39-45 asks): `crates/gizai-agents/tests/changes_test.rs` on temporary repositories (a worktree with commits, uncommitted and untracked files, a rename, a binary; only a branch; a merge commit; a squash; a run's range); `crates/gizai-agents/tests/editor_test.rs` (the path gate: `..`, links out, `.git`, refused folders; saving keeps CRLF and refuses a changed file; search's cap; git status counts); `crates/gizai-core/tests/` for the folders the Editor may open and for the person's card rules (pin, hand over, `qa_fail` pins again); `src-tauri/tests/` flow tests: `card_changes` in each column, a person's card from start to Testing with a push to a bare repository, Update refused with uncommitted changes and done with a fast-forward.
- **TypeScript** (`npm test`): the language map, Blade's tokens on sample templates, the file list's grouping and labels, the tabs' state, the sidebar's `"editor"` fold.
- **UI** (`scripts/ui-test.sh`, headless in `cage`, on `prep_changes` data): Changes opens, folds and unfolds, a file list click scrolls, and the 2,000-line and 15,000-line diffs meet the speed targets (§4.2); the Editor opens a folder, types, Ctrl+S writes the file on disk, an outside change reloads an untouched tab, search finds a word; Editor folds in the sidebar. Screenshots with `scripts/shot-cage.sh` for the design review.
- **Linux, macOS, Windows:** paths with backslashes and any case on Windows, CRLF files, `code.cmd` found through `os::find_in`; CI runs it all on the three (`.github/workflows/ci.yml`).

### 5.6 What is left out

Language servers, a terminal, formatting, side-by-side diffs, comments on lines (Q4), git blame, staging single lines, resolving merge conflicts, image previews, several folders in one window, Commit and Push for folders that aren't a person's card (use a terminal or your own editor), and a file watcher (polling first, §5.3).

---

## 6. Proposed cards

Eleven cards, each one pull request that fits in one agent run. "Builds on" names the cards that must be merged first. The contracts between a backend card and its frontend card are below the list, word for word.

| # | Card | Starts in | Role | Builds on |
|---|---|---|---|---|
| D1 | Design: the Changes view, and colours for code and diffs | Design | design | – |
| D2 | Design: the Editor | Design | design | D1's token names (can run at the same time) |
| B1 | A card's changes, in the backend | To do | backend | – |
| B2 | The Editor's folders and files, in the backend | To do | backend | – |
| F0 | Code colours and languages, Blade included | To do | frontend | D1 |
| F1 | The Changes view on a card | To do | frontend | D1, B1, F0 |
| F2 | The Editor page and its sidebar section | To do | frontend | D2, B2, F0 |
| B3 | Behind main, Fetch and Update for Editor folders | To do | backend | B2 |
| F3 | Behind main in the Editor | To do | frontend | F2, B3 |
| B4 | A person's card, in the backend | To do | backend | B2 |
| F4 | A person's card in the Editor and on the board | To do | frontend | D2, F2, B4 |

At the same time: **D1, D2, B1 and B2** first (designs and backends need nothing else); then **F0, B3 and B4**; then **F1 and F2**; then **F3 and F4**. The Changes cards (B1, F0, F1) don't wait for any Editor card, so a card's changes can ship first.

### D1. Design: the Changes view, and colours for code and diffs

- **Column, role:** Design, design.
- **Builds:** the design of §5.1 as a page in the repository, for Jefsev's review: the Card | Changes tabs, the Changes view (file list, diffs below each other, folds, big, binary and lock files, one run's changes, every empty and error state), in dark and light; and the values of the new `diff-*` and `syn-*` tokens (§5.1, §5.2) for both themes, with the rules for `design/README.md`.
- **Acceptance criteria:**
  - [ ] Every screen and state of §5.1 in dark and light, with the app's own components and type styles.
  - [ ] `diff-*` and `syn-*` values for dark and light; each text colour at least 4.5:1 on `--bg` and `--raised` (the numbers are listed); no teal or magenta hues.
  - [ ] A PHP, a Blade, a TSX and a Rust sample shown with the syntax colours.
  - [ ] The rules for the new colours written for `design/README.md`.
  - [ ] Jefsev approved it on the card.

### D2. Design: the Editor

- **Column, role:** Design, design.
- **Builds:** the design of §5.2 as a page in the repository: the sidebar's Editor section (open and folded), the folder picker, the Editor page (tree, tabs, editor, status bar), search in the folder, the behind bar and Update with its refusals, the person's card bar and the board's "By hand" mark, the changed-on-disk bar, Settings → General → Editor folders; dark and light.
- **Acceptance criteria:**
  - [ ] Every screen of §5.2 in dark and light, with D1's syntax colours (its token names, when D1 isn't approved yet) and the app's components.
  - [ ] The tree's rows, the tabs and the status bar specified (sizes in tokens, states: hover, selected, focus, unsaved, read-only, ignored).
  - [ ] The texts of every bar, button and refusal written out, in sentence case.
  - [ ] Jefsev approved it on the card.

### B1. A card's changes, in the backend

- **Column, role:** To do, backend.
- **Builds:** `crates/gizai-agents/src/changes.rs` (the sources of §3.2 Q1 in that order; numstat and name-status parsing, renames, binaries, untracked files; each file's texts, at most 1 MB a side); `mergeCommit` in `github::FIELDS` and Bitbucket's `merge_commit`; migration `0017_merge_commit.sql` with `tasks.merge_sha`, saved by `pulls::merged`, and looked up once for older merged cards; `src-tauri/src/changes.rs` with the three commands of contract C1, registered in `lib.rs`; `src/api.ts` and `src/types.ts` as in C1.
- **Acceptance criteria:**
  - [ ] Tests on temporary repositories: a worktree with commits, uncommitted and untracked files, a rename and a binary; a card with only its branch; a merge commit; a squash merge; a run's range; a card without a branch.
  - [ ] Against the merge base with main as last fetched: commits main got since the card started don't show.
  - [ ] A merged card whose worktree and branch are gone still shows its changes, from `merge_sha`, offline.
  - [ ] `.gizai-tmp/` never shows; a file over 1 MB comes with `tooLarge` and no text.
  - [ ] `SCHEMA_VERSION` 17 and the upgrade tests updated; `cargo test --workspace`, `npm test` and `npm run build` pass.

### B2. The Editor's folders and files, in the backend

- **Column, role:** To do, backend.
- **Builds:** `crates/gizai-core/src/editor.rs` (the folders the Editor may open, §3.2 Q5, with `folders.rs`'s refusals; `editor_roots`, default `~/Herd`); `crates/gizai-agents/src/editor.rs` (list, read, write, new, rename, delete, files, search with `git grep`, git status, versions, the path gate, the editors found on PATH); `src-tauri/src/editor.rs` with the commands of contract C2; `editorRoots` in `runs::Settings`.
- **Acceptance criteria:**
  - [ ] The path gate refuses `..`, any `.git` part, links that lead out of the folder, and folders the Editor may not open, each with a plain sentence; tested on Linux, and on Windows paths.
  - [ ] Save writes atomically, keeps CRLF and a byte order mark, and refuses with "changed on disk" when the version differs.
  - [ ] A worktree of a card an agent is working on is read-only (`readOnlyWhy` says so); ignored folders and files over 1 MB are read-only; binaries and files over 5 MB come without text.
  - [ ] Search stops at 2,000 hits (`truncated`) and 10 seconds; files lists tracked and untracked files that git doesn't ignore.
  - [ ] `editor_apps` lists only the editors found (`code`, `zed`, `phpstorm`, also `.cmd` shims on Windows); `open_in_app` starts them with `os::command`.
  - [ ] `cargo test --workspace`, `npm test` and `npm run build` pass.

### F0. Code colours and languages, Blade included

- **Column, role:** To do, frontend.
- **Builds:** D1's `diff-*` and `syn-*` tokens in `design/tokens.json` and `src/styles/tokens.css` (with `scripts/tokens-css.ts`) and their rules in `design/README.md`; `src/lib/codeTheme.ts` (a `HighlightStyle` on the `syn-*` tokens and an editor theme in the mono font); `src/lib/codeLanguages.ts` (file name → language, loaded on first use: PHP with HTML, Blade, TS, TSX, JS, JSX, Rust, CSS, SCSS, JSON, YAML, Markdown, SQL, HTML, `.env`, TOML, shell); `src/lib/blade.ts`; the packages of §4.1 option A.
- **Acceptance criteria:**
  - [ ] Blade: `{{ }}`, `{!! !!}` and `@php … @endphp` coloured as PHP, `@if(…)` and other directives as keywords with their arguments as PHP, `{{-- --}}` as comments, the rest as HTML; vitest tests on sample templates.
  - [ ] Each listed file type gets its language (tested); an unknown one shows as plain text.
  - [ ] The colours follow the `t` key and Settings → Appearance at once, in dark and light.
  - [ ] Nothing else in the app looks different; `npm test` and `npm run build` pass.

### F1. The Changes view on a card

- **Column, role:** To do, frontend.
- **Builds:** the Card | Changes tabs on the task page with the counts; the Changes view of §5.1 with `@codemirror/merge` (`unifiedMergeView`, read-only, `collapseUnchanged`), files mounted only near the screen; `#/task/<id>/changes`; Show changes per run in the Runs tab and in the Pull request panel; `crates/gizai-core/examples/prep_changes.rs` and UI probes.
- **Acceptance criteria:**
  - [ ] The view as D1 designed it, in dark and light, for a card in each column (no branch, worktree, branch only, merged).
  - [ ] Unchanged code folds with 3 lines around changes; the fold's arrows show 20 more lines, a click shows all.
  - [ ] Speed, measured by a UI probe in `cage`: a 2,000-line diff and a 15,000-line, 245-file diff show their first screen within 300 ms and scroll without stalls; if not, the fallback of §4.2.
  - [ ] Show changes on a run shows only that run's commits.
  - [ ] `npm test`, `npm run build` and `scripts/ui-test.sh` pass.

### F2. The Editor page and its sidebar section

- **Column, role:** To do, frontend.
- **Builds:** the Editor section in the sidebar above Agents (it folds, `"editor"` in `Fold`), the folder picker, the Editor page of §5.2 (tree, tabs, CodeMirror with F0's languages and `@codemirror/search`, go to file, search in the folder, Ctrl+S, git status, the changed-on-disk bar, the status bar), polling as in §5.3, Open in, Settings → General → Editor folders; "the code editor and a card's changes" in `docs/PAID-FEATURES.md`'s free list and in README.md.
- **Acceptance criteria:**
  - [ ] Open `~/Herd/<folder>` (in the test, a folder under a test root), open a file, type, Ctrl+S: the file on disk has the text, the tab loses its dot, the tree shows M.
  - [ ] A file changed outside shows within 3 seconds: an untouched tab reloads, a tab with unsaved edits offers Reload and Keep mine.
  - [ ] Search in the folder finds a word and opens the file at its line; go to file finds a file.
  - [ ] Read-only files and folders can't be edited and say why.
  - [ ] The Editor section folds like Agents and Memory and remembers it; `npm test`, `npm run build` and `scripts/ui-test.sh` pass.

### B3. Behind main, Fetch and Update for Editor folders

- **Column, role:** To do, backend.
- **Builds:** the commands of contract C3: how a folder stands against main (local), Fetch (`git::start_point` for a project's folder, its `origin` default branch for another; not twice within a minute), Update in the background with `checkout::plan` (never switching branch) and `checkout::update` under `code::folder_lock`, and its `editor-updated` event.
- **Acceptance criteria:**
  - [ ] Behind, ahead and uncommitted counted right in tests (a folder behind, ahead, both, with uncommitted changes).
  - [ ] Update refuses with the plan's sentence when tracked files have uncommitted changes, during a merge, off the default branch, or when the branch has its own commits; nothing changes then.
  - [ ] Update fast-forwards and runs `composer install` or `npm ci` only for a lock file that changed; never the setup command; never during a card's preparation from the same folder.
  - [ ] `cargo test --workspace` passes.

### F3. Behind main in the Editor

- **Column, role:** To do, frontend.
- **Builds:** the status in the Editor's top bar, the behind bar with Update and Fetch, "3 behind" on the sidebar row, Fetch on open and every 10 minutes while the Editor shows the folder; the update's progress and end.
- **Acceptance criteria:**
  - [ ] A folder behind main shows "3 commits behind origin/main" with when it was fetched; Update is offered and says what it will do.
  - [ ] A refused Update says why, in the bar, and changes nothing.
  - [ ] After Update the bar and the sidebar show the new state without a reload.
  - [ ] `npm test`, `npm run build` and `scripts/ui-test.sh` pass.

### B4. A person's card, in the backend

- **Column, role:** To do, backend.
- **Builds:** the commands of contract C4: start by hand (the card's worktree made and prepared as for an agent, pinned to you, you its implementer, moved on from To do as you), commit, push (as after a run), hand over (refuses uncommitted changes; pushes; moves the card as a builder's `ready_for_testing`; takes the pin off; a comment); `qa_fail` pins a card whose implementer is a person to them again; `byHand` on `Task`.
- **Acceptance criteria:**
  - [ ] An Auto column's agents never take a card started by hand (tested with the queue).
  - [ ] Hand over moves it to Testing, or to Review with the Testing switch off; QA then takes it; a QA fail brings it back to the person, still not taken by agents.
  - [ ] Push goes where Open pull request pushes, the same way (tested with a bare repository); hand over with uncommitted changes is refused with a plain sentence.
  - [ ] Refused for a card assigned to an agent or to nobody, an archived card, or while an agent works on it.
  - [ ] `cargo test --workspace` passes.

### F4. A person's card in the Editor and on the board

- **Column, role:** To do, frontend.
- **Builds:** Open in editor on a card assigned to you (top bar), the card bar in the Editor (Commit… with its message box, Push, Hand over), "By hand" on the board card, the card's worktree in the sidebar's Editor section.
- **Acceptance criteria:**
  - [ ] Open in editor opens the card's worktree in the Editor, and the card shows In progress, By hand.
  - [ ] Commit, Push and Hand over work from the bar and say what they did; Hand over moves the card to Testing (Review with Testing off).
  - [ ] The card's Changes tab shows the person's uncommitted changes too.
  - [ ] `npm test`, `npm run build` and `scripts/ui-test.sh` pass.

### Contract C1: B1 → F1 (a card's changes)

Rust (`src-tauri/src/changes.rs`, registered in `lib.rs`; types with `#[serde(rename_all = "camelCase")]`, enums in lower case):

```rust
#[tauri::command] pub async fn card_changes(st: State<'_, AppState>, task_id: String) -> Result<CardChanges, String>
#[tauri::command] pub async fn card_file_diff(st: State<'_, AppState>, task_id: String, path: String, old_path: Option<String>,
                                              from: String, to: Option<String>) -> Result<FileDiff, String>
#[tauri::command] pub async fn run_changes(st: State<'_, AppState>, run_id: String) -> Result<CardChanges, String>
```

`src/api.ts`:

```ts
/** A card's changed files with their counts: its worktree (uncommitted changes included) against its merge base with main,
 *  else its branch, else its merge commit on main, else the pull request's diff. Each file's texts come from cardFileDiff. */
export const cardChanges = (taskId: string) => invoke<T.CardChanges>("card_changes", { taskId });
/** One changed file: its text at `from` and at `to` (null: the worktree as it is on disk). */
export const cardFileDiff = (taskId: string, path: string, oldPath: string | null, from: string, to: string | null) =>
  invoke<T.FileDiff>("card_file_diff", { taskId, path, oldPath, from, to });
/** The changes one run committed: from the commit it started at to the one it ended at. */
export const runChanges = (runId: string) => invoke<T.CardChanges>("run_changes", { runId });
```

`src/types.ts`:

```ts
/** Where a card's changes come from (§3.2 Q1). "pull": GitHub's or Bitbucket's diff, without the files' texts. */
export type ChangeSource = "worktree" | "branch" | "merge" | "pull" | "run" | "none";
export type FileStatus = "added" | "modified" | "deleted" | "renamed" | "copied" | "typechange";
export type FileChange = {
  path: string;
  oldPath: string | null;     // renamed or copied from
  status: FileStatus;
  additions: number | null;   // null for a binary file
  deletions: number | null;
  binary: boolean;
  uncommitted: boolean;       // not committed yet (worktree only); an untracked file is "added" and uncommitted
};
export type CardChanges = {
  source: ChangeSource;
  from: string | null;        // the commit the changes are measured from: the merge base, or the merge commit's first parent
  to: string | null;          // the commit they end at; null for the worktree as it is on disk
  baseLabel: string;          // "origin/main", or "main" for a project without a link
  additions: number;
  deletions: number;
  files: FileChange[];        // in path order
  note: string | null;        // why there is nothing to show, or where the changes come from when not the worktree
};
export type FileDiff = {
  path: string;
  oldText: string | null;     // null when added, binary or too large
  newText: string | null;     // null when deleted, binary or too large
  patch: string | null;       // only for source "pull": this file's part of the diff
  binary: boolean;
  tooLarge: boolean;          // over 1 MB on a side
  oldSize: number;
  newSize: number;
  crlf: boolean;
};
```

Errors are plain sentences, like the rest of the app ("GA-12's merge commit isn't on this computer: fetch main, or log gh in (Settings → GitHub)").

### Contract C2: B2 → F2 (the Editor's folders and files)

Rust: `src-tauri/src/editor.rs`, one `#[tauri::command]` per function below, with the same names in snake case and the arguments in snake case (`case_sensitive`, `whole_word`); types with `#[serde(rename_all = "camelCase")]`. `folder` is always an `EditorFolder.id`, `path` always relative to it, with `/`.

`src/api.ts`:

```ts
/** The folders the Editor may open: projects' linked folders, the folders under Settings → Editor folders, card worktrees. */
export const editorFolders = () => invoke<T.EditorFolder[]>("editor_folders");
/** One folder level, folders first; `dir` "" for the top. */
export const editorList = (folder: string, dir: string) => invoke<T.DirEntry[]>("editor_list", { folder, dir });
export const editorRead = (folder: string, path: string) => invoke<T.OpenedFile>("editor_read", { folder, path });
/** Saves; refuses with "changed on disk" when the file's version isn't `version` any more. Resolves with the new version. */
export const editorWrite = (folder: string, path: string, text: string, version: string) =>
  invoke<string>("editor_write", { folder, path, text, version });
export const editorCreate = (folder: string, path: string, dir: boolean) => invoke<void>("editor_create", { folder, path, dir });
export const editorRename = (folder: string, from: string, to: string) => invoke<void>("editor_rename", { folder, from, to });
export const editorDelete = (folder: string, path: string) => invoke<void>("editor_delete", { folder, path });
/** Go to file: tracked and untracked files git doesn't ignore. */
export const editorFiles = (folder: string) => invoke<string[]>("editor_files", { folder });
export const editorSearch = (folder: string, query: string, caseSensitive: boolean, wholeWord: boolean, regex: boolean) =>
  invoke<T.SearchResult>("editor_search", { folder, query, caseSensitive, wholeWord, regex });
/** The branch and each changed file's git status (local, no fetch). */
export const editorGit = (folder: string) => invoke<T.FolderGit>("editor_git", { folder });
/** The current version of each path ("" for a file that is gone), to notice outside changes. */
export const editorVersions = (folder: string, paths: string[]) => invoke<string[]>("editor_versions", { folder, paths });
/** VS Code, Zed and PhpStorm, those found on this computer. */
export const editorApps = () => invoke<T.EditorApp[]>("editor_apps");
/** Opens the folder, or a file in it, in that editor. */
export const openInApp = (app: T.EditorApp["id"], folder: string, path: string | null) =>
  invoke<void>("open_in_app", { app, folder, path });
```

`src/types.ts`:

```ts
export type EditorFolder = {
  id: string;                 // the folder's canonical path
  path: string;               // as shown, ~ for the home folder
  name: string;               // its own name, or the card's identifier
  kind: "project" | "root" | "card";
  projectId: string | null;
  taskId: string | null;
  readOnly: boolean;          // a card an agent is working on now
  readOnlyWhy: string | null;
};
export type GitMark = "modified" | "added" | "deleted" | "renamed" | "untracked" | "conflicted";
export type DirEntry = {
  name: string;
  path: string;
  dir: boolean;
  size: number;
  ignored: boolean;           // git ignores it: greyed; read-only inside an ignored folder
  git: GitMark | null;        // for a folder: whether something inside it changed ("modified")
};
export type OpenedFile = {
  path: string;
  text: string | null;        // null when binary or too large
  binary: boolean;
  tooLarge: boolean;          // over 5 MB
  readOnly: boolean;
  readOnlyWhy: string | null; // "Over 1 MB", "Ignored by git", "Backend Agent is working on GA-12"
  crlf: boolean;
  bom: boolean;
  version: string;            // what editorWrite checks
};
export type SearchHit = { path: string; line: number; column: number; text: string };
export type SearchResult = { hits: SearchHit[]; truncated: boolean };
export type FolderGit = { branch: string | null; files: Record<string, GitMark>; uncommitted: number };
export type EditorApp = { id: "vscode" | "zed" | "phpstorm"; name: string };
// in Settings:
//   editorRoots: string[];   // Settings → General → Editor folders; default ["~/Herd"]
```

### Contract C3: B3 → F3 (behind main)

Rust: `src-tauri/src/editor.rs`, commands `editor_main`, `editor_fetch`, `editor_update`; the event `editor-updated` emitted from the update's end.

`src/api.ts`:

```ts
/** How the folder stands against main as last fetched (local only). */
export const editorMain = (folder: string) => invoke<T.FolderMain>("editor_main", { folder });
/** Fetches main first (at most 60 s; not again within a minute), then as editorMain. */
export const editorFetch = (folder: string) => invoke<T.FolderMain>("editor_fetch", { folder });
/** Starts Update in the background: resolves with what it will do, or rejects with why not (nothing changed). */
export const editorUpdate = (folder: string) => invoke<string>("editor_update", { folder });
export const onEditorUpdated = (cb: (e: T.EditorUpdated) => void): Promise<UnlistenFn> =>
  listen<T.EditorUpdated>("editor-updated", (m) => cb(m.payload));
```

`src/types.ts`:

```ts
export type FolderMain = {
  mainLabel: string;          // "origin/main"
  fetchedAt: number | null;   // ms since 1970; null: never fetched
  fetchError: string | null;
  branch: string | null;
  onDefault: boolean;         // on the default branch
  behind: number;
  ahead: number;
  uncommitted: number;
  canUpdate: boolean;
  updateSays: string;         // what Update will do (the plan's sentence), or why it can't
  updating: boolean;
};
export type EditorUpdated = { folder: string; ok: boolean; said: string };   // "Moved main 3 commits to 319bf0b and ran npm ci."
```

### Contract C4: B4 → F4 (a person's card)

Rust: `src-tauri/src/editor.rs` (or `runs.rs` for the start), commands `start_by_hand`, `hand_card`, `hand_commit`, `hand_push`, `hand_over`; `Task` gains `by_hand` (`byHand`).

`src/api.ts`:

```ts
/** Open in editor, on a card assigned to you: makes and prepares its worktree as for an agent, pins the card to you and moves
 *  it on from To do. Resolves with where it is (its folder is an EditorFolder id). */
export const startByHand = (taskId: string) => invoke<T.HandCard>("start_by_hand", { taskId });
export const handCard = (taskId: string) => invoke<T.HandCard>("hand_card", { taskId });
/** Commits every change in the card's worktree with this message. */
export const handCommit = (taskId: string, message: string) => invoke<T.Commit>("hand_commit", { taskId, message });
/** Pushes the card's branch like Gizai's push after a run; resolves with what it did ("Pushed gizai/ga-123-… (2 commits)."). */
export const handPush = (taskId: string) => invoke<string>("hand_push", { taskId });
/** Refuses uncommitted changes; pushes, moves the card to Testing (Review with its Testing switch off) and takes the pin off.
 *  Resolves with the column's name. */
export const handOver = (taskId: string) => invoke<string>("hand_over", { taskId });
```

`src/types.ts`:

```ts
export type HandCard = { taskId: string; folder: string; branch: string; changed: number; unpushed: number };
// Task gains:
//   byHand: boolean;         // pinned to a person: agents leave it alone
```

---

## 7. Open questions for Jefsev

1. **Where do a card's changes go?** Recommended: a Changes tab at the top of the card, next to Card, full width (§3.2 Q2). The other way is a panel beside the card like Zed's, which leaves each half about 430 px at the usual window width.
2. **Which folders may the Editor open?** Recommended: the projects' linked folders, the folders under `~/Herd` (more roots in Settings), and card worktrees. Should Browse… also open any other folder once, without adding it as a root? Recommended: no, add it as a root instead, so the list stays clear.
3. **Editing an agent's worktree when no agent works on it.** Recommended: read-only, with "Edit anyway" (your changes stay uncommitted in the card for its next run). Or always read-only outside a person's card?
4. **Where does a person's Hand over go?** Recommended: like an agent's work, to Testing for the QA Agent when the card's Testing switch is on, else to Review. Or always straight to Review?
5. **Who is "a person" for Open in editor?** Recommended: only you (the person using this Gizai); a card assigned to another person shows who has it, without Open in editor.
6. **Blade in the first version.** Recommended: our own small Blade language (about a day of F0). The cheaper start colours `.blade.php` as HTML with PHP, leaving `{{ }}` and `@directives` plain.
7. **Remember the merge commit in the database.** Recommended: yes, `tasks.merge_sha` (migration 0017, so the release with B1 is 0.8.0), so merged cards show their changes offline. Or ask GitHub or Bitbucket every time a merged card's changes open?
8. **Which external editors first?** Recommended: VS Code, Zed and PhpStorm; others (Cursor, Sublime Text) when asked.
9. **Outside changes.** Recommended: polling first (open files every 3 seconds, git status every 10), a file watcher later if it feels slow. Or a watcher from the start?
10. **Comments on a line.** Recommended: later, as a small card after F1, with no database change (§3.2 Q4).
11. **Order.** Recommended: a card's changes first (B1, F0, F1: useful at once in Review), then the Editor (B2, F2, B3, F3), then a person's card (B4, F4).

---

## 8. Sources

Read on 2026-10-11 unless noted. Registry and GitHub facts were read with `npm view` and `gh` from this worktree; web pages could not be fetched in this run.

The npm registry (`npm view <package>`; the package pages are `https://www.npmjs.com/package/<package>`):

- `@codemirror/merge` 6.12.2 (licence, size, dependencies, release dates, README) https://www.npmjs.com/package/@codemirror/merge ; repository https://code.haverbeke.berlin/codemirror/merge
- `@codemirror/search` 6.7.2, `@codemirror/lang-php` 6.0.2, `@codemirror/lang-rust` 6.0.2, `@codemirror/lang-json` 6.0.2, `@codemirror/lang-yaml` 6.1.3, `@codemirror/lang-sql` 6.10.0, `@codemirror/legacy-modes` 6.5.5, `@codemirror/language-data` 6.5.2, `@codemirror/lsp-client` 6.3.0 (README) https://www.npmjs.com/package/@codemirror/lsp-client
- `@lezer/php` 1.0.6, `@lezer/rust` 1.0.3, `@lezer/json`, `@lezer/yaml` (sizes)
- `@pierre/diffs` 1.5.2 (licence, size, files, dependencies, exports, maintainers, 107 versions since 2025-12-10; no README, homepage or repository in the registry) https://www.npmjs.com/package/@pierre/diffs
- `@git-diff-view/react` 0.1.7 (README), `@git-diff-view/core` 0.1.7, `@git-diff-view/shiki` 0.1.7 https://www.npmjs.com/package/@git-diff-view/react ; repository https://github.com/MrWangJustToDo/git-diff-view ; demo https://mrwangjusttodo.github.io/git-diff-view (not opened)
- `shiki` 4.5.0, `@shikijs/langs` 4.5.0 (exports `./blade`) https://www.npmjs.com/package/@shikijs/langs
- `highlight.js` 11.12.0 (BSD-3-Clause; no Blade in its exports)
- `monaco-editor` 0.57.0, `@monaco-editor/react` 4.7.0 https://www.npmjs.com/package/monaco-editor
- `codemirror-shiki` 0.3.0; no `codemirror-lang-blade` or `codemirror-blade` package exists (404)

GitHub, through `gh`, for this repository (Yotech-AI/gizai):

- `gh pr list --state merged --limit 100 --json number,headRefName,additions,deletions,changedFiles`: the sizes of the 70 merged card pull requests (§1 item 3)
- `gh pr view 81 --json …`: the field list (`mergeCommit`, `baseRefOid`, `headRefOid`, `files`, …) and PR #81's merge commit and counts
- `gh pr diff 81 --name-only`, `gh pr diff 81`, `gh pr diff --help`

The code, at `f3541f0`: every file and line named in this document; `git diff --shortstat f3541f0^1 f3541f0` and `git diff --shortstat 4d03026...e3b1d2c`; `wc -c node_modules/@codemirror/*/dist/index.js node_modules/@lezer/*/dist/index.js`; `ls node_modules/lucide-react/dist/esm/icons/`.

Not read in this run (web search and page fetches refused): the CodeMirror reference manual for `@codemirror/merge` (`unifiedMergeView`, `collapseUnchanged`, `diffConfig`) https://codemirror.net/docs/ref/#merge ; the `@pierre/diffs` documentation; Zed's documentation of its Changes panel; the `notify` crate https://crates.io/crates/notify ; GitHub's limits on pull request diffs; Bitbucket's pull request diff API https://developer.atlassian.com/cloud/bitbucket/rest/api-group-pullrequests/ ; the editors' command line documentation (VS Code, Zed, PhpStorm). Claims that rest on them are marked [I].

