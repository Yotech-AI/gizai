# Gizai

A desktop app where a team of Claude Code agents works through your software tasks. You keep clients, projects and tasks in Gizai. The agents pick up the cards, build each one in its own git worktree, test each other's work and hand it to you for review. You can also ask the Team Lead agent for things in chat.

Everything runs on your machine, with your own git repositories and your own Claude Code login. It's early (v0.1) and runs on Linux only for now. MIT licence.

## Install

You need:

- Linux with WebKitGTK 4.1;
- [Claude Code](https://docs.claude.com/en/docs/claude-code), installed and logged in (run `claude` once);
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
- A git worktree and branch for every card, started from main on GitHub when the project is linked
- Worktrees that start warm: per project, paths copied from your checkout (`cp --reflink=auto`), missing dependencies installed (`composer install`, `npm ci`) and a setup command; a new card takes over a finished card's worktree, and Settings → Data removes old ones
- Routing rules and a QA gate: In progress → Testing → your Review
- Review on GitHub: Open pull request pushes a card's branch over SSH with your keys (or HTTPS with gh's login) and opens its pull request with gh; a merge on GitHub moves the card to Done and removes its worktree
- Settings → GitHub: whether gh is found and logged in, how pushes go, Check connection for every linked project, and Log in with GitHub; Gizai never stores a token or password
- Agents that start when you press Run, when a card is assigned, or on a heartbeat, on several cards at once
- Live run output, run history with the reason each run ended, and Continue
- The Team Lead chat, which manages clients, projects, tasks and agents with Gizai's own tools
- An org chart of your team
- Time and tool-call limits per run, a spending limit per run, a monthly budget per agent
- Updates from GitHub Releases: a notice in the sidebar, a build in the background, a backup first, then a restart
- A backup before every update, one Gizai per data folder, no telemetry (the release check only asks GitHub for the latest release)

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
