<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="design/brand/logo-horizontal-on-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="design/brand/logo-horizontal-on-light.svg">
    <img alt="GizAi" src="design/brand/logo-horizontal-on-light.svg" width="320">
  </picture>
</p>

<div align="center">

**A team of AI agents works through your board, on your own computer.**

[Install](#install) · [Docs](#documentation) · [Releases](https://github.com/Yotech-AI/gizai/releases) · [Licence](LICENSE)

![GizAi: the board, a live agent run, the Team Lead's chat, the team and an agent](docs/gizai.gif)

</div>

## What it is

GizAi is an Agentic Development Application (ADA): a team of AI agents builds your software through your board, while you review and decide. Each card gets its own git worktree, an agent builds it, a QA agent tests it, and it lands in your Review. Everything runs on your own computer, with your own git repositories, logins and coding CLIs.

## Features

- **The board decides who works.** Each column holds the agents that work its cards, so cards move from builder to tester to you by themselves. [Read more](docs/features/board.md)
- **A Team Lead you talk to.** Ask the Team Lead in chat to plan work, make cards or start agents, and Approve with one click (or Approve all) the steps that wait for your OK. [Read more](docs/features/team-lead.md)
- **Tested before you see it.** A QA agent tests every card and opens its pull request, so what reaches your Review has passed. [Read more](docs/features/testing.md)
- **A code editor, free.** Read what a card changed, and edit a project's files with a file tree, tabs and search. [Read more](docs/features/code-editor.md)
- **Terminals, free.** Open your own shell in a project's folder or a card's worktree, as many as you like. [Read more](docs/features/terminal.md)
- **Bring your own CLI.** Each agent runs on the coding CLI you pick, like Claude Code, Codex or Gemini, on your own login. [Read more](docs/features/cli.md)
- **Memory.** The Team Lead, the agents and you keep notes for later, and each run gets the notes for its card. [Read more](docs/features/memory.md)
- **Tools per agent.** Give each agent only the commands, folders, MCP servers, web access and hidden browser it needs. [Read more](docs/features/tools.md)
- **The Inbox.** Questions, approvals, commands to run and cards to review wait for you in one list. [Read more](docs/features/inbox.md)
- **Local-first and open.** Your code, data and logins stay on your computer, with no telemetry, under the MIT licence. [Read more](docs/features/local-first.md)

## Install

GizAi builds from source and installs for you alone, without sudo or administrator rights. Every system needs git, Rust, Node.js 20 or newer, and a coding CLI you're logged in to, like Claude Code.

- **Linux:** also WebKitGTK 4.1, and for the tray icon `libayatana-appindicator` on Arch and Omarchy or `libayatana-appindicator3-1` on Debian and Ubuntu. `./install.sh --check` names the packages.
- **macOS** on Apple silicon: also Apple's Command Line Tools, from `xcode-select --install`.

```sh
git clone --branch production https://github.com/Yotech-AI/gizai.git
cd gizai
./install.sh
```

- **Windows 11:** also the Microsoft C++ Build Tools and Git for Windows. Run this in PowerShell:

```powershell
git clone --branch production https://github.com/Yotech-AI/gizai.git
cd gizai
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

The details are in [docs/install.md](docs/install.md): what each system needs, updates, your data, and what to do when macOS or Windows warns.

## Getting started

1. Install a coding CLI, like Claude Code or Codex, and log in to it. The Team Lead's chat needs Claude Code.
2. For projects on GitHub, install gh and log in (`gh auth login`, or Settings → GitHub): QA opens the pull requests with it.
3. Open GizAi: the workflow and five agents are ready. Talk to the Team Lead on the Chat page.
4. Add a project with its git repository, and put a card in To do.
5. The Backend or Frontend Agent builds it, the QA Agent tests it, and it lands in your Review.

## Documentation

- [Install](docs/install.md): what each system needs, updates, your data, and when macOS or Windows warns.
- [Settings](docs/settings.md): what each tab holds, how GizAi looks, and the tray.
- Features: [Board](docs/features/board.md) · [Team Lead](docs/features/team-lead.md) · [Testing](docs/features/testing.md) · [Code editor](docs/features/code-editor.md) · [Terminals](docs/features/terminal.md) · [CLIs](docs/features/cli.md) · [Memory](docs/features/memory.md) · [Tools](docs/features/tools.md) · [Inbox](docs/features/inbox.md) · [Local-first](docs/features/local-first.md)
- [Memory in depth](docs/memory.md): folders, properties, links, search, and how runs get notes.
- [Agent tools in depth](docs/agent-tools.md): MCP servers, signing in, the web, the browser and the safety rules.
- [Linux, macOS and Windows](docs/PLATFORMS.md): what differs on each system.
- [Releasing](docs/RELEASING.md): how a release is made, and how GizAi updates itself.
- [Paid features](docs/PAID-FEATURES.md): the plan for paid hosted services; the app stays free.

## Development

```sh
source scripts/env.sh
cargo test --workspace && npm test
scripts/ui-test.sh        # UI tests in a headless compositor, never on your desktop
scripts/readme-gif.sh     # remakes the GIF above
```

On every pull request and every push to `main`, CI (`.github/workflows/ci.yml`) builds GizAi and runs `cargo test --workspace` and `npm test` on Linux, macOS (Apple silicon) and Windows, and checks the install scripts there. A pull request that changes an install script also installs GizAi from source on macOS and Windows with it (`.github/workflows/install.yml`).

[CLAUDE.md](CLAUDE.md) has the rules for working in this repository, [docs/HANDOFF-2026-10-07.md](docs/HANDOFF-2026-10-07.md) explains how it is built, and [docs/RELEASING.md](docs/RELEASING.md) how a release is made.

## Licence

MIT, see [LICENSE](LICENSE). GizAi is not affiliated with Anthropic. Claude and Claude Code are trademarks of Anthropic.
