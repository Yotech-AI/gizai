# Gizai: screenshots of what an agent built, on the card in a Screenshots tab (research, 2026-10-14)

Status: research only, no code. Written by the R&D Agent for GA-173 on 2026-10-14, against `main` at `24535e3` (Gizai 0.11.0).
Legend: **[V]** = verified today, in the code (file and line) or with the command named (`git`, `rg`, `npm view`), or in the primary source named (its link and the date read are in §8). **[I]** = my judgement, or a fact I could not check in a primary source (the reason is given).
Limits: this run may not start node, cargo, php, a server or a browser, so nothing below was measured or tried; what rests on a measurement is marked [I] and given to a card to measure. The other projects (GW, MN, QC, YO, OO) were read in their main checkouts under `~/Herd`, read-only.

---

## 1. Summary

(Written last; see the end of this document's first version for the state of the work.)

---

## 2. The project now

### 2.1 What Gizai does for its users

Gizai is a local desktop app for one developer and a team of coding agents: clients, projects and cards on a board, agents (Claude Code and other CLIs) that work each card headless in its own git worktree, a Team Lead in Chat, Memory, Usage and Settings (README.md; `docs/research/code-editor.md` §2.1 describes it in full). A card moves To do → In progress (a builder: Frontend, Backend, Design) → Testing (QA) → Review (you) → Done; with the card's **Test before Review** switch off it skips Testing (`src/components/Properties.tsx:45`, `:56-57`; `crates/gizai-core/src/workflow.rs:293-310`). [V] **Nothing in Gizai takes, keeps or shows an agent's screenshots today** (`rg -i screenshot` finds only Gizai's own README tooling, §2.3 item 6). [V]

### 2.2 How it is built (the parts this feature touches)

| Part | What it is | Where |
|---|---|---|
| Desktop shell | Tauri 2.12 (`@tauri-apps/api` 2.12.1 in `node_modules`), crate `gizai`; one window, 1440 × 900, at least 900 × 600 (`src-tauri/tauri.conf.json`). Security policy: `img-src 'self' data: blob:` (`tauri.conf.json:18`), so the window can show an image from bytes (a `blob:` URL) but not from a file path. | `src-tauri/` |
| Core | Data and rules, never Tauri; SQLite with migrations `0001`–`0017` (`crates/gizai-core/migrations/`). | `crates/gizai-core/src/` |
| Agents | Everything that starts programs: worktrees, the coding CLIs, a run's process tree, the hidden browser's entry. | `crates/gizai-agents/src/` |
| Runs | A run's start, its events and its end, in the adapter: `src-tauri/src/runs.rs` (the end at `:1096-1160`, `finish_run` at `:1265-1393`). | `src-tauri/src/runs.rs` |
| Screens | React 19, TypeScript, Vite (`vite.config.ts:8`: port 1420, `strictPort`). `src/api.ts` holds every backend call (`invoke`, 366 lines). UI tests mock `../api` per test with `vi.mock` (`src/pages/TaskPageChanges.test.tsx` and others); nothing uses Tauri's `mockIPC` yet, though `@tauri-apps/api/mocks` has it (`node_modules/@tauri-apps/api/mocks.d.ts:129`, events with `shouldMockEvents` `:44-50`). | `src/` |
| Design system | Tokens in `design/tokens.json` → `src/styles/tokens.css`; dark first, light too; Lucide icons, with a fixed map of concepts to icons (`design/README.md:131-154`: attachments `paperclip`; no icon for screenshots yet). | `design/`, `src/styles/` |
| Data on disk | `~/.local/share/gizai` on Linux (`%APPDATA%\Gizai`, `~/Library/Application Support/Gizai`): the database, `files/` (blobs), `runs/` (run logs), `worktrees/`. | |

### 2.3 What Gizai has for screenshots today (the Team Lead's list, checked)

1. **The hidden browser.** Chrome DevTools MCP is a built-in entry in Settings → MCP servers: `BROWSER = "chrome-devtools"`, `BROWSER_VERSION = "1.10.1"` (`crates/gizai-core/src/mcp_servers.rs:20-25`). Its command is always `npx -y chrome-devtools-mcp@<exact version> --headless --isolated --no-usage-statistics --no-performance-crux`, with `--executablePath=…` when Gizai picked the browser and `--acceptInsecureCerts` for local `.test` sites (`crates/gizai-agents/src/browser.rs:39-49`); `safe` refuses a command without `--headless` and `--isolated` or with an option that reaches a real browser or profile (`:94-111`, the list `:30-34`), and Brave is never the browser (`:135-139`). It starts Chrome at the first tool call, in a process group of its own, and ends it when its own process ends (`:6-8`). An agent has it when its form switches it on (`AgentTools::browser_on`, `mcp_servers.rs:93-96`), and only on Claude Code (`mcp_not_on`, `:343-351`: Codex and Gemini agents go without it). Which agents on this team have it (Frontend and QA) is in your data, not the code: taken from the card. [V]
2. **Files on a card.** Blobs are stored once by content hash at `files/<sha[0..2]>/<sha>`, read-only (`crates/gizai-core/src/files.rs:1`, `:15-17`, `:72-105`), at most 1 GB each (`:13`, `:44-56`). The `files` table has `run_id TEXT REFERENCES runs(id) -- set for evidence: QA screenshots, traces, test reports` (`crates/gizai-core/migrations/0001_init.sql:195-205`, the column `:203`), but `files::insert` never sets it (`files.rs:108-124`), and nothing else does (`rg run_id crates/gizai-core/src/files.rs`). There is **no caption column**. A file's type comes from its name's extension only (`mime_for`, `:19-30`). Removing a file is a soft delete; the blob stays (`:162-175`). [V]
3. **The card page** (`src/pages/TaskPage.tsx`): the tabs **Card | Changes** at the top (`:221-227`; Changes at `#/task/<id>/changes`, with an optional run, `:101-103`); the **Attachments** block, "Screenshots, exports, specs" (`:254-257`), which lists the card's files as buttons that open in the system's default app (`src/components/FileDrop.tsx:45`, `open_file` in `src-tauri/src/commands.rs:320`): no picture of a file is shown in Gizai; then the Run panel and the pull request (`:258-259`), and the tabs Comments, Activity, Runs (`:261-296`). [V]
4. **A run's end.** The agent's last line `GIZAI_RESULT: {json}` (`crates/gizai-agents/src/outcome.rs:31-36`). Besides `outcome`, `summary` and `issues` it has **two** optional fields, not one: `run_for_me` (GA-31, in `Outcome`, `:13-17`, read tolerantly by `commands`, `:22-29`) and `learned` (GA-19, read apart from `Outcome` so a result line parses as before, at most 20 lines, `:38-56`). `finish_run` reads them (`src-tauri/src/runs.rs:1274-1279`), `runs::finish` records the run (`crates/gizai-core/src/runs.rs:223`, called at `src-tauri/src/runs.rs:1316`) and `workflow::apply_outcome_with` moves the card (`crates/gizai-core/src/workflow.rs:396`, called at `src-tauri/src/runs.rs:1353`). Runs get no Gizai tools: Gizai's own MCP server (`gizai`) is only in the Team Lead's chat (`mcp_servers.rs:19-20`). [V]
5. **The scratch folder.** `.gizai-tmp/` at the worktree's root, kept out of git by a line in the repository's `info/exclude` (`crates/gizai-agents/src/worktree.rs:202-206`, `:227-254`), is the run's TMPDIR, TMP and TEMP (`:280-283`). It is made empty when a run starts (`prepare_temp`, `:211-225`, called at `src-tauri/src/runs.rs:929`) and **emptied when the run ends, however it ended** (`empty_temp`, `worktree.rs:259-278`, called at `src-tauri/src/runs.rs:1147`), *before* `finish_run` reads the result line (`:1158`). So whatever collects screenshots from the worktree must run before that line. [V]
6. **Gizai's own screenshots.** `scripts/shot-cage.sh <route> <out.png> [data dir]` builds the release app when it is missing or stale (`scripts/app-ready.sh`, `gz_app`: `npm run tauri build -- --no-bundle`), makes demo data (`gz_demo`: `cargo run -p gizai-core --example demo`), starts the real app in a headless Wayland compositor (`cage` with `WLR_BACKENDS=headless`), opens the route (`GIZAI_ROUTE`, steps with `GIZAI_SELFTEST_MODE`) and shoots it with `grim` (`scripts/shot-cage.sh:1-35`). Linux only (cage, grim, wlr-randr, dbus-run-session). [V]

What the agents may run (the default lists, `crates/gizai-core/src/seed.rs:245-290`): every agent starts from `DEFAULT_TOOLS`, which already has `npm`, `npx`, `composer`, `php`, `./vendor/bin/*`, `cargo` and `sleep` (`:245-250`); builders add `node`, `echo`, `printf` and git's push, pull and fetch (`BUILDER_TOOLS`, `:254-257`); QA adds `gh pr create`, `list`, `view` and `edit` (`:260`). A run's prompt names the list and its rules: no `$(…)`, no variables, no loops, nothing outside the worktree and the agent's folders, and a command left running in the background is stopped when the agent ends its message; Gizai then ends the run's whole process group (`crates/gizai-agents/src/prompt.rs:279-281`, `:297-353`, `:372-395`). [V]

One worktree per card, `<data>/worktrees/<project key>/<card>`, shared by all of the card's runs (the builder's and QA's), and a finished card's worktree can be taken over so its build stays warm (`open_worktree_now`, `src-tauri/src/runs.rs:715-739`). So `vendor/`, `node_modules/` and `target/` are usually there already when a run wants to serve a page. [V]

### 2.4 What the feature touches

| Area | Today | For screenshots |
|---|---|---|
| A run's end (`src-tauri/src/runs.rs:1139-1158`) | process group ends → MCP config removed → `empty_temp` → `finish_run` | a **collect** step just before `empty_temp`: images from `.gizai-tmp/screenshots/` onto the card (§3.4) |
| A run's prompt (`crates/gizai-agents/src/prompt.rs`) | "How this run works": commands, temp folder, waiting, push, Run this for me | one more line when the agent takes screenshots: where to save them, the sizes, the captions, and the preview address (§5.4) |
| A run's start (`src-tauri/src/runs.rs`, after the worktree is prepared) | worktree, prepare command, temp folder, MCP config | optionally Gizai starts the project's **preview command** on a free port, and ends it with the run (§3.5) |
| Files (`crates/gizai-core/src/files.rs`, `files` table) | `run_id` never set; no caption | evidence rows with `run_id`, a caption, width and height (migration `0018`) |
| Card page (`src/pages/TaskPage.tsx:221-227`) | Card \| Changes | Card \| Changes \| **Screenshots** with a count (§3.7) |
| Attachments (`TaskPage.tsx:254-257`, `FileDrop.tsx`) | every file of the card | only the files people added (no `run_id`) |
| Board card, Inbox | (§2.5) | a sign on a card in Review with screenshots (§3.7) |
| Settings, project form, agent form | (§3.8) | the switches and values of §3.8 |
| Housekeeping | (§3.6) | old screenshots go (§3.6) |

---

## 3. The feature

### 3.1 What it should do

When an agent builds something with a screen, the card has screenshots of it by the time it reaches Review, on a **Screenshots** tab, so you look at what was built before you read the pull request (the card's Why; Jefsev, 14 Oct 2026). Every part is a switch or a value within a range, with a default (§3.8). Research only: nothing is built before you approve.

### 3.2 Claude Desktop, checked

(Being written: the web part.)

### 3.3 Who takes them, and when

**Recommended: the builder, on every card that changes a screen; QA adds a few of its own, never the builder's set again.** [I]

- **The builder (Frontend, or Design when it builds)** takes the set at the end of its run: after its last change, build and commit, before its result line, so the pictures are of the commit Gizai pushes. It is the one agent that knows which screens changed, it has the page up anyway to check its work, and on a card with testing off it is the only agent before Review. A builder whose card changes no screen (most Backend cards) takes none: the prompt says "when your change shows on a screen".
- **QA** adds screenshots only where they say something the builder's don't: on `qa_fail`, a picture of each failure (it goes back to the builder with the issues, and you see it on the tab); on `qa_pass`, at most a few of the acceptance criteria it checked that the builder's set doesn't show, captioned with the criterion. Repeating the builder's set would double the cost and the disk space for the same picture. [I]
- **Testing on** (Test before Review, the default: `0007_card_flow.sql:6`): builder's set (`ready_for_testing`) → QA's additions (`qa_pass` or `qa_fail`) → in Review the tab shows QA's run on top and the builder's under it. After a `qa_fail` the builder's next run takes a new set, and Compare (§3.7) shows what changed.
- **Testing off:** the builder's set, straight into Review.
- **Which screens and states:** each screen the card changed (a page, a drawer, a dialog), in the state it was built for, with realistic demo data; plus its empty state and its error state when the card touches them or its acceptance criteria name them. Not the screens it didn't touch. [I]
- **Sizes:** the project's sizes (§3.8): a **desktop** size for every project (1280 × 800 for a website; 1440 × 900 for Gizai, its window's default) and a **mobile** size (390 × 844) for websites. Gizai has no mobile size: its smallest window is 900 × 600 (`src-tauri/tauri.conf.json:15`), its "small" size. Every changed screen at each size. [I]
- **Light and dark:** only where the project has both (Gizai, dark first). Every changed screen in the project's first theme, and the card's main screen in the other one too: a theme bug usually shows on any screen, and every screen twice doubles the set. [I]
- **How many:** typically 4 to 8 for a builder (two screens × two sizes, plus a theme or a state), 0 to 4 for QA; at most 12 a run by default (§3.8). [I]
- **File names say what is in them,** numbered in the order to look at them: `01-board-with-cards-desktop-dark.png`. The tab sorts by name, and a name that comes back in the next run is how Compare pairs two pictures (§3.7).

### 3.4 How they get onto the card

In all three ways the agent makes the pictures the same way: Chrome DevTools MCP's `take_screenshot` with `filePath`, so the image goes to a file and not into the model's context (§3.6). They differ in how Gizai learns about them.

| | A. A field in the result line | B. A fixed folder Gizai collects (recommended) | C. A small Gizai tool for runs |
|---|---|---|---|
| What the agent does | saves the images in `.gizai-tmp/screenshots/`, then lists them in its last line: `"screenshots":[{"file":"01-board.png","caption":"The board with three cards"}]` | saves the images in `.gizai-tmp/screenshots/`, and may write a caption per file in `.gizai-tmp/screenshots/captions.json`: `{"01-board.png":"The board with three cards"}` (Write tool) | calls `attach_screenshot` with a path and a caption, for each |
| When Gizai takes them | at the run's end, only when the result line parses | at the run's end, however it ended (also after Stop, a limit or a failure: they show how far it got) | at once, during the run |
| What changes in Gizai | a tolerant reader beside `learned` (`outcome.rs:44-56`), and the collect step | only the collect step | a run-side MCP server: runs have no Gizai tools today (`mcp_servers.rs:19-20`); a socket and a token per run; the tool in the agent's list; Claude Code only (`mcp_not_on`, `mcp_servers.rs:343-351`) |
| Captions | in the line: one stray double quote in a caption spoils the whole result line, so the card loses its verdict and Gizai nudges the run (`runs.rs:1363-1366`) | in their own file: a bad one loses only the captions | per call |
| Weak spots | a longer last line on top of `summary`, `issues` and `learned`; an agent that forgets the field shows nothing; a run stopped at a limit shows nothing | an image the agent saved there by mistake shows too | the most code, and a new way into Gizai's data folder for every run; Codex and Gemini agents can't use it |
| Size | S | S | L |

**Recommended: B, the fixed folder.** It asks the least of the agent (one `filePath` per picture), it can't cost the card its verdict, it keeps the pictures of a run that stopped part-way, and it is the smallest change: `prepare_temp` already empties the folder at a run's start, so everything in it at the end is that run's. [I]

**What Gizai checks on each file** (the same rules would hold for A and C):

1. **Only `.gizai-tmp/screenshots/` itself:** its files, not its subfolders, not the rest of `.gizai-tmp/`. The folder is found from the worktree Gizai made, never from a path the agent wrote.
2. **Real files only:** read with `symlink_metadata`; a link (to anything) is left out and never followed, as `empty_temp` does (`worktree.rs:256-258`); on Unix a file with more than one hard link is left out too, so nothing outside the worktree is copied through a link. Then the file is opened and read once, no second look by path.
3. **Images only, by their content, not their name:** the first bytes must be PNG's, JPEG's or WebP's signature, and the header must give a width and a height. SVG, GIF, HTML and anything else are left out (an SVG can hold script).
4. **Limits** (§3.8): at most 12 files a run (the first 12 by name), at most 10 MB each, at least 16 and at most 16,384 pixels a side and at most 50 megapixels (no "image bombs"). [I] 16,384 pixels is the usual largest texture of a GPU, and full-page screenshots of very long pages are known to break around there; a card checks it with a real long page.
5. **Captions:** `captions.json` at most 64 KB, a JSON object of file name → text; each caption plain text, control characters removed, at most 200 characters. A file without one gets its name as the caption ("01-board-with-cards-desktop-dark" → "Board with cards, desktop, dark"). Anything wrong in the file costs only the captions.
6. **What was left out is said** in the run's output, like Gizai's other notes (`note_run`): "Gizai kept 12 of 15 screenshots (Settings → Runs allows 12 a run). Left out: 13-…png, …; notes.txt isn't an image."

**How they are saved:** each file goes into the blob store as attachments do (`files::store`, `files.rs:72-105`: copied and hashed, read-only), then a `files` row: `owner_type` `task` and the card's id, **`run_id` the run** (the column made for this, `0001_init.sql:203`), `created_by` the agent, and three new columns from migration `0018`: `caption`, `width` and `height`. The Attachments block then lists the files without a run (what people added), and the Screenshots tab the images with one. After the collect step, `empty_temp` empties the folder as today. [V for the parts that exist; I for the design]



