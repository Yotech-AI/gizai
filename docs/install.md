# Install GizAi

GizAi builds from source on your own computer and installs for you alone, without sudo or administrator rights.

There are no downloads or installers, on any system. The install script checks what is missing, builds GizAi and installs it. The `production` branch holds the released version; `main` is development.

## What every system needs

- [Claude Code](https://docs.claude.com/en/docs/claude-code) or Codex, installed and logged in; for Claude Code, run `claude` once. The Team Lead's chat needs Claude Code. Agents can also run on Gemini or another coding CLI you have installed and logged in (Settings → Coding CLIs).
- git, Rust ([rustup](https://rustup.rs)) and Node.js 20 or newer.
- Optional, to review cards as pull requests on GitHub: the [GitHub CLI](https://cli.github.com) and an SSH key on your GitHub account. Settings → GitHub shows what's missing and can log gh in.
- Optional, for a project on Bitbucket Cloud: an SSH key on your Bitbucket account and an API token with the scopes `read:user:bitbucket`, `read:pullrequest:bitbucket` and `write:pullrequest:bitbucket` (Atlassian account → Security → API tokens). Save the token with your Atlassian email in Settings → Bitbucket. GizAi keeps them in your keychain.

## Linux

You also need WebKitGTK 4.1: `./install.sh --check` names the packages for your distribution. The tray icon needs libayatana-appindicator: `libayatana-appindicator` on Arch and Omarchy, `libayatana-appindicator3-1` on Debian and Ubuntu (the .deb depends on it). Without it GizAi runs without a tray icon.

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

Then, in a new PowerShell window:

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

If PowerShell won't run scripts, start these the way the block above does: `powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Check`.

## Not signed

GizAi isn't signed yet: there is no Apple Developer account or Windows certificate until GizAi has paid features. A build made on your own computer isn't a download, so Gatekeeper and SmartScreen shouldn't stop it. If they still do:

- macOS: System Settings → Privacy & Security → Open Anyway, or `xattr -dr com.apple.quarantine ~/.local/lib/gizai/Gizai.app`;
- Windows: More info → Run anyway.

After an update, macOS may ask once whether GizAi may use what it saved in your keychain: choose Always Allow.

## Updates

GizAi asks GitHub for the latest release 20 seconds after it starts, then every six hours. When a newer one is out, **Update to X.Y.Z** shows above Company in the sidebar. Click it and GizAi:

1. builds the new version from source in the background with your system's install script, while you keep working;
2. backs up your data;
3. installs the new version;
4. offers a restart.

If a step fails, the version you have keeps working, and Settings → Updates says why. Settings → Updates also has Check now, and switches the check off.

You can still update from a terminal with `git pull`, then `./install.sh` (on Windows `.\install.ps1`). How an update works, step by step: [docs/RELEASING.md](RELEASING.md#how-gizai-updates-itself).

## Your data

GizAi keeps your data in `~/.local/share/gizai` on Linux, `~/Library/Application Support/Gizai` on macOS and `%APPDATA%\Gizai` on Windows. `GIZAI_DATA_DIR` points it elsewhere. One GizAi runs per data folder.

It cleans up after itself when it starts and once a day while it runs:

- **Run and chat logs** (`runs/` and `chat/`): kept for 30 days after the run, chat answer or board check ended, then removed. The run stays in the history with its summary, cost and commits; Show output says its log is gone.
- **Keys for the Team Lead's tools** (`api_tokens` in `gizai.db`): each one lasts a chat answer or board check, and is removed a day after it expired or was revoked.
- **Backups** (`backups/`): one before every update, install and database upgrade, and `gizai --backup` makes one when you ask. The newest 20 are kept. Their names use your local time, like `gizai-before-update-20261009-143502-123.db`.

## Read more

- [Local-first and open](features/local-first.md): what stays on your computer.
- [Settings](settings.md): where each setting lives.
- [docs/PLATFORMS.md](PLATFORMS.md): what differs on Linux, macOS and Windows.
- [docs/RELEASING.md](RELEASING.md): how a release is made and how GizAi updates itself.
