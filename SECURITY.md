# Security policy

## Supported versions

Only the latest release gets security fixes. GizAi updates itself from its releases ([docs/RELEASING.md](docs/RELEASING.md)), so a fix reaches you as an update: **Update to X.Y.Z** above Company in the sidebar, or Settings → Updates → Check now.

## Report a problem privately

Use GitHub's private vulnerability reporting: this repository's **Security** tab → **Report a vulnerability**, or [the report form](https://github.com/Yotech-AI/gizai/security/advisories/new). Only you and the maintainers see the report.

That form is the only way to report a security problem. Never report one in a public issue, a discussion or a pull request.

## What counts

- An agent getting past what you allowed it: its commands, folders or tools ([Tools per agent](docs/features/tools.md), and the [safety rules](docs/agent-tools.md#safety-rules) in depth).
- The Team Lead's approval step: a call that should wait for your Approve and runs without it ([Approve with one click](docs/features/team-lead.md#approve-with-one-click)).
- The socket or named pipe of the Team Lead's tools ([Linux, macOS and Windows](docs/PLATFORMS.md#every-linux-only-spot)).
- Keys and tokens: a sign-in, key or token that leaves your keychain, or reaches a run or a file it shouldn't ([Your keys stay yours](docs/features/cli.md#your-keys-stay-yours)).
- The install scripts, the one-line installs and updates ([How GizAi updates itself](docs/RELEASING.md#how-gizai-updates-itself)).

## What doesn't count

- Problems in Claude Code, Codex CLI or Gemini CLI themselves: report those to Anthropic, OpenAI or Google, as each CLI's own security policy says ([Claude Code](https://github.com/anthropics/claude-code/security/policy), [Codex CLI](https://github.com/openai/codex/security/policy), [Gemini CLI](https://github.com/google-gemini/gemini-cli/security/policy)).
- An agent doing what you allowed it to do, like running a command on its allowed list or changing a folder you gave it.

## What to send

- the GizAi version (Settings → Updates, or `gizai --version`) and your system;
- the steps that show the problem;
- what someone could do with it.

## What happens next

- You get a first reply within 7 days.
- A confirmed problem is fixed in the next patch release.
- You're credited for it, if you want to be.

## How GizAi keeps your data

GizAi keeps your data in one folder on your computer and reports nothing about you or your work: see [Your data](docs/install.md#your-data) and [No telemetry](docs/install.md#no-telemetry).
