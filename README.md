# Gizai

A desktop app where a team of coding agents (Claude Code, Codex, Gemini or another coding CLI) works through your software tasks. You keep clients, projects and tasks in Gizai. The agents pick up the cards, build each one in its own git worktree, test each other's work and hand it to you for review. You can also ask the Team Lead agent for things in chat.

Everything runs on your machine, with your own git repositories and your own logins. It's early (v0.1). It runs on Linux, on Macs with Apple silicon and on Windows 11. MIT licence.

## Install

Gizai builds from source on your own computer, on every system: there are no downloads or installers. Its install script checks what is missing, builds Gizai and installs it for you alone, without sudo or administrator rights.

Every system needs:

- [Claude Code](https://docs.claude.com/en/docs/claude-code), installed and logged in (run `claude` once). Agents can also run on Codex, Gemini or another coding CLI you have installed and logged in (Settings → Coding CLIs); the Team Lead chat needs Claude Code;
- git, Rust ([rustup](https://rustup.rs)) and Node.js 20 or newer;
- optionally the [GitHub CLI](https://cli.github.com) and an SSH key on your GitHub account, to review cards as pull requests on GitHub. Settings → GitHub shows what's missing and can log gh in;
- or, for a project on Bitbucket Cloud, an SSH key on your Bitbucket account and an API token with the scopes `read:user:bitbucket`, `read:pullrequest:bitbucket` and `write:pullrequest:bitbucket` (Atlassian account → Security → API tokens), saved with your Atlassian email in Settings → Bitbucket. Gizai keeps them in your keychain.

### Linux

You also need WebKitGTK 4.1 (`./install.sh --check` names the packages for your distribution) and, for the tray icon, libayatana-appindicator: `libayatana-appindicator` on Arch and Omarchy, `libayatana-appindicator3-1` on Debian and Ubuntu (the .deb depends on it). Without it Gizai runs without a tray icon.

```sh
git clone --branch production https://github.com/Yotech-AI/gizai.git
cd gizai
./install.sh
```

The installer builds Gizai and installs it for your user in `~/.local`, with no sudo. It also adds Gizai to your app launcher. The `production` branch holds the released version; `main` is development.

| Command | What it does |
|---|---|
| `./install.sh --check` | Shows what is missing and the command that installs it |
| `./install.sh` | Installs, or updates (your data is backed up first) |
| `./install.sh --uninstall` | Removes Gizai and keeps your data in `~/.local/share/gizai` |

### macOS

For Macs with Apple silicon (M1 and later). You also need Apple's Command Line Tools, which bring git and a C compiler: run `xcode-select --install`. Node.js can come from Homebrew (`brew install node`), mise, nvm or the installer on nodejs.org.

```sh
git clone --branch production https://github.com/Yotech-AI/gizai.git
cd gizai
./install.sh
```

It's the same installer, with the same commands as on Linux. It builds Gizai.app and installs it in `~/.local/lib/gizai`, links it into the Applications folder in your home folder (`~/Applications`), and puts the `gizai` command in `~/.local/bin`. Open Gizai from there and keep it in the Dock. Its tray icon is in the menu bar.

### Windows

For Windows 11. You also need:

- the Microsoft C++ Build Tools, with "Desktop development with C++" ([download](https://visualstudio.microsoft.com/visual-cpp-build-tools/); the Rust installer offers to install them);
- Git for Windows: Claude Code runs its commands in Git for Windows' Git Bash;
- WebView2, which comes with Windows 11.

Rust, Node.js and Git can come from winget:

```powershell
winget install --id Rustlang.Rustup -e
winget install --id OpenJS.NodeJS.LTS -e
winget install --id Git.Git -e
```

Then, in a new PowerShell window:

```powershell
git clone --branch production https://github.com/Yotech-AI/gizai.git
cd gizai
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

`install.ps1` builds Gizai and installs it in `%LOCALAPPDATA%\Programs\Gizai`, without administrator rights. It adds Gizai to the Start menu with a shortcut that carries Gizai's app ID, which Windows needs to show Gizai's notifications. Gizai runs the coding CLIs installed on Windows itself, like `claude.exe` or npm's `gemini.cmd`. It doesn't use WSL.

| Command | What it does |
|---|---|
| `.\install.ps1 -Check` | Shows what is missing and how to install it |
| `.\install.ps1` | Installs, or updates (your data is backed up first) |
| `.\install.ps1 -Uninstall` | Removes Gizai and keeps your data in `%APPDATA%\Gizai` |

If PowerShell won't run scripts, start these the way the block above does: `powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Check`.

### Not signed

Gizai isn't signed yet: there is no Apple Developer account or Windows certificate until Gizai has paid features. A build made on your own computer isn't a download, so Gatekeeper and SmartScreen shouldn't stop it. If they still do:

- macOS: System Settings → Privacy & Security → Open Anyway, or `xattr -dr com.apple.quarantine ~/.local/lib/gizai/Gizai.app`;
- Windows: More info → Run anyway.

After an update, macOS may ask once whether Gizai may use what it saved in your keychain: choose Always Allow.

### Updates

Gizai asks GitHub for the latest release 20 seconds after it starts, then every six hours. When a newer one is out, **Update to X.Y.Z** shows above Company in the sidebar.

Click it and Gizai:

1. builds the new version from source in the background with your system's install script, while you keep working;
2. backs up your data;
3. installs the new version;
4. offers a restart.

If a step fails, the version you have keeps working, and Settings → Updates says why. Settings → Updates also has Check now, and switches the check off. You can still update from a terminal with `git pull`, then `./install.sh` (on Windows `.\install.ps1`).

### Your data

Gizai keeps your data in `~/.local/share/gizai` on Linux, `~/Library/Application Support/Gizai` on macOS and `%APPDATA%\Gizai` on Windows (`GIZAI_DATA_DIR` points it elsewhere). It cleans up after itself when it starts and once a day while it runs:

- **Run and chat logs** (`runs/` and `chat/`): kept for 30 days after the run, chat answer or board check ended, then removed. The run stays in the history with its summary, cost and commits; Show output says its log is gone.
- **Keys for the Team Lead's tools** (`api_tokens` in `gizai.db`): each one lasts a chat answer or board check, and is removed a day after it expired or was revoked.
- **Backups** (`backups/`): one before every update, install and database upgrade, and `gizai --backup` makes one when you ask. The newest 20 are kept. Their names use your local time, like `gizai-before-update-20261009-143502-123.db`.

## Getting started

1. Install Claude Code or Codex and sign in to it. The Team Lead chat needs Claude Code.
2. For projects on GitHub, install gh and sign in (`gh auth login`, or Settings → GitHub): QA opens the pull requests with it.
3. Open Gizai: the workflow and five agents are ready. The Backend and Frontend Agents work the cards in To do, the QA Agent tests them, the DevOps Agent releases what you merged when you press Run in Deploy, and you talk to the Team Lead on the Chat page. With only Codex installed, Gizai adds it under Settings → Coding CLIs and runs every agent but the Team Lead on it.
4. Add a project with its git repository, and put a card in To do.

Tools for agents: web search, fetching pages, a hidden browser for testing web pages, the CLI's own built-in tools and MCP servers (Settings → MCP servers) are switched on per agent in the agent form → Tools. Everything is off until you switch it on.

![Gizai: the board, a live agent run, the Team Lead chat, the team and an agent](docs/gizai.gif)

## Features

- Clients with contacts, projects with docs and files
- Tasks on a board or in a list, with labels, priorities, acceptance criteria and an Inbox
- Agents for each role (frontend, backend, design, QA, DevOps), with their own model, effort and allowed commands
- Folders per agent besides its worktree, each set to read or read and change (agent form → Permissions). They limit the agent's file tools, not the commands it runs; `/`, your home folder, Gizai's data and folders with keys are refused
- Each agent runs on the coding CLI you pick: Claude Code, Codex, Gemini, any other CLI (its output is read as text), or a second account of one with its own environment (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`). Settings → Coding CLIs adds them, or finds the ones installed
- A git worktree and branch for every card, started from main on GitHub or Bitbucket when the project is linked
- Worktrees that start warm: per project, paths copied from your checkout (`cp --reflink=auto`), missing dependencies installed (`composer install`, `npm ci`) and a setup command; a new card takes over a finished card's worktree, and Settings → Data removes old ones
- A card flow: Backlog → To do → In progress → Testing → Review → Deploy → Done, set up on the Team page (Team → Workflow). Each column holds the agents that work its cards, is Auto (its agents pick up its cards by priority as they have room) or Manual (only Run starts one), and links to the column its cards go to next. Add your own columns (a Design column with a design agent, linked to Review), drag them into any order, or remove one
- The column decides, labels don't: starting a card in To do moves it to To do's next column, and an agent's done answer moves it on (In progress → Testing → your Review). A card assigned to an agent is started only by that agent; a failed test sends a card back to the column it came from, to its builder. A card with Testing off skips Testing columns, and a DevOps run never sends a card to QA
- Labels are tags for people, such as Must have and Could have: create, rename, recolour and remove them on the Team page
- Review on GitHub: Open pull request pushes a card's branch over SSH with your keys (or HTTPS with gh's login) and opens its pull request with gh; a merge on GitHub moves the card to Review's next column (Deploy; Done for a team without a Deploy column) and removes its worktree
- Review on Bitbucket Cloud works the same way: the push goes over SSH with your keys, the pull request through Bitbucket's API with your email and API token; declined and superseded pull requests show as closed
- Archive a card in Done: it leaves the board and every list, and keeps its ID, comments, runs and branch. The bin on the Tasks page lists archived cards, and Restore puts one back in Done
- Deploy: Manual by default, so no agent starts there by itself. Press Run for the agent on the column (its `deployed` moves the card to Deploy's next column, Done), or deploy it yourself and drag the card to Done
- Settings → GitHub: whether gh is found and logged in, how pushes go, Check connection for every linked project, and Log in with GitHub; Gizai never stores a token or password
- Agents that start when you press Run or when a card lands in an Auto column they are on (or is assigned to them there), on several cards at once. A start that can't work (Claude Code missing or not logged in, a wrong model, no repository) holds the card "blocked" without counting as a failed run, and that agent takes no new cards until you start or edit it (or Gizai restarts)
- Live run output, run history with the reason each run ended, what started it and the commits it made, and Continue, with a note for the agent if you write one. A run that ends without its result is continued once by itself (Nudge in the Runs list)
- Run this for me: an agent that needs a command it may not run (sudo, an install) asks you for it. The card waits at the top of your Inbox with the exact commands, a copy button each, and Done, continue, which resumes the agent's run
- After every run Gizai pushes the card's branch itself, the way Open pull request does, so an agent whose `git push` was refused doesn't hold the card up; a push that fails holds the card with the reason
- The Team Lead chat, which manages clients, projects, tasks and agents with Gizai's own tools
  - Runs on, under the text box, moves one chat to another Claude Code account (Settings → Coding CLIs), for example when one is near its weekly limit. The new account gets the conversation handed over, and the chat notes the switch. An answer that hits a usage limit offers Answer on another account.
  - Messages you send while the Team Lead answers wait in a queue and go together when the answer is done. After a stopped or failed answer they wait for Send now.
  - Recent shows the 30 chats with the newest activity. Archive, under the list, shows every chat, newest first, and searches their titles and the messages you and the Team Lead wrote. No chat is hidden or deleted: a new message in an old chat moves it back to the top of Recent.
  - + next to the text box adds files to a message (Add files, or drop them on the Chat page) or links an item. A message's files are kept with it; the Team Lead reads a copy of each in its own folder and can attach one to a task, project or client.
- @ in the chat and in every Markdown text box links a task, project, client, agent, person or doc: only @ lists the kinds, `@task.` (and the others) one kind, and typing searches by ID, key or name. `@task.` shows the tasks under a heading per column, in the board's order. The link shows as a chip with the item's name, which opens the item (Ctrl+click while you edit)
- The Team Lead's board check (agent settings → Chat → Check the board every 15 min, off until you turn it on): it looks for answered questions, held and stuck cards, and cards no agent will start. Only something new starts it: it gets agents going again (Continue or Run) and asks you what it can't decide in a chat of its own, labelled Question or Approval, at the top of your Inbox. Dragging a held card back to To do or In progress takes the hold off
- The Team Lead reads its own read-only copy of each active project's code (`<data dir>/code/<KEY>`, a worktree without a branch), kept at the commit a new card starts from and refreshed before each answer; your own checkouts are never passed to it
- When a project's linked folder has dependencies behind main (`vendor/` or `node_modules/` missing, or a lock file that differs from main's), the Team Lead tells you what an update would do and, after your yes, updates it: a fast-forward to main (switching branch only if you agreed), then `composer install` or `npm ci`, never a merge, reset or the setup command
- An org chart of your team: the Team Lead on top, then branches (Design, Development, Quality, Operations and your own), each with one empty spot that adds an agent there. Drag an agent onto a column in Team → Workflow to put it to work there
- Time and tool-call limits per run, a spending limit per run, a monthly budget per agent
- Usage (in Company): the agents' input tokens (cache included), output tokens and API cost for today, 7 days, 30 days or this month, in total with a bar per day, per agent and per project, with the Team Lead's chat on its own line. The Projects list shows each project's API cost this month. API cost is what the tokens would cost at API prices, not a bill; a CLI that reports no cost (Codex, Gemini) shows its tokens and an unknown cost
- Usage → Subscription (the tab the page opens on): a block per coding CLI in Settings, so two accounts of one CLI show apart. A Claude Code account shows its session, weekly and Fable limits, a Codex account its plan's two windows (usually 5 hours and a week): how much is used, when it resets and when the number was read, amber from 80% and red when reached, with the agents on it and the chats that run there, so you see whom to move to another account. The numbers are the newest the CLI itself reported in a run or chat turn: Claude Code in its stream, Codex in its session log (`$CODEX_HOME/sessions`), and a run that hit a limit; Gizai never reads their logins or asks Anthropic or OpenAI. Gemini and other CLIs say Gizai can't read their limits yet
- Gizai keeps running when you close its window (Super+W on Omarchy, the X button): the agents, the board check and the chat go on. A tray icon (top right in Omarchy's Waybar, the menu bar on macOS) has Open Gizai and Quit Gizai completely; starting Gizai again also brings the window back. Settings → Quit quits too, and says how many runs and chat answers it stops
- Desktop notifications when a card goes on hold, a card waits for your review or deploy, the Team Lead asks you something (a Question or Approval chat), or the Team Lead answered while the Gizai window was hidden or in the background. Each kind has a switch in Settings → Notifications, all on by default. Clicking a notification while it shows opens the card or the chat, where the desktop's notifications support it (Omarchy's do); otherwise Open Gizai in the tray brings the window back
- Settings in tabs: General, Appearance, Notifications, Agents and runs, MCP servers, and GitHub and Bitbucket. Settings → Appearance picks one font for the whole app (Atkinson Hyperlegible, JetBrains Mono, Inter, Geist or Hack, all bundled), a text size for the chat, for the interface, and for tasks and docs (reading text grows, headings half as much, IDs and labels stay put; the chat column gets wider with it), and the theme and density (also the t and d keys). Changes show at once and are kept on this computer
- Updates from GitHub Releases: a notice in the sidebar, a build in the background, a backup first, then a restart
- A backup before every update, one Gizai per data folder, no telemetry (the release check only asks GitHub for the latest release)

## How agent runs work

Agent runs are headless: nobody is there to approve anything while one runs, so a command or tool call that needs approval is refused.

- **A temp folder per card.** Each card's worktree has a `.gizai-tmp/` folder. Before every run Gizai makes it, empty, and sets `TMPDIR`, `TMP` and `TEMP` to it for every CLI. So `mktemp`, PHP's `sys_get_temp_dir()`, Node's `os.tmpdir()` and Python's `tempfile` write inside the worktree, where the agent may write. Gizai adds `/.gizai-tmp/` once to the repository's `.git/info/exclude`, which your checkout and every worktree share, so the folder never shows in `git status`; `.gitignore` is left alone. The folder is emptied when the run ends, also after Stop or a limit, and goes with the worktree.
- **The rules in every prompt.** Every task prompt, new or continued, ends with "How this run works": nobody can approve anything, the commands the agent may run (its allowed list), the shell rules of its CLI, the temp folder's path, and that Gizai pushes the branch when the run ends. For Claude Code in acceptEdits mode (the default), checked against Claude Code 2.1.289:
  - commands and file tools reach only the worktree and the agent's folders: `ls /tmp`, `ls ..`, a redirect to `/tmp` and the Write tool on `/tmp` are refused, also for an allowed command;
  - `$(…)`, backticks, variables such as `$TMPDIR` and heredocs with an unquoted delimiter (`<<EOF`) are refused;
  - pipes, `2>&1`, `&&` and `;` between allowed commands work, and so does a redirect to a file in the worktree.

  Codex and Gemini hear only what holds for them: Codex asks for nothing and its sandbox blocks what it doesn't allow; Gemini hears its allowed commands.
- **Waiting in a run.** Ending its message ends an agent's run, and nothing wakes it up later. So "How this run works" also says how to wait for something outside the run, like a CI run, a release or deploy workflow or a pull request's checks: check it in the foreground about once a minute (one check, then `sleep 45`, in one command, and again), and when it won't be done before the run's limit, end with the result line and say what is left. `sleep` is named only for an agent that may run it. Checked against Claude Code 2.1.289: a command that starts with a sleep of more than 20 seconds is blocked, most shell loops are refused, a command that runs more than 2 minutes (or its own timeout, at most 10) is moved to the background, and a background command is stopped when the message ends.
- **One nudge.** A run that ends normally without its `GIZAI_RESULT` line is continued once by itself, in the same session, like Continue: Gizai tells the agent that nothing wakes it up later, to check in the foreground now if it was waiting, and to end with its result line. It never does this after Stop, a time or tool-call limit, a failed run or Gizai quitting, and only when a start is allowed now (agents not paused, the agent active, within its budget and cards at once, and room in Runs at once); otherwise nothing changes. If the nudged run also ends without a result, the card goes on hold "stalled" and shows in the Inbox. The Runs list shows the nudge as Nudge (trigger `result_nudge`), apart from a Continue.
- **Continue with a note.** The box above Continue takes a note for the agent, like "use the existing CSV writer". The continued run hears it next to why its last run stopped, and it is saved on the card as your comment. The Team Lead's `continue_agent_run` takes a `note` the same way; on a run that asked for a decision, the Team Lead's note counts as an answer.
- **Run this for me.** An agent that can't finish without a command it may not run (sudo, an install, a command its list refuses) ends with `needs_decision` and names the commands in `run_for_me` on its result line, like `GIZAI_RESULT: {"outcome":"needs_decision","summary":"…","issues":[],"run_for_me":["sudo pacman -S libayatana-appindicator"]}` (older result lines still work). The card goes on hold and shows at the top of the Inbox and on its Run panel, with each command exactly as written and a copy button. Run them in a terminal and press Done, continue: the run continues with your note that the commands were run (also saved on the card), so the agent checks that they worked and carries on. The commands still show when Gizai's push after that run failed. "How this run works" and the role instructions tell agents how to ask.
- **Agents ask the Team Lead first.** An agent that ends its run asking for a decision (`needs_decision`) asks the Team Lead before you (Settings → Runs → Ask the Team Lead first, on by default). The card stays on hold but out of your Inbox ("With the Team Lead"), while a short run of the Team Lead reads the question, looks in memory, the card, the docs and its copies of the code, and either answers (its comment on the card, the agent's session continues with it as the Continue note, and the answer is kept in memory, linked to the card) or asks you: a comment "Needs <you>: why, the options, my advice", and the card lands in the Inbox with "Team Lead escalated to you: why". Money, scope, deadlines, client messages, security, deleting and anything it can't find always come to you, and so does any error, timeout or run without an answer. At most one try per question and two per card, and the question right after a Team Lead answer comes to you. Run this for me requests and failed pushes skip it. When you answer a question it escalated, the Team Lead keeps your answer in memory as the agent starts again. The Runs tab shows who answered and what the Team Lead's look cost (on its budget). See `docs/memory.md`.
- **Gizai pushes after every run.** When a run on a card ends (also after Stop or a limit) and its project is linked to GitHub, Bitbucket or a git URL, Gizai pushes the card's branch itself when it has commits the remote doesn't have: to the same remote and over the same Push over setting as Open pull request, never by force, and before the card moves on, so the next agent (QA, for its pull request) finds the branch. Only commits go: uncommitted changes stay in the worktree, and the run's output says how many there are. The run's output and the Run panel say "Gizai pushed <branch> (3 commits)", or why it couldn't. A push that fails (the branch on GitHub has commits this one doesn't, no access, no network) is not forced or tried again: the card stays where it is, on hold "blocked" with the reason, and shows in the Inbox. So a `git push` the agent's CLI refuses no longer holds a card up, and "How this run works" tells agents so; they keep `git push` in their list.
- **New agents' commands.** A new agent starts with its role's allowed commands. Gizai's default list has the usual git, package manager and test commands, the read-only helpers agents use in pipes (`head`, `tail`, `wc`, `sort`, `uniq`, `cut`, `diff`, `grep`, `jq`, `pwd`, `which` and `tree`), and `sleep`, to wait between checks. The Team Lead gets that list; builders (Backend, Frontend, Design and your own roles) also get `git push`, `git pull`, `git fetch`, a few read-only git commands, `node`, `echo` and `printf`; QA gets the builders' list plus `gh pr create`, `list`, `view` and `edit`; DevOps gets a list of its own for releases. An agent without a list runs with Gizai's default list. An agent with its own list needs `Bash(sleep:*)` added to it to wait between checks.
- **Refused in this run.** Claude Code reports every tool call it refused, with the reason. Gizai saves them on the run: the Run panel lists them under "Refused in this run", Show output marks each one where it happened, and the Team Lead's `get_task` and `get_agent` return them for each run. Codex and Gemini don't report refusals, so their runs list none.

## Tools for agents: MCP servers, the web and a hidden browser

Agents can use outside services through MCP servers, like Otus OS. Settings → MCP servers holds one list for all agents: add a server by hand (a command, or an address) or **Import from Claude Code**, which reads the servers your Claude Code accounts already have (read only, values into the keychain). **List tools** shows what each tool does, its parameters and its risk before you switch anything on. A server that asks for it gets **Sign in**: Gizai signs in as a device of its own, in your default browser.

The agent form → Tools also has:

- **Web:** search the web, and fetch pages (any, or only the domains you list). Claude Code gets `WebSearch` and `WebFetch`; Codex its own web search; Gemini fetches pages (its web search is on by its own policy).
- **Browser:** a hidden Chrome for testing web pages, through Chrome DevTools MCP, the built-in entry at the top of Settings → MCP servers. It always runs headless with a throwaway profile: never on your screen, never your own browser, profile or logins. It needs Node 20.19 or newer with npx, and Google Chrome or Chromium (never Brave); the form says what is missing. Gizai installs none of it: `npx` fetches the pinned server, and the server starts the browser.
- **Built-in tools:** what the agent's CLI offers, from Gizai's catalog merged with what the CLI itself reports (Claude Code's list from the agent's last run, or **Ask Claude Code again**, which starts it without a login). Each says what it allows and how risky it is.

Everything is off until you switch it on per agent, with a switch per server and per tool: a web or built-in tool named in an agent's allowed commands is left out of its runs. Gizai builds no tools of its own: each one is the CLI's own or comes from an MCP server. Environment values, headers and sign-in tokens stay in the OS keychain, never in Gizai's database. After a chat answer used an outside tool (an MCP server, the web, the browser), the Team Lead asks you to confirm before it starts runs or changes agents or columns. MCP servers and the browser work on Claude Code; Codex and Gemini agents show them disabled, with why. Details: [docs/agent-tools.md](docs/agent-tools.md).

## Development

```sh
source scripts/env.sh
cargo test --workspace && npm test
scripts/ui-test.sh        # UI tests in a headless compositor, never on your desktop
scripts/readme-gif.sh     # remakes the GIF above
```

On every pull request and every push to `main`, CI (`.github/workflows/ci.yml`) builds Gizai and runs `cargo test --workspace` and `npm test` on Linux, macOS (Apple silicon) and Windows, and checks the install scripts there. A pull request that changes an install script also installs Gizai from source on macOS and Windows with it (`.github/workflows/install.yml`).

`CLAUDE.md` has the rules for working in this repository, `docs/HANDOFF-2026-10-07.md` explains how it is built, and `docs/RELEASING.md` how a release is made.

## Licence

MIT, see [LICENSE](LICENSE). Gizai is not affiliated with Anthropic. Claude and Claude Code are trademarks of Anthropic.
