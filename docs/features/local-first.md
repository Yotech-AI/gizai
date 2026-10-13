# Local-first and open

GizAi runs on your computer, with your own git repositories, logins and data, and its code is open under the MIT licence.

## On your computer

- Everything runs on your machine, with your own git repositories and your own logins.
- Agents run on the coding CLIs you have installed, with your own accounts ([Bring your own CLI](cli.md)).
- GizAi needs no account of its own.
- Your data stays in one folder on your computer: the database, logs, worktrees and backups ([Your data](../install.md#your-data)).

It runs on Linux, on Macs with Apple silicon and on Windows 11.

## No telemetry

GizAi doesn't report on you or your work. Its release check only asks GitHub for the latest release, and sends nothing about you ([Updates](../install.md#updates)).

Your code still goes where you send it: your agents' coding CLIs work with their own providers, and GizAi pushes a card's branch to your project's own remote.

## Your keys stay yours

- MCP sign-ins and values, and your Bitbucket login, stay in your system's keychain.
- GizAi never stores a GitHub token or password: pushes go over SSH with your keys, or over HTTPS with gh's login.
- An agent's folders never include `/`, your home folder, GizAi's data or folders with keys ([Folders](tools.md#folders)).

## Open and free

- **MIT licence:** read the code, change it and build it yourself ([LICENSE](../../LICENSE)).
- **Free and complete:** the whole app, with the agents, the Team Lead's chat, the code editor and the terminals. The plan for paid features is hosted services only, like an encrypted backup and sync; the app stays free ([docs/PAID-FEATURES.md](../PAID-FEATURES.md)).
- **Built from source** on your own computer: no downloads, installers or binaries ([Install](../install.md)).

GizAi is early: its version is still 0.x. Every release is on [GitHub Releases](https://github.com/Yotech-AI/gizai/releases).

## Read more

- [Install](../install.md): what each system needs, updates and your data.
- [docs/PLATFORMS.md](../PLATFORMS.md): what differs on Linux, macOS and Windows.
- [docs/RELEASING.md](../RELEASING.md): how a release is made, and what the release check reads.
- [docs/PAID-FEATURES.md](../PAID-FEATURES.md): the plan for paid hosted services, and what stays free.
