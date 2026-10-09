# Releasing Gizai

This is how a new version of Gizai reaches the people who use it. It is for people and for agents (the DevOps Agent releases when a card asks for it and names the version).

In short: a release is a GitHub Release with a version tag, `vX.Y.Z`, on the `production` branch. Every installed Gizai asks GitHub for the latest release. When it finds a newer one, the sidebar shows **Update to X.Y.Z** above Company. Gizai updates from Releases only, never from `main`: a merge into `main` reaches people with the next release.

## Branches

| Branch | What it holds | How it changes |
|---|---|---|
| `main` | Development. Cards' branches start here, and their pull requests merge here. | Pull requests, merged on GitHub |
| `production` | The released version: what `install.sh` (Linux and macOS) and `install.ps1` (Windows) install, and where release tags point. | Only a pull request from `main`. It is protected: no direct pushes, no force pushes, no deleting. |

## When to release

- When `main` has something worth giving to the people who use Gizai: a finished card or a fix.
- Only when `main` passes everything in CLAUDE.md (`cargo test --workspace`, `npm test`, `npm run build`, `scripts/ui-test.sh`) and builds with `npm run tauri build -- --no-bundle`. A release that doesn't build fails for everyone who presses Update. Their installed Gizai keeps working, but they're stuck on it.
- Only when CI passes on Linux, macOS and Windows. Gizai runs on all three, and each one builds the release from source. CI (`.github/workflows/ci.yml`) runs on every pull request and every push to `main`. Its three checks show on the pull request: `ubuntu-24.04`, `macos-14` and `windows-latest`.
- Jeffrey decides when to release. An agent releases only when a card asks for it and names the version.

## Choosing the version

Versions are `MAJOR.MINOR.PATCH` and the tag is `v` plus the version: `v0.1.6`. Gizai offers nothing else: no `v0.2.0-beta.1`, no drafts, no pre-releases.

- **PATCH** (0.1.5 → 0.1.6): fixes and small changes.
- **MINOR** (0.1.6 → 0.2.0): new features, and always a change to the database schema (`SCHEMA_VERSION` in `crates/gizai-core/src/db.rs`). The new version upgrades the data, and an older build refuses it afterwards.
- **MAJOR**: when Jeffrey says so.

The new version must be higher than the last release (`gh release list`). Gizai only offers a release that is newer than itself.

## Making a release

Replace `X.Y.Z` with the new version.

1. **Bump the version** on a branch from `main`, in all three places:
   - `Cargo.toml`: `version` under `[workspace.package]`;
   - `package.json`: `version`;
   - `src-tauri/tauri.conf.json`: `version`.

   Then update the lock files, which carry it too:

   ```sh
   source scripts/env.sh
   cargo update --workspace --offline                        # Cargo.lock: gizai, gizai-agents, gizai-core, gizai-mcp
   npm install --package-lock-only --ignore-scripts --offline # package-lock.json, in two places
   git grep -n 'X.Y.Z' -- Cargo.toml package.json src-tauri/tauri.conf.json Cargo.lock package-lock.json
   ```

   Commit it as `Bump version to X.Y.Z`, push the branch, and merge its pull request into `main` once its CI checks pass:

   ```sh
   gh pr view <number> --json statusCheckRollup --jq '.statusCheckRollup[] | "\(.name): \(.status) \(.conclusion)"'
   ```

   Each of the three must say `COMPLETED SUCCESS`. A check that is still `IN_PROGRESS` needs a few more minutes (the Windows build is the slowest). On a failure, don't release: the card goes back to whoever can fix it.

   Gizai checks the bump. An update installs nothing when the release's `Cargo.toml` says another version, and it checks `gizai --version` after installing. A forgotten bump would otherwise offer the same update forever.

2. **Merge `main` into `production`** with a pull request and a merge commit (never squash or rebase):

   ```sh
   gh pr create --base production --head main --title "Release vX.Y.Z" --body "What's in it, in a few lines."
   gh pr merge <number> --merge
   ```

   CI runs on this pull request too. Its checks should pass at once, as `main` already passed them.

3. **Create the release** on `production`, with notes made from the merged pull requests:

   ```sh
   gh release create vX.Y.Z --target production --title "vX.Y.Z" --generate-notes
   ```

   Don't add `--draft` or `--prerelease`. Gizai reads GitHub's "latest release", which skips both. Its notes show in Settings → Updates under "What's new".

4. **Check it:**

   ```sh
   gh release view vX.Y.Z                                     # not a draft, not a pre-release
   git ls-remote origin refs/tags/vX.Y.Z refs/heads/production # the tag is production's latest commit
   curl -s https://api.github.com/repos/Yotech-AI/gizai/releases/latest | grep '"tag_name"'   # what Gizai reads: vX.Y.Z
   ```

   Every installed Gizai sees the release within six hours, or at once with Settings → Updates → Check now.

If `git push` asks for a username (the remote is an https address and git has no login for it), push over SSH with your keys:

```sh
git -c url.git@github.com:.insteadOf=https://github.com/ push -u origin HEAD
```

## Never

- **Never move or delete a released tag, and never reuse a version.** Gizais may have fetched it already. A mistake gets fixed with the next patch version.
- **Never push to `production` directly or force a push.** Only the pull request from `main` changes it.
- **Never tag `main` or a card's branch.** Release tags point at `production`.
- **No binaries attached yet, and nothing signed.** Gizai builds every release from source on the computer it runs on: Linux, macOS (Apple silicon) and Windows. There is no Apple Developer account and no Windows certificate. Prebuilt binaries, installers and signing come later, when Gizai has paid features. Never set up an account or buy a certificate for it.

## How Gizai updates itself

Every release must keep this working, because the Gizai that updates runs the **new** release's installer: `install.sh` on Linux and macOS, `install.ps1` on Windows.

- **The check.**
  - It runs 20 seconds after Gizai starts (when it is due), then every six hours while Settings → Updates → "Check for new releases automatically" is on. A check that didn't work tries again after an hour. Check now asks at any time.
  - It reads `https://api.github.com/repos/Yotech-AI/gizai/releases/latest` with curl, without a login. GitHub allows 60 such requests an hour per network, which is plenty.
  - Nothing about the person or their work is sent.
- **The notice.** When the latest release is newer than the Gizai that runs, the sidebar shows "Update to X.Y.Z" above Company. Settings → Updates shows the same, with the release notes.
- **The update.** Pressing it runs these steps:
  1. **Get the source:** a shallow fetch of the tag `vX.Y.Z` only, into `<data folder>/update/source`. git works only in that folder's own repository. The folder is kept with its `target/`, so later updates build faster.
  2. **Check the version:** the source's `Cargo.toml` must say `X.Y.Z`.
  3. **Build:** the release's own `./install.sh --build-only`, at low CPU priority. Gizai stays usable meanwhile. Every command's output goes to `<data folder>/update/update.log`. On Windows: `.\install.ps1 -BuildOnly`, run with Windows PowerShell (`-ExecutionPolicy Bypass`, as the script isn't signed), at below-normal priority.
  4. **Back up the data:** `<data folder>/backups/gizai-before-update-<time>.db`.
  5. **Install:** the release's own `./install.sh --skip-build`, with `GIZAI_PREFIX` set to where the running Gizai was installed (usually `~/.local`) and `XDG_DATA_HOME` set to `<prefix>/share`. So the desktop entry and icons go with the install: `~/.local/share` for the usual one, and never over yours for a test's scratch prefix. The installer backs up `<prefix>/share/gizai` again with the new build, then replaces the programs with a rename.
     - macOS: the same `install.sh`. It backs up `~/Library/Application Support/Gizai` and replaces `Gizai.app` in `<prefix>/lib/gizai` with a rename.
     - Windows: `.\install.ps1 -SkipBuild` with `GIZAI_PREFIX` (usually `%LOCALAPPDATA%\Programs\Gizai`). It backs up `%APPDATA%\Gizai`. Windows can't overwrite a program that runs, so it renames the running `gizai.exe` and `gizai-mcp.exe` aside (`.gizai.exe.old.<pid>`), moves the new ones in, and removes the old ones at the next install.
  6. **Check:** `gizai --version` of the installed program must say `X.Y.Z`. Then the notice offers **Restart to use X.Y.Z**. Restart quits Gizai the usual way (agents at work are stopped first) and starts the installed one.
- **When a step fails,** nothing installed changes: the notice says the update failed, and Settings → Updates says why, with the end of the output, the log and Try again.
  - The installed version decides. When the installer fails after the new programs are in place (the icons, say), the update counts as installed, with what the installer said.
  - Only an install that stopped between its renames (seconds) can leave a mix. Settings → Updates then says to try again or to run `./install.sh`.
- **Stop** ends the update while it gets the source or builds. Quitting Gizai does that too.
- **So `install.sh` must keep these working:** `--build-only`, `--skip-build`, `GIZAI_PREFIX` and `XDG_DATA_HOME`, with macOS's own bash 3.2 and BSD tools too. **And `install.ps1`:** `-BuildOnly`, `-SkipBuild` and `GIZAI_PREFIX`, with Windows PowerShell 5.1 (what an update runs) and PowerShell 7. CI checks that both scripts parse and that their checks (`--check`, `-Check`) pass.
- **Only an installed Gizai updates itself.** That is a Gizai that runs as `<prefix>/lib/gizai/gizai` (on macOS also `<prefix>/lib/gizai/Gizai.app/Contents/MacOS/gizai`, when it starts from the Dock), or on Windows as a `gizai.exe` with `install.ps1`'s `gizai-installed.txt` next to it. A dev build (`scripts/run.sh`) or a test build says why it doesn't, and never installs anything.
- **From a terminal** it works as before: `git pull` in a checkout of `production`, then `./install.sh` (on Windows `.\install.ps1`).
- **Headless test and screenshot runs** (`GIZAI_SELFTEST` or `GIZAI_ROUTE` set) skip the automatic check, unless `GIZAI_RELEASES_URL` is set too.
- **Settings for tests and forks:**
  - `GIZAI_REPO`: the repository the tag is fetched from. Default `https://github.com/Yotech-AI/gizai.git`.
  - `GIZAI_RELEASES_URL`: what the check reads. Default: GitHub's latest release of `GIZAI_REPO`. It also takes `http://` and `file://` addresses.

## Testing an update without touching a real Gizai

Never test against the Gizai Jeffrey uses: not `./install.sh`, not `~/.local/lib/gizai`, not `~/.local/share/gizai`. Everything goes in one scratch folder, `$T` below.

1. **A fake release.**
   - A git repository with a tag, for example `v9.9.9`. Its `Cargo.toml` must say `version = "9.9.9"` under `[workspace.package]`.
   - It needs an `install.sh` that takes `--build-only` and `--skip-build`. For a quick test, use a stub:
     - `--build-only` writes `target/release/gizai` (a script that prints `gizai 9.9.9` for `--version` and a path for `--backup`) and `target/release/gizai-mcp`;
     - `--skip-build` copies them to `$GIZAI_PREFIX/lib/gizai/`.

     For a real test, use a copy of this repository with the version bumped; its build takes minutes.
   - A file shaped like GitHub's answer:

     ```json
     {"tag_name": "v9.9.9", "name": "v9.9.9", "html_url": "https://github.com/Yotech-AI/gizai/releases/tag/v9.9.9", "draft": false, "prerelease": false, "body": "- What changed"}
     ```

2. **A scratch install of this branch's build:**

   ```sh
   env -i HOME="$T/home" PATH="$PATH" ./install.sh --skip-build
   ```

   This is what `scripts/test-install.sh` does. It puts the programs in `$T/home/.local/lib/gizai/`.

3. **Start that Gizai headless,** in `cage` the way `scripts/shot-cage.sh` starts one, but with these settings:

   ```sh
   HOME="$T/home" XDG_DATA_HOME="$T/home/.local/share" GIZAI_DATA_DIR="$T/data" \
     GIZAI_REPO="$T/fake-repo" GIZAI_RELEASES_URL="file://$T/latest.json" \
     "$T/home/.local/lib/gizai/gizai"
   ```

   - Gizai installs an update into the prefix it runs from: here `$T/home/.local`. The installer's desktop entry and icons go to `$T/home/.local/share`.
   - Keep `HOME` and `XDG_DATA_HOME` in the scratch folder anyway: other tools write there too.
   - A Gizai that runs from `target/release` never installs; it says why in Settings → Updates.

   Make the stub's `install.sh` fail to see the failure path: the old programs must stay in place, and Settings → Updates must say why.

This is a Linux test, as the headless `cage` runs are. On macOS and Windows an update builds with the same steps and that system's installer. CI builds and tests Gizai there on every pull request, and an update on a real Mac and Windows PC is part of the check list in `docs/PLATFORMS.md`.
