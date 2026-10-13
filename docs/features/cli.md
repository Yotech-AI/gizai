# Bring your own CLI

Each agent runs on the coding CLI you pick, on your own login: Claude Code, Codex, Gemini or another.

## Pick a CLI per agent

- **Claude Code, Codex, Gemini**, or any other coding CLI, whose output GizAi reads as text.
- **A second account** of one, with its own environment (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`).
- **Settings → Coding CLIs** adds them, or finds the ones installed. Each agent picks its CLI under Runs on, in its form.

With only Codex installed, GizAi adds it under Settings → Coding CLIs and runs every agent but the Team Lead on it.

## The Team Lead needs Claude Code

The Team Lead's chat runs on Claude Code. Runs on, under the chat's text box, moves a chat to another Claude Code account: see [The chat](team-lead.md#the-chat).

## What each CLI gets

- **Claude Code** gets everything: the Team Lead's chat, MCP servers and the browser, and slash commands and skills.
- **Codex** has its own web search, and its sandbox blocks what it doesn't allow.
- **Gemini** fetches pages, and searches the web by its own policy.
- **Other CLIs** run with their output read as text.

Memory works the same on every CLI. Which tools each CLI can use: [Tools per agent](tools.md).

## Usage

Usage (in Company) shows the agents' input tokens (cache included), output tokens and API cost for today, 7 days, 30 days or this month. It shows them in total with a bar per day, per agent and per project, with the Team Lead's chat on its own line.

- The Projects list shows each project's API cost this month.
- API cost is what the tokens would cost at API prices, not a bill.
- A CLI that reports no cost (Codex, Gemini) shows its tokens and an unknown cost.

## Subscription limits

Usage → Subscription, the tab the page opens on, has a block per coding CLI in Settings, so two accounts of one CLI show apart.

- A Claude Code account shows its session, weekly and Fable limits; a Codex account its plan's two windows (usually 5 hours and a week).
- Each shows how much is used, when it resets and when the number was read, amber from 80% and red when reached.
- Each lists the agents on it and the chats that run there, so you see whom to move to another account.
- Gemini and other CLIs say GizAi can't read their limits yet.

The numbers are the newest the CLI itself reported in a run or chat turn: Claude Code in its stream, Codex in its session log (`$CODEX_HOME/sessions`), and a run that hit a limit. GizAi never reads their logins or asks Anthropic or OpenAI.

## Your own login

Every agent runs on your own login of its CLI. GizAi never sells Claude usage: agents keep running on your own Claude Code login or API key. On Windows, GizAi runs the CLIs installed on Windows itself, never inside WSL ([Install](../install.md#windows)).

## Read more

- [Tools per agent](tools.md): what each agent may use, per CLI.
- [docs/PLATFORMS.md](../PLATFORMS.md#reaching-the-coding-clis): how GizAi finds the coding CLIs on Linux, macOS and Windows.
- [docs/PAID-FEATURES.md](../PAID-FEATURES.md): what GizAi will sell, and why it never sells Claude usage.
