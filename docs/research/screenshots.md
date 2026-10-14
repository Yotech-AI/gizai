# Gizai: screenshots of what an agent built, on the card in a Screenshots tab (research, 2026-10-14)

Status: research only, no code. Written by the R&D Agent for GA-173 on 2026-10-14, against `main` at `24535e3` (Gizai 0.11.0).
Legend: **[V]** = verified today, in the code (file and line) or with the command named (`git`, `rg`, `npm view`), or in the primary source named (its link and the date read are in §8). **[I]** = my judgement, or a fact I could not check in a primary source (the reason is given).
Limits: this run may not start node, cargo, php, a server or a browser, so nothing below was measured or tried; what rests on a measurement is marked [I] and given to a card to measure. The other projects (GW, MN, QC, YO, OO) were read in their main checkouts under `~/Herd`, read-only.

---

## 1. Summary

- **Yes, Gizai can.** Its agents already have a hidden browser (Chrome DevTools MCP 1.10.1, the newest) whose `take_screenshot` saves to a file instead of the model's context. Missing: a page to shoot, a way onto the card, and a tab.
- **Claude Desktop:** its Browser pane starts your dev server from `.claude/launch.json` (port in `PORT`) and checks its work with screenshots. Those go only into the conversation, never to disk, and the tools exist only in the desktop app, so Gizai's runs can't use them. Worth copying: one launch command per project.
- **Who:** the builder, on every card that changes a screen (so cards with testing off have them too); QA adds only its failures and a few criteria.
- **How:** the agent saves PNGs in `.gizai-tmp/screenshots/` with a `captions.json`. At every run's end Gizai checks them (images by content, no links, limits) and keeps them with the run's id (`files.run_id`, made for this), before it empties the folder.
- **The page:** Gizai runs a per-project **Preview command** on a port it picks, with demo data in a throwaway database (never your `.env` one), and ends it with the run. Agents can't serve it well: Astro detaches under Claude Code, and the Laravel projects forbid agents to serve. Gizai's own screens: a browser mode with a fake backend.
- **Cost:** about 150 tokens a picture with `filePath`, against 1,716 a picture in the context on every later turn (about $0.22 a run for 8). Also 20–30 tool calls and 1–4 MB a set; keep 3 sets, delete 30 days after Done.
- **Recommended:** option A (§4). **Cards (§6):** SD, SB1, SB2 and SG at once, then SF1, SB3 and SF2, then a preview script per project (MN, QC, YO, OO). **Open questions (§7):** 12, each with a recommendation.

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

### 2.4 Settings, the project form, the board and the pull request today

- **Settings** is a plain key/value store (`crates/gizai-core/src/settings.rs:1-31`); each feature has its default in code and checks its range when saving, refusing a value outside it. The model to copy is Settings → Agents and runs → Runs: minutes per run 90 (5–480), tool calls per run 200 (20–2000), runs at once 3 (1–20) (`src-tauri/src/runs.rs:22-29`, checks `:234-252`, the fields `src/pages/SettingsPage.tsx:106-112`). The agent form's "Cards at once" is 1 (1–10) (`crates/gizai-core/src/team.rs:332-333`). Housekeeping keeps run logs 30 days, a constant, not a setting (`crates/gizai-core/src/housekeeping.rs:11`), and runs at start and once a day; **nothing ever deletes a file's blob** (`files.rs:162`). [V]
- **The project form's "New worktrees"** section (`src/components/ProjectForm.tsx:104-117`): paths to copy from the main checkout (the placeholder suggests `.env`, `node_modules/`, `vendor/`, `target/`), Install missing dependencies, and a **Setup command** run once in each new worktree with `bash -c` (Git Bash on Windows) before the agent starts, at most 20 minutes, after which **everything it left running is killed** (`crates/gizai-agents/src/prepare.rs:14`, `:143-159`, `:309-310`). So a setup command can't leave a server running for the run, and there is **no preview or dev-server command anywhere**. The Testing switch is the card's, not the project's (§2.1). [V]
- **The worktree's `.env` is usually a copy of the main checkout's,** so a Laravel worktree talks to the same local database as your own checkout, and the agent form's browser can accept self-signed certificates "for local .test sites" (`src/components/AgentCliTools.tsx:121`). A `.test` site serves your **main checkout**, not the card's branch: a screenshot of it shows the wrong code, and your local data. §3.5 serves the card's branch on its own port with demo data instead. [V for the form texts; I for what your projects' `.env` files hold, which this run did not open]
- **The agent form's browser switch:** "Test web pages in a hidden browser", with a switch per tool once they are listed (`AgentCliTools.tsx:105-128`). [V]
- **The board card** shows its id, the pull request's state, On hold or Team Lead, priority, "<agent> is working", labels, By hand and the assignee (`src/components/Board.tsx:36-52`): no count of files. **The Inbox** lists cards on hold and cards waiting for you in Review or Deploy (`src/lib/inbox.ts:41-47`), with Run this for me cards on top. [V]
- **Images in Markdown** (comments, descriptions) show as "[image: alt]": the security policy blocks remote images (`src/components/MarkdownView.tsx:39-40`). No `<img>` of a file exists in Gizai yet. [V]
- **The pull request:** Gizai writes its title and body (the description and the acceptance criteria, ending "From Gizai card X.", `src-tauri/src/pulls.rs:284-302`) and sends them with `gh pr create --body-file -` (`crates/gizai-agents/src/github.rs:208-217`) or Bitbucket's API. **Gizai uploads nothing to GitHub but git pushes, the pull request and its merge.** [V]
- **A run's leftovers:** when the CLI exits, Gizai sends its whole process group SIGTERM and SIGKILL after 3 s: "Background processes the agent started (a dev server, a watcher) would outlive the run" (`crates/gizai-agents/src/process.rs:264-276`); on Windows the run's Job Object ends everything in it. Only a process that leaves the group on purpose (`setsid`, a daemon) survives on Linux and macOS. So a server an agent starts for screenshots stops with its run. [V]

### 2.5 What the feature touches

| Area | Today | For screenshots |
|---|---|---|
| A run's end (`src-tauri/src/runs.rs:1139-1158`) | process group ends → MCP config removed → `empty_temp` → `finish_run` | a **collect** step just before `empty_temp`: images from `.gizai-tmp/screenshots/` onto the card (§3.4) |
| A run's prompt (`crates/gizai-agents/src/prompt.rs`) | "How this run works": commands, temp folder, waiting, push, Run this for me | a Screenshots section when the agent takes screenshots: where to save them, the sizes, the captions, and the preview's address (§5.4) |
| A run's start (`src-tauri/src/runs.rs`, after the worktree is prepared) | worktree, prepare command, temp folder, MCP config | optionally Gizai starts the project's **preview command** on a free port, and ends it with the run (§3.5) |
| Files (`crates/gizai-core/src/files.rs`, `files` table) | `run_id` never set; no caption | evidence rows with `run_id`, a caption, width and height (migration `0018`) |
| Card page (`src/pages/TaskPage.tsx:221-227`) | Card \| Changes | Card \| Changes \| **Screenshots** with a count (§3.7) |
| Attachments (`TaskPage.tsx:254-257`, `FileDrop.tsx`) | every file of the card | only the files people added (no `run_id`) |
| Board card, Inbox | no count of files (§2.4) | a sign on a card in Review with screenshots (§3.7) |
| Settings, project form, agent form, the card's Properties | no preview command; the browser switch (§2.4) | the switches and values of §3.8 |
| Housekeeping (`crates/gizai-core/src/housekeeping.rs`) | run logs after 30 days; never a blob (§2.4) | old screenshots go, then unused blobs (§3.6) |

---

## 3. The feature

### 3.1 What it should do

When an agent builds something with a screen, the card has screenshots of it by the time it reaches Review, on a **Screenshots** tab, so you look at what was built before you read the pull request (the card's Why; Jefsev, 14 Oct 2026). Every part is a switch or a value within a range, with a default (§3.8). Research only: nothing is built before you approve.

### 3.2 Claude Desktop, checked

**What it is.** Claude Code in the Claude desktop app (its Code tab) has a **Browser pane**, and "Claude can start a dev server and open it in the Browser pane to verify its changes." [V desktop docs] It came as app preview on 20 February 2026 ("Claude Code on desktop can now start dev servers and preview your running app directly in the desktop interface") and became a built-in browser in week 28 of 2026 (6–10 July, Claude Code v2.1.202–2.1.206: "Claude Code on desktop now has a built-in browser"). [V blog, what's new] It is in the macOS and Windows apps (Cmd+Shift+B or Ctrl+Shift+B) and the Linux beta ("live app preview"). [V desktop docs, desktop-linux docs]

**How it starts the page.** "Claude automatically detects your dev server setup and stores the configuration in `.claude/launch.json` at the root of the folder you selected when starting the session." Each configuration has `name`, `runtimeExecutable` (like `npm`), `runtimeArgs` (like `["run", "dev"]`), `port` ("Defaults to 3000"), `cwd`, `env`, `autoPort`, `program`, `args` and `url`; and "When Claude picks a different port, it passes the assigned port to your server via the `PORT` environment variable." You "Start or stop servers, or stop them all at once, from the **Dev servers** menu in the Browser pane's header". [V desktop docs] OO already has such a file, from your own Claude Desktop sessions: a `template-preview` on port 4173 (`otus-os/.claude/launch.json:1-14`). [V]

**How it takes screenshots.** "Auto-verify is on by default": "When `autoVerify` is enabled, Claude automatically verifies code changes after editing files. It takes screenshots, checks for errors, and confirms changes work before completing its response." [V desktop docs] Anthropic's docs name no tools; users' reports in the `anthropics/claude-code` issues do (quoted from the issues, not from Anthropic): today a built-in MCP server `Claude_Browser` with `navigate`, `preview_start`, `preview_list`, `preview_stop`, `preview_logs`, `computer`, `read_page`, `get_page_text`, `find`, `form_input`, `javascript_tool`, `read_console_messages`, `read_network_requests`, `tabs_*` and `resize_window` (#92046), before that `Claude_Preview` with `preview_start`, `preview_screenshot`, `preview_snapshot`, `preview_click`, `preview_eval` and `preview_resize` (#62653, #30122). A screenshot is the `computer` tool's `screenshot` action. [V in the issues]

**Where the images go, and what you saw.** Into the conversation only: "`mcp__Claude_Browser__computer` with `action: "screenshot"` returns the image into the conversation. It has no `filePath` parameter, so there is no way to get the bytes onto disk." (#94503, titled "Browser pane screenshot has no filePath — can't produce a file for gh --attach or any file consumer"). You see the live page in the pane, not a gallery of what was shot. The answer you saw in September, "region crop not yet supported", is the Browser pane's `zoom`: "zoom: region crop not yet supported in the Browser pane; full screenshot returned" (#96717, Windows, Claude Desktop 2.7032.0); a comment of 8 October says `zoom` now crops. `javascript_tool` is the pane's too: its tools copy the Chrome extension's, as "Browsing in the Browser pane uses the same safety model as the Claude in Chrome extension". [V desktop docs, issues]

**What Gizai can use or copy.**

- **Use: nothing directly.** [I] The Browser pane's tools exist only in the desktop app's own sessions (not even in its SSH sessions: #98288), so a `claude -p` that Gizai starts doesn't get them. The CLI's way, Claude in Chrome (`--chrome`), drives *your* Chrome with your logins, which Gizai's hidden browser exists to avoid (`browser.rs:1-4`), and it stays off with an API key or a `claude setup-token` login. [V chrome docs, for the last]
- **Copy: a launch command per project, with the port handed in.** That is §3.5's Preview command, with `GIZAI_PREVIEW_PORT`, and, as Claude Desktop does, the same port in `PORT` for the preview command only (OO's API reads `PORT`). When a repository has a `.claude/launch.json`, the project form can offer its configurations as the preview command, as it offers paths "found in the repository" to copy today (`ProjectForm.tsx:109-110`). [I]
- **Copy: verify, then answer.** The agent takes its pictures after its last change and before its result line (§3.3).
- **Don't copy: images in the conversation.** Gizai's agents save each picture to a file (`filePath`), so a set costs almost nothing in tokens (§3.6), and Gizai can keep and show them, which Claude Desktop can't.

**Other coding agents, in a line each:**

- **GitHub Copilot coding agent** runs the web app with Playwright, on by default, and "Copilot can even share screenshots of what it has done in its pull request" (GitHub changelog, 2 July 2025). [V]
- **Devin** tests in its own browser and records a video of it: "The recording is sent directly to you as an attachment". [V, read by a helper; not re-read]
- **Cursor** cloud agents "produce merge-ready PRs with artifacts (videos, screenshots, and logs)" (changelog, 24 February 2026); posting them to GitHub is a setting, with links that are "viewable without authentication". [V, read by a helper; not re-read]

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

### 3.5 The page up, per stack

**Recommended: Gizai starts it, from a per-project Preview command, on a port it picked, and ends it with the run.** [I] Why not leave it to the agent:

1. **A server an agent starts can outlive the run.** Gizai ends the run's process group (§2.4), but a server that leaves the group survives. Astro does exactly that under an agent: Astro 7.3.8 (GW's) notices Claude Code by its `CLAUDECODE` variable (`node_modules/am-i-vibing/dist/detector-1yx2Hoe0.mjs:26-29`) and then starts `astro dev` and `astro preview` as a **detached** background process (`node_modules/astro/dist/cli/server.js:116-130`, `cli/preview/index.js:37-45`; not on Windows, where the Job Object would catch it: `cli/agent.js:10-14`), so it keeps its port after the run, unless `--ignore-lock` is given. Gizai starts the preview without `CLAUDECODE`, in a tree of its own that it ends itself. [V]
2. **The projects' own rules forbid it:** MN's, QC's and YO's agent files say "Never run commands to serve the site. It is always available." (Laravel Boost's Herd section: `spoorwegmuseum/CLAUDE.md:92`, `quiz-cms/CLAUDE.md:91`, `stapsgewijz/AGENTS.md:159`). That `.test` site is your main checkout (§2.4). Gizai serving the card's branch needs no agent to break its project's rule. [V]
3. **A run can't do the plumbing:** it can't type a variable, find a free port or end a process (§2.3); Gizai can. And the recipe lives once, in the project, not in every agent's Memory. [V for the run's rules; I for the rest]

**How it works** (card SB2):

- The project form gets a **Preview** section: **Preview command** (empty = no preview) and **Start time** (§3.8). Gizai runs the command when a run starts whose agent takes screenshots, after the worktree is prepared and before the CLI starts: with `bash -c` (Git Bash on Windows) in the worktree, as the setup command (`prepare.rs:143-159`), stdin closed, its output in `.gizai-tmp/preview/preview.log`, in its own process tree (`os::spawn_tree`).
- Its environment: **`GIZAI_PREVIEW_PORT`**, a port Gizai checked is free (it binds `127.0.0.1` on it) and gives no other live run, from a range far from the usual dev ports (§3.8: 47100–47999, never 8000, 5173, 3000, 4321 or 1420); the nine ports after it are the run's too, for a second server (an API, Vite). **`GIZAI_PREVIEW_URL`** (`http://127.0.0.1:<port>`) and **`GIZAI_PREVIEW_DIR`** (`<worktree>/.gizai-tmp/preview`, for a throwaway database; it goes with the temp folder). The agent's own commands get the same three, so a project script it runs (to load the demo data again) finds the same database.
- Gizai waits until the port answers, at most the start time, then starts the agent; the prompt says the address. If it doesn't come up, the run starts anyway: the prompt and the run's output say why (the log's last lines) and that it takes no screenshots; the card isn't held for it.
- When the run ends: the preview's tree is ended (SIGTERM, SIGKILL after 3 s; on Windows its Job Object), then the screenshots are collected, then the temp folder is emptied, the throwaway database with it.
- Without a preview command the agent may still serve the branch itself, on the port Gizai gives it, with `run_in_background`; it then stops with the run (except a detaching server: Astro needs `--ignore-lock`).

**Allowed commands:** with Gizai's preview, the agent needs **nothing new**: the browser's tools, and for rebuilding or reloading data the `npm`, `npx`, `php`, `composer` and `node` its default list already has (`seed.rs:245-257`). That is why each project's recipe below is a script behind `composer` or `npm`: the same entry point works when Gizai starts it and when an agent runs it by hand, where an agent can't write `DB_CONNECTION=sqlite php …` or `$GIZAI_PREVIEW_PORT` itself. Checking that the page answers, by hand: `node -e "fetch('http://127.0.0.1:47130/').then(r => console.log(r.status), e => console.log(e.message))"` (builders and QA have `node`). [V for the lists; I for the scripts]

| Project | Preview command (Gizai starts it) | Data | Login for the agent | Notes |
|---|---|---|---|---|
| **GA** Gizai (Tauri) | `npx vite --mode browser --host 127.0.0.1 --port "$GIZAI_PREVIEW_PORT" --strictPort`: the screens in Chrome with a fake backend (card SG) | the demo data, as fixtures | none | Linux, macOS, Windows; `scripts/shot-cage.sh` stays for the README, tour and QA's real-app check on Linux |
| **GW** gizai-website (Astro 7.3.8, static) | `npx astro dev --host 127.0.0.1 --port "$GIZAI_PREVIEW_PORT" --ignore-lock` (changes show as the agent edits), or `npm run build && npx astro preview … --ignore-lock` | none needed | none | pages: `/`, the 404; `--ignore-lock` keeps Astro from detaching |
| **MN** spoorwegmuseum (Laravel 13, Filament 5) | `composer preview` (a new script, below) | `DatabaseSeeder` + `CollectionDemoSeeder`, `TouchscreenDemoSeeder`, SQLite in `GIZAI_PREVIEW_DIR` | public site: none; `/admin`, `/touchscreens`, `/vrijwilligersportaal`: the seeded local admin | map tiles and YouTube load from the web |
| **QC** quiz-cms (Laravel 12, Twill 3.5) | `composer preview` (new) | `DemoQuizSeeder`, `DemoAnswersSeeder`, `QuizTranslationSeeder`, SQLite | public previews and the embed: none; `/admin`: a throwaway Twill superadmin made by the script | the embed needs `npm run build` (both builds) |
| **YO** stapsgewijz (Laravel 13, Inertia React) | `composer preview` (new) | `DatabaseSeeder` (7 users, one per role, shift types); **no roster or visit data yet**: a demo seeder first | every page: a seeded user, with 2FA off for the preview | `/componenten` (the component library) with `APP_DEBUG` |
| **OO** otus-os (npm workspaces, NestJS 11 + Vite) | `npm run preview:gizai` (new): the API serving the dashboard's build on one port | empty, the API migrates itself; `scripts/demo-registers.mjs` | a throwaway owner account from the script's environment | the API listens on `0.0.0.0` (`apps/api/src/main.ts:113`): the preview is reachable from your network |

**GA, Gizai itself.** Two ways, and the first works on all three systems:

- **(recommended) Gizai's screens in a plain browser, with a fake backend** (card SG). Today the UI can't load outside Tauri: `src/api.ts` is the only file that talks to Tauri, through `invoke`, and without it `appInfo()` fails and the page shows only an error (`src/App.tsx:57`, `:170`, `:181`). Tauri's own `@tauri-apps/api/mocks` (2.12.1, in `node_modules`) has `mockIPC(cb, { shouldMockEvents: true })`, `mockWindows` and `mockConvertFileSrc` (`mocks.d.ts:44-50`, `:129`, `:169`, `:188`). A `browser` mode (a second entry, `browser.html`, so the app's own build never contains it) calls `mockIPC` before React starts, answering each command from fixtures of the demo data that a small Rust example writes from `cargo run -p gizai-core --example demo`, so the shapes are the real ones; a command without a fixture answers empty and says so in the console. The agent opens `http://127.0.0.1:<port>/browser.html#/task/<id>`, sets the light theme with the browser's `press_key` (`t`, `src/App.tsx:49`) or `evaluate_script` (`localStorage`, key `gizai-theme`, `src/lib/appearance.ts:28-31`), and sizes with `resize_page`. What differs from the app: Chrome draws it, not WebKitGTK or WKWebView (Windows' WebView2 is Chromium); native dialogs, file drops and the tray don't exist; the fonts are the same files (`src/styles/fonts.css`). [V for the code; I for the plan]
- **`scripts/shot-cage.sh <route> <out.png>`**: the real app and backend with the demo data, Linux only (cage, grim, wlr-randr, dbus-run-session), and it builds the release app first when it is missing or stale (`scripts/app-ready.sh`, `gz_app`: minutes in a cold worktree). Not on macOS or Windows: there is no headless compositor there, and a real window would open on your screen, which this repository's rules forbid (CLAUDE.md, "Never start a GUI on Jeffrey's screen"). An agent would need `Bash(scripts/shot-cage.sh:*)` on its list; its size is set with `SHOT_SIZE=900x600 scripts/shot-cage.sh …`, an assignment before the command that a run's rules may refuse (not checked). Keep it for the README, the tour and a QA check of the real app on Linux. [V for the script; I for the rest]

**GW, the website** (Astro 7.3.8; `astro.config.mjs` has no adapter or `output`, so it is static: `:47-49`; `dist/` from `npm run build`, README `:9-30`): `astro dev` or `astro preview` take `--port` (`cli/preview/index.js:24`) and `--ignore-lock` (`:28`), which keeps the server in the foreground, untracked, and is needed when an agent starts it (point 1). No data, no login. [V]

**MN, QC and YO, Laravel.** What a fresh worktree lacks (`.env`, `vendor/`, `node_modules/`, `public/build`, `public/hot`, `public/storage` are git-ignored) the project's New worktrees settings copy or install. The recipe, as a script in each repository (`composer preview`, a composer script calling `scripts/preview.sh`; Git Bash runs it on Windows): [I, a card per project tries it]

```bash
#!/usr/bin/env bash
# Serves this checkout on GIZAI_PREVIEW_PORT with demo data in a throwaway SQLite file (Gizai's preview).
set -euo pipefail
: "${GIZAI_PREVIEW_PORT:?}" "${GIZAI_PREVIEW_DIR:?}"
mkdir -p "$GIZAI_PREVIEW_DIR"
export APP_ENV=local APP_DEBUG=true APP_URL="http://127.0.0.1:$GIZAI_PREVIEW_PORT" \
  DB_CONNECTION=sqlite DB_DATABASE="$GIZAI_PREVIEW_DIR/db.sqlite" \
  SESSION_DRIVER=file CACHE_STORE=file QUEUE_CONNECTION=sync MAIL_MAILER=log
: > "$DB_DATABASE"
php artisan migrate --force --seed                              # plus the project's demo seeders
rm -f public/hot                                                # else pages load their assets from a Vite dev server
npm run build
php artisan storage:link --relative 2>/dev/null || true
exec php artisan serve --no-reload --host=127.0.0.1 --port="$GIZAI_PREVIEW_PORT"
```

- **Why environment variables, not `.env`:** the worktree's `.env` is a copy of the main checkout's, and its database is your local one: MySQL in MN and QC (`spoorwegmuseum/.env.example:23-26`, `quiz-cms/.env.example:25-28`), "the shared dev database" in QC's own preview notes (`quiz-cms/design/kennisquiz/README.md:147`), where MN's plans say "migrate, never `migrate:fresh`". A preview must never write there, nor show its data. Laravel's `.env` doesn't override a variable that is already set [I: phpdotenv's immutable loader; the card checks it]. **`--no-reload` matters:** without it, and with a `.env` file there, `artisan serve` starts its PHP server with every variable it read removed, so that the server reads `.env` again (`vendor/laravel/framework/src/Illuminate/Foundation/Console/ServeCommand.php:183-186`; the option `:451`). QC's earlier preview used `php -S` with a router for the same reason (`README.md:145-197`). [V]
- **Built assets, not Vite's dev server:** one port, no `public/hot`, and the pages render from `public/build/manifest.json` (MN's Filament panels need it: `.github/workflows/tests.yml:77-81`; QC's embed `q.js` is served from it: `app/Http/Controllers/QuizController.php:58-60`). After changing CSS or JS the agent runs `npm run build` again; PHP and Blade changes show at the next request. [V]
- **SQLite works** for MN (its CI runs on SQLite, `tests.yml:83-88`; migrations branch on the driver) and QC (its tests run every migration on it); YO uses SQLite already (`stapsgewijz/.env.example:23`). [V, from the sweep]
- **Logins:** MN's `AdminUserSeeder` makes a local Filament admin only when `APP_ENV=local` (`database/seeders/AdminUserSeeder.php:19`); QC's script adds `php artisan twill:superadmin <email> <password>` with a throwaway login; YO's every page needs a login with confirmed 2FA, which only `APP_ENV=local` with `TWO_FACTOR_REQUIRED=false` switches off (`app/Http/Middleware/EnsureTwoFactorIsConfirmed.php:33-36`), so the script sets that too. The agent logs in through the page's own form each run (the browser's profile is thrown away). A seeder's dev password is not a secret worth keeping, but it must never be a real one. [V for where they are]
- **Network:** MN's map tiles (OpenFreeMap, PDOK) and YouTube embeds, and YO's Google Fonts at build time, come from the web; the hidden browser can reach any address (`AgentCliTools.tsx:116`). [V, from the sweep]

**OO, Otus OS** (npm workspaces; NestJS 11 API with SQLite through `node:sqlite` that migrates itself; the dashboard on Vite): the API needs `OTUS_SECRET_KEY`, `ADMIN_USERNAME` and `ADMIN_PASSWORD_HASH` or it exits (`apps/api/src/config/env.spec.ts:144-166`), keeps its database at `DB_PATH` (default `database/database.sqlite`), and can serve the dashboard's build from `apps/api/public` on its own port (`apps/api/src/main.ts:33-36`, `:73-89`, as the Dockerfile does). So `npm run preview:gizai`: build, copy the dashboard's build there, and start the API with `PORT=$GIZAI_PREVIEW_PORT`, `DB_PATH=$GIZAI_PREVIEW_DIR/otus.sqlite` and a throwaway owner (`npm run hash-password -w @otus/api` makes the hash) — one port, no CORS to set. The two-port way is Vite's dashboard on the port with `VITE_API_URL` and `DASHBOARD_ORIGIN` pointing at the API on the next port (`apps/dashboard/vite.config.ts:13-16` has 5173 with `strictPort`, `src/api.ts:66`). Its data starts empty; `scripts/demo-registers.mjs` fills registers through the API. OO also has a **`.claude/launch.json`** with a `template-preview` configuration on port 4173 (`.claude/launch.json:1-14`): the file Claude Desktop's preview reads (§3.2). [V for the code; I for the script]

### 3.6 Cost and safety

**Tokens: a picture saved to a file costs almost nothing; one in the conversation costs on every later turn.**

- With `filePath`, Chrome DevTools MCP 1.10.1 writes the file and answers one line: `Saved screenshot to ${result.filename}.`; without it, a picture under 2,000,000 bytes is attached to the answer for the model to see (`attachImage`), a bigger one goes to a temp file (`src/tools/screenshot.ts` at tag `chrome-devtools-mcp-v1.10.1`). `filePath` is "The absolute path, or a path relative to the current working directory, to save the screenshot to instead of attaching it to the response." 1.10.1 (23 September 2026) is still the newest version (`npm view chrome-devtools-mcp version`, via the helper). [V]
- A picture in the conversation costs "`⌈width / 28⌉ × ⌈height / 28⌉` visual tokens"; Opus 5.5 and Sonnet 5.5 are in the high-resolution tier ("Claude 4.7 and later models", long edge 2576 px, at most 4784 tokens), which "can use up to roughly three times more visual tokens" than the standard tier. Opus 5.5's input is $4 per million tokens. [V vision docs] So [I, computed]: a 1440 × 900 screenshot is 52 × 33 = **1,716 tokens**, 1280 × 800 is 1,334, 900 × 600 is 726, 390 × 844 is 434, and a whole page of 1280 × 5000 is scaled to about 660 × 2576, 2,208 tokens.
- Once in the conversation it is sent again with every later turn (from the cache, at a fraction of the price: GA-171 found 0.05× for cache reads and 2× for the 1-hour cache's writes on Opus 5.5 [I, not re-read today]). Eight desktop screenshots looked at early in a run with 40 more turns: about 13,700 tokens written, then 13,700 read 40 times, together about 55,000 input tokens' worth, **about $0.22 a run** at API prices, or that much of a subscription's limits. With `filePath`: eight tool calls and their one-line answers, about 1,200 tokens (roughly $0.005). [I]
- And more than 20 pictures in one request lower the limit for each to 2000 px a side (vision docs), so an agent that keeps looking at whole pages would start failing. [V]
- **So the rule for agents:** always `filePath`; look at a picture only when it must (opening the saved file costs the same tokens as above); check what a page says with `take_snapshot` (the page as text) instead. [I]
- **Tool calls:** about 2–4 browser calls a picture (navigate or click, resize or emulate, `take_screenshot`), so a set of 8 is 20–30 of the run's 200 (Settings → Runs). The agent form already warns: "Every click is a tool call, and the cap per run is in Settings" (`AgentCliTools.tsx:106`). [V for the hint; I for the counts]

**Time per run** [I, to be measured by SB2 on each project]:

| Step | Estimate |
|---|---|
| Preview start: GW (`astro dev`), Gizai's browser mode (Vite) | 2–5 s |
| Preview start: MN, YO (migrate and seed SQLite, `npm run build`) | 15–90 s; QC more, it builds twice |
| Preview start: OO (`npm run build` of the workspaces) | 30–120 s |
| The hidden browser's first call (it starts Chrome then: `browser.rs:7`) | 1–3 s |
| Each picture (go there, size, wait, shoot) | 2–5 s; a set of 8 in 1–2 minutes |

All of it inside the run's own limit (90 minutes by default, `runs.rs:23`); the preview starts while nothing else waits for it but the agent.

**Disk** [I]: an interface PNG at 1440 × 900 is usually 100–400 KB (flat colours compress well), a mobile one 50–150 KB, a whole page or a photo-heavy page (MN's collection, maps) 1–5 MB. A set of 8 is 1–4 MB, a card with three sets 3–12 MB, 100 cards 0.3–1.2 GB. The same picture in two runs is stored once (content hash, `files.rs:94-97`). **When they go** (the settings in §3.8): a card keeps its newest 3 sets, the rows of an older set go when a fourth arrives; all of a card's screenshots go 30 days after it reached Done or Cancelled; and a new housekeeping step deletes a blob once no row uses it any more, because today nothing ever deletes a blob (`files.rs:162`, `housekeeping.rs:9-18`).

**Privacy rules:**

1. **Demo data, never yours.** The preview serves the card's branch with a throwaway database filled by the project's seeders (§3.5), never the database in the worktree's `.env`; the agent's prompt says to shoot only the preview's address, never a `.test` site, a live site or an account of yours. Gizai can't stop the browser opening another address ("they can reach any address", `AgentCliTools.tsx:116`), so this is a rule in the prompt, with the preview as the easy way. [I]
2. **They stay on this computer.** In Gizai's data folder (the blob store), shown only in Gizai's window. `.gizai-tmp/` is excluded from git in every repository (`worktree.rs:202-206`; all five projects' `info/exclude` have it), so they can't be committed by accident, and Gizai pushes only commits. [V]
3. **The model sees only what the agent opens.** With `filePath`, nothing; a picture it opens goes to Anthropic as the rest of its conversation does ("Anthropic does not use uploaded images to train models.", vision docs). [V]
4. **The hidden browser** is headless with a throwaway profile, never your browser, profile or logins (`browser.rs:1-4`). [V]
5. **To GitHub only when you choose, later.** Since gh v2.99.0 (1 September 2026) `gh pr create`, `gh pr edit` and `gh pr comment` take `--attach`; "You need push access to the repository to attach files."; it works with "either the OAuth token from `gh auth login` or a classic personal access token". "In the case of private and internal repositories, only people with access to the repository can view the uploaded files", and "For public repositories, uploaded files can be accessed without authentication"; at most "10MB for images and gifs". [V GitHub docs and changelog] So a later **Add to pull request** on the tab can post a set as a PR comment with `gh pr comment --attach`, as your choice per set; Bitbucket has no such path in Gizai. Not in the first version (§5.8), and **never by an agent**: QA's list allows `gh pr create` and `gh pr edit` with any arguments (`seed.rs:260`), so with gh 2.99.0 a QA agent could attach a file today; its screenshot line says never to, and the folder is emptied at its run's end. Committing pictures to the branch is never an option: git keeps them for good, in a repository that may be public. [I]

### 3.7 The Screenshots tab, in words

For the Design Agent, who designs it (card SD); every choice here is a recommendation. [I]

- **Where:** a third tab at the top, **Card | Changes | Screenshots**, at `#/task/<id>/screenshots` (and `…/screenshots/<run>` for one run's set), full width like Changes (`TaskPage.tsx:221-227`, `:230`). At the top because it is what you look at first in Review, before Changes, and pictures need the width; not among Comments, Activity and Runs, which sit under the card's text. Icon: Lucide `images` (the design system has no concept for it yet: `design/README.md:137-154`).
- **The count on the tab:** the newest set's number ("Screenshots 6"), as Changes shows its counts; "None" in faint text when the card has none, as Changes says "No changes". The tab shows only on a project whose screenshots are on (§3.8).
- **Per run, newest first:** a heading per run with screenshots: the agent (avatar, name, role icon), its answer in words (Ready for testing, QA passed, QA failed, Needs a decision, or Stopped at the time limit), when ("2 h ago"), how many, and a link to the run in the Runs tab. Under it a grid of thumbnails at their own proportions, each with its caption, its size ("1440 × 900") and, small, its file name. Older runs folded after the newest two.
- **A larger view:** a click opens the picture over the page, fitted to the window, its caption under it; a click or `1` shows it at 100% (drag or scroll to look around), ← and → go to the set's next and previous picture, Esc closes. **Open** opens it in the system's image viewer, as Attachments do (`open_file`).
- **Compare with the previous run:** in the larger view, when the run before had a picture with the same file name: side by side, or one over the other with a slider. The same picture (the same content hash) says "Unchanged"; a name the previous run didn't have says "New"; names it had that this run hasn't are listed under the set, "Not taken this time". The heading of a set says "3 changed, 2 unchanged, 1 new" when there is a set before it.
- **A run that took none,** while the project's screenshots are on, says why in its heading: "No screen changed" (the agent said so in its summary), "The preview didn't start", or "Left out 3: not images".
- **On the board:** a card in a Review column whose newest set has screenshots shows a small `images` sign with the count, next to its pull request sign (`Board.tsx:37`); other columns show none, to keep the board quiet. **In the Inbox,** a card waiting for your review says "6 screenshots" and opens on the tab.
- **Empty tab:** "No screenshots yet. Agents with the browser save screenshots of the screens they changed at the end of their run." with a link to the project's Preview settings when it has no preview command.
- **Light and dark:** the thumbnails sit on the page's surface with a 1 px border, so a light screenshot in the dark theme (and the other way round) keeps its edge.

### 3.8 Settings you control

Jefsev's rule (14 Oct 2026): every part is a switch or a value within a range, with a default. Gizai checks each range when saving, as Settings → Runs does (`src-tauri/src/runs.rs:234-252`). [I for the values]

| Setting | Default | Range | Where | Card |
|---|---|---|---|---|
| **Screenshots** for the project | on | on, off | project form → Preview | SB1, SF2 |
| **Preview command** | empty (no preview, so no screenshots) | a command, run with `bash -c` | project form → Preview | SB2, SF2 |
| **Start time** for the preview | 120 s | 10–600 s | project form → Preview | SB2, SF2 |
| **Sizes** | Desktop 1280 × 800 and Mobile 390 × 844; for Gizai Window 1440 × 900 and Small 900 × 600 | 1–4 sizes; width 320–3840, height 320–2400 | project form → Preview | SB1, SF2 |
| **Both themes** | off; on for Gizai | on, off | project form → Preview | SB1, SF2 |
| **Whole page** (the full length, not one screen) | off | on, off | project form → Preview | SB1, SF2 |
| **Take screenshots of what it built** for the agent | on for Frontend, Design and QA agents that have the browser; off for the rest | on, off; only with the browser on | agent form → Tools → Browser | SB1, SF2 |
| **Screenshots per run** | 12 | 1–30 | Settings → Agents and runs → Runs | SB1, SF2 |
| **Largest screenshot** | 10 MB | 1–50 MB | Settings → Agents and runs → Runs | SB1, SF2 |
| **Keep** per card | the newest 3 sets | 1–20 sets | Settings → Agents and runs → Runs | SB1, SF2 |
| **Delete a finished card's screenshots after** | 30 days in Done or Cancelled | 0–365 days (0 = keep them) | Settings → Agents and runs → Runs | SB1, SF2 |
| **Preview ports** | 47100–47999 | from 1024 to 65535, at least ten ports for each run at once | Settings → Agents and runs → Runs | SB2, SF2 |
| **Screenshots before Review** for the card | the project's switch | on, off | the card's Properties, under Test before Review (`Properties.tsx:56-57`) | SB1, SF2 |

The prompt line an agent gets (§5.4) names the sizes, the themes and the limit, so the agent knows its set before it starts; the collect step holds Gizai to the limits whatever the agent did.

---

## 4. Options

All three use the hidden browser Gizai already has: Chrome DevTools MCP 1.10.1, Apache-2.0, 14.3 MB unpacked, fetched by `npx` at an exact version (`npm view chrome-devtools-mcp@1.10.1 license version dist.unpackedSize`; `browser.rs:36-49`), and still its newest release. None needs a new library. [V]

### Option A (recommended): the agents shoot, Gizai serves and collects

The builder (and QA, for its additions) takes the pictures with `take_screenshot` and `filePath` into `.gizai-tmp/screenshots/`; Gizai starts the project's preview command on a free port before the agent and ends it after; at the run's end Gizai collects the folder onto the card (§3.4 B); the card shows a Screenshots tab.

- **Takes:** a collect step and evidence rows (SB1), the preview command (SB2), the tab (SF1), the settings (SF2), a design first (SD), and for Gizai itself a browser mode with a fake backend (SG). Each other project needs a preview script in its own repository (a card on its own board).
- **Libraries:** none new. Reading an image's width and height needs only its header: PNG's, JPEG's and WebP's are a few dozen lines by hand; or `image` 0.25.10, already in `Cargo.lock` through the clipboard plugin's `arboard` with only its PNG and TIFF features (`cargo tree -i image@0.25.10`), with its `jpeg` and `webp` features switched on.
- **Risks:** the agents' sets vary with their care (the prompt line and the limits keep them in bounds); each project's preview recipe needs looking after when its stack changes; a set costs 20–30 of a run's tool calls.
- **Size:** about six cards of S to M, plus a small card per project.

### Option B: Gizai shoots a list of pages itself

The agent writes the pages to shoot (`shots.json`: addresses and captions); at the run's end Gizai drives the hidden browser itself with its own MCP client (`crates/gizai-agents/src/mcp_client.rs`, which today says hello and reads `tools/list` only: `:1-2`, `:399-420`) and calls `new_page`, `resize_page`, `emulate` and `take_screenshot` for every page at every size and theme.

- **For it:** the same complete set every time; no model tokens or tool calls for the pictures; every size and theme for free.
- **Against it:** only what an address reaches: no open drawer, no form with its errors, no page behind a login without a login script; a second browser start after the agent's; Gizai becomes a browser driver (a `tools/call` path, timeouts, pages that never settle). The agent still needs the browser to check its own work.
- **Size:** L. Worth adding later on top of A, for "every page at every size", if the agents' sets turn out uneven. [I]

### Option C: a Gizai tool for runs

`attach_screenshot(path, caption)` in a run-side Gizai MCP server (§3.4 C): pictures arrive at once and the agent hears at once when one is refused, but runs have no Gizai server today, it needs a socket and a token per run, and Codex and Gemini agents can't use it (`mcp_not_on`). **Size:** L. [I]

**Why A:** it is the smallest change that gives a useful set on every card with a screen, it can't cost a card its verdict, the pictures show states only an agent can reach, and the preview it needs is worth having anyway (later: a Preview button on the card for you). [I]

---

## 5. The design (option A)

### 5.1 The screens

The Design Agent designs them (SD); in words in §3.7. A sketch of the tab:

```
┌ Tasks / Gizai / GA-180 ───────────────────────────────────────────────────────────────┐
│ ▣ Card   ⎘ Changes +120 −8   ▦ Screenshots 6                                          │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ ✓ QA Agent · QA passed · 2 h ago · 2 screenshots                     View the run ›   │
│ ┌───────────┐ ┌───────────┐                                                           │
│ │           │ │           │   AC 2: the empty board says what to do                   │
│ └───────────┘ └───────────┘                                                           │
│ ▢ Frontend Agent · Ready for testing · 3 h ago · 4 screenshots · 1 changed, 3 new     │
│ ┌───────────┐ ┌───────────┐ ┌──────┐ ┌───────────┐                                    │
│ │           │ │           │ │      │ │           │                                    │
│ └───────────┘ └───────────┘ └──────┘ └───────────┘                                    │
│ Board, three cards  Board, empty   Board, mobile  Board, light                        │
│ 1440 × 900          1440 × 900     390 × 844      1440 × 900                          │
│ ▸ 2 older sets                                                                        │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

The larger view: the picture fitted to the window over a dark backdrop, its caption and size under it, ‹ › for the set's others, **Compare** (side by side, or a slider over the previous run's picture with the same name), **100%**, **Open**, and Esc. The board card in Review gets `▦ 6` next to its pull request sign. Settings: the project form's **Preview** section (Screenshots, Preview command, Start time, Sizes, Both themes, Whole page), one switch under the agent form's browser, four rows in Settings → Runs, one switch in the card's Properties (§3.8).

They use what the design system has: the top tabs (`.tabs.task-tabs`), badges, `FormSection` and `Field` with hints, Lucide icons (`images` is new in the icon map), the tokens, dark first and light. No teal or magenta: nothing here is an agent at work or "needs you" (`design/README.md:35-38`). [V for the parts]

### 5.2 The data

Migration `0018_screenshots.sql` (SB1; the preview's columns come with SB2):

```sql
-- Screenshots (GA-173): a run's pictures on its card keep a caption and their size; the run says what it left out.
ALTER TABLE files ADD COLUMN caption TEXT;
ALTER TABLE files ADD COLUMN width INTEGER;
ALTER TABLE files ADD COLUMN height INTEGER;
CREATE INDEX files_run ON files(run_id) WHERE run_id IS NOT NULL;
ALTER TABLE runs ADD COLUMN screenshots_note TEXT;   -- JSON {"leftOut": ["13-x.png: over the limit of 12 a run"], "preview": null}
ALTER TABLE tasks ADD COLUMN screenshots INTEGER;    -- NULL: the project's switch; 0 or 1: the card's own
ALTER TABLE projects ADD COLUMN screenshots_json TEXT; -- {"on":true,"sizes":[{"name":"Desktop","width":1280,"height":800}],"bothThemes":false,"fullPage":false}
```

SB2 adds `projects.preview_command TEXT` and `projects.preview_start_secs INTEGER` (`0019_preview.sql`). The agent's switch is a field of `CliTools` in its `mcp_extra_json` (`screenshots`, next to `insecure_certs`: `mcp_servers.rs:104-118`), so no column. The app-wide values are keys of the Settings struct (`src-tauri/src/runs.rs:168-214`), checked in `save_settings` (`:234-252`). [I]

### 5.3 The run, step by step

1. **Start** (SB2): after the worktree is prepared, when the agent takes screenshots and the card's or project's switch is on: pick the ports, start the preview command, wait for its port (at most the start time).
2. **Prompt** (SB1, SB2): the Screenshots section (§5.4), with the address, the sizes, the themes and the limit.
3. **The agent works**, then shoots its set into `.gizai-tmp/screenshots/` and ends with its result line.
4. **End** (`src-tauri/src/runs.rs:1139-1147`): the CLI's group has ended → end the preview's tree → **collect** (`gizai_agents::screenshots::collect`, the checks of §3.4, no database) → store the blobs and insert the rows with the run's id (`gizai_core::files::insert_evidence`) → note what was left out on the run → `empty_temp` as today → `finish_run`. A run Gizai's quitting stopped collects nothing (its folder is emptied at the next start).
5. **Housekeeping** (daily, `housekeeping.rs`): older sets than the newest N per card go, a finished card's go after the days set, then blobs no row uses.

### 5.4 What the agent is told

A section after the task, only when the agent's switch, the card's (or project's) switch and the browser are on (`prompt.rs`, SB1). For a builder, with the project's values filled in:

```
## Screenshots

When your change shows on a screen, take screenshots of it after your last commit and before your result line: the card
shows them in Review, before the pull request.
- The card's branch is served at http://127.0.0.1:47130/ (Gizai started the project's preview; its output is in
  /…/GA-180/.gizai-tmp/preview/preview.log). Use only this address: never a .test site, a live site or a real account.
- Shoot with the browser's take_screenshot, always with filePath, as /…/GA-180/.gizai-tmp/screenshots/<nn>-<what>.png,
  numbered in the order to look at them: the screens you changed, in the states the card names, and their empty and error
  states when you touched them; each at Desktop 1280 x 800 and Mobile 390 x 844 (resize_page or emulate's viewport).
- At most 12. Write a caption for each in /…/GA-180/.gizai-tmp/screenshots/captions.json, like
  {"01-board-desktop.png": "The board with three cards"}.
- Don't open your pictures unless you must: each one in your conversation costs tokens on every later turn. Check what a
  page says with take_snapshot.
- Never commit them or attach them to the pull request: Gizai keeps them on the card.
```

QA's version replaces the third line with: "Add only what the builder's screenshots don't show: each failure you found, and at most four acceptance criteria you checked, each captioned with the criterion." Without a preview the first line says "There is no preview for this project: take no screenshots." (or, when the preview didn't start, why). [I]

### 5.5 What changes in which files

| File | Change | Card |
|---|---|---|
| `crates/gizai-core/migrations/0018_screenshots.sql` | new (§5.2) | SB1 |
| `crates/gizai-core/src/files.rs` | `insert_evidence`; `screenshots(task)` grouped by run; `list` (Attachments) only rows without a run; `prune_screenshots` and the blob sweep | SB1 |
| `crates/gizai-core/src/housekeeping.rs` | call the prune and sweep daily | SB1 |
| `crates/gizai-core/src/mcp_servers.rs`, `model.rs`, `projects.rs`, `tasks.rs` | the agent's `screenshots` switch, the project's `screenshots_json`, the card's `screenshots` | SB1 |
| `crates/gizai-agents/src/screenshots.rs` | new: `collect(worktree, limits)`, the checks, image headers | SB1 |
| `crates/gizai-agents/src/prompt.rs` | `screenshots_section` | SB1 (the address line SB2) |
| `crates/gizai-agents/src/preview.rs` | new: start in its own tree, wait for the port, end | SB2 |
| `src-tauri/src/runs.rs` | collect before `empty_temp` (`:1146`); the preview's start and end; the ports; the Settings fields and checks | SB1, SB2 |
| `src-tauri/src/commands.rs`, `lib.rs` | `list_screenshots`, `screenshot_bytes` | SB1 |
| `src/api.ts`, `src/types.ts` | contract CS (§6) | SB1 |
| `src/router.ts`, `src/pages/TaskPage.tsx` | `view: "screenshots"`; the third tab | SF1 |
| `src/components/screenshots/` | new: `ScreenshotsView.tsx`, `ShotViewer.tsx` | SF1 |
| `src/components/Board.tsx`, `src/pages/TasksPage.tsx` | the sign; the Inbox's line | SF1 |
| `src/components/ProjectForm.tsx`, `AgentCliTools.tsx`, `Properties.tsx`, `src/pages/SettingsPage.tsx` | the settings of §3.8 | SF2 |
| `browser.html`, `src/browser/` (fake backend), `crates/gizai-core/examples/fixtures.rs`, `vite.config.ts` | Gizai's screens in a plain browser | SG |
| `design/README.md`, `docs/features/screenshots.md`, `docs/settings.md` | the icon; the feature; the settings | SD, SF2 |

### 5.6 Security and privacy

- The checks on each file (§3.4) and the privacy rules (§3.6). `screenshot_bytes` serves only rows that are a run's images, by id, never a path.
- The preview command is what you typed in the project form, as the setup command is: the agents' and the Team Lead's tools can't change it (no Team Lead tool touches the project's worktree fields today, and none gets the preview ones). It runs with your rights, like the setup command (`prepare.rs:143-144`).
- The preview listens on `127.0.0.1` where its stack allows (OO's API listens on all addresses: §3.5); its data is demo data.
- Nothing new leaves the computer. [I]

### 5.7 How it is tested

- **Rust, `gizai-agents`:** `collect` keeps PNG, JPEG and WebP by their signature and refuses the rest (an SVG, a text file named `.png`); skips links and, on Unix, files with more than one hard link (`#[cfg(unix)]`); ignores subfolders; keeps the first N by name; refuses too many bytes or pixels; reads `captions.json`, and a broken one only loses captions. Image headers from tiny fixture files.
- **Rust, `gizai-core`:** `insert_evidence` sets the run, caption and size; Attachments leave run files out; `screenshots(task)` gives sets newest first; the prune keeps N sets and the days; the sweep deletes only blobs no row uses.
- **A run end to end:** a fake Claude Code script (`crates/gizai-agents/tests/fake-claude*.sh`) that writes two PNGs and a `captions.json` into the folder: the card has two screenshots with the run's id; a run stopped at its time limit keeps its pictures; the prompt has the section only with the switches on.
- **Preview (SB2):** a fake preview (`node -e` serving on `GIZAI_PREVIEW_PORT`): the agent starts after the port answers and its prompt has the address; three runs at once get different ports; after the run the server's PID is gone; one that never listens lets the run start after the start time, with the note; on Windows its Job Object ends it.
- **UI:** vitest for the sets, counts and Compare's pairing by name and hash, and a contract test like `src/changesContract.test.ts`; a `scripts/ui-test.sh` scenario with a demo screenshot set: the tab, the larger view, Esc.
- **By hand, once each:** a GW card and an MN card with the browser on, then the tab in Review. [I]

### 5.8 What is left out

Gizai re-shooting every page at every size (option B); **Add to pull request** with `gh pr comment --attach`; a Preview button for you on the card; QA's failure pictures handed to the builder's next run; videos (as Devin and Cursor make); a pixel diff (Compare is side by side); screenshots by Codex and Gemini agents (no browser for them: `mcp_not_on`); Bitbucket's pull requests.

**Free,** like everything in the desktop app (`docs/PAID-FEATURES.md:30-37`).

---

## 6. Proposed cards

In order. Each is one pull request that fits in one agent run. **At the same time:** SD, SB1, SB2 and SG (SB2 merges after SB1: it adds the address line to SB1's prompt section). **Then:** SF1 (after SD and SB1), SB3 (after SB1) and SF2 (after SD, SB1 and SB2) at the same time. **Then** each project's preview script, on its own board (after SB2), and you set its Preview command.

### SD. Design: the Screenshots tab, its larger view, the board's sign and the settings

- **Column:** Design. **Role:** design. **Builds on:** nothing.
- **Builds:** the screens of §3.7, §3.8 and §5.1, as mockups in `design/screenshots/`.
- **Acceptance criteria:**
  - [ ] The tab: empty, one set, three sets (the older folded), a set with files left out, a run whose preview didn't start.
  - [ ] The larger view: fitted and at 100%, ‹ ›, Compare side by side and with a slider, Unchanged and New.
  - [ ] The board card in Review with its sign; the Inbox's line.
  - [ ] The project form's Preview section, the agent form's switch, the Runs rows and the card's switch.
  - [ ] Dark and light, with the design system's tokens and components; `images` added to the icon map in `design/README.md`.

### SB1. Backend: a run's screenshots onto its card

- **Column:** To do. **Role:** backend. **Builds on:** nothing.
- **Builds:** §3.4 B (the collect step and its checks), migration `0018` (§5.2), `files::insert_evidence` and the Attachments filter, contract **CS**, the prompt's Screenshots section (§5.4) and the switches and limits of §3.8 that aren't the preview's or the keeping's: the project's Screenshots, Sizes, Both themes and Whole page; the agent's switch; the card's switch; Screenshots per run; Largest screenshot.
- **Acceptance criteria:**
  - [ ] At a run's end (any end but Gizai quitting), the images in its `.gizai-tmp/screenshots/` are on the card with the run's id, their caption, width and height, before the folder is emptied as today.
  - [ ] The checks of §3.4: that folder only, no links, images by their content, the limits; what is left out is said in the run's output and in `leftOut`.
  - [ ] Attachments no longer list a run's files.
  - [ ] `list_screenshots` and `screenshot_bytes` as in contract CS; `screenshot_bytes` refuses any other file.
  - [ ] The settings above, with the defaults and ranges of §3.8, refused outside them (contract CP).
  - [ ] The Screenshots section is in the prompt only when the agent's switch, the card's (or project's) switch and the browser are on; QA gets its own version.
  - [ ] The tests of §5.7 (Rust, the fake run); `cargo test --workspace`, `npm test` and `npm run build` pass.

### SB2. Backend: the project's preview, started and ended by Gizai

- **Column:** To do. **Role:** backend. **Builds on:** SB1 (its prompt section); can be built at the same time and merged after it.
- **Builds:** §3.5's How it works: the project's Preview command and Start time (`0019_preview.sql`), Settings → Runs' Preview ports, the ports per run, the start, the wait and the end, the address in the prompt.
- **Acceptance criteria:**
  - [ ] For a run whose agent takes screenshots, with a preview command: a free block of ten ports from the range, no other live run's; the command runs with `bash -c` (Git Bash on Windows) in the worktree, in its own tree, with `GIZAI_PREVIEW_PORT`, `GIZAI_PREVIEW_URL`, `GIZAI_PREVIEW_DIR` and `PORT`, its output in `.gizai-tmp/preview/preview.log`.
  - [ ] The agent starts once the port answers, at most the start time later; its prompt names the address, or why there is none. The card is never held for a preview.
  - [ ] When the run ends, the preview's whole tree is ended before the screenshots are collected (SIGTERM, SIGKILL after 3 s; Windows' Job Object).
  - [ ] The agent's own commands get `GIZAI_PREVIEW_URL` and `GIZAI_PREVIEW_DIR` too.
  - [ ] Start time 10–600 s (120) and the ports (47100–47999; at least ten for each run at once) are refused outside their ranges.
  - [ ] Tests of §5.7: the order, three runs at once, a preview that never answers, the server gone after the run.

### SG. Frontend: Gizai's screens in a plain browser, with a fake backend

- **Column:** To do. **Role:** frontend. **Builds on:** nothing.
- **Builds:** §3.5's GA way: `browser.html` and `src/browser/` (Tauri's `mockIPC` with `shouldMockEvents`, `mockWindows`), fixtures written from the demo data by a Rust example (`crates/gizai-core/examples/fixtures.rs`), and `vite --mode browser`. Then the GA project's Preview command is `npx vite --mode browser --host 127.0.0.1 --port "$GIZAI_PREVIEW_PORT" --strictPort`.
- **Acceptance criteria:**
  - [ ] It serves the board, a card with its Changes, the team, Settings and Memory with the demo data in Chrome, on Linux, macOS and Windows.
  - [ ] A command without a fixture answers empty and says which in the console; events work.
  - [ ] The app's own build contains none of it (a test checks `dist/`).
  - [ ] `t` switches to light; `npm test` checks the fixtures still match `src/types.ts`'s shapes.

### SF1. Frontend: the Screenshots tab

- **Column:** To do. **Role:** frontend. **Builds on:** SD, SB1 (contract CS).
- **Builds:** §3.7 as SD designed it.
- **Acceptance criteria:**
  - [ ] Card | Changes | Screenshots with the newest set's count, at `#/task/<id>/screenshots` and `…/screenshots/<run>`.
  - [ ] The sets newest first, with the agent, its answer, when, the count and what was left out; older sets folded.
  - [ ] The larger view with ‹ ›, 100%, Open and Esc; Compare with the previous run by file name, Unchanged by hash.
  - [ ] A Review card on the board shows the sign and count; the Inbox's line opens the tab.
  - [ ] Pictures come through `screenshot_bytes` as `blob:` URLs, released when they leave the screen.
  - [ ] Dark and light as designed; vitest, a contract test and a `scripts/ui-test.sh` scenario.

### SB3. Backend: old screenshots go

- **Column:** To do. **Role:** backend. **Builds on:** SB1.
- **Builds:** §3.6's When they go: Keep and Delete after (contract CP), and the blob sweep.
- **Acceptance criteria:**
  - [ ] Housekeeping (at start and daily) removes the sets past the newest N per card and a finished card's screenshots after the days set (0 keeps them).
  - [ ] Then it deletes the blobs no row uses (any kind of file), never one a row still uses.
  - [ ] Keep 3 (1–20) and Delete after 30 days (0–365), refused outside them; tests.

### SF2. Frontend: the screenshot and preview settings

- **Column:** To do. **Role:** frontend. **Builds on:** SD, SB1, SB2 (contract CP).
- **Builds:** the settings of §3.8 in the project form, the agent form, Settings → Runs and the card's Properties; the docs.
- **Acceptance criteria:**
  - [ ] The project form's Preview section: Screenshots, Preview command, Start time, Sizes (one to four), Both themes, Whole page, each with its hint.
  - [ ] The agent form, under the browser: "Take screenshots of what it built", only with the browser on, its default by role.
  - [ ] Settings → Runs: per run, largest, keep, delete after, preview ports; the card's "Screenshots before Review".
  - [ ] `docs/settings.md`, a new `docs/features/screenshots.md`, and screenshots in the free list of `docs/PAID-FEATURES.md`.

### On the other projects' boards (after SB2)

| Card | Role | Builds |
|---|---|---|
| MN: `composer preview` for Gizai | backend | §3.5's script with `DatabaseSeeder`, `CollectionDemoSeeder` and `TouchscreenDemoSeeder`; its CLAUDE.md says Gizai's preview is the exception to "never serve" |
| QC: `composer preview` for Gizai | backend | the script, with the demo seeders, both builds and a throwaway Twill superadmin |
| YO: demo data, then `composer preview` | backend | a demo seeder for rosters and visits, then the script with 2FA off for the preview |
| OO: `npm run preview:gizai` | backend | the API serving the dashboard's build on one port, a throwaway owner, `DB_PATH` in the preview folder |
| GW | none | its Preview command needs no change in the repository: `npx astro dev --host 127.0.0.1 --port "$GIZAI_PREVIEW_PORT" --ignore-lock` |

### Contract CS: SB1 → SF1 (screenshots)

`src/api.ts`:

```ts
/** The card's screenshot sets, newest run first: each run that saved pictures, left some out, or whose preview didn't
 *  start. They arrive when a run ends, before "runs-changed": reload on onRunsChanged. */
export const listScreenshots = (taskId: string) => invoke<T.ScreenshotSet[]>("list_screenshots", { taskId });
/** One screenshot's bytes (PNG, JPEG or WebP), for a blob: URL. Any id that isn't a run's screenshot is refused. */
export const screenshotBytes = (fileId: string) => invoke<ArrayBuffer>("screenshot_bytes", { fileId });
```

`src/types.ts`:

```ts
export type ScreenshotSet = {
  runId: string;
  agentId: string;
  agentName: string;
  role: string | null;           // "frontend", "design", "qa", …
  outcome: string | null;        // "ready_for_testing", "qa_pass", "qa_fail", "needs_decision"; null without a result line
  status: "succeeded" | "failed" | "cancelled" | "timed_out";
  endedAt: number;               // ms since 1970
  shots: Screenshot[];           // by file name
  leftOut: string[];             // "13-board.png: over the limit of 12 a run", "notes.txt: not an image"
  preview: string | null;        // why there was no preview, when there wasn't: "It didn't answer on port 47130 in 120 s"
};
export type Screenshot = {
  fileId: string;
  name: string;                  // "01-board-with-cards-desktop-dark.png"
  caption: string;               // from captions.json, else made from the name
  width: number;
  height: number;
  sizeBytes: number;
  sha256: string;                // the same picture in two runs has the same hash: Compare says "Unchanged"
};
```

In Rust: `list_screenshots(task_id: String) -> Vec<ScreenshotSet>` and `screenshot_bytes(file_id: String) -> tauri::ipc::Response` (the raw bytes, which arrive as an `ArrayBuffer`). Errors are plain sentences, like the rest of the app.

### Contract CP: SB1, SB2, SB3 → SF2 (settings)

```ts
// T.Settings, new fields (Settings → Agents and runs → Runs); save_settings refuses values outside the ranges.
screenshotsPerRun: number;          // 12, 1–30                        (SB1)
screenshotMaxMb: number;            // 10, 1–50                        (SB1)
screenshotsKeepSets: number;        // 3, 1–20                         (SB3)
screenshotsKeepDays: number;        // 30, 0–365; 0 keeps them         (SB3)
previewPortFrom: number;            // 47100                           (SB2)
previewPortTo: number;              // 47999; 1024–65535, at least 10 × maxConcurrentRuns ports (SB2)

// T.Project and T.ProjectInput
screenshots: ProjectScreenshots;    // (SB1)
previewCommand: string | null;      // null or "": no preview          (SB2)
previewStartSecs: number;           // 120, 10–600                     (SB2)
export type ProjectScreenshots = {
  on: boolean;                                                // true
  sizes: { name: string; width: number; height: number }[];   // 1–4 sizes; width 320–3840, height 320–2400;
                                                              // default Desktop 1280×800 and Mobile 390×844
  bothThemes: boolean;                                        // false
  fullPage: boolean;                                          // false
};

// T.CliTools (agent form → Tools)
screenshots: boolean;               // only with the browser on; a new agent's default: true for frontend, design and qa (SB1)

// T.Task, and updateTask's patch
screenshots: boolean | null;        // null: the project's switch       (SB1)
```

---

## 7. Open questions for Jefsev

1. **Who takes them.** Recommended: the builder on every card that changes a screen, and QA only its additions (failures; at most four criteria the builder's set doesn't show). Or QA only (then cards with testing off get none), or both full sets (twice the cost and disk for the same pictures)?
2. **How they reach the card.** Recommended: the fixed folder `.gizai-tmp/screenshots/` with `captions.json`, collected at every run's end. Or a `screenshots` list in the result line, which a stray quote can spoil?
3. **Who serves the page.** Recommended: Gizai, from the project's Preview command, on a port it picks, ended with the run; the Laravel projects' "Never run commands to serve the site" stays as it is for agents. Or should agents serve it themselves (each project's rules changed, servers that may detach)?
4. **Demo data only.** Recommended: previews always use a throwaway database with the seeders, never the `.env` one, so no client data is ever in a screenshot. Is there a project where the seeders can't show what matters, where you'd rather have a copy of real data (and accept it in screenshots)?
5. **Sizes and themes.** Recommended: websites at Desktop 1280 × 800 and Mobile 390 × 844; Gizai at 1440 × 900 and 900 × 600 in both themes; each changed screen at each size, the main screen in the other theme. Different sizes, or every screen in both themes?
6. **Limits.** Recommended: at most 12 a run, 10 MB each. More or fewer?
7. **Keeping.** Recommended: the newest 3 sets per card, and all of a card's screenshots 30 days after Done. Longer?
8. **Gizai's own screens.** Recommended: a browser mode with a fake backend (SG), drawn by Chrome, on all three systems; `scripts/shot-cage.sh` stays for the README, the tour and QA's real-app check on Linux. Is Chrome's drawing close enough for your review?
9. **The pull request.** Recommended: screenshots never go to GitHub by themselves; later an **Add to pull request** button per set (gh 2.99 `--attach`, as a PR comment; private repositories show them only to people with access). Wanted, and for which projects?
10. **Signs.** Recommended: the count on the tab, a sign on the board only in Review, and a line in the Inbox. Also on the other columns?
11. **YO's data.** Its seeders make users and shift types only. Recommended: a YO card for demo rosters and visits before its screenshots are switched on.
12. **`.claude/launch.json`.** OO (and its worktrees) have one from your Claude Desktop sessions. Recommended: the project form offers its configurations as the Preview command when one is there (a small addition to SF2 and SB2). Wanted?

---

## 8. Sources

**The repository**, read on 2026-10-14 in this card's worktree at `24535e3` (`origin/main`, Gizai 0.11.0): every `path:line` above. Commands: `git log`, `git rev-parse`, `rg`, `cargo tree -i image@0.25.10`, `npm view chrome-devtools-mcp@1.10.1 license version dist.unpackedSize`. `node_modules/@tauri-apps/api` 2.12.1 (`mocks.d.ts`) in the worktree.

**The other projects**, read-only on 2026-10-14 in their main checkouts: `~/Herd/gizai-website` (GW), `~/Herd/spoorwegmuseum` (MN), `~/Herd/quiz-cms` (QC), `~/Herd/stapsgewijz` (YO) and `~/Herd/otus-os` (OO), found by their Gizai worktrees' names in `.git/worktrees/`. Their `.env` files were not opened. A helper agent swept them; the facts this document leans on were checked again by hand: the Herd rule (`CLAUDE.md:92`, `CLAUDE.md:91`, `AGENTS.md:159`), the `.env.example` databases, `ServeCommand.php:183-186` and `:451`, Astro's `cli/server.js:116-130`, `cli/agent.js:10-14`, `cli/preview/index.js:24-45` and `am-i-vibing`'s detector, YO's `EnsureTwoFactorIsConfirmed.php:33-36`, MN's `AdminUserSeeder.php:19`, OO's `.claude/launch.json`, `vite.config.ts:12-17`, `src/api.ts:66` and `main.ts:113`, and QC's `design/kennisquiz/README.md:145-197`.

**Outside sources**, all read on 2026-10-14. "By hand" = re-read word for word in this run; the rest a helper agent read, quoting word for word.

| What | Link | Read |
|---|---|---|
| Claude Code desktop docs: Browser pane, `.claude/launch.json`, auto-verify, Dev servers | https://code.claude.com/docs/en/desktop.md | by hand |
| Claude Code on desktop, app preview (20 Feb 2026) | https://claude.com/blog/preview-review-and-merge-with-claude-code | helper |
| What's new, week 28 of 2026: the built-in browser | https://code.claude.com/docs/en/whats-new/2026-w28.md | helper |
| Claude Code desktop on Linux (beta) | https://code.claude.com/docs/en/desktop-linux.md | helper |
| Claude in Chrome (`--chrome`; off with an API key or setup-token) | https://code.claude.com/docs/en/chrome.md | helper |
| Built-in browser in Claude Cowork (same capabilities as Claude in Chrome) | https://support.claude.com/en/articles/16607400-use-the-built-in-browser-in-claude-cowork | helper |
| Issue #94503: the Browser pane's screenshot has no `filePath` | https://github.com/anthropics/claude-code/issues/94503 | by hand |
| Issue #96717: `zoom` "region crop not yet supported in the Browser pane" (Windows, Desktop 2.7032.0), cropping since 8 Oct | https://github.com/anthropics/claude-code/issues/96717 | by hand |
| Issues #92046, #62653, #30122, #98288: the pane's tool names, SSH sessions | https://github.com/anthropics/claude-code/issues/92046, https://github.com/anthropics/claude-code/issues/62653, https://github.com/anthropics/claude-code/issues/30122, https://github.com/anthropics/claude-code/issues/98288 | helper |
| Vision: visual tokens, resolution tiers, examples, Opus 5.5's price, 20-image rule, no training | https://platform.claude.com/docs/en/build-with-claude/vision.md | by hand |
| Pricing: Opus 5.5 $4 / MTok, Sonnet 5.5 $2 / MTok input | https://platform.claude.com/docs/en/about-claude/pricing.md | helper (Opus 5.5's price also by hand, on the vision page) |
| chrome-devtools-mcp 1.10.1, `take_screenshot` with `filePath` | https://raw.githubusercontent.com/ChromeDevTools/chrome-devtools-mcp/chrome-devtools-mcp-v1.10.1/src/tools/screenshot.ts | by hand |
| chrome-devtools-mcp 1.10.1, tool reference and flags (`resize_page`, `emulate`, `--viewport`) | https://github.com/ChromeDevTools/chrome-devtools-mcp/blob/chrome-devtools-mcp-v1.10.1/docs/tool-reference.md, https://github.com/ChromeDevTools/chrome-devtools-mcp/blob/chrome-devtools-mcp-v1.10.1/docs/configuration.md | helper |
| GitHub: attaching files (who can see them, 10 MB images) | https://docs.github.com/api/article/body?pathname=/en/get-started/writing-on-github/working-with-advanced-formatting/attaching-files | by hand |
| GitHub CLI: attaching files (`--attach`, push access) | https://docs.github.com/api/article/body?pathname=/en/github-cli/github-cli/attaching-files-with-github-cli | by hand |
| GitHub changelog: gh v2.99.0, media in issues and pull requests (1 Sep 2026) | https://github.blog/changelog/2026-09-01-github-cli-media-in-issues-pull-requests-and-comments/ | by hand |
| GitHub changelog: Copilot coding agent's own web browser (2 Jul 2025) | https://github.blog/changelog/2025-07-02-copilot-coding-agent-now-has-its-own-web-browser/ | by hand |
| GitHub docs: Copilot coding agent and MCP (Playwright, localhost only) | https://docs.github.com/en/copilot/concepts/agents/coding-agent/mcp-and-coding-agent | helper |
| Devin: testing and recordings | https://docs.devin.ai/work-with-devin/testing-and-recordings | helper |
| Cursor changelog (24 Feb 2026) and cloud agent capabilities | https://cursor.com/changelog/02-24-26, https://cursor.com/docs/cloud-agent/capabilities | helper |

Not checked, so marked [I] where used: how long each preview takes to start and a set to shoot (nothing was run), PNG sizes per project, whether Laravel's `.env` loader leaves an already-set variable alone in these projects, whether a run's rules allow `SHOT_SIZE=… scripts/shot-cage.sh`, Claude Code's cache prices (from GA-171's notes, not re-read), and Anthropic's own list of the Browser pane's tools (none is published; the names come from users' issues).



