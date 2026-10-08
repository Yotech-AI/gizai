# Gizai

A desktop app where a team of coding agents (Claude Code, Codex, Gemini or another coding CLI) works through your software tasks. You keep clients, projects and tasks in Gizai. The agents pick up the cards, build each one in its own git worktree, test each other's work and hand it to you for review. You can also ask the Team Lead agent for things in chat.

Everything runs on your machine, with your own git repositories and your own logins. It's early (v0.1) and runs on Linux only for now. MIT licence.

## Install

You need:

- Linux with WebKitGTK 4.1;
- [Claude Code](https://docs.claude.com/en/docs/claude-code), installed and logged in (run `claude` once). Agents can also run on Codex, Gemini or another coding CLI you have installed and logged in (Settings → Coding CLIs); the Team Lead chat needs Claude Code;
- git, Rust ([rustup](https://rustup.rs)) and Node.js 20 or newer;
- optionally the [GitHub CLI](https://cli.github.com) and an SSH key on your GitHub account, to review cards as pull requests on GitHub. Settings → GitHub shows what's missing and can log gh in.

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

### Updates

Gizai asks GitHub for the latest release 20 seconds after it starts, then every six hours. When a newer one is out, **Update to X.Y.Z** shows above Company in the sidebar.

Click it and Gizai:

1. builds the new version from source in the background, while you keep working;
2. backs up your data;
3. installs the new version;
4. offers a restart.

If a step fails, the version you have keeps working, and Settings → Updates says why. Settings → Updates also has Check now, and switches the check off. You can still update from a terminal with `git pull` and `./install.sh`.

![Gizai: the board, a live agent run, the Team Lead chat, the team and an agent](docs/gizai.gif)

## Features

- Clients with contacts, projects with docs and files
- Tasks on a board or in a list, with labels, priorities, acceptance criteria and an Inbox
- Agents for each role (frontend, backend, design, QA, DevOps), with their own model, effort and allowed commands
- Folders per agent besides its worktree, each set to read or read and change (agent form → Permissions). They limit the agent's file tools, not the commands it runs; `/`, your home folder, Gizai's data and folders with keys are refused
- Each agent runs on the coding CLI you pick: Claude Code, Codex, Gemini, any other CLI (its output is read as text), or a second account of one with its own environment (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`). Settings → Coding CLIs adds them, or finds the ones installed
- A git worktree and branch for every card, started from main on GitHub when the project is linked
- Worktrees that start warm: per project, paths copied from your checkout (`cp --reflink=auto`), missing dependencies installed (`composer install`, `npm ci`) and a setup command; a new card takes over a finished card's worktree, and Settings → Data removes old ones
- A card flow: Backlog → To do → In progress → Testing → Review → Deploy → Done. To do is a queue: agents take its cards by priority as they have room, and a card moves to In progress when its agent starts
- Routing rules and a QA gate: In progress → Testing → your Review. A card with Testing off goes straight to Review, and a DevOps run never sends a card to QA
- Review on GitHub: Open pull request pushes a card's branch over SSH with your keys (or HTTPS with gh's login) and opens its pull request with gh; a merge on GitHub moves the card to Deploy (Done for a team without a Deploy column) and removes its worktree
- Archive a card in Done: it leaves the board and every list, and keeps its ID, comments, runs and branch. The bin on the Tasks page lists archived cards, and Restore puts one back in Done
- Deploy: no agent starts there by itself. Press Run for the DevOps Agent (its `deployed` moves the card to Done), or deploy it yourself and drag the card to Done. Team → Workflow → Add column adds the Deploy column after Review
- Settings → GitHub: whether gh is found and logged in, how pushes go, Check connection for every linked project, and Log in with GitHub; Gizai never stores a token or password
- Agents that start when you press Run, when a card is assigned, or on a heartbeat, on several cards at once. A start that can't work (Claude Code missing or not logged in, a wrong model, no repository) holds the card "blocked" without counting as a failed run, and that agent takes no new cards until you start or edit it (or Gizai restarts)
- Live run output, run history with the reason each run ended and the commits it made, and Continue
- The Team Lead chat, which manages clients, projects, tasks and agents with Gizai's own tools
- The Team Lead's board check (agent settings → Chat → Check the board every 15 min, off until you turn it on): it looks for answered questions, held and stuck cards, and cards no agent will start. Only something new starts it: it gets agents going again (Continue or Run) and asks you what it can't decide in a chat of its own, labelled Question or Approval, at the top of your Inbox. Dragging a held card back to To do or In progress takes the hold off
- The Team Lead reads its own read-only copy of each active project's code (`<data dir>/code/<KEY>`, a worktree without a branch), kept at the commit a new card starts from and refreshed before each answer; your own checkouts are never passed to it
- When a project's linked folder has dependencies behind main (`vendor/` or `node_modules/` missing, or a lock file that differs from main's), the Team Lead tells you what an update would do and, after your yes, updates it: a fast-forward to main (switching branch only if you agreed), then `composer install` or `npm ci`, never a merge, reset or the setup command
- An org chart of your team
- Time and tool-call limits per run, a spending limit per run, a monthly budget per agent
- Updates from GitHub Releases: a notice in the sidebar, a build in the background, a backup first, then a restart
- A backup before every update, one Gizai per data folder, no telemetry (the release check only asks GitHub for the latest release)

## How agent runs work

Agent runs are headless: nobody is there to approve anything while one runs, so a command or tool call that needs approval is refused.

- **A temp folder per card.** Each card's worktree has a `.gizai-tmp/` folder. Before every run Gizai makes it, empty, and sets `TMPDIR`, `TMP` and `TEMP` to it for every CLI. So `mktemp`, PHP's `sys_get_temp_dir()`, Node's `os.tmpdir()` and Python's `tempfile` write inside the worktree, where the agent may write. Gizai adds `/.gizai-tmp/` once to the repository's `.git/info/exclude`, which your checkout and every worktree share, so the folder never shows in `git status`; `.gitignore` is left alone. The folder is emptied when the run ends, also after Stop or a limit, and goes with the worktree.
- **The rules in every prompt.** Every task prompt, new or continued, ends with "How this run works": nobody can approve anything, the commands the agent may run (its allowed list), the shell rules of its CLI and the temp folder's path. For Claude Code in acceptEdits mode (the default), checked against Claude Code 2.1.289:
  - commands and file tools reach only the worktree and the agent's folders: `ls /tmp`, `ls ..`, a redirect to `/tmp` and the Write tool on `/tmp` are refused, also for an allowed command;
  - `$(…)`, backticks, variables such as `$TMPDIR` and heredocs with an unquoted delimiter (`<<EOF`) are refused;
  - pipes, `2>&1`, `&&` and `;` between allowed commands work, and so does a redirect to a file in the worktree.

  Codex and Gemini hear only what holds for them: Codex asks for nothing and its sandbox blocks what it doesn't allow; Gemini hears its allowed commands.
- **New agents' commands.** New agents start with the usual git, package manager and test commands, and the read-only helpers agents use in pipes: `head`, `tail`, `wc`, `sort`, `uniq`, `cut`, `diff`, `grep`, `jq`, `pwd`, `which` and `tree`.
- **Refused in this run.** Claude Code reports every tool call it refused, with the reason. Gizai saves them on the run: the Run panel lists them under "Refused in this run", Show output marks each one where it happened, and the Team Lead's `get_task` and `get_agent` return them for each run. Codex and Gemini don't report refusals, so their runs list none.

## Development

```sh
source scripts/env.sh
cargo test --workspace && npm test
scripts/ui-test.sh        # UI tests in a headless compositor, never on your desktop
scripts/readme-gif.sh     # remakes the GIF above
```

`CLAUDE.md` has the rules for working in this repository, `docs/HANDOFF-2026-10-07.md` explains how it is built, and `docs/RELEASING.md` how a release is made.

## Licence

MIT, see [LICENSE](LICENSE). Gizai is not affiliated with Anthropic. Claude and Claude Code are trademarks of Anthropic.
