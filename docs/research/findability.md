# Research: make GizAi easier to find and understand (GA-167)

R&D for Jefsev, 13 October 2026. Research and drafts only: the build cards come after your OK.

- **[V]** means verified in the code or in a primary source, named next to it.
- **"helper"** next to a source means one of this run's research helpers read and quoted it, and I didn't open it
  again. I did re-read the sources that decide something: Anthropic's legal page, Google's post on Gemini CLI, Search
  Central's changelog, giz.ai, the GitHub API, npm, GitHub's topic rules, and the source of `gh repo edit` and
  `gh release create`.
- **[I]** is my judgement.
- Every web page was read on 2026-10-13.

## 1. Summary

- **What visitors see now:** the About still says "Giz AI", with no website and no topics. The README's first screen
  doesn't say who GizAi is for. The repository has no community files: GitHub rates its health at 50%.
  gizai.app already has a sitemap, robots.txt and SoftwareApplication data, but no llms.txt, and its page titles hold
  no search words. [V] (GitHub API, live site)
- **The biggest finding:** another company owns the name in search. Giz Inc. (giz.ai, "GizAI") is first for every
  spelling (the helper's searches). It holds github.com/GizAI and npm `@gizai`, and it now sells a CLI and MCP server for Claude Code and Codex.
  So "Giz AI" with a space is the worst spelling. One spelling, "GizAi", always next to "desktop app", "AI coding
  agents" or gizai.app, is the fix. A trademark search is worth doing before more goes into the name (§3.5).
- **What the brief gets wrong** (§3.0):
  - The team: a new install has a Team Lead, Backend, Frontend, QA and **DevOps**. Design is a role you add, and R&D is
    an agent with your own instructions.
  - The names: Gemini CLI now serves only paid API keys, and OpenAI calls its terminal tool Codex CLI.
  - A page titled "Claude Code kanban" breaks Anthropic's naming rule.
  - SoftwareApplication data and a sitemap exist already. FAQ results are gone from Google since 7 May 2026.
- **Recommended:** two pull requests and a list of settings you change by hand.
  - **GA:** the README's first screen, "GizAi" in the docs, three issue forms, CONTRIBUTING.md, SECURITY.md, and release
    notes with a short "What's new".
  - **GW:** llms.txt, search-shaped titles and descriptions, WebSite data, and one Remotion still that makes both share
    images. It lands after GW-7.
  - **The app** keeps "Gizai" in its UI and identifiers until a rebrand card of its own (§3.4).
- **Run this for me (now):** one `gh repo edit` that sets the About, the website and 20 topics (§3.1).

## 2. The project now

- **GizAi** (this repository, public, Apache-2.0) is a Tauri 2 desktop app:
  - core logic in Rust crates (`crates/gizai-core`, `gizai-agents`, `gizai-mcp`), a thin adapter in `src-tauri/`, and a
    React UI in `src/`;
  - local SQLite in the data folder;
  - built from source by `install.sh` and `install.ps1`, CI on Linux, macOS and Windows (`.github/workflows/ci.yml`),
    and releases as `vX.Y.Z` tags on `production` (`docs/RELEASING.md`). [V]
- **What a user sees about it:**
  - the README, with `docs/features/*.md` behind it;
  - the GitHub About;
  - the release notes, which also show in the app: Settings → Updates → "What's new in X.Y.Z" renders the release body
    as Markdown (`src/components/UpdateSettings.tsx:33-34`), cut at 20,000 characters (`crates/gizai-agents/src/update.rs:79`). [V]
- **gizai.app** is GW (`Yotech-AI/gizai-website`, private, Astro, Cloudflare Pages). Its main has GW-1 to GW-4 (PRs #1
  to #4, merged).
  - GW-7 (PR #5, **open**) adds `/roadmap` and `/docs`. `/docs` holds copies of this repository's
    `docs/install.md`, `settings.md`, `features/*.md`, `agent-tools.md` and `memory.md`, made by `npm run docs:sync`
    (`src/data/docs.ts` in PR #5). [V] (`gh pr diff`)
  - So the /roadmap and /docs work is one open pull request (GW-7), and the docs' spelling will show on the website.
- **What this card touches:** README, `docs/`, new files in `.github/` and the root, `docs/RELEASING.md`; in GW
  `src/data/site.ts`, `src/content.config.ts` and `src/content/features/*.md`, `src/layouts/Feature.astro`,
  `src/pages/index.astro`, a new `llms.txt` route and a share-card render in `video/`. No app code.

## 3. The feature: each item of the brief

### 3.0 Where the brief's assumptions don't hold

| Brief says | What is true | Source |
|---|---|---|
| "a Team Lead, builder, QA, Design and R&D agents" | A new install starts with five: Team Lead, Backend, Frontend, QA and DevOps. Design is a built-in role with its own instructions that you add. R&D is not a role GizAi has: an agent with any other role gets the Backend Agent's instructions under its own name, and you write its job. | `crates/gizai-core/src/seed.rs:32-35`, `:195-228`; `docs/features/board.md:20`, `:32-34` [V] |
| "Claude Code, Codex or Gemini" | OpenAI calls the terminal tool "Codex CLI", and Google calls its tool "Gemini CLI". Since 18 June 2026, Gemini CLI serves only paid Gemini API keys; Google points everyone else to Antigravity CLI. | the READMEs of github.com/openai/codex and google-gemini/gemini-cli (helper); Google Developers Blog, 19 May 2026 [V] |
| "Free, local, open source" | Free and open source, yes. But "local" reads as local models: the agents' models run at Anthropic, OpenAI or Google through the CLIs. Say "on your computer, with your own logins". | `docs/features/cli.md:13-20` [V]; wording [I] |
| Topic `local-first` | That word means apps that sync local data without a server (CRDTs). It promises more than GizAi does, so drop it. | [I] |
| A page titled "Claude Code kanban" | Anthropic: "you can't use the Claude Code or Anthropic names or logos as part of your own product, feature, or company name". A title may say that GizAi runs Claude Code. | code.claude.com/docs/en/legal-and-compliance.md [V] |
| Add SoftwareApplication data and a sitemap | Both exist: `src/pages/index.astro` (GW #1), and gizai.app/sitemap.xml with 9 addresses. Missing: WebSite data, llms.txt and search words in the titles. | GW PR #1 diff; live site [V] |
| Social preview "based on og.png" | og.png is 1200 × 630, and GitHub wants 1280 × 640 (2 : 1), so it needs a render of its own, not a copy. | GW `src/layouts/Base.astro`; GitHub docs [V] |
| GW-7 builds /roadmap, GW-8 builds /docs | GW PR #5 ("GW-7") builds both. It is open. | `gh pr list --repo Yotech-AI/gizai-website` [V] |

### 3.1 The repository's About, website and topics

**Now:** description "Giz AI", homepage `null`, topics `[]`. Discussions are off; Issues, Projects and the Wiki are on;
1 star. [V] (api.github.com/repos/Yotech-AI/gizai)

**The About line (286 characters):**

> Desktop app where AI coding agents work your kanban board in parallel on Claude Code, Codex CLI or Gemini CLI: a Team
> Lead you chat with, builders, QA, DevOps and roles you add, like Design or R&D. Each card gets its own git worktree.
> Built-in editor and terminal. Free and open source.

Why this wording:

- GitHub's page title is "GitHub - Yotech-AI/gizai: <About>". So the first words are what search results show, and they
  say what GizAi is: a desktop app, AI coding agents, a kanban board, in parallel. [I]
- Each claim is in the docs:
  - in parallel: `board.md:39`, `:43`;
  - the CLIs: `cli.md:7`;
  - the team: `seed.rs:35`, `:222`, `:224-228`;
  - the worktree: `board.md:47`;
  - the Editor and Terminal: `editor-and-terminal.md:3`;
  - free and open source: `README.md:112-114`. [V]
- The CLIs appear in the plain-text form Anthropic and Google allow: GizAi "runs Claude Code". [V] (§3.6)
- It holds no apostrophe, so it fits in the single-quoted command below.
- **The length:** GitHub documents no limit for the description. The 350 characters comes only from third parties. [V]
  (GitHub REST docs, "Update a repository"; helper)

**Topics (20, GitHub's maximum: "Use lowercase letters, numbers, and hyphens. Use 50 characters or less. Add no more
than 20 topics.")** [V] (docs.github.com, "Classifying your repository with topics")

`claude-code, codex, gemini-cli, ai-agents, coding-agents, multi-agent, agentic-coding, agent-orchestration, kanban,
git-worktree, mcp, code-review, developer-tools, desktop-app, tauri, rust, linux, macos, windows, omarchy`

- **From the brief:** 18 of its 19 topics.
- **Dropped:** `local-first` (§3.0).
- **Added:** `mcp` (agents get MCP servers, `docs/features/tools.md:11`, and the Team Lead's tools are an MCP server,
  `gizai-mcp`) and `agent-orchestration`, which the lists of tools like GizAi use.
- **How many public repositories use each** (GitHub search, 13 Oct 2026) [V]:

  | Topic | Repositories |
  |---|---|
  | mcp | 94,567 |
  | claude-code | 90,713 |
  | multi-agent | 19,113 |
  | code-review | 9,030 |
  | coding-agents | 6,173 |
  | agent-orchestration | 4,174 |
  | kanban | 4,112 |
  | omarchy | 3,950 |
  | agentic-coding | 2,407 |
  | git-worktree | 1,330 |

  The rest went unchecked: the search API's rate limit answered 403.
- **What that means:** the big topics bring searchers, and the small ones (`git-worktree`, `agentic-coding`, `omarchy`)
  are where a new repository can show up near the top. [I]
- **The brand rules:** `claude-code`, `codex` and `gemini-cli` are descriptive tags, fine under the plain-text rules.
  Google's own repository uses `ai-agents` and `mcp-server`, not `gemini-cli`. [V] (google-gemini/gemini-cli; helper)

**Run this for me** (the flags are from GitHub CLI's manual and source: `gh repo edit [<repository>]`, `-d/--description`,
`-h/--homepage`; `--add-topic` takes a comma-separated list and adds to the topics there are) [V]
(cli/cli `pkg/cmd/repo/edit/edit.go`: `Use: "edit [<repository>]"`, `StringSliceVar(&opts.AddTopics, "add-topic", …)`,
and `newTopics.AddValues(opts.topicsCache)` then `AddValues(opts.AddTopics)`; the manual, helper):

```sh
gh repo edit Yotech-AI/gizai --description 'Desktop app where AI coding agents work your kanban board in parallel on Claude Code, Codex CLI or Gemini CLI: a Team Lead you chat with, builders, QA, DevOps and roles you add, like Design or R&D. Each card gets its own git worktree. Built-in editor and terminal. Free and open source.' --homepage 'https://gizai.app' --add-topic 'claude-code,codex,gemini-cli,ai-agents,coding-agents,multi-agent,agentic-coding,agent-orchestration,kanban,git-worktree,mcp,code-review,developer-tools,desktop-app,tauri,rust,linux,macos,windows,omarchy'
```

It changes only these three things. After it, the GitHub API should show the new description,
`"homepage": "https://gizai.app"` and the 20 topics. I check that once you've run it, and note it here.

### 3.2 Social preview

- **GitHub's rules** [V] (docs.github.com, "Customizing your repository's social media preview"; helper):
  - "a PNG, JPG, or GIF file under 1 MB … at least 640 by 320 pixels (1280 by 640 pixels for best display)";
  - "Until you add an image, repository links expand to show basic information about the repository and the owner's
    avatar";
  - it is set in Settings → Social preview → Edit → Upload an image. No API or CLI sets it, so you upload it by hand.
- **The plan [I]:** don't stretch og.png. Its 1200 × 630 is 1.9 : 1, and GitHub's 2 : 1 would cut or squash it.
  Render both images from one design in GW instead:
  - **The tool:** a `ShareCard` composition in GW's Remotion project (its `video` folder). It already holds the brand
    logos, the fonts, the colours and the mountains (the tour's closing frame, `Ends.tsx`, GW #3), and GW-4's
    `render-features.mjs` already makes stills with `renderStill`. [V] (GW PR #3 and #4 diffs)
  - **A rule for this repository:** `scripts/tour-renders.test.mjs:90-94` fails `npm test` when any file here names
    the Remotion project's source paths (GA-163 hit it). So GA's files, this one included, name GW's files without
    that folder.
  - **The layout:** the one og.png has now. That is the logo top left, "Your agents build it. You review." with
    "review" in pink, and the guides with their plus marks over the mountains. [V] (`public/og.png`) It gets two
    changes:
    - the line becomes "A free desktop app where AI coding agents work your board. For Linux, macOS and Windows.";
    - `gizai.app` goes in the bottom right.
    The CLIs are left off: a share image is close to a logo, and Anthropic and Google allow their names in plain text,
    not as artwork.
  - **The script:** `npm run render:share` in `video/` writes `public/og.png` (1200 × 630, the site's share card) and
    `design/github-social-preview.png` (1280 × 640, kept in GW's repository but not served), each under 1 MB.
  - **The upload:** you upload the second one to GitHub by hand (§5).
- **Instead of a script:** a Design card in Figma ("Giz-AI", a Share card frame at 1280 × 640). It suits a bigger
  change to the look, but every later change means exporting again by hand. I recommend the script, with the Design
  Agent or you checking the result in Review.

### 3.3 The README's first screen, word for word

**Replace `README.md:9-36`**, from the `<div align="center">` to the end of Features (the logo above it stays):

```markdown
<div align="center">

**Your agents build it. You review.**

A free, open-source desktop app where a team of AI coding agents works through your board in parallel, on Claude Code,
Codex CLI or Gemini CLI. Made for freelancers and software companies with several clients and projects.

[Website](https://gizai.app) · [Install](#install) · [Docs](#documentation) · [Releases](https://github.com/Yotech-AI/gizai/releases) · [Licence](LICENSE)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/tour/tour-dark.gif">
  <source media="(prefers-color-scheme: light)" srcset="docs/tour/tour-light.gif">
  <img alt="GizAi's product tour: the board and a live agent run, the Team Lead's chat, Claude Code and Codex, a card tested by QA, the Editor and its Terminal, Memory, an agent's tools and the Inbox" src="docs/tour/tour-light.gif">
</picture>

</div>

## What it is

GizAi is an Agentic Development Application (ADA). You write the cards; agents build them side by side, each card in
its own git worktree and branch; a QA agent tests every card and opens its pull request; and you review and merge.
Everything runs on your own computer, with your own git repositories, logins and coding CLIs.

## Made for work with several clients

- **Clients, projects and cards in one app.** Clients with their contacts, projects with their own repository and docs,
  and every card on one board.
- **Memory kept apart per client.** Agents keep notes on every card, and a note about one client or project never
  reaches a run for another.
- **Several accounts per CLI.** Add a second Claude Code or Codex CLI account, see the usage limits of each, and move
  an agent or a chat to another account.

## The team

A new install starts with five agents. Each runs on the CLI you pick, with its own model, instructions and tools:

- **Team Lead:** you chat with it (its chat runs on Claude Code). It turns requests into cards, starts agents and
  answers their questions before they reach you.
- **Backend and Frontend Agents** build the cards in To do.
- **QA Agent** tests each card, adds missing tests and opens the pull request.
- **DevOps Agent** releases what you merged, when you ask.

Add a **Design Agent** for a Design column, or agents with roles and instructions of your own, like an **R&D Agent**
that researches a feature before it's built.

## Features

- **The board decides who works.** Each column holds the agents that work its cards, so cards move from builder to tester to you by themselves. [Read more](docs/features/board.md)
- **A Team Lead you talk to.** Ask the Team Lead in chat to plan work, make cards or start agents, and Approve with one click (or Approve all) the steps that wait for your OK. [Read more](docs/features/team-lead.md)
- **Bring your own CLI.** Each agent runs on the coding CLI you pick, like Claude Code, Codex CLI or Gemini CLI, on your own machine with your own logins and accounts. [Read more](docs/features/cli.md)
- **Tested before you see it.** A QA agent tests a card before it reaches your Review, and opens its pull request on GitHub. [Read more](docs/features/testing.md)
- **The Editor and Terminal, built in.** Open a project or a card's worktree, see its git status and edit it, with your own shell in a panel under the code or on its own page. [Read more](docs/features/editor-and-terminal.md)
- **Memory.** The Team Lead, the agents and you keep notes for later, and each run gets the notes for its card. [Read more](docs/features/memory.md)
- **Tools per agent.** Give each agent only the commands, folders, MCP servers, web access and hidden browser it needs. [Read more](docs/features/tools.md)
- **The Inbox.** Questions, approvals, commands to run and cards to review wait for you in one list. [Read more](docs/features/inbox.md)
```

What changes:

- The headline is the website's (`src/data/site.ts:9` in GW). One line under it names the CLIs and who it's for, and a
  Website link joins the links.
- Two short sections come before Features: who it's for, and the team.
- Features keeps its eight lines; only bullet 3 now writes "Codex CLI" and "Gemini CLI".

At the end, `README.md:116` becomes:

> GizAi is an independent open-source project by YoTech, not affiliated with or endorsed by Anthropic, OpenAI or Google.
> It runs Claude Code, Codex CLI and Gemini CLI with your own accounts, and never sells AI usage. Claude Code is a
> trademark of Anthropic, Codex of OpenAI, and Gemini of Google LLC.

**Each claim, checked** [V]:

| Claim | Where |
|---|---|
| Free, open source | `README.md:112-114`, `LICENSE` |
| Works through your board, in parallel; a worktree and branch per card | `docs/features/board.md:3`, `:39`, `:43`, `:47` |
| Claude Code, Codex CLI, Gemini CLI (or another) | `docs/features/cli.md:7`, `docs/memory.md:4-5` |
| QA tests every card and opens the pull request | `docs/features/testing.md:3`, `:8-9` |
| On your own computer, your own repositories and logins | `docs/features/cli.md:13-18` |
| Clients with contacts, projects with docs, cards on a board | `docs/features/board.md:7-8` |
| Memory kept apart per client | `docs/features/memory.md:21-22`; `docs/memory.md:178-180` ("a note about another client or another project never goes in, not even its path") |
| A second account per CLI; limits per account; move a chat to another account | `docs/features/cli.md:8`, `:51-56`; `docs/features/team-lead.md:20` |
| Five agents in a new install | `crates/gizai-core/src/seed.rs:32-35`; `docs/features/board.md:33` |
| Each agent its own CLI, model, instructions and tools | `docs/features/cli.md:9`; `board.md:32`; `testing.md:12` (instructions you can change); `tools.md` |
| The Team Lead's chat runs on Claude Code | `docs/features/cli.md:28-30` |
| It answers agents' questions before they reach you | `docs/features/team-lead.md:50-57` |
| The builders work To do; DevOps releases when you ask | `docs/features/board.md:33`, `:81-84`; `crates/gizai-core/roles/devops.md:1` |
| A Design role; roles of your own | `seed.rs:195-197`, `:222`, `:224-228`; `board.md:20`, `:34`; `src-tauri/src/tools/mod.rs:433` |
| Editor and Terminal | `docs/features/editor-and-terminal.md:3` |

Two things are left out on purpose:

- "Gemini CLI" gets no limits claim: GizAi can't read them yet (`cli.md:56`).
- "Several accounts" doesn't say "to get around limits". Anthropic's terms say Pro and Max limits "assume ordinary,
  individual usage" [V] (code.claude.com legal page), so the README doesn't sell extra accounts as a way past limits
  (question 3).

### 3.4 One name: GizAi

**What the brand decided:** "The wordmark reads GizAi … Never "Gizai", "GIZAI" or "Giz AI"" (`design/brand/README.md:33-34`). [V]

**Where the name is written another way**, line by line [V] (`rg` on this branch):

| Place | Lines | Now |
|---|---|---|
| `README.md` | :104, :106 | prose "Gizai": change |
| `docs/PLATFORMS.md` (75 times) | :1, :3, :7, :8, :16, :17, :18, :20, :21, :22, :24, :25, :29, :30, :35, :38, :40, :41, :42, :45, :46, :53, :55, :57, :59, :61, :63, :70, :72, :73, :74, :76, :77, :83, :84, :86, :89, :90, :92, :97, :98 | prose: change; keep the paths and labels in the list under the table |
| `docs/agent-tools.md` (55) | :7, :16, :18, :46, :50, :52, :55, :56, :61, :63, :75, :96, :98, :99, :102, :106, :114, :126, :128, :136, :140, :145, :146, :154, :156, :157, :161, :167, :169, :182, :191 | change. GW-7 puts this doc on gizai.app/docs/agent-tools |
| `docs/RELEASING.md` (39) | :1, :3, :5, :16, :17, :18, :23, :29, :57, :59, :77, :84, :87, :97, :100, :102, :104, :107, :110, :114, :116, :117, :118, :119, :123, :126, :134, :136, :159, :167, :169, :173 | prose: change; keep the paths |
| `docs/memory.md` (16) | :16, :21, :39, :72, :98, :129, :130, :159, :169, :188, :237, :251, :255, :258, :269, :273 | change, except :169, which quotes the line the app writes into prompts ("they stay in Gizai's Memory …"). GW-7 puts this doc on gizai.app/docs/memory |
| `docs/PAID-FEATURES.md` (16) | :1, :8, :22, :39, :59, :67, :72, :87, :103, :119, :148, :151, :167, :178, :202 | change |
| `docs/install.md` | :36, :79, :109, :117, :125, :145 | only paths (`Gizai.app`, `%APPDATA%\Gizai`, `%LOCALAPPDATA%\Programs\Gizai`, `~/Library/Application Support/Gizai`): keep |
| `CLAUDE.md` | :1, :3, :8, :20, :24, :47, :51 | prose: change (agents read it in every run) |
| Dated records: `docs/HANDOFF-2026-10-07.md` (36), `docs/v0.3-notes.md` (18), `docs/v0.1-notes.md` (2), `docs/MORNING-2026-10-07.md` (4, one "GizAI"), `docs/OPEN-ITEMS.md` (10), `docs/superpowers/` (31), `docs/research/` (157) | | keep: records of their day, which a rename would only churn |
| `docs/features/*.md`, `docs/settings.md` | none | already "GizAi" |
| `design/` mock-ups (`design/code-editor`, `design/terminal`, `design/branding`, `design/README.md`) | | keep: they draw the app as it is, and change with it |
| The GitHub About | | "Giz AI": §3.1 |
| The Figma file | `design/brand/README.md:4` | named "Giz-AI": rename it to "GizAi" by hand if you like. Links to it keep working, as Figma finds a file by its key |
| gizai.app, the pages | | "GizAi" everywhere visible [V] (GW #1 to #5 diffs) |
| gizai.app, the HTML source | `src/assets/mountains-hero.svg`, line 1: `<!-- Gizai's mountains … -->`, inlined on every page by `Base.astro` | change in the GW card |
| gizai.app, after GW-7 | `/docs/agent-tools`, `/docs/memory` | follow this repository's docs at the next `npm run docs:sync` |

**What must stay as it is**, because code and installs find GizAi by it [V]:

| Name | Where | Why |
|---|---|---|
| productName `"Gizai"` | `src-tauri/tauri.conf.json:3` | `install.sh:177` and `install.ps1:193` grep for it to know a checkout. It names `Gizai.app`, which `crates/gizai-agents/src/update.rs:205` (`ends_with("Gizai.app/Contents/MacOS")`) and `install.sh:37`, `:216`, `:246-262` look for. A rename breaks updates on macOS. |
| Identifier `ai.gizai.app` | `tauri.conf.json:6` | Windows' AppUserModelID for notifications and the Start menu shortcut (`docs/PLATFORMS.md:46`, `:73`) |
| Window title `"Gizai"` | `tauri.conf.json:15` | Window rules and the headless tests may match it. Check before the app's rebrand. |
| Data folders `~/Library/Application Support/Gizai`, `%APPDATA%\Gizai`, `~/.local/share/gizai` | `src-tauri/src/lib.rs:132-140` | everyone's data |
| Desktop entry `Name=Gizai`, Start menu `Gizai.lnk` | `install.sh:339`; `install.ps1:89` | an update would leave the old shortcut next to a new one |
| `gizai`, `gizai-mcp`, `GIZAI_*`, `GIZAI_RESULT`, `refs/gizai/merged/…`, the crate names, the repository slug | everywhere | commands, settings, parsers and links |

**The plan:**

1. **Now (the GA card):**
   - "GizAi" in the README, the user docs in the table and CLAUDE.md;
   - paths, file names and code stay as they are, in backticks;
   - the dated records stay.
   - **Labels:** `docs/settings.md:31` already writes "Open GizAi" where the tray says "Open Gizai"
     (`src-tauri/src/lib.rs:326-327`). Docs follow the brand, and the labels catch up with the rebrand.
2. **Now (by hand):** the About (§3.1). Optionally, the Figma file's name.
3. **Later, the app's rebrand card** (the brand README says the app takes the name over "in a card of its own",
   `design/brand/README.md:6-7`):
   - The text people see: about 50 UI strings in `src/` [V] (rough `rg` count); the tray and menus
     (`src-tauri/src/lib.rs:326-335`, `:363`); notifications; the window title; the agents' prompts ("You are the Team
     Lead in Gizai's Software team", `seed.rs:203`, `crates/gizai-core/roles/*.md`); and the tests that check them
     (`crates/gizai-core/tests/role_template_test.rs:116`).
   - **The risks:**
     - agents keep the instructions saved in their forms, so old installs mix both names in prompts (harmless);
     - "Gizai" appears about 1,360 times in the code, comments included [V] (`rg -c`), so a careless find-and-replace
       would hit the identifiers above;
     - the macOS app bundle must stay `Gizai.app` unless `install.sh`, `install.ps1` and `update.rs` move with it, in
       the same release.

### 3.5 Other products called Giz AI or GizAi

| Name | What it is | Source | Matters? |
|---|---|---|---|
| **giz.ai, "Giz", "GizAI", "Giz AI"** | Giz Inc. (Delaware C-Corp, team in Seoul): an AI platform for chat and media, "AI that finishes the work, not just the answer". Its MCP page says "One secure connection gives ChatGPT, Codex, Claude Code, and your terminal live access to Giz models, Drive, and Computer". Its Google AI competition page is titled "Giz AI". | https://giz.ai, https://www.giz.ai/mcp/ [V]; https://ai.google.dev/competition/projects/giz-ai (helper [V]) | **Yes**: the same space (AI agents, Claude Code, Codex), first in search for every spelling |
| **github.com/GizAI** | Giz Inc.'s organisation, made 2026-09-08, 18 repositories, among them coding-agent tools (`giz-code`, `arche` "Long-lived coding agent for Claude Code") | https://api.github.com/orgs/GizAI [V]; repositories (helper [V]) | Yes: "GizAI on GitHub" isn't us |
| **npm `@gizai/cli`** 1.3.0 | "Giz CLI, MCP installer, OpenAI-compatible models, Drive, and Computer", repo GizAI/giz, made 2026-07-23 | `npm view @gizai/cli` [V] | Yes: the `@gizai` scope is taken. Plain `gizai` is free on npm (404) [V] and crates.io (helper [V]) |
| PyPI `gizai` | An unrelated package by Arash Abadpour ("A Mathematical model for AI languages") | https://pypi.org/project/gizai/ (helper [V]) | Little |
| GIZ (Deutsche Gesellschaft für Internationale Zusammenarbeit) | Germany's development agency, with AI programmes ("FAIR Forward") | https://www.bmz-digital.global/en/overview-of-initiatives/fair-forward/ (helper [V]) | Only for "Giz AI" with a space |
| Near names: Gizmo (gizmo.ai, AI study app), Gizmo (gizmo.party, vibe-coded mini apps), Giza (gizatech.xyz, DeFi agents), gizza.ai (browser AI tools) | | helper [V] on each homepage | Little |
| GitHub search "gizai" | A spam repository (amberjasper44/gizai, gambling SEO) ranks above Yotech-AI/gizai, whose "Giz AI" About is second | https://api.github.com/search/repositories?q=gizai (helper [V]) | The new About and topics should lift ours |

What it means [I]:

- A search for any spelling lands on Giz Inc. first, even "gizai.app" on its own (helper's searches).
- So never write "Giz AI".
- Pair the name with what it is, in every title and the About: "GizAi, a desktop app for AI coding agents".
- Give search engines one site name (WebSite data, §3.8).
- Tell AI assistants in llms.txt that GizAi isn't giz.ai.
- The helper found no GIZ AI or GizAi trademark in a web search, but that isn't a clearance. A formal search (EUIPO
  TMview, USPTO, KIPO in Korea) before more goes into the name is question 1.

### 3.6 Naming Claude Code, Codex CLI and Gemini CLI

- **Anthropic** [V] (code.claude.com/docs/en/legal-and-compliance.md): "You can accurately say, in plain text, that your
  product has Claude Code preinstalled or that it runs Claude Code. But you can't use the Claude Code or Anthropic names
  or logos as part of your own product, feature, or company name, in your own logo, or in a way that suggests Anthropic
  built, endorses, or is partnered with your product."
  - The same page allows "an end user … signing in to the unmodified Claude Code binary with their own Claude
    subscription", which is what GizAi does (`docs/features/cli.md:20`).
  - Anthropic's Trademark Guidelines: "Do not use a trademark symbol with the trademarks" (anthropic.com/legal/trademark-guidelines,
    helper [V]).
- **Google** [V] (about.google/brand-resource-center/guidance, helper's exact-match check):
  - "Use Google in plain text … in an informational context";
  - "Don't use … any of our trademarks … into your brand name, product name";
  - use a trademark "only as an adjective … never in the plural or possessive form": write "the limits of Gemini CLI",
    not "Gemini CLI's limits".
- **OpenAI:** openai.com/brand answered 403, so its rules are unchecked. Its Codex licence gives no rights to its names
  "except as required for reasonable and customary use in describing the origin of the Work" [V] (openai/codex LICENSE;
  helper).
- **So [I]:**
  - name the CLIs in plain text, as what GizAi runs;
  - no logos, no ™, no "powered by" (GizAi supplies no AI);
  - never "Claude Code kanban", "Claude Code agent" or a CLI's name as a feature or page name;
  - in a title that names a CLI, the sentence says GizAi runs it, and the title ends in "· GizAi";
  - the README, the About and the website write the products' names, "Codex CLI" and "Gemini CLI";
  - the docs may keep "Codex" and "Gemini" where they name the app's own labels in Settings → Coding CLIs
    (`src/lib/clis.ts:7`: "Claude Code", "Codex", "Gemini", "Other") [V].

### 3.7 Community files

**Discussions:** switch them on (`gh repo edit Yotech-AI/gizai --enable-discussions`, or Settings → Features). [V] The
categories:

| Category | Format | Why |
|---|---|---|
| Q&A | question and answer | questions about setup and use; the answer is marked |
| Ideas | open-ended | feature ideas, before an issue |
| Show and tell | open-ended | what people built with their agents |
| Announcements | announcement (only maintainers post) | each release's "What's new" (§3.9) |

GitHub makes six to start with: Announcements, General, Ideas, Polls, Q&A and Show and tell [V] (docs.github.com,
"Managing categories for discussions"; helper). Remove General and Polls, so questions land in Q&A (question 6).

**Issue forms**, in `.github/ISSUE_TEMPLATE/`. GitHub still calls issue forms a "public preview" [V]. Each form needs
`name`, `description` and `body`, and `required` works in public repositories [V] (docs.github.com, "Syntax for issue
forms"; helper).

- `bug.yml`: name "Bug report"; labels `bug`. Fields:
  - what happened (required);
  - steps to reproduce (required);
  - what you expected;
  - GizAi version (from Settings → Updates or `gizai --version`, required);
  - system: a dropdown of Linux (which distribution), macOS (Apple silicon) and Windows 11 (required);
  - coding CLI and its version: a dropdown of Claude Code, Codex CLI, Gemini CLI and another;
  - logs, as `render: text`;
  - a required checkbox: "I removed tokens, keys, client names and private code from what I pasted".
- `feature.yml`: name "Feature request"; labels `enhancement`. Fields:
  - the problem it solves (required);
  - your idea;
  - who it helps (a freelancer, a team, both);
  - the part of GizAi: a dropdown of the eight features, Install and updates, and other;
  - other ways you tried;
  - a markdown line: "The app stays free; the paid plan is hosted services only (`docs/PAID-FEATURES.md`)".
- `install.yml`: name "Install or update problem"; labels `bug`. Fields:
  - system and version (required);
  - how you installed: the one-line install, `install.sh` or `install.ps1` from a clone, or an update from inside
    GizAi (required);
  - the command you ran;
  - the output of `./install.sh --check` or `.\install.ps1 -Check` (they name what's missing, `README.md:54`; required);
  - the end of the output or of `<data folder>/update/update.log` (`docs/RELEASING.md:114`);
  - `rustc --version` and `node --version`;
  - on Linux, WebKitGTK's version.
- `config.yml`: `blank_issues_enabled: false`, and `contact_links` to:
  - Q&A in Discussions ("Questions go here");
  - the security policy ("Report a security problem privately");
  - the docs (gizai.app/docs once GW-7 is live).
  [V] (docs.github.com, "Configuring issue templates"; helper)
- Optionally `.github/pull_request_template.md`: what changed for the person using GizAi, how it was tested, the
  systems tried. It is a box on GitHub's Community Standards list [V] (github.com/cli/cli/community; helper).

**CONTRIBUTING.md** (root), an outline. GitHub links it when someone opens an issue or a pull request, and from the
`contribute` page [V] (docs.github.com, "Setting guidelines for repository contributors"; helper):

1. GizAi in two lines, and where to talk: Discussions for questions and ideas, issues for bugs.
2. Before you start: comment on an issue, or open one for anything bigger than a fix. Good first issues are labelled
   `good first issue`.
3. **Set up:**
   - what each system needs (link `docs/install.md`);
   - `source scripts/env.sh`, then `cargo test --workspace && npm test` and `npm run build`;
   - `scripts/ui-test.sh` for UI changes (headless, never on your desktop).
4. **Never test against your own GizAi:** use a scratch `GIZAI_DATA_DIR` and the steps in `docs/RELEASING.md` →
   Testing an update.
5. **The rules from CLAUDE.md**, for people:
   - core logic in `crates/`, which never imports Tauri;
   - `src-tauri/` as a thin adapter, and the UI through `src/api.ts`;
   - tests first, with the fake CLIs only;
   - code for one system behind `cfg(...)`; CI runs Linux, macOS and Windows.
6. **Writing:** plain, short, sentence case. Commit messages say what changed for the person using GizAi.
7. **Branches:** pull requests into `main`; `production` and releases are for the maintainer.
8. **AI-written work is welcome** (GizAi's own agents write most of it), and the person who opens the pull request
   answers for it.
9. **Licence:** contributions come under Apache-2.0 (its section 5), without a CLA (question 5). The name and logo
   aren't licensed (`README.md:112`).

**SECURITY.md** (root), an outline:

1. **Supported:** the latest release only. GizAi updates itself from releases (`docs/RELEASING.md:5`).
2. **Report privately** with GitHub's private vulnerability reporting: the Security tab → Report a vulnerability. Never
   in a public issue. [V] (docs.github.com, "Configuring private vulnerability reporting"; helper) It needs switching on
   (§6).
3. **What counts:**
   - an agent getting past its allowed commands, folders or tools (`docs/features/tools.md`, `docs/agent-tools.md` →
     Safety rules);
   - the Team Lead's approval step (`docs/features/team-lead.md:33-42`);
   - the socket or named pipe of the Team Lead's tools (`docs/PLATFORMS.md:20`);
   - keys and tokens (`docs/features/cli.md:22-26`);
   - the install scripts, the one-line installs and updates (`docs/RELEASING.md` → How GizAi updates itself).
4. **What doesn't count:**
   - bugs in Claude Code, Codex CLI or Gemini CLI themselves: report those to Anthropic (HackerOne, linked from its
     legal page [V]), OpenAI or Google;
   - an agent doing what you allowed it to.
5. **What to send:** the version, the system, the steps, the impact.
6. **What happens:** a reply within the days you choose (question 7), a fix in the next patch release, and credit if
   you want it.
7. **How GizAi keeps your data:** link `docs/install.md` → Your data and No telemetry.

**Good first issues**, from real gaps (all checked) [V]:

1. **Open in: add Cursor** (or another editor).
   - The Editor's Open in knows VS Code, Zed and PhpStorm. Each is one entry of `AppId`, with its id, its name and the
     commands it goes by (`crates/gizai-agents/src/editor.rs:175-223`).
   - Add one, with a test in `crates/gizai-agents/tests/editor_test.rs`, and update `docs/features/editor-and-terminal.md:24`
     and `docs/PLATFORMS.md:39`.
2. **docs/memory.md: the "Not yet" list repeats itself, and lists the graph, which is built.**
   - `docs/memory.md:346-352` repeats the list, and `:318` describes the graph.
   - A docs-only fix.
3. **Try the check list on a Mac or a Windows 11 PC.**
   - `docs/PLATFORMS.md:79-93` lists 11 checks that only you have done so far.
   - Run them on your machine and report each, with the install form.
   - Labels: `good first issue`, `help wanted`.
4. **Windows: long paths in worktrees.**
   - Worktrees live in `%APPDATA%\Gizai\worktrees`, and a project's `node_modules` can pass 260 characters
     (`docs/PLATFORMS.md:97`).
   - Check it on a real project, and set `core.longpaths` for GizAi's worktrees if it bites (behind `cfg(windows)`).
5. *(Help wanted, not first)* **Usage limits for Gemini CLI.** "Gemini and other CLIs say GizAi can't read their limits
   yet" (`docs/features/cli.md:56`).

### 3.8 gizai.app

**What exists** [V] (GW PRs #1 and #2; the live site):

- `sitemap.xml`: written after the build with every page but the 404 (GW `astro.config.mjs`, PR #1). It lists 9
  addresses, without `lastmod`, and picks up `/roadmap` and `/docs/*` by itself once GW-7 is in.
- `robots.txt`: `User-agent: *`, `Allow: /` and the sitemap (`src/pages/robots.txt.ts`).
- On the homepage, SoftwareApplication data: name, description, url, image, `DeveloperApplication`, "Linux, macOS,
  Windows", price 0 EUR, `isAccessibleForFree`, the Apache-2.0 licence, `downloadUrl`, `softwareHelp`, `sameAs` GitHub
  and publisher YoTech.
- On each feature page, BreadcrumbList and FAQPage.
- In the head: canonical URLs, Open Graph and Twitter tags, og.png at 1200 × 630, and noindex on the 404 page
  (`src/layouts/Base.astro`).
- The titles: the homepage is "GizAi · Your agents build it. You review." and each feature page "<name> · GizAi". The
  description is the feature's intro (`src/layouts/Feature.astro`, `:64` on GW main).

**What is missing or wrong:**

1. **No llms.txt** (gizai.app/llms.txt answers 404) [V].
   - Google Search doesn't use the file. Search Central's changelog of 15 June 2026: "these files aren't needed for
     Google Search (and won't negatively or positively impact your visibility or rankings), it's fine if you want to
     maintain these files for other services or systems that use them" [V] (developers.google.com/search/updates).
   - OpenAI, Anthropic and Perplexity don't say they read it either (helper [V]).
   - Coding agents fetch it, though, and Chrome's Lighthouse checks for it [V] (developer.chrome.com, Lighthouse
     "llms.txt"; helper).
   - It's cheap, so add it, and expect help with AI assistants, not with rankings. [I]
2. **The titles hold no search words.** A searcher for "run coding agents in parallel" or "kanban for AI coding
   agents" finds none in "The board decides who works · GizAi". New titles below.
3. **No WebSite data, so no site name for Google.** Google wants WebSite data on the homepage with `name` and `url`,
   and recommends `alternateName` [V] (developers.google.com, "Site names"; helper). Add `name: "GizAi"` and
   `alternateName: ["Gizai"]` (the window still says Gizai). Not "Giz AI": that's Giz Inc.'s spelling.
4. **SoftwareApplication can't earn a rich result.** Google requires `aggregateRating` or `review`, and "Ratings must
   be sourced directly from users" [V] (developers.google.com, "Software app" and "Review snippet"; helper). Keep it for meaning,
   don't add ratings, and fix three things:
   - `downloadUrl` points at `/#get-gizai`, which isn't a download: use `installUrl`;
   - `operatingSystem` should say "Linux, macOS (Apple silicon), Windows 11" (`README.md:46`, `docs/install.md:67`);
   - add `featureList` (the eight feature names) and `screenshot` (a still).
5. **FAQPage does nothing in Google any more:** "This feature will no longer appear in Google Search starting May 7,
   2026" [V] (Google Search Central changelog). Keep the questions as text on the page, and keep or drop the markup.
6. `mountains-hero.svg`'s comment says "Gizai" in every page's source (§3.4).

**The llms.txt draft**, at `/llms.txt` [V] for the format (llmstxt.org, helper: an H1, a blockquote summary, text
without headings, then H2 lists of links, with "Optional" last). It's built from `src/data/features.ts`, `src/data/docs.ts`
and a short `src/data/llms.ts`, so new features and docs join by themselves:

```markdown
# GizAi

> GizAi is a free, open-source (Apache-2.0) desktop app for Linux, macOS and Windows where a team of AI coding agents
> works through your kanban board. You chat with a Team Lead that plans the work and starts agents; Backend and
> Frontend agents build each card in its own git worktree and branch, in parallel; a QA agent tests it and opens the
> pull request; you review and merge. The agents run on the coding CLIs you already use (Claude Code, Codex CLI,
> Gemini CLI or another), on your own computer, with your own logins.

Facts to get right:

- GizAi is made by YoTech. Website: https://gizai.app. Source and releases: https://github.com/Yotech-AI/gizai.
- It is free, and it sells no AI usage: every agent runs on the user's own Claude Code, Codex CLI or Gemini CLI login
  or API key. GizAi needs no account of its own and sends no telemetry.
- It is not affiliated with Anthropic, OpenAI or Google, and it is not giz.ai, another company's AI platform.
- A new install has five agents: Team Lead, Backend, Frontend, QA and DevOps. You can add a Design agent and agents
  with roles of your own. The Team Lead's chat runs on Claude Code.
- Memory is kept apart per client: a note about one client or project never reaches a run for another.
- It installs from source with one line: `curl -fsSL https://gizai.app/install.sh | sh` on Linux and macOS (Apple
  silicon), `irm https://gizai.app/install.ps1 | iex` on Windows 11.
- The name is written GizAi. The app's window and some older docs still say Gizai.

## Features

- [The board decides who works](https://gizai.app/features/board): agents sit on columns; each card gets its own git worktree and branch, so agents work side by side.
- [A Team Lead you talk to](https://gizai.app/features/team-lead): a chat that turns requests into cards, starts agents and answers their questions before they reach you.
- [Bring your own CLI](https://gizai.app/features/cli): Claude Code, Codex CLI, Gemini CLI or another, with your own logins; a second account per CLI, with its usage limits.
- [Tested before you see it](https://gizai.app/features/testing): a QA agent runs the tests, adds missing ones and opens the pull request.
- [The Editor and Terminal, built in](https://gizai.app/features/editor-and-terminal): open a project or a card's worktree, edit, and run your own shell under the code.
- [Memory](https://gizai.app/features/memory): linked notes kept by you, the Team Lead and the agents; every run gets the notes for its card.
- [Tools per agent](https://gizai.app/features/tools): MCP servers, web search and a hidden browser, each off until you switch it on.
- [The Inbox](https://gizai.app/features/inbox): questions, approvals, commands to run and cards to review, in one list.

## Docs

- [Install](https://raw.githubusercontent.com/Yotech-AI/gizai/production/docs/install.md): what each system needs, updates, your data, no telemetry.
- [Settings](https://raw.githubusercontent.com/Yotech-AI/gizai/production/docs/settings.md)
- [Agent tools and MCP servers](https://raw.githubusercontent.com/Yotech-AI/gizai/production/docs/agent-tools.md)
- [Memory in depth](https://raw.githubusercontent.com/Yotech-AI/gizai/production/docs/memory.md)

## Optional

- [Roadmap](https://gizai.app/roadmap)
- [Releases](https://github.com/Yotech-AI/gizai/releases)
- [Licence](https://github.com/Yotech-AI/gizai/blob/main/LICENSE)
```

The Docs links point at the Markdown of the released docs, which is what the format asks for ("links to detailed
markdown files"). The pages on gizai.app/docs are HTML. If GW-7's pages get `.md` copies later, link those instead.

**New titles and descriptions**: `<title>` and the meta description only. The H1 and the row names stay the brand's.
Lengths are characters. Google cuts titles at about 60, so the search words come first.

| Page | Title | Description | The searches it aims at |
|---|---|---|---|
| `/` | GizAi · Run AI coding agents in parallel on a kanban board (58) | Free, open-source desktop app: AI coding agents work your kanban board in parallel on Claude Code, Codex CLI or Gemini CLI, and you review every card. (150) | AI coding agent kanban board; run coding agents in parallel; multi-agent coding desktop app; gizai |
| `/features/board` | Run Claude Code and Codex CLI in parallel · GizAi (48) | Agents sit on your board's columns and work cards side by side, each in its own git worktree and branch, from builder to QA to your review. (139) | run multiple Claude Code agents in parallel; parallel Claude Code worktrees; a kanban board for coding agents |
| `/features/team-lead` | A Team Lead agent that plans your board and runs agents · GizAi (63) | Chat with the Team Lead: it turns requests into cards, starts agents, answers their questions first and asks for your OK in one click. (134) | coding agent orchestrator; AI team lead agent; AI dev team |
| `/features/cli` | Claude Code, Codex CLI and Gemini CLI, several accounts · GizAi (62) | Every agent runs on Claude Code, Codex CLI, Gemini CLI or another CLI, with your own logins. Add a second Claude Code or Codex CLI account and see its limits. (156) | Claude Code multiple accounts; Claude Code usage limits; Codex CLI GUI |
| `/features/testing` | A QA agent tests every card and opens its pull request · GizAi (62) | A QA agent runs the tests, lint and typecheck in the card's own worktree, adds missing tests and opens the pull request before the card reaches you. (148) | AI QA agent; AI agent writes tests; review AI pull requests |
| `/features/editor-and-terminal` | A code editor and terminal for your agents' worktrees · GizAi (61) | A code editor and your own terminals, inside GizAi. Open a project or a card's worktree, check its git status, edit, and run commands under the code. (149) | review coding agent changes; git worktree editor; editor with terminal |
| `/features/memory` | Memory for AI coding agents, kept apart per client · GizAi (58) | Decisions and gotchas as linked notes with a graph. Every agent run gets the notes for its card, and one client's notes never reach another's. (142) | memory for coding agents; Claude Code memory per project |
| `/features/tools` | MCP servers, web search and a browser, per agent · GizAi (56) | Give each agent only the commands, folders, MCP servers, web search and hidden browser it needs. Every tool stays off until you switch it on. (141) | MCP servers per agent; MCP server manager |
| `/features/inbox` | One inbox for your agents' questions and approvals · GizAi (58) | The Team Lead's questions, calls that wait for your OK, commands to run, held cards and cards to review: one list, with desktop notifications. (142) | approve coding agent actions; human in the loop for coding agents |

- **The searches** come from the results the helper read: pages titled with exactly these phrases rank for them.
  "Claude Code kanban" has seven pages built for it, from kanboard.io, devboardai.com, nimbalyst.com and others, and
  "run multiple Claude Code agents in parallel" brings up only how-to guides [V] (the helper's searches; a US engine,
  not Google).
- **No search volumes** were public: Google Trends answered 429. So this ranks the searches by what the competitors
  aim at, not by numbers. [I]
- **Brand rules:** no title uses a CLI's name as a name. "Run Claude Code and Codex CLI in parallel" says what GizAi
  does with them. [I] on Anthropic's rule, §3.6.
- **How it's built:** two optional fields in the features collection (`src/content.config.ts`), `seoTitle` and
  `description`. `Feature.astro` uses `seoTitle` for `<title>`, and `description` (else `intro`) for the meta
  description, Open Graph and Twitter. The homepage's title goes in `src/data/site.ts`.
- **Later, not in this card:**
  - a how-to guide, "Run several coding agents in parallel with git worktrees", for the query only guides win;
  - a page about what GizAi does differently from tools that are closing or moving on. Vibe Kanban "is sunsetting"
    and Crystal is "Deprecated: February 2026" [V] (vibekanban.com/blog/shutdown, github.com/stravu/crystal, helper).
    Facts only (question 9).

### 3.9 Release notes

**Now:** `gh release create vX.Y.Z --target production --title "vX.Y.Z" --generate-notes` (`docs/RELEASING.md:71-77`).
- Every release body is GitHub's list of pull request titles, card numbers included, and "Bump version" and "Release
  vX.Y.Z" are in it [V] (api.github.com/…/releases, v0.9.0 to v0.11.0).
- That list is also what Settings → Updates shows under "What's new in X.Y.Z".

**The format:** a share line, "What's new" in up to six bullets, how to update, then GitHub's list.

- `gh release create` puts `--notes` or `--notes-file` text above the generated notes [V]: "Additional release notes
  can be prepended to automatically generated notes" (cli.github.com/manual/gh_release_create; helper). In the
  source, `--notes-file` sets the body, and the REST API prepends a body to the generated notes (helper).
- `--discussion-category` starts a discussion for the release [V] (cli/cli `pkg/cmd/release/create/create.go`). So
  every release also posts in Announcements.

**A sample**, written from v0.11.0's pull requests, each line checked against its doc:

```markdown
GizAi 0.11.0 installs with one line on Linux, macOS and Windows, and is now open source under the Apache License 2.0.

## What's new

- **Install with one line.** `curl -fsSL https://gizai.app/install.sh | sh` on Linux and macOS, or `irm https://gizai.app/install.ps1 | iex` in PowerShell on Windows 11. Run it again to update.
- **Apache-2.0.** GizAi's licence is now the Apache License 2.0. Releases before it stay under MIT.
- **The Team Lead can update a linked folder with uncommitted changes:** after your yes, it puts them in git's stash first.
- **The chat says when an answer is done** after your last Approve or Discard.
- **Fixed:** long card titles no longer run over the rows in the folder picker and the sidebar's Editor list, a folder you open again keeps its place, and the sidebar's + and caret are centred.

**Update:** press Update to 0.11.0 above Company in the sidebar, or Settings → Updates → Check now.

## All changes
```

GitHub's list goes under "All changes".

- `.github/release.yml` (GA card) leaves out pull requests labelled `release`. The DevOps Agent labels the bump and
  release pull requests with it (`gh pr create … --label release`). [V] for the keys, `changelog.exclude.labels`
  (docs.github.com, "Automatically generated release notes"; helper)
- The step in `docs/RELEASING.md` becomes:

  ```sh
  gh release create vX.Y.Z --target production --title "vX.Y.Z" --notes-file <notes.md> --generate-notes --discussion-category "Announcements"
  ```

  It keeps the release without `--draft` or `--prerelease`.

**Who writes it** [I]:

1. **The Team Lead drafts** the share line and "What's new" in the release card's description, from the cards in Done
   since the last release. It knows the board and writes for people.
2. **You edit it, or leave it, and start the DevOps Agent.** Starting the agent is your OK, as today: "Jeffrey decides
   when to release" (`docs/RELEASING.md:19`).
3. **The DevOps Agent** copies the block into a notes file in its scratch folder, not the repository (its role commits
   "only release files", `crates/gizai-core/roles/devops.md:20`), and publishes.

   If the card has no "What's new", it writes one from the merged pull requests and stops with `needs_decision`, so
   nothing goes out unread.

## 4. Options

How to deliver the work. The content is the same in each.

| | A. Two pull requests and a hand list (recommended) | B. One card per item (seven cards) | C. Repository first, website later |
|---|---|---|---|
| What | GA: the README, the name in the docs, `.github/` and the community files, RELEASING. GW: llms.txt, titles, data, share images. You: settings. | Each item of the brief its own card and pull request | Only the GA card now; the GW card after GW-7 and a few weeks of data from Search Console |
| Size | GA about 15 files, mostly text; GW about 10 files, plus a Remotion composition | The same in total, seven reviews | GA as in A |
| Risks | The GA card is long but plain text. The GW card must wait for GW-7 (same files: `Base.astro`, `Feature.astro`, `index.astro`, `site.ts`) | Seven reviews; the README and docs overlap; the name pass splits up | The site keeps titles without search words for weeks |
| Libraries | None new: GW already has Remotion (`video/`) and sharp (`scripts/icons.mjs`) | The same | The same |

**A** is what the brief asks for: one pull request for the repository, one for gizai.app and a list for you. It keeps
the name pass in one place, and the GW card waits for GW-7 instead of fighting it.

## 5. The design of option A

**GA pull request**, all text:

- `README.md` (§3.3);
- the name in `README.md`, `CLAUDE.md`, `docs/PLATFORMS.md`, `docs/agent-tools.md`, `docs/RELEASING.md`,
  `docs/memory.md` and `docs/PAID-FEATURES.md` (§3.4);
- but not `docs/memory.md:346-352`, which is left for good first issue 2;
- `.github/ISSUE_TEMPLATE/bug.yml`, `feature.yml`, `install.yml` and `config.yml`;
- `.github/pull_request_template.md`, `.github/release.yml`;
- `CONTRIBUTING.md`, `SECURITY.md` (§3.7);
- `docs/RELEASING.md` step 3 and the "What's new" format (§3.9).

**Tests:** `npm test` and `cargo test --workspace` must pass, with the existing tests unchanged.

- **The README:** `scripts/tour-renders.test.mjs` checks the tour's `<picture>`, the Development section and that the
  README names no `video/` folder, MP4, WebM or poster (`:116-143`). §3.3's text keeps all of that as it is.
- **Paths:** the same file greps the whole repository for the Remotion project's paths (`:90-94`), so the new files
  must not name them.

Also:

- **The forms:** a small vitest checks that each form has `name`, `description` and `body`, and unique `id`s. GA has no
  YAML parser (`npm ls yaml js-yaml` is empty), so it is a text check, or the builder adds `yaml` (ISC) as a dev
  dependency;
- every link in the new files resolves to a file in the repository;
- `rg -n "Gizai"` on the changed docs finds only the paths and quotes in §3.4's keep lists.

**GW pull request**, after GW-7:

- `src/data/llms.ts` and `src/pages/llms.txt.ts`;
- `src/content.config.ts` (`seoTitle`, `description`) and the eight `src/content/features/*.md`;
- `src/layouts/Feature.astro` (title, description);
- `src/data/site.ts` (the homepage title);
- `src/pages/index.astro`: one `@graph` with WebSite, SoftwareApplication (fixed) and Organization;
- `src/assets/mountains-hero.svg`'s comment;
- in the Remotion project, a `ShareCard.tsx` composition registered in its `Root.tsx` and a `render-share.mjs`
  script, with `public/og.png` rendered again and `design/github-social-preview.png`.

**Tests** in GW's `tests/site.test.mjs` (GW-7 adds the file), after `npm run build`:

- `dist/llms.txt` starts with `# GizAi`, and every gizai.app link in it is a built page;
- every feature page's `<title>` and meta description match its file, and each title is at most 70 characters;
- the homepage's JSON-LD parses and holds WebSite `name` "GizAi" and no `downloadUrl`;
- both PNGs have the right size and are under 1 MB.

**Security and privacy:** nothing new is collected. The issue forms ask people to strip tokens, keys and client names.
SECURITY.md sends reports to a private channel. llms.txt repeats only public facts.

**Left out:**

- the app's own rebrand;
- new pages (a guide, comparisons);
- Search Console data;
- a `.md` copy of each page;
- a code of conduct: GitHub's checklist has one, so it's question 8;
- creating the issues, labels and Discussions (that's for you, below).

## 6. Settings you change by hand

1. **Now, by Run this for me:** the About, the website and the 20 topics (§3.1).
2. **Social preview:** Settings → Social preview → Edit → Upload an image: `design/github-social-preview.png` from the
   GW pull request.
3. **Discussions:** Settings → Features → Discussions (or `gh repo edit Yotech-AI/gizai --enable-discussions`). Then
   keep Announcements, Q&A, Ideas and Show and tell, remove General and Polls, and write the welcome post GitHub asks
   for.
4. **Private vulnerability reporting:** Settings → Advanced Security (under "Security and quality") → Private
   vulnerability reporting → Enable. [V] (docs.github.com; helper)
5. **A `release` label** for the release notes' exclude list. The default labels, `good first issue` and `help wanted`
   among them, exist already [V] (api.github.com/…/labels).
6. **The good first issues** (§3.7): open them after the GA pull request is merged, so their links work.
7. **The Wiki:** it's on, but github.com/Yotech-AI/gizai/wiki shows the repository page instead, so it has no pages
   [I]. Switch it off (`gh repo edit Yotech-AI/gizai --enable-wiki=false`), so the docs stay in one place. The same goes
   for Projects if you don't use them.
8. **Search engines:** add gizai.app to Google Search Console and Bing Webmaster Tools (a DNS TXT record in Cloudflare),
   and submit `https://gizai.app/sitemap.xml`. That's where the real search words will show.
9. **Cloudflare's AI crawler settings** for the gizai.app zone: allow the AI assistants' crawlers if you want them to
   read llms.txt and the pages. I couldn't check the zone. The live robots.txt is the site's own, without Cloudflare's
   managed lines [V].
10. **Optional:**
    - rename the Figma file "Giz-AI" to "GizAi";
    - pin the repository on the Yotech-AI profile;
    - a trademark search (question 1).

## 7. Proposed cards

**Card 1 (GA): "Findability, repository side: the README's first screen, one name in the docs, issue forms,
CONTRIBUTING, SECURITY and release notes"**. To do; backend (repository files).

Builds §5's GA list from §3.3, §3.4, §3.7 and §3.9.

- [ ] `README.md:9-36` is §3.3's text word for word, and `README.md:116` is the new line.
- [ ] "GizAi" in the docs of §3.4's table, with the paths, file names, quotes and dated records kept. `rg "Gizai"` on
  them finds only those.
- [ ] `.github/ISSUE_TEMPLATE/` holds `bug.yml`, `feature.yml`, `install.yml` and `config.yml` as outlined, and each
  passes §5's forms check.
- [ ] `CONTRIBUTING.md`, `SECURITY.md`, `.github/pull_request_template.md` and `.github/release.yml` (exclude label
  `release`) exist.
- [ ] `docs/RELEASING.md` step 3 has `--notes-file … --generate-notes --discussion-category "Announcements"`, the format
  and the sample, and says who writes it.
- [ ] Steps 1 and 2 open their pull requests with `--label release`.
- [ ] `npm test` and `cargo test --workspace` pass. No app code changes.

Builds on: nothing. It can run now.

**Card 2 (GW): "gizai.app findability: llms.txt, titles for searches, WebSite data and the share images"**. To do;
frontend.

Builds §5's GW list from §3.2 and §3.8.

- [ ] `/llms.txt` is §3.8's draft, made from the features, the docs list and `src/data/llms.ts`.
- [ ] The nine titles and descriptions are §3.8's table, word for word. The H1s don't change.
- [ ] The homepage has one JSON-LD `@graph`:
  - WebSite with `name` "GizAi", `alternateName` ["Gizai"] and `url`;
  - SoftwareApplication with `installUrl` (no `downloadUrl`), the systems, `featureList` and `screenshot`;
  - Organization YoTech.
- [ ] `npm run render:share` in `video/` writes `public/og.png` (1200 × 630) and `design/github-social-preview.png`
  (1280 × 640), each under 1 MB, in §3.2's layout.
- [ ] The tests in §5 pass, and `npm run build` passes.

Builds on: GW-7 (PR #5) merged, as it changes `Base.astro`, `Feature.astro`, `index.astro` and `site.ts`. It runs at
the same time as card 1. Its Docs links in llms.txt name the docs card 1 changes, but they don't depend on it.

**Card 3 (optional, Design): "Share card in Figma"**. Design column; design. Only if you want the share image designed
before card 2 builds it (§3.2). Card 2 then builds on it.

**Later, not now:** "The app's rebrand: GizAi in its UI and prompts" (Backlog; backend and frontend), with §3.4's keep
list as its rules.

**The contract between the cards:** none in code. Card 2's llms.txt links the released docs on `production`, so it
reads card 1's spelling once a release carries it.

## 8. Open questions

1. **The name.**
   - Giz Inc. (giz.ai) uses "GizAI" for AI software with Claude Code and Codex tools, and has done since 2024. Do you
     want a trademark search (EUIPO TMview, USPTO, KIPO) before more goes into "GizAi"?
   - *My advice:* yes, before the next paid step. A name change is cheap now and dear later.
2. **Gemini CLI.**
   - It serves only paid API keys since 18 June 2026. Keep it in the About and the README, or write "Claude Code, Codex
     CLI or another coding CLI"?
   - *My advice:* keep it (it works, and the topic brings searches), and later find out whether Antigravity CLI runs as
     an "other CLI".
3. **Several accounts.** Keep "move an agent or a chat to another account", which the website's row 03 says too, or
   reword it to "a personal and a work account"?
   - Anthropic says Pro and Max limits assume "ordinary, individual usage".
   - *My advice:* the README text in §3.3, which doesn't promise more room.
4. **"Not giz.ai" in llms.txt.** Keep the line?
   - *My advice:* yes. It's the one place where naming them helps and costs nothing.
5. **CLA or not?**
   - *My advice:* no CLA. Apache-2.0's section 5 covers contributions, and the paid plan is hosted services, not a
     closed app.
6. **Discussions:** remove General and Polls?
   - *My advice:* yes. Four clear categories send questions to Q&A.
7. **SECURITY.md:** how many days until a first reply, and is there an e-mail besides GitHub's private reporting?
   - *My advice:* 7 days, GitHub only.
8. **A code of conduct** (Contributor Covenant) for GitHub's checklist?
   - *My advice:* yes, as a small part of card 1, if you agree.
9. **Pages about competitors** (Vibe Kanban is closing): fine with facts-only pages later?
   - *My advice:* after Search Console shows what people type.
10. **YoTech's website** for the publisher in the structured data and the footer (now github.com/Yotech-AI): which
    address?

## 9. Sources

All read on 2026-10-13. "(helper)" marks a page one of this run's research helpers read and quoted, and I didn't open
again.

- This repository at `24535e3` (main), and Yotech-AI/gizai-website PRs #1 to #5 (`gh pr diff`)
- GitHub API: https://api.github.com/repos/Yotech-AI/gizai, `/releases`, `/labels`, `/issues`, `/community/profile`; topic counts from https://api.github.com/search/repositories?q=topic:…
- gizai.app: https://gizai.app/, /sitemap.xml, /robots.txt, /llms.txt (404), /features/cli
- GitHub CLI: https://cli.github.com/manual/gh_repo_edit, https://cli.github.com/manual/gh_release_create, https://raw.githubusercontent.com/cli/cli/trunk/pkg/cmd/release/create/create.go, `pkg/cmd/repo/edit/edit.go`
- GitHub Docs: topics https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/classifying-your-repository-with-topics; social preview …/customizing-your-repositorys-social-media-preview; Discussions categories https://docs.github.com/en/discussions/managing-discussions-for-your-community/managing-categories-for-discussions; issue forms https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/syntax-for-issue-forms; release notes https://docs.github.com/en/repositories/releasing-projects-on-github/automatically-generated-release-notes; private vulnerability reporting https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/configuring-private-vulnerability-reporting-for-a-repository; CONTRIBUTING https://docs.github.com/en/communities/setting-up-your-project-for-healthy-contributions/setting-guidelines-for-repository-contributors; labels https://docs.github.com/en/issues/using-labels-and-milestones-to-track-work/managing-labels; REST https://docs.github.com/en/rest/repos/repos
- Anthropic: https://code.claude.com/docs/en/legal-and-compliance.md; https://code.claude.com/docs/en/agent-sdk/overview (helper); https://www.anthropic.com/legal/trademark-guidelines (helper)
- Google: https://about.google/brand-resource-center/guidance/ (helper); https://partnermarketinghub.withgoogle.com/brands/google/use-cases/product-co-branding/ (helper); https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/; https://github.com/google-gemini/gemini-cli (helper)
- OpenAI: https://github.com/openai/codex README and LICENSE (helper); https://openai.com/brand/ (403)
- Search: https://developers.google.com/search/updates; https://developers.google.com/search/docs/fundamentals/ai-optimization-guide (helper); https://developers.google.com/search/docs/appearance/structured-data/software-app (helper); …/review-snippet (helper); https://developers.google.com/search/docs/appearance/site-names (helper); https://developer.chrome.com/docs/lighthouse/agentic-browsing/llms-txt (helper)
- llms.txt: https://llmstxt.org/ and https://raw.githubusercontent.com/AnswerDotAI/llms-txt/main/nbs/index.qmd (helper)
- Other names: https://giz.ai, https://www.giz.ai/mcp/, https://api.github.com/orgs/GizAI, `npm view @gizai/cli`, `npm view gizai` (404); https://ai.google.dev/competition/projects/giz-ai, https://pypi.org/project/gizai/, https://crates.io/api/v1/crates/gizai, https://api.github.com/search/repositories?q=gizai, https://www.bmz-digital.global/en/overview-of-initiatives/fair-forward/, https://gizmo.ai, https://www.gizatech.xyz/, https://gizza.ai (helper)
- Competitors and searches: https://vibekanban.com/ and https://www.vibekanban.com/blog/shutdown, https://www.conductor.build/, https://github.com/smtg-ai/claude-squad, https://emdash.com/, https://nimbalyst.com/, https://kangentic.com/, https://kanbots.dev/, https://github.com/stravu/crystal, https://github.com/andyrewlee/awesome-agent-orchestrators (helper; the search results in §3.8 are from the helper's searches)
