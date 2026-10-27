# Install GizAi

GizAi builds from source on your own computer and installs for you alone, without sudo or administrator rights. It runs on Linux, on Macs with Apple silicon and on Windows 11.

There are no downloads, installers or binaries, on any system. The install script checks what is missing, builds GizAi and installs it. The `production` branch holds the released version; `main` is development.

GizAi is early: its version is still 0.x. Every release is on [GitHub Releases](https://github.com/Yotech-AI/gizai/releases).

## In one line

On Linux and macOS, in a terminal:

```sh
curl -fsSL https://gizai.app/install.sh | sh
```

On Windows, in PowerShell:

```powershell
irm https://gizai.app/install.ps1 | iex
```

It gets the source of the latest release into `~/.local/share/gizai-src` (on Windows `%LOCALAPPDATA%\gizai-src`), or updates it there. Then it runs that release's install script: first its check, which names what is missing, then the build and the install. Run it again to update.

- gizai.app sends the line on to [`scripts/get.sh`](https://github.com/Yotech-AI/gizai/blob/production/scripts/get.sh) (on Windows [`scripts/get.ps1`](https://github.com/Yotech-AI/gizai/blob/production/scripts/get.ps1)) on the `production` branch, where you can read it first. The source it gets comes from GitHub.
- Without git (on Linux and macOS also bash and curl), it says what to install first and does nothing else.
- The source folder is the install's own: changes in it are replaced. Your data stays in [its own folder](#your-data).
- `--uninstall --purge` (`-Uninstall -Purge` on Windows) deletes the source folder with your data.
- CI proves the line: it installs, updates and uninstalls GizAi with it on Ubuntu 24.04, Fedora, Arch Linux, openSUSE Tumbleweed, macOS 14 on Apple silicon and Windows whenever an install script changes, and runs gizai.app's own lines on Ubuntu, macOS and Windows after each release and every week.

The install script's options go after the line:

| Linux and macOS | Windows | What it does |
|---|---|---|
| `curl -fsSL … \| sh -s -- --check` | `& ([scriptblock]::Create((irm …))) -Check` | Gets the source and shows what is missing |
| `curl -fsSL … \| sh -s -- --uninstall` | `& ([scriptblock]::Create((irm …))) -Uninstall` | Removes GizAi and keeps your data |
| `curl -fsSL … \| GIZAI_PREFIX=$HOME/opt/gizai sh` | `$env:GIZAI_PREFIX = 'D:\Apps\Gizai'`, then the line | Installs into another folder than `~/.local` (on Windows `%LOCALAPPDATA%\Programs\Gizai`) |

## What every system needs

- [Claude Code](https://docs.claude.com/en/docs/claude-code) or Codex, installed and logged in; for Claude Code, run `claude` once. The Team Lead's chat needs Claude Code. Agents can also run on Gemini or another coding CLI you have installed and logged in (Settings → Coding CLIs).
- git, Rust ([rustup](https://rustup.rs)) and Node.js 20 or newer.
- Optional, to review cards as pull requests on GitHub: the [GitHub CLI](https://cli.github.com) and an SSH key on your GitHub account. Settings → GitHub shows what's missing and can log gh in.
- Optional, for a project on Bitbucket Cloud: an SSH key on your Bitbucket account and an API token with the scopes `read:user:bitbucket`, `read:pullrequest:bitbucket` and `write:pullrequest:bitbucket` (Atlassian account → Security → API tokens). Save the token with your Atlassian email in Settings → Bitbucket. GizAi keeps them in your keychain.

## Linux

You also need WebKitGTK 4.1: the check (`./install.sh --check` in a clone) names the packages for your distribution. The tray icon needs libayatana-appindicator: `libayatana-appindicator` on Arch and Omarchy, `libayatana-appindicator3-1` on Debian and Ubuntu (the .deb depends on it). Without it GizAi runs without a tray icon.

Install [in one line](#in-one-line), or build from a clone:

```sh
git clone --branch production https://github.com/Yotech-AI/gizai.git
cd gizai
./install.sh
```

The installer builds GizAi and installs it for your user in `~/.local`, with no sudo. It also adds GizAi to your app launcher.

| Command | What it does |
|---|---|
| `./install.sh --check` | Shows what is missing and the command that installs it |
| `./install.sh` | Installs, or updates (your data is backed up first) |
| `./install.sh --uninstall` | Removes GizAi and keeps your data in `~/.local/share/gizai` |

## macOS

For Macs with Apple silicon (M1 and later). You also need Apple's Command Line Tools, which bring git and a C compiler: run `xcode-select --install`. Node.js can come from Homebrew (`brew install node`), mise, nvm or the installer on nodejs.org.

Install [in one line](#in-one-line), or build from a clone:

```sh
git clone --branch production https://github.com/Yotech-AI/gizai.git
cd gizai
./install.sh
```

It's the same installer, with the same commands as on Linux. On a Mac it:

- builds `Gizai.app` and installs it in `~/.local/lib/gizai`;
- links it into the Applications folder in your home folder (`~/Applications`): open GizAi from there and keep it in the Dock;
- puts the `gizai` command in `~/.local/bin`.

GizAi's tray icon is in the menu bar.

## Windows

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

Then, in a new PowerShell window, install [in one line](#in-one-line), or build from a clone:

```powershell
git clone --branch production https://github.com/Yotech-AI/gizai.git
cd gizai
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

`install.ps1` builds GizAi and installs it in `%LOCALAPPDATA%\Programs\Gizai`, without administrator rights. It adds GizAi to the Start menu with a shortcut that carries GizAi's app ID, which Windows needs to show GizAi's notifications.

GizAi runs the coding CLIs installed on Windows itself, like `claude.exe` or npm's `gemini.cmd`. It doesn't use WSL.

| Command | What it does |
|---|---|
| `.\install.ps1 -Check` | Shows what is missing and how to install it |
| `.\install.ps1` | Installs, or updates (your data is backed up first) |
| `.\install.ps1 -Uninstall` | Removes GizAi and keeps your data in `%APPDATA%\Gizai` |

If PowerShell won't run scripts, start these the way the block above does: `powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Check`. The one line needs no such change: it starts `install.ps1` that way itself.

## Not signed

GizAi isn't signed yet: there is no Apple Developer account or Windows certificate until GizAi has paid features. A build made on your own computer isn't a download, so Gatekeeper and SmartScreen shouldn't stop it. If they still do:

- macOS: System Settings → Privacy & Security → Open Anyway, or `xattr -dr com.apple.quarantine ~/.local/lib/gizai/Gizai.app`;
- Windows: More info → Run anyway.

After an update, macOS may ask once whether GizAi may use what it saved in your keychain: choose Always Allow.

## First start

The first time GizAi opens, it walks you through four steps:

1. your name and your team's name;
2. which coding CLIs to use: it lists the ones it finds installed. The Backend, Frontend, QA and DevOps Agents run on the one you pick; the Team Lead stays on Claude Code, where its chat runs;
3. importing MCP servers from Claude Code, if you want;
4. the Team Lead's chat, when Claude Code is installed: it welcomes you and asks whether you'd like help setting up agents, clients, projects, tasks or anything else.

You can skip it. GizAi is set up already: you (named after your login), My company, My team, and five agents (the Team Lead, Backend, Frontend, QA and DevOps) on Claude Code, or on Codex when only Codex is installed. Change them later on the Team page and in Settings.

`GIZAI_SKIP_ONBOARDING=1` starts GizAi as if you skipped it: for scripts and tests on a new data folder.

## Updates

GizAi asks GitHub for the latest release 20 seconds after it starts, then every six hours. When a newer one is out, **Update to X.Y.Z** shows above Company in the sidebar. Click it and GizAi:

1. builds the new version from source in the background with your system's install script, while you keep working;
2. backs up your data;
3. installs the new version;
4. offers a restart.

If a step fails, the version you have keeps working, and Settings → Updates says why. Settings → Updates also has Check now, and switches the check off.

You can still update from a terminal: run the [one line](#in-one-line) again, or in a clone `git pull`, then `./install.sh` (on Windows `.\install.ps1`). How an update works, step by step: [docs/RELEASING.md](RELEASING.md#how-gizai-updates-itself).

## Your data

GizAi keeps your data in one folder on your computer: the database, logs, worktrees and backups. It is `~/.local/share/gizai` on Linux, `~/Library/Application Support/Gizai` on macOS and `%APPDATA%\Gizai` on Windows, and `GIZAI_DATA_DIR` points it elsewhere. One GizAi runs per data folder.

It cleans up after itself when it starts and once a day while it runs:

- **Run and chat logs** (`runs/` and `chat/`): kept for 30 days after the run, chat answer or board check ended, then removed. The run stays in the history with its summary, cost and commits; Show output says its log is gone.
- **Keys for the Team Lead's tools** (`api_tokens` in `gizai.db`): each one lasts a chat answer or board check, and is removed a day after it expired or was revoked.
- **Screenshots** of what agents built (`files/`): a card keeps its newest 3 sets, and all of them go 30 days after it reached Done or Cancelled (0 days keeps them). What you attached to a card is never removed.
- **Stored files nothing uses any more** (`files/`): a file no attachment, screenshot or chat message names any more is removed, a day after it was last stored.
- **Backups** (`backups/`): one before every update, install and database upgrade, and `gizai --backup` makes one when you ask. The newest 20 are kept. Their names use your local time, like `gizai-before-update-20261009-143502-123.db`.
- **Backups in a folder you choose** (Settings → General → Backups): copies of the database and your stored files in a folder of your own, like a USB disk or a folder that Dropbox or Syncthing keeps, daily, weekly or when you ask. After each one, this computer's backups past Keep (10 by default) are removed; one made before a restore, and other computers' backups, never are. Keys and passwords are never in them, and they are not encrypted: a synced folder gets your data as it is ([Backups](settings.md#backups)).

## No telemetry

GizAi doesn't report on you or your work. Its release check only asks GitHub for the latest release, and sends nothing about you ([Updates](#updates)).

Your code still goes where you send it: your agents' coding CLIs work with their own providers, and GizAi pushes a card's branch to your project's own remote.

## Read more

- [Bring your own CLI](features/cli.md#on-your-own-machine): your own machine, your logins and accounts, and your keys.
- [Settings](settings.md): where each setting lives.
- [docs/PLATFORMS.md](PLATFORMS.md): what differs on Linux, macOS and Windows.
- [docs/RELEASING.md](RELEASING.md): how a release is made, how GizAi updates itself, and what its release check reads.
