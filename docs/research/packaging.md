# Gizai with yay (AUR) and brew (our own tap), built from source (research, 2026-10-13)

Status: research only, no code. Written by the R&D Agent for GA-165 on 2026-10-13, against `main` at `8e16ab8` (Gizai 0.10.0). The latest release is `v0.10.0`, tagged on `production` that day (`git log --tags --no-walk`).
Legend: **[V]** = verified today, in the code (file and line) or in the primary source named (link and date in §8). **[I]** = my judgement, or a fact I could not check in a primary source (the reason is given).
Limits: this run may not run makepkg, brew, curl or a build, so the PKGBUILD, its .SRCINFO and the formula below are **drafts, not built or tested**; §5.9 says how a card tests them. wiki.archlinux.org, the AUR's web pages and API, and Arch's GitLab answer automated reads with a bot check (Anubis). So the AUR names were checked on the AUR's own git server, Arch's packages on archlinux.org, and the Arch wiki through a Kiwix copy of its April 2026 dump (§8).

---

## 1. Summary

- **Both work without a certificate, a paid account or a prebuilt binary.** An AUR package `gizai` and a formula in our own tap, `Yotech-AI/homebrew-tap`, both build the release tag's source on your computer, as the one-line install does. The names are free: `gizai` and `gizai-git` on the AUR, and `gizai` in Homebrew (formula and cask). [V]
- **The trap is real for the AUR.** A package that installs `/usr/lib/gizai/gizai` makes Gizai take `/usr` for an install script's install, so Update builds for minutes and then fails. The draft formula avoids the layout. **Recommended:** a build-time setting, `GIZAI_PACKAGE=aur` or `homebrew`. A Gizai built with it never updates itself, and Settings → Updates gives the command (`yay -Syu`, or `brew update && brew upgrade gizai`; exact texts in §5.1).
- **Homebrew leaves Gizai.app in its own folder** (`$(brew --prefix)/opt/gizai/Gizai.app`), because a formula may not write to Applications. The formula's notes give the one `ln -sfn` line for `~/Applications`. A Gizai kept in the Dock probably breaks at each `brew upgrade`; a Mac test decides whether the formula keeps a stable copy instead. **macOS only:** on Linux, Homebrew would run Gizai on its own GTK and WebKitGTK, and the launcher entry wouldn't show.
- **Not worth making:** `gizai-git` (`production` only moves for a release, and the release is tagged minutes later), bottles, a `-bin` package or a cask (all prebuilt).
- **Per release**, a step 5 for the DevOps Agent: one sha256 of the tag's GitHub tarball goes into both files, with the version.
- **Scoop:** it could build from source without a certificate, but only after the user installs the C++ Build Tools, and it needs the same package setting. Not now.
- **Cards (§6):** PK1 (backend) and PK2 (frontend) at the same time, then a release, then PK3 (AUR) and PK4 (tap) at the same time, then PK5 (docs). **Open questions (§7):** 11.

---

## 2. The project now

### 2.1 What Gizai does for its users

Gizai is a desktop app where a team of AI agents works through your board. Each card gets its own git worktree; an agent builds it, a QA agent tests it, and it lands in your Review. It all runs on your own computer, with your own repositories, logins and coding CLIs (README.md:23-25). [V] It installs only from source. The one line (`curl -fsSL https://gizai.app/install.sh | sh`) gets the latest release's source and runs that release's `install.sh` (scripts/get.sh:2-8). An installed Gizai updates itself by building the next release the same way (docs/RELEASING.md:102-133). [V] The decisions that bind this card: no Apple Developer account, no Windows certificate, no prebuilt binaries in releases and no Tauri updater (docs/PLATFORMS.md:5-10, :74-76); and never set up an account or buy a certificate for it (docs/RELEASING.md:100). [V]

### 2.2 How it is built, installed and updated

| Part | What it is | Where |
|---|---|---|
| Build | `npm ci`, then `npm run tauri build -- --no-bundle` on Linux or `npm run tauri build -- --bundles app` on macOS, then `cargo build --release -p gizai-mcp`, with `TAURI_TELEMETRY_DISABLED=1` and npm's notices off. Never a plain `cargo build --release -p gizai`: it can load the dev URL. | install.sh:194-209; CLAUDE.md:46 |
| Toolchain | Tauri 2.12.1, Rust 2024 edition, Cargo resolver 3, let chains (so a recent Rust), Node.js 20 or newer | Cargo.lock:4240-4241; Cargo.toml:2, :6; crates/gizai-agents/src/update.rs:234; install.sh:116-121 |
| Native libraries on Linux | WebKitGTK 4.1 and JavaScriptCore, libsoup 3, GTK 3 with GDK, ATK, cairo, pango and gdk-pixbuf, GLib, and libdbus (the `dbus` crate, for the keyring and Tao). SQLite is built in (`rusqlite` with `bundled`), and TLS is rustls: **no OpenSSL**. The tray's libayatana-appindicator is loaded with `dlopen` at start, so Gizai runs without it (no tray icon then). | the `-sys` crates in Cargo.lock (`rg '^name = ".*-sys"'`), `rustls` Cargo.lock:3550, `dbus` :773, `keyring` :2274-2286; Cargo.toml:11; src-tauri/src/lib.rs:318-325, :368-377 |
| Linux install | `<prefix>/lib/gizai/` holds `gizai`, `gizai-mcp` and the launcher `gizai-launch`, which starts Gizai with your login shell's PATH (`bash -lc`) and Mesa's EGL on a PC without NVIDIA. `<prefix>/bin/gizai` links to the launcher. The icon goes in five sizes plus the SVG into `hicolor`, and `gizai.desktop` gets `Exec=<lib>/gizai-launch`. | install.sh:301-357 (launcher :312-323, icons :329-335, entry :336-347) |
| macOS install | `<prefix>/lib/gizai/Gizai.app`, with `gizai-mcp` copied into `Contents/MacOS` and an ad hoc signature made again. `lib/gizai/gizai` and `gizai-mcp` link into the app; then the launcher, `<prefix>/bin/gizai`, `lsregister`, and `~/Applications/Gizai.app` linking to the app (for the usual prefix only). | install.sh:240-299 |
| gizai-mcp | Gizai looks for it next to its own binary (the folder of `current_exe`), or at `GIZAI_MCP_BIN` | src-tauri/src/mcp.rs:65-72 |
| Data | `~/.local/share/gizai` on Linux, `~/Library/Application Support/Gizai` on macOS; install and uninstall never touch it | install.sh:7; docs/install.md:143-145 |
| The update | The release check reads GitHub's latest release with curl. Then the release's own `install.sh --build-only`, and `--skip-build` with `GIZAI_PREFIX` set to the prefix Gizai runs from and `XDG_DATA_HOME` to `<prefix>/share`. | crates/gizai-agents/src/update.rs:132-160, :379-401; src-tauri/src/update.rs:321-436 |
| Who updates itself | `install_prefix`: on Linux and macOS, any Gizai at `<prefix>/lib/gizai/gizai` (on macOS also the app's own binary); on Windows, a `gizai.exe` next to install.ps1's marker `gizai-installed.txt` | crates/gizai-agents/src/update.rs:174-208; install.ps1:92, :300 |
| Releases | A `vX.Y.Z` tag on `production` and a GitHub Release with no binaries, made by the DevOps Agent from a card that names the version | docs/RELEASING.md:3-12, :71-77, :100 |
| Tests and CI | `cargo test --workspace` and `npm test` on Linux, macOS and Windows on every pull request; `install.yml` installs from source on macOS and Windows when an installer changes | docs/PLATFORMS.md:76-77; .github/workflows/install.yml:8-15 |

### 2.3 What the feature touches

| Area | Today | With packages |
|---|---|---|
| The updater (`crates/gizai-agents/src/update.rs`, `src-tauri/src/update.rs`) | any `<prefix>/lib/gizai/gizai` updates itself | a package's Gizai never does, and knows which command does (§5.2) |
| Settings → Updates (`src/components/UpdateSettings.tsx`), the sidebar's notice (`src/components/UpdateNotice.tsx`, `src/lib/update.ts`) | "Update to X.Y.Z", or why a dev build can't update | the latest version and the package manager's command (§5.1) |
| Install docs (`docs/install.md`, `README.md:38-74`) | the one line and a clone | a "With a package manager" part (§5.7) |
| Releases (`docs/RELEASING.md`) | steps 1-4 | a step 5 for both packages (§5.6) |
| New, outside this repository | none | the AUR package `gizai` (a git repository on aur.archlinux.org) and the tap `Yotech-AI/homebrew-tap` (a GitHub repository) |
| The install scripts and the one-line installs | unchanged | one line each, so their build never carries `GIZAI_PACKAGE` (§5.2) |

---

## 3. The feature

### 3.1 What it should do (from the card)

- `yay -S gizai` on Arch and Omarchy, and `brew install yotech-ai/tap/gizai` on a Mac with Apple silicon, each building a release from source. No certificate, developer account or paid licence.
- Each package installs what `install.sh` installs.
- A package's Gizai doesn't update itself: Settings → Updates shows the latest version and says to update with yay or brew.
- The one line stays the main route, building from source.
- Every release updates both packages.

### 3.2 The questions, answered

**Q1. The AUR.**

- **The names are free** [V, 2026-10-13]. The AUR's git server has no commits for either name: `git fetch --dry-run https://aur.archlinux.org/gizai.git` and `…/gizai-git.git` both answer `fatal: couldn't find remote ref HEAD`, while the same command on `yay.git` fetches its `HEAD`. Arch's official repositories have no `gizai` either (archlinux.org's package search: 0 results). The AUR's web page and its API (`https://aur.archlinux.org/packages/gizai`, `/rpc/v5/info?arg[]=gizai&arg[]=gizai-git`) showed only Anubis's bot check to this run, and Repology's API refused it (403).
- **Free is not reserved** [I]. The AUR has no reservation: a package base exists once someone pushes its first commit (AUR submission guidelines, §8). The name is first come, first served until the AUR account pushes it (Q1 in §7).
- **`gizai-git` on `production` isn't worth it.** `production` changes only through a release's pull request from `main`, and the release tag on its latest commit follows minutes later (docs/RELEASING.md:12, :61-77). [V] So `gizai-git` would build what `gizai` builds, a few minutes earlier, with no checksum (VCS sources use `SKIP`, VCS package guidelines). A `-git` package of `main` would bring unreleased database upgrades, which the released package can't open afterwards: "an older build refuses it" (docs/RELEASING.md:26). [V] Leave the name free.
- **The PKGBUILD** is in §5.3. Where it departs from the brief, with the reason:
  1. **`makedepends=('cargo' 'nodejs' 'npm')`, not `rust`.** Arch's Rust guidelines say `makedepends=(cargo)` [V]. `cargo` is provided by both `rust` (1:1.99.0, provides `cargo`) and `rustup` (provides `cargo` and `rust`) [V archlinux.org], so `rust` works too. Don't add a version (`cargo>=…`): both provide `cargo` without one, which doesn't satisfy a versioned dependency [I: pacman's rule for unversioned provides]. Arch has Rust 1.99, Node.js 26.11.1 and npm 12.2.0 [V], new enough for Gizai's Rust 2024 code and its Node 20 minimum (install.sh:116-121).
  2. **No `git` in makedepends.** The release package builds GitHub's tarball of the tag. Cargo.lock and package-lock.json have no git dependencies (`rg 'source = "git'`, `rg '"resolved": "git'`: none), and neither the build script nor Vite's config runs git (src-tauri/build.rs:1-3, vite.config.ts). [V] `git` is needed **when Gizai runs** (worktrees, the Editor), so it goes into `depends`, with `curl` for the release check (crates/gizai-agents/src/update.rs:136). [V] A `gizai-git` would need git to build.
  3. **`depends` lists every library Gizai links, not just two.** Arch lists the libraries a binary links itself. Tauri's own AUR guide lists cairo, desktop-file-utils, gdk-pixbuf2, glib2, gtk3, hicolor-icon-theme, libsoup, pango and webkit2gtk-4.1 [V]. Its `libsoup` is libsoup 2: WebKitGTK 4.1 uses libsoup 3, which Gizai links (`soup3-sys` in Cargo.lock), so the draft says `libsoup3` [V]. The draft leaves out `desktop-file-utils`, whose pacman hook runs whenever it is installed [I]. Gizai also links libdbus (`dbus`; no package is named `libdbus` [V]) and Rust's runtime: `libgcc` and `glibc`, as Arch has split gcc-libs, which webkit2gtk-4.1's own depends show (`libgcc`, `libstdc++`) [V]. namcap on the built package confirms the list (card PK3).
  4. **`libayatana-appindicator` stays in `depends`**, as the brief has it, so the tray works out of the box (Omarchy's Waybar). Gizai loads it with `dlopen` (src-tauri/src/lib.rs:368-377), so namcap may report it as not needed, and `optdepends` would be the strict choice (Q7 in §7). [V/I]
  5. **No `librsvg` and no `openssl`.** install.sh's check asks for them (install.sh:125), but nothing links them: Cargo.lock has no `openssl-sys` and no librsvg crate. [V]

**Q2. Homebrew.**

- **A formula in our own tap works.** The tap's repository name starts with `homebrew-`, and people install with `brew install user/repository/formula` [V Homebrew docs]. So `Yotech-AI/homebrew-tap` gives `brew install yotech-ai/tap/gizai`. `gizai` is free in Homebrew: `formulae.brew.sh/api/formula/gizai.json` and `…/cask/gizai.json` both answer 404. [V]
- **It can never move to homebrew-core**: "A formula whose primary output is a native macOS `.app` bundle is not eligible" (Acceptable Formulae). [V] A cask, where Homebrew keeps apps, installs a prebuilt app, which Gizai doesn't publish (decided). So the brief is right: a formula, in our tap.
- **No bottles.** `brew tap-new` sets up GitHub Actions that build bottles, and `brew pr-pull` publishes them [V]. Those are prebuilt binaries on GitHub, which docs/PLATFORMS.md:10 and docs/RELEASING.md:100 rule out. So the tap's CI only builds and tests (§5.4), and every `brew install` builds from source.
- **Where Gizai.app goes.** Into the formula's own folder (the keg): `$(brew --prefix)/Cellar/gizai/X.Y.Z/Gizai.app`, always reachable as `$(brew --prefix)/opt/gizai/Gizai.app` (`/opt/homebrew/opt/gizai/Gizai.app` on Apple silicon). The command `gizai` goes in `$(brew --prefix)/bin`. Homebrew never links apps into Applications [V]:
  - `brew linkapps` was deprecated (Homebrew/brew#1808, merged 2017-01-11; listed in Homebrew 1.2.0's notes), because it "cannot behave nicely with e.g. Spotlight using either aliases or symlinks and Homebrew formulae do not build "proper" `.app` bundles that can be relocated";
  - a formula that writes "to the disk outside of your sandbox area" gets `Operation not permitted`, "enforced on macOS by `sandbox-exec`", and post-install steps run "inside the formula post-install sandbox" too (Formula Cookbook);
  - emacs-plus's tap says it plainly: "Formulas don't install apps there; if that is what you want, the cask is the better fit."
  - MacVim, a core formula that builds an app, leaves `MacVim.app` in its keg and links only commands into `bin`.
- **Starting it from the Dock.** After `brew install`, the formula's notes (caveats) give one line: `mkdir -p ~/Applications && ln -sfn $(brew --prefix)/opt/gizai/Gizai.app ~/Applications/Gizai.app`. That is install.sh's own link (install.sh:278-286), pointed at Homebrew's folder. Through `opt` it follows upgrades [V emacs-plus: "the links follow upgrades, but Spotlight may not show them"]. Then you open Gizai from Applications in your home folder (Finder: Go → Home), and keep it in the Dock (its Dock icon → Options → Keep in Dock), as install.sh says today (install.sh:291). Two things only a Mac can show, for card PK4:
  - **Does the Dock's item still start Gizai after `brew upgrade`?** Probably not. By default `brew upgrade` removes the old version at once (`HOMEBREW_NO_INSTALL_CLEANUP` turns that off) [V]. emacs-plus warns that "an alias points at the versioned keg it was made for and stops working once an upgrade replaces it" [V]. The Dock keeps its items much as an alias does [I], so a kept Gizai would point at `Cellar/gizai/X.Y.Z` and break at each upgrade. Gizai made 13 releases from 7 to 13 October (`git log --tags`) [V], so that would happen often. If PK4's test confirms it, the formula keeps a copy of the app at a path that never changes (§5.4, "a stable copy"), as install.sh keeps `~/.local/lib/gizai/Gizai.app`.
  - **Does Spotlight show a linked app?** Homebrew (#1808) and emacs-plus both say links and aliases don't work well with Spotlight [V]. docs/PLATFORMS.md:72 says install.sh's same link lets Spotlight start Gizai, so check that on the Mac too.
- **Homebrew on Linux: possible, not recommended.**
  - Homebrew uses none of the host's libraries except glibc and gcc [V]. It does have Gizai's stack: `webkitgtk` 2.54.1 is Linux-only with bottles for x86_64 and arm64 Linux, built with `USE_GTK4=OFF`, and its test checks `webkit2gtk-4.1`, the API Tauri uses; `libayatana-appindicator` 0.6.1 is there too [V]. So the formula could build with an `on_linux` block of about ten dependencies.
  - But Gizai would run on Homebrew's own GTK, WebKitGTK and Mesa (`webkitgtk` depends on Homebrew's `mesa` [V]), not the desktop's. The launcher's EGL fix points at the system's Mesa files (install.sh:316-318), and themes, input methods and GPU drivers would all need testing [I].
  - The launcher entry and icons would land in `$(brew --prefix)/share`, which app launchers don't read: `brew shellenv` sets PATH, MANPATH and INFOPATH but not `XDG_DATA_DIRS` [V].
  - Linux already has the one line and the AUR. **Recommendation: macOS only** (`depends_on :macos`), and look again if a Linux user asks.

**Q3. The updater.** The trap, the way out and the texts are in §5.1 and §5.2. In short:

- **The trap, step by step** [V by reading the code]. On the AUR package, `install_prefix("/usr/lib/gizai/gizai")` gives `/usr` (crates/gizai-agents/src/update.rs:193-196). The update then builds the release in the data folder (minutes of work, and a full `target/` in `update/source`). Then it runs `install.sh --skip-build` with `GIZAI_PREFIX=/usr` and `XDG_DATA_HOME=/usr/share` (crates/gizai-agents/src/update.rs:394-401). As a normal user that stops at "Could not copy Gizai into /usr/lib/gizai, so nothing was installed" (install.sh:308-311): safe, but a long build for nothing, and an Update button that can never work.
- **It is worse with Homebrew.** A formula that keeps `lib/gizai/gizai` in its keg would be updated **inside** Homebrew's Cellar, which is yours to write: the install would succeed behind brew's back. The draft formula (§5.4) puts the app at the keg's top, so `install_prefix` already says no: `mac_exe` turns `…/Cellar/gizai/X.Y.Z/Gizai.app/Contents/MacOS/gizai` into `…/Cellar/gizai/X.Y.Z/gizai`, whose folder isn't named `gizai` (update.rs:200-208, :195). It would still show a dev build's text: "This Gizai runs from …, not from an install… Update its checkout with git" (src-tauri/src/update.rs:223).
- **Recommended:** the build-time setting `GIZAI_PACKAGE` (§5.2, which compares four ways). The package's Gizai keeps checking GitHub and shows the latest version. It never offers Update. The sidebar says "Gizai X.Y.Z is out" and opens Settings → Updates, which gives the command with a Copy button.

**Q4. The release flow.** One step 5 in docs/RELEASING.md, right after step 4, in the same release card, by the **DevOps Agent**, which makes releases from a card that names the version (docs/RELEASING.md:3, :19). [V] Both packages build GitHub's tarball of the tag, so one sha256 serves both files. The AUR also needs `pkgver`, `pkgrel=1` and a new `.SRCINFO`; the formula needs its `url` and `sha256`. Draft text in §5.6.

**Q5. Scoop (report only): yes, without a certificate, but not by itself.**

- **What works.** A manifest in our own bucket can download the tag's source (`url` and `hash`). Its `installer.script` (PowerShell) can build it with `install.ps1 -BuildOnly` and copy the programs into the app's folder. `checkver: "github"` follows GitHub releases, and `autoupdate` fills in the new version; "the target files will be downloaded and hashed locally" when no hash is given [V Scoop wiki]. Gizai is built on the PC, never downloaded as a program, so there is nothing for SmartScreen to check and nothing to sign, the same reason install.ps1 needs no certificate (docs/PLATFORMS.md:75). [V]
- **The C++ Build Tools come first.** Rust's Windows build needs them, and Microsoft offers them only through the Visual Studio Installer, its command line or winget, from an administrator prompt (Microsoft Learn, 2026-08-20) [V]. Scoop can't install them, and install.ps1 asks for them already (docs/install.md:89). [V]
- **The trap again.** If the manifest ran `install.ps1 -SkipBuild` into Scoop's folder (`~\scoop\apps\gizai\<version>`), install.ps1's marker `gizai-installed.txt` would make that Gizai update itself there (crates/gizai-agents/src/update.rs:183-188). [V] It would need `GIZAI_PACKAGE=scoop` and its own copy step instead.
- **Notifications.** Toasts need a Start menu shortcut that carries Gizai's app ID (docs/PLATFORMS.md:46). [V] Scoop's `shortcuts` field makes a plain shortcut, so the script would have to make that one itself [I].
- **Time.** A Rust build of minutes inside every `scoop install` and `scoop update` is unusual for Scoop [I].
- **Recommendation:** not now. The Windows one line covers it; look again if Windows users ask for Scoop.

### 3.3 Where the brief's assumptions don't hold

1. makedepends `rust, nodejs, npm, git`: the guideline name is `cargo` (rust works too), and `git` isn't needed to build a release tarball. `git` and `curl` are **runtime** dependencies (Q1).
2. depends `webkit2gtk-4.1, libayatana-appindicator`: Arch lists every linked library: about fourteen (§5.3).
3. "Check whether the name is free": it is, but nothing reserves it until the first push.
4. `gizai-git` on `production` gives nothing that `gizai` doesn't (Q1).
5. A tap made with `brew tap-new` publishes bottles, which are prebuilt binaries: the tap must not (Q2).
6. "Where Gizai.app goes (install.sh links it into ~/Applications)": a formula can't make that link. You make it once, with the line the formula prints (Q2).
7. Homebrew on Linux would not use the system's WebKitGTK (Q2).
8. "Update with brew upgrade": by default brew refreshes its formulae before `brew upgrade` only once a day (`HOMEBREW_AUTO_UPDATE_SECS`, default 86400) [V]. So the command is `brew update && brew upgrade gizai`, or a release made that day can be missed [I].

---

## 4. Options

All three keep the one line as the main route, and none needs an account fee, a certificate or a binary. They differ in how the packages are made and kept up to date.

### Option A (recommended): our own AUR package and tap formula, built from the tag's tarball, marked at build time, updated by hand per release

- **What it takes:** the PKGBUILD and .SRCINFO (§5.3) and the formula (§5.4), both building `https://github.com/Yotech-AI/gizai/archive/refs/tags/vX.Y.Z.tar.gz` with one sha256. `GIZAI_PACKAGE` in the updater and Settings → Updates (§5.1, §5.2). A step 5 per release for the DevOps Agent (§5.6). The docs (§5.7).
- **Libraries:** none new in Gizai. The tools are makepkg (pacman's) and Homebrew, both well kept [I: their licences, GPL-2.0-or-later and BSD-2-Clause, weren't read in a primary source today].
- **Risks:**
  - A forgotten step 5 leaves the packages a release behind, while Settings already says a release is out. Mitigation: the step is in the release card itself.
  - Every package update builds from scratch, unlike the in-app update, which keeps `target/` (docs/RELEASING.md:112) [V]. So a package update is slower [I].
  - The Dock after `brew upgrade` (Q2).
  - GitHub's tarball checksums: GitHub promised byte-for-byte stable archives and six months' notice before any change (GitHub blog, 2023-02-21) [V]. Released tags never move (docs/RELEASING.md:97). [V]
- **Size:** the updater S (PK1, PK2), the AUR M (PK3), the tap M (PK4), the docs S (PK5).

### Option B: the same packages, updated by CI on each release

- **What it takes:** a workflow on `release: published` in this repository. It computes the checksum and pushes to the AUR (with an SSH key stored as a secret) and to the tap (with a deploy key or token). It also needs `makepkg --printsrcinfo`, so an Arch container in CI.
- **Risk:** whoever gets those secrets can publish code that every AUR and Homebrew user of Gizai builds and runs. It also means CI changes and more to maintain.
- **When:** only if the step by hand goes wrong in practice. Not now.

### Option C: packages that call install.sh

- **What it takes:** `package()` runs `GIZAI_PREFIX="$pkgdir/usr" XDG_DATA_HOME="$pkgdir/usr/share" ./install.sh --skip-build`, and the formula does the same.
- **Rejected:**
  - install.sh writes the prefix it was given into the launcher and the desktop entry (install.sh:320-322, :341), so `$pkgdir` would end up inside the installed files.
  - Its backup step runs the new build against any data folder it finds (install.sh:227-238).
  - An install-script layout is exactly what the updater takes for its own.

Also rejected: a cask, `gizai-bin` and bottles (all prebuilt, against docs/PLATFORMS.md:10), Homebrew on Linux (Q2), and `gizai-git` (Q1).

---

## 5. The design (Option A)

### 5.1 The screens

Settings → Updates for a Gizai from the AUR, while 0.11.0 is out and 0.10.0 runs:

```
Updates
Gizai came from the AUR, so yay updates it, not Gizai itself. Gizai still asks GitHub for new releases
and tells you when one is out.

Version        ⊕ Gizai 0.10.0. Version 0.11.0 is out (checked 2 minutes ago).
               [↗ Release 0.11.0 on GitHub]
               ▸ What's new in 0.11.0

Update         ⊖ Gizai came from the AUR, so it doesn't update itself. Update it with yay -Syu, then restart Gizai.
               ┌──────────┐
               │ yay -Syu │  [⧉ Copy]
               └──────────┘
               The AUR package follows each release, usually on the same day.

New releases   [x] Check for new releases automatically
               [⟳ Check now]
```

The sidebar, above Company: `⊕ Gizai 0.11.0 is out`, a link that opens Settings, with the "Update" sentence as its tooltip.

The exact texts:

| Where | AUR (`GIZAI_PACKAGE=aur`) | Homebrew (`homebrew`) | Any other value |
|---|---|---|---|
| The section's text (`UpdateSettings.tsx:27`) | Gizai came from the AUR, so yay updates it, not Gizai itself. Gizai still asks GitHub for new releases and tells you when one is out. | Gizai came from Homebrew, so brew updates it, not Gizai itself. Gizai still asks GitHub for new releases and tells you when one is out. | Gizai came from a package, so its package manager updates it, not Gizai itself. Gizai still asks GitHub for new releases and tells you when one is out. |
| Version (`checkLine`, unchanged) | Gizai 0.10.0. Version 0.11.0 is out (checked 2 minutes ago). | the same | the same |
| Update, when a newer release is out (`cannotInstall`) | Gizai came from the AUR, so it doesn't update itself. Update it with yay -Syu, then restart Gizai. | Gizai came from Homebrew, so it doesn't update itself. Update it with brew update && brew upgrade gizai, then restart Gizai. | Gizai came from a package, so it doesn't update itself. Update it with the package manager that installed it, then restart Gizai. |
| The command, in mono with Copy (`updateCommand`) | `yay -Syu` | `brew update && brew upgrade gizai` | none |
| The hint under it | The AUR package follows each release, usually on the same day. | The formula in yotech-ai/tap follows each release, usually on the same day. | none |
| The sidebar's notice (`src/lib/update.ts:30`) | Gizai 0.11.0 is out | the same | the same |

With nothing newer out, a package's Gizai shows only the Version line ("This is the latest version (checked …)."): the Update field shows only for a job, a release or an installed version (UpdateSettings.tsx:37). [V]

Why these commands: `yay` with no arguments is "Alias to `yay -Syu`", the whole system's upgrade (yay's README) [V], which is how Arch updates [I]. `yay -S gizai` updates Gizai alone; the docs name it too. For Homebrew, see §3.3 item 8.

**How it fits the design system:** nothing new is drawn. The texts use the section's `text` and the `Field` with its `hint` (src/components/Form.tsx:4, :14-15). The line uses the `gh-line skipped` row with `CircleMinus`, as `cannotInstall` does today (UpdateSettings.tsx:66-67). The command is in `mono`, with a `btn ghost sm` Copy button and lucide's `Copy` icon, as Copy ID on a card (src/pages/TaskPage.tsx:208). [V] No teal or magenta: nothing is live and nothing needs you. So no design card is needed [I]. UI strings keep the code's spelling "Gizai"; the user docs keep theirs, "GizAi".

### 5.2 The updater: telling a package's Gizai apart

Four ways, compared:

| Way | How | Sure? | Knows which manager? | Cost |
|---|---|---|---|---|
| **A. A build-time setting, `GIZAI_PACKAGE` (recommended)** | The PKGBUILD and the formula set it while they build; `option_env!` puts it into the binary | Yes, for our packages: it travels with the binary, wherever its files go | Yes | One line per package; the updater and its texts |
| B. A marker file that the package installs (`/usr/lib/gizai/gizai-package.txt`) | `package()` writes it next to `gizai` | Only while the file is there and next to the binary. On macOS it can't go inside Gizai.app (the signature's seal) | Yes, its content | A file per package; the same code |
| C. A marker that the install scripts write (`gizai-installed.txt` on Linux and macOS too, as install.ps1 does on Windows) | install.sh writes it, and Gizai updates only with it | Yes, for every package, also those others make: it fails closed | No (needs A or B for the texts) | install.sh, the updater and their tests. Existing installs get the marker at their next update, which the old code runs with the new install.sh |
| D. Rules on the path (`/usr/…`, `…/Cellar/…`) | Guess from where it runs | No: prefixes vary (`/opt`, `/home/linuxbrew`, nix) | Partly | Small |

**Recommended: A**, now. C is good hardening for packages that others might make, but no one makes one today (Q4 in §7).

What changes, file by file:

1. `crates/gizai-agents/src/update.rs`: the build-time value and the commands.

   ```rust
   /// The package manager that built this Gizai, as GIZAI_PACKAGE said while it was built ("aur", "homebrew"), or
   /// None: a build of the install scripts or of a checkout. A package's Gizai never updates itself.
   pub const PACKAGE: Option<&str> = option_env!("GIZAI_PACKAGE");

   /// The command that updates a package's Gizai (None: Gizai doesn't know it).
   pub fn package_update_command(package: &str) -> Option<&'static str> {
       match package {
           "aur" => Some("yay -Syu"),
           "homebrew" => Some("brew update && brew upgrade gizai"),
           _ => None,
       }
   }
   ```

   `installer()` also drops it, so an update's own build is never marked, whatever Gizai's environment holds: `.env_remove("GIZAI_PACKAGE")` next to the others (update.rs:419).
2. `src-tauri/src/update.rs`:
   - `Inner` gets `package: Option<String>`. `Updates::default` fills it from `up::PACKAGE`, ignoring an empty value, and `Updates::set_package` sets it for tests, as `set_exe` does (:109-112).
   - The `install_prefix` wrapper (:216-226) answers a package first: `Err` with the texts of §5.1. So `status`, `check`, `start` and `restart_command`, which all go through it (:232, :262, :334, :510, :546), refuse a package's install.
   - `UpdateStatus` (:162-189) gets `package` and `update_command` (contract in §6).
3. `install.sh` and `install.ps1`: their build drops the variable too. In install.sh that is `unset GIZAI_PACKAGE` in the build's subshell (install.sh:197-208). In install.ps1 it is `GIZAI_PACKAGE = $null` in `$quiet`, the variables its build sets and then restores (install.ps1:211-228) [V]. So an install script's Gizai is never marked by accident.
4. The UI (PK2): `src/types.ts:441-459`, `UpdateSettings.tsx:27`, `:37-38` and `:66-67`, `UpdateNotice.tsx:60`, `src/lib/update.ts:30`.
5. Docs: docs/RELEASING.md:126 and docs/PLATFORMS.md:41 and :70 get one sentence each: "A Gizai built by a package (`GIZAI_PACKAGE`, the AUR or Homebrew) never updates itself: its package manager does."

The tests that pin the trap stay: `only_a_gizai_in_prefix_lib_gizai_has_an_install_to_update` (crates/gizai-agents/tests/update_test.rs:200-208) still holds for a Gizai without the setting. [V] Cargo rebuilds a crate when a variable read with `option_env!` changes [I: from memory, not read today]. That doesn't matter for packages: makepkg and brew always build clean.

### 5.3 The AUR package (draft, untested)

What it installs, against `install.sh` on Linux:

| install.sh (prefix `~/.local`) | The AUR package |
|---|---|
| `~/.local/lib/gizai/gizai`, `gizai-mcp` | `/usr/lib/gizai/gizai`, `gizai-mcp` (gizai-mcp next to gizai, src-tauri/src/mcp.rs:70-71) |
| `~/.local/lib/gizai/gizai-launch` | `/usr/lib/gizai/gizai-launch`: the same text with `/usr/lib/gizai` |
| `~/.local/bin/gizai` → the launcher | `/usr/bin/gizai` → the launcher |
| icons 32, 64, 128, 256 and 512 px, and the SVG, under `~/.local/share/icons/hicolor` | the same files under `/usr/share/icons/hicolor` |
| `~/.local/share/applications/gizai.desktop` | `/usr/share/applications/gizai.desktop` (`Exec=/usr/lib/gizai/gizai-launch`) |
| `update-desktop-database`, `gtk-update-icon-cache` | pacman's hooks, in `desktop-file-utils` and `gtk-update-icon-cache` [V] |
| none | `/usr/share/licenses/gizai/NOTICE` |

`PKGBUILD`:

```bash
# Maintainer: <name> <address at domain dot tld>
# GizAi, built from the source of a release on your own computer, as gizai.app/install.sh builds it.
# Draft from docs/research/packaging.md in Yotech-AI/gizai (GA-165): not built or tested yet.

pkgname=gizai
pkgver=0.10.0
pkgrel=1
pkgdesc='A desktop app where a team of AI agents works through your board'
arch=('x86_64')
url='https://gizai.app'
license=('Apache-2.0')
depends=(
  'cairo'
  'curl'
  'dbus'
  'gdk-pixbuf2'
  'git'
  'glib2'
  'glibc'
  'gtk3'
  'hicolor-icon-theme'
  'libayatana-appindicator'
  'libgcc'
  'libsoup3'
  'pango'
  'webkit2gtk-4.1'
)
makedepends=('cargo' 'nodejs' 'npm')
optdepends=('github-cli: review cards as pull requests on GitHub')
# !lto: makepkg's LTO flags would reach the C code cargo compiles (the bundled SQLite); Cargo.toml's own thin LTO stays.
# !debug: the release profile strips the programs (strip = true), so a debug package would be empty.
options=('!lto' '!debug')
source=("$pkgname-$pkgver.tar.gz::https://github.com/Yotech-AI/gizai/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('<the sha256 of that tarball: the release step fills it in>')

prepare() {
  cd "$pkgname-$pkgver"
  export RUSTUP_TOOLCHAIN=stable
  export npm_config_update_notifier=false npm_config_fund=false npm_config_audit=false
  cargo fetch --locked --target host-tuple
  npm ci --cache "$srcdir/npm-cache"
}

build() {
  cd "$pkgname-$pkgver"
  export RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR=target CARGO_NET_OFFLINE=true
  export TAURI_TELEMETRY_DISABLED=1 npm_config_update_notifier=false npm_config_fund=false npm_config_audit=false
  # The AUR's Gizai: it never updates itself, and Settings → Updates says to update it with yay.
  export GIZAI_PACKAGE=aur
  npm run tauri build -- --no-bundle
  cargo build --frozen --release -p gizai-mcp
}

check() {
  cd "$pkgname-$pkgver"
  # the release's own version, as Gizai's update checks it
  [ "$(target/release/gizai --version)" = "gizai $pkgver" ]
}

package() {
  cd "$pkgname-$pkgver"
  local lib=/usr/lib/gizai pair size file
  install -Dm755 target/release/gizai "$pkgdir$lib/gizai"
  install -Dm755 target/release/gizai-mcp "$pkgdir$lib/gizai-mcp"

  # The launcher, as install.sh writes it (install.sh:312-323), for /usr/lib/gizai.
  install -Dm755 /dev/stdin "$pkgdir$lib/gizai-launch" <<'LAUNCH'
#!/bin/sh
# Starts Gizai with your login shell's PATH, so its agents find claude, git, npm and cargo even when
# Gizai is started from the app launcher. On machines without an NVIDIA GPU, WebKitGTK needs Mesa's EGL.
if ! lspci 2>/dev/null | grep -qi nvidia && [ -f /usr/share/glvnd/egl_vendor.d/50_mesa.json ]; then
  export __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json
fi
if command -v bash >/dev/null 2>&1; then
  exec bash -lc 'exec "$0" "$@"' /usr/lib/gizai/gizai "$@"
fi
exec /usr/lib/gizai/gizai "$@"
LAUNCH
  install -dm755 "$pkgdir/usr/bin"
  ln -s "$lib/gizai-launch" "$pkgdir/usr/bin/gizai"

  # The icon in every size launchers ask for, and the SVG it is drawn from (install.sh:329-335).
  for pair in 32:32x32.png 64:64x64.png 128:128x128.png 256:128x128@2x.png 512:icon.png; do
    size="${pair%%:*}" file="${pair#*:}"
    install -Dm644 "src-tauri/icons/$file" "$pkgdir/usr/share/icons/hicolor/${size}x${size}/apps/gizai.png"
  done
  install -Dm644 src-tauri/icons/gizai.svg "$pkgdir/usr/share/icons/hicolor/scalable/apps/gizai.svg"

  # The desktop entry, as install.sh writes it (install.sh:336-347).
  install -Dm644 /dev/stdin "$pkgdir/usr/share/applications/gizai.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Gizai
Comment=Clients, projects and tasks, worked on by local AI agents
Exec=$lib/gizai-launch
Icon=gizai
Terminal=false
Categories=Development;ProjectManagement;
Keywords=tasks;kanban;agents;claude;projects;clients;
StartupWMClass=gizai
DESKTOP

  # Apache-2.0 itself is in Arch's licenses package; the NOTICE goes with the package (the licence's section 4(d)).
  install -Dm644 NOTICE -t "$pkgdir/usr/share/licenses/$pkgname/"
}
```

`.SRCINFO` (what `makepkg --printsrcinfo` should print for it; fields indented with a tab):

```
pkgbase = gizai
	pkgdesc = A desktop app where a team of AI agents works through your board
	pkgver = 0.10.0
	pkgrel = 1
	url = https://gizai.app
	arch = x86_64
	license = Apache-2.0
	makedepends = cargo
	makedepends = nodejs
	makedepends = npm
	depends = cairo
	depends = curl
	depends = dbus
	depends = gdk-pixbuf2
	depends = git
	depends = glib2
	depends = glibc
	depends = gtk3
	depends = hicolor-icon-theme
	depends = libayatana-appindicator
	depends = libgcc
	depends = libsoup3
	depends = pango
	depends = webkit2gtk-4.1
	optdepends = github-cli: review cards as pull requests on GitHub
	options = !lto
	options = !debug
	source = gizai-0.10.0.tar.gz::https://github.com/Yotech-AI/gizai/archive/refs/tags/v0.10.0.tar.gz
	sha256sums = <the sha256 of that tarball: the release step fills it in>

pkgname = gizai
```

Notes on the draft:

- **Source.** GitHub's tarball of the tag, renamed with `filename::url` (PKGBUILD(5)) [V]. Its folder is `gizai-0.10.0`, because GitHub drops the tag's `v`. yay's own AUR package builds `…/archive/v${pkgver}.tar.gz` the same way, in `$pkgname-$pkgver` [V: its PKGBUILD, fetched from the AUR's git]. The checksum is the tarball's sha256, the same number the formula uses (§5.6).
- **The .SRCINFO's order** of fields is the one makepkg writes, as in yay's `.SRCINFO`: pkgdesc, pkgver, pkgrel, url, arch, license, makedepends, depends, optdepends, options, source, the checksums [V].
- **prepare() and build()** follow Arch's Rust guidelines: `cargo fetch --locked --target host-tuple` in `prepare()`, and the build offline from the lock file in `build()` [V]. The Node guidelines put npm's cache in `$srcdir` (`--cache "${srcdir}/npm-cache"`) [V]. `CARGO_NET_OFFLINE=true` keeps the Tauri CLI's own `cargo build` offline too [I: the variable for cargo's `net.offline`]. The build commands are install.sh's own (install.sh:199-207) [V], plus `GIZAI_PACKAGE=aur`.
- **check()** compares `gizai --version` with `pkgver`, the bump check Gizai's update makes (docs/RELEASING.md:59). On Linux nothing runs before Gizai prints its version: reading the login shell's PATH happens on macOS only (src-tauri/src/shell_path.rs:6-9, main.rs:25-33). [V] A full `cargo test --workspace` is left to CI, which runs it on every pull request (docs/PLATFORMS.md:76) [V], since it needs fake CLIs and takes long [I].
- **options.** `!lto` [I]: makepkg's LTO flags would reach the C code that cc compiles for the bundled SQLite (Cargo.toml:11) [V], a known cause of failed Rust links. yay's PKGBUILD has `options=(!lto)` too, for its cgo build [V]. PK3 keeps it only if the build needs it. `!debug` [I]: `strip = true` (Cargo.toml:27) [V] leaves nothing for a debug package.
- **x86_64 only.** Arch itself is x86_64, and aarch64 is Arch Linux ARM's (PKGBUILD wiki page) [V]. Gizai's CI builds Linux on `ubuntu-24.04` x86_64 only (docs/RELEASING.md:18) [V/I].
- **The launcher and the desktop entry are copies** of install.sh's. When a release changes them, step 5 copies the change (§5.6). Q8 in §7 asks whether to give them one source in the repository later.
- **One install at a time.** A script install and the package share the data folder, and an older build refuses a newer database (docs/RELEASING.md:26) [V]. A leftover `~/.local/share/applications/gizai.desktop` also hides `/usr/share/applications/gizai.desktop`, and `~/.local/bin` usually comes before `/usr/bin` on PATH [I]. The docs say to remove the script's install first (§5.7).
- **The maintainer line** stays a placeholder until Jefsev picks the AUR account (Q1). The AUR asks for a licence for the packaging files themselves and suggests 0BSD [V].

### 5.4 The Homebrew tap (draft, untested)

The tap repository `Yotech-AI/homebrew-tap`:

```
Formula/gizai.rb          the formula below
README.md                 what the tap holds, the two commands, the Applications link, how to update
LICENSE                   for the formula itself (Q9 in §7)
.github/workflows/test.yml  on a pull request: build from source on macos-14, then brew test and brew audit; no bottles
```

What it installs, against `install.sh` on macOS:

| install.sh | The formula |
|---|---|
| `~/.local/lib/gizai/Gizai.app`, with gizai-mcp inside, signed ad hoc | `$(brew --prefix)/Cellar/gizai/X.Y.Z/Gizai.app` (also `$(brew --prefix)/opt/gizai/Gizai.app`), with gizai-mcp inside, signed ad hoc |
| `~/.local/lib/gizai/gizai`, `gizai-mcp` (links into the app) | none: kept out of `lib/gizai` on purpose (§3.2 Q3) |
| `~/.local/lib/gizai/gizai-launch` and `~/.local/bin/gizai` | `$(brew --prefix)/bin/gizai`, a script that runs the app's program |
| `lsregister` | `lsregister` in `post_install`, if Homebrew's sandbox lets it [I]; otherwise the first start from Finder registers the app |
| `~/Applications/Gizai.app`, a link | the same link, made once by you (the caveats) |

`Formula/gizai.rb`:

```ruby
# GizAi, built from the source of a release on your Mac, as gizai.app/install.sh builds it.
# Draft from docs/research/packaging.md in Yotech-AI/gizai (GA-165): not built or tested yet.
class Gizai < Formula
  desc "Desktop app where a team of AI agents works through your board"
  homepage "https://gizai.app"
  url "https://github.com/Yotech-AI/gizai/archive/refs/tags/v0.10.0.tar.gz"
  sha256 "<the sha256 of that tarball: the release step fills it in>"
  license "Apache-2.0"

  livecheck do
    url :stable
    strategy :github_latest
  end

  depends_on "node" => :build
  depends_on "rust" => :build
  depends_on arch: :arm64
  depends_on :macos

  def install
    # Homebrew's Gizai: it never updates itself, and Settings → Updates says to update it with brew.
    ENV["GIZAI_PACKAGE"] = "homebrew"
    ENV["TAURI_TELEMETRY_DISABLED"] = "1"
    ENV["npm_config_update_notifier"] = "false"
    ENV["npm_config_fund"] = "false"
    ENV["npm_config_audit"] = "false"
    system "npm", "ci", "--cache", buildpath/".npm-cache"
    system "npm", "run", "tauri", "build", "--", "--bundles", "app"
    system "cargo", "build", "--locked", "--release", "-p", "gizai-mcp"

    # Gizai.app holds both programs: Gizai looks for gizai-mcp next to itself.
    app = buildpath/"target/release/bundle/macos/Gizai.app"
    (app/"Contents/MacOS").install "target/release/gizai-mcp"
    # Signed ad hoc again, so the seal covers gizai-mcp too. An ad hoc signature names no one, as install.sh's.
    system "codesign", "--force", "--deep", "--sign", "-", app
    prefix.install app
    # The gizai command runs the program inside the app (not a link: macOS gives a program the path of the link).
    bin.write_exec_script prefix/"Gizai.app/Contents/MacOS/gizai"
  end

  def post_install
    # LaunchServices learns the app now, so its notifications come from Gizai, not Finder (as install.sh does).
    quiet_system "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
                 "-f", opt_prefix/"Gizai.app"
  end

  def caveats
    <<~EOS
      Gizai.app is in #{opt_prefix}. To start it from Finder and keep it in the Dock, link it into
      Applications in your home folder, once (the link follows upgrades):
        mkdir -p ~/Applications && ln -sfn #{opt_prefix}/Gizai.app ~/Applications/Gizai.app

      Gizai doesn't update itself when Homebrew installed it. Update it with:
        brew update && brew upgrade gizai
      then restart Gizai. Your data stays in ~/Library/Application Support/Gizai.
    EOS
  end

  test do
    assert_equal "gizai #{version}", shell_output("#{bin}/gizai --version").strip
    assert_predicate prefix/"Gizai.app/Contents/MacOS/gizai-mcp", :executable?
    ENV["GIZAI_DATA_DIR"] = (testpath/"data").to_s
    assert_match "no Gizai data", shell_output("#{bin}/gizai --backup test 2>&1", 1)
  end
end
```

Notes on the draft:

- **Build dependencies only.** Node.js and Rust are needed to build Gizai, not to run it (`=> :build` [V Formula Cookbook]). The browser for agents uses your own node and npx, found on PATH (docs/PLATFORMS.md:34). [V]
- **`depends_on :macos` and `depends_on arch: :arm64`** follow Gizai's decision for Apple silicon only (docs/PLATFORMS.md:9). Homebrew's architecture requirement knows `:arm64` (`arch_requirement.rb`). [V]
- **The build's network.** Homebrew's sandbox allows the network during builds unless a formula or `HOMEBREW_FORMULA_BUILD_NETWORK` denies it [V]. So `npm ci` and cargo can fetch, pinned by the lock files. npm's cache goes into the build's folder. Where cargo's cache goes in Homebrew's sandbox wasn't read today [I]: hundreds of core Rust formulae build there, and PK4's CI shows it.
- **The app's place and signature.** `prefix.install` puts Gizai.app at the keg's top, as MacVim's formula does [V]. The ad hoc `codesign` is install.sh's (install.sh:254-256) [V]. Whether Homebrew changes a program in the keg after the build, which would break the seal, is unknown [I]: PK4's CI runs `codesign --verify --deep --strict`, as install.yml:76 does [V].
- **`bin.write_exec_script`** writes `#!/bin/bash` and `exec "<target>" "$@"` (Homebrew's `pathname.rb`) [V], the same as install.sh's macOS launcher (install.sh:257-263) [V]. That the script gets its execute bit from Homebrew's cleaner is [I].
- **The test** runs the program for real: `--version` (main.rs:31-34), and `--backup` with an empty data folder, which fails with "no Gizai data" and exit code 1 (src-tauri/src/lib.rs:202-205, main.rs:39-45) [V]. On macOS Gizai first reads the login shell's PATH, for at most 5 seconds (docs/PLATFORMS.md:53) [V].
- **livecheck** `:github_latest` reads GitHub's "latest" release, as Gizai's own check does [V Brew Livecheck docs].
- **A stable copy, if the Dock needs it** (PK4 decides on the Mac; Q11 in §7). `post_install` runs after every install and upgrade. It can keep a copy of the app at a path that doesn't change with the version, replaced the way install.sh replaces its app (install.sh:246-271) [V]. The `ln -sfn` line and `bin/gizai` then point at that copy:

  ```ruby
  def post_install
    # A copy at a path that doesn't change with the version, so a Gizai kept in the Dock survives upgrades.
    dir = var/"gizai"
    dir.mkpath
    rm_r dir/".Gizai.app.new" if (dir/".Gizai.app.new").exist?
    system "ditto", opt_prefix/"Gizai.app", dir/".Gizai.app.new"
    rm_r dir/"Gizai.app" if (dir/"Gizai.app").exist?
    mv dir/".Gizai.app.new", dir/"Gizai.app"
  end
  ```

  The costs: the app is on disk twice; `brew uninstall` leaves the copy (the docs add `rm -rf "$(brew --prefix)/var/gizai"`); and whether the post-install sandbox lets it write `var` is [I], though formulae commonly make folders in `var` there. The setting `GIZAI_PACKAGE` still marks the copy, and `install_prefix` doesn't match its path either (its folder's parent is `var`, not `lib`).
- **Each `brew upgrade` builds from scratch.** With no bottle, every install and upgrade builds from source [I: Homebrew builds when there is no bottle].

### 5.5 Homebrew on Linux

Not in this design (Q2). If it is wanted later, the formula would get an `on_linux` block:

- dependencies `webkitgtk`, `gtk+3`, `libsoup`, `glib`, `cairo`, `pango`, `gdk-pixbuf`, `dbus`, `libayatana-appindicator`, and `pkgconf => :build`;
- `npm run tauri build -- --no-bundle`;
- `libexec` for the programs, and a launcher in `bin`;
- a caveat about `XDG_DATA_DIRS`.

Then a test on a real desktop, with GPU, themes and input methods.

### 5.6 The work per release (a new step 5 in docs/RELEASING.md)

Draft text, to follow step 4 (after docs/RELEASING.md:87):

> 5. **Update the packages**, in the same card, once step 4 shows the release. Both build GitHub's tarball of the tag, so both get its checksum:
>
>    ```sh
>    curl -fsSL -o gizai-X.Y.Z.tar.gz https://github.com/Yotech-AI/gizai/archive/refs/tags/vX.Y.Z.tar.gz
>    sha256sum gizai-X.Y.Z.tar.gz                  # on a Mac: shasum -a 256
>    ```
>
>    - **AUR**, in a clone of `ssh://aur@aur.archlinux.org/gizai.git` in a scratch folder: set `pkgver=X.Y.Z` and `pkgrel=1` in `PKGBUILD`, then:
>
>      ```sh
>      updpkgsums                                     # the checksum (pacman-contrib)
>      makepkg --verifysource                         # downloads the tarball and checks it
>      makepkg --printsrcinfo > .SRCINFO
>      git diff                                       # only pkgver, pkgrel, the checksum and .SRCINFO
>      git commit -am "Update to X.Y.Z"
>      git push                                       # the AUR takes pushes to master only
>      ```
>
>      When the release changed install.sh's launcher or desktop entry (`git diff vA.B.C vX.Y.Z -- install.sh`), copy the change into `package()` first.
>    - **Homebrew**, in `Yotech-AI/homebrew-tap`: set `url` to the new tag and `sha256` to the checksum in `Formula/gizai.rb`, and push it as a pull request named `gizai X.Y.Z`. Its CI builds it on macOS; merge it when that passes.
>    - **Check:** https://aur.archlinux.org/packages/gizai shows `X.Y.Z-1`, and `Formula/gizai.rb` on the tap's main branch says `vX.Y.Z`.
>
>    If a push fails, the release still stands, and Gizais from the install scripts update as before. Say so in the card. Package users find nothing new until the package follows.

And to "Never" (docs/RELEASING.md:95-100): "Never publish bottles or a `-bin` package: the packages build from source too."

**Who does it:** the DevOps Agent, in the release card (docs/RELEASING.md:3, :19). [V] What it needs on the PC it runs on (Jefsev's Arch PC) [I]:

- the AUR account's SSH key, and push rights to the tap;
- `curl`, `sha256sum`, `makepkg` and `updpkgsums` (pacman-contrib) among its allowed commands;
- `git push` over SSH, as releases already use (docs/RELEASING.md:89-93).

The first time, PK3 and PK4 publish; from then on, step 5 runs in every release.

### 5.7 The install docs

`docs/install.md`: a new part after "In one line" (after docs/install.md:36):

> ## With a package manager
>
> GizAi is also in the AUR (Arch Linux and Omarchy) and in YoTech's own Homebrew tap (Macs with Apple silicon). Both build the latest release from source on your computer, as the one line does. Your package manager updates GizAi, not GizAi itself: Settings → Updates says when a release is out, and what to run.
>
> Use one way to install GizAi, not two: they share your data. To move from the one line to a package, first remove the one line's install, which keeps your data: `curl -fsSL https://gizai.app/install.sh | sh -s -- --uninstall`.
>
> **Arch Linux and Omarchy:**
>
> ```sh
> yay -S gizai
> ```
>
> It installs GizAi in `/usr/lib/gizai`, the `gizai` command and the launcher entry. Update with `yay -Syu` (or only GizAi: `yay -S gizai`). Remove with `sudo pacman -R gizai`. Your data stays in `~/.local/share/gizai`.
>
> **macOS:**
>
> ```sh
> brew install yotech-ai/tap/gizai
> mkdir -p ~/Applications && ln -sfn "$(brew --prefix)/opt/gizai/Gizai.app" ~/Applications/Gizai.app
> ```
>
> The first line builds GizAi (this takes a while) and installs the `gizai` command. The second puts GizAi in Applications in your home folder, so you can open it from Finder and keep it in the Dock. Update with `brew update && brew upgrade gizai`, then restart GizAi. Remove with `brew uninstall gizai` and `rm ~/Applications/Gizai.app`. Your data stays in `~/Library/Application Support/Gizai`.

`docs/install.md`, in Updates (docs/install.md:130-141), one more sentence: "A GizAi from the AUR or Homebrew doesn't update itself: Settings → Updates shows the command that does."

`README.md`, after the clone blocks (after README.md:72):

> With a package manager: `yay -S gizai` on Arch Linux and Omarchy, or `brew install yotech-ai/tap/gizai` on a Mac with Apple silicon. They build from source too, and your package manager updates GizAi ([how](docs/install.md#with-a-package-manager)).

docs/PLATFORMS.md:41, :70 and docs/RELEASING.md:126 get the sentence from §5.2, item 5.

### 5.8 Security and privacy

- **What gets built.** The tarball's sha256 is pinned in both files, so a different file is refused. Tags never move (docs/RELEASING.md:97). [V] npm and cargo fetch only what `package-lock.json` and `Cargo.lock` pin (`npm ci`, `--locked`, `--frozen`).
- **Who can publish.** The AUR account's SSH key and push rights to the tap are publishing rights: whoever holds them can ship code that every package user builds and runs. Keep them on Jefsev's PC, never in CI (that rules out Option B for now) [I]. Two-factor login on the GitHub organisation covers the tap [I].
- **Nothing new is sent.** The release check stays as it is and sends nothing about you (docs/install.md:153-155). [V]
- **Data.** Packages never touch the data folder, and removing a package leaves it [I: pacman and brew remove only the files they installed].
- **macOS signing.** Only an ad hoc signature, as install.sh makes: no identity and no account (docs/install.md:121-126). [V]

### 5.9 How it is tested

- **PK1 and PK2.** Unit and flow tests in the style of `a_gizai_that_isnt_installed_never_installs_anything` (src-tauri/tests/update_flow_test.rs:359-373): with `set_package(Some("aur"))` and an exe at a scratch `<prefix>/lib/gizai/gizai`, `cannot_install` has the AUR text, `install_to` is null, `start` fails, and nothing appears in `data/update`. Also a test that an update's build gets no `GIZAI_PACKAGE`, and the UI tests (`src/lib/update.test.ts`, `src/components/UpdateNotice.test.tsx`) plus `scripts/ui-test.sh`.
- **PK3, the AUR.** On the Arch PC, in a scratch folder, as a normal user:
  - `makepkg --verifysource`, then `makepkg` (build only: never `-i`, nothing goes into `/usr`);
  - `namcap PKGBUILD` and `namcap gizai-*.pkg.tar.zst`;
  - the file list (`tar -tf`) against install.sh's Linux install (§5.3), and `makepkg --printsrcinfo | diff - .SRCINFO`.
  - Then the packaged program, `pkg/gizai/usr/lib/gizai/gizai`, runs headless in `cage` with `HOME`, `XDG_DATA_HOME` and `GIZAI_DATA_DIR` in the scratch folder and a fake release (docs/RELEASING.md:134-171): Settings → Updates shows the AUR text. The setting is in the program, so its path doesn't matter.
  - Installing into `/usr` needs sudo. Jefsev runs `sudo pacman -U <the package>` from `run_for_me` and checks the launcher entry and the tray.
- **PK4, the tap.** Its CI on `macos-14` (Apple silicon, as Gizai's CI, docs/PLATFORMS.md:49):
  - `brew install --build-from-source yotech-ai/tap/gizai`, `brew test gizai` and `brew audit --strict yotech-ai/tap/gizai` (any findings go into the card's summary);
  - `codesign --verify --deep --strict` on the installed app.
  - Then on Jefsev's Mac: install, make the link, start from Finder, keep it in the Dock; after the next release, `brew update && brew upgrade gizai`: does the Dock item still start Gizai, and does Spotlight find it? This is also a line for docs/PLATFORMS.md's check list (:81-93).

### 5.10 What is left out

- **Gizai noticing a package's upgrade** and offering "Restart to use X.Y.Z", as after its own update. Until then, you quit and start Gizai after `yay -Syu` or `brew upgrade`. Until you do, chat's helper may be missing (brew removes the old version's `gizai-mcp`) [I]. Q6 in §7.
- `gizai-git`, a `-bin` package, a cask, bottles, Homebrew on Linux, Scoop, winget, Flatpak, and Debian or Fedora packages.
- Updating the packages from CI (Option B).
- The website's install page (another repository; Q10 in §7).
- aarch64 Linux on the AUR.

---

## 6. Proposed cards

Order: **PK1 and PK2 at the same time** → a release that has both (the usual release card) → **PK3 and PK4 at the same time** → **PK5**. The packages need a release with PK1 in it: `GIZAI_PACKAGE` means nothing to an older Gizai. No card needs a design first (§5.1).

### PK1. Updates: a Gizai built by a package doesn't update itself

- **Column:** To do. **Role:** backend. **Builds on:** none. Needs Jefsev's OK for the updater change (Q3 in §7).
- **Builds:** `GIZAI_PACKAGE` read with `option_env!` (`gizai_agents::update::PACKAGE`) and `package_update_command`. In `src-tauri/src/update.rs`: `Inner.package` with `Updates::set_package`, the package texts first in `install_prefix`, and `package` and `update_command` in `UpdateStatus`. `installer()` drops `GIZAI_PACKAGE`, and so do install.sh's and install.ps1's builds. The sentence from §5.2 item 5 in RELEASING.md and PLATFORMS.md.
- **Acceptance:**
  - [ ] With `package` `aur`, a Gizai at `<prefix>/lib/gizai/gizai` has no `installTo`. Its `cannotInstall` is the AUR text of §5.1, word for word, and `start_update` fails with it, fetching and building nothing (a flow test).
  - [ ] The same for `homebrew` and its text. Any other value gives the general text and no command; an empty value is no package.
  - [ ] The release check still runs, and `available` still shows a newer release.
  - [ ] An update's build never has `GIZAI_PACKAGE` (a test with a stub install.sh that writes down its environment); install.sh and install.ps1 drop it in their build.
  - [ ] Without `GIZAI_PACKAGE` nothing changes: all existing update tests pass.
  - [ ] `cargo test --workspace`, `npm test` and `npm run build` pass.

### PK2. Settings → Updates: a package's Gizai shows the command that updates it

- **Column:** To do. **Role:** frontend. **Builds on:** PK1's contract below. It can be built at the same time as PK1.
- **Builds:** the two new fields in `src/types.ts`. The section's text per package (`UpdateSettings.tsx:27`). In the Update field: the `cannotInstall` line, the command in mono with Copy, and the hint (`:66-67`). The sidebar's notice "Gizai X.Y.Z is out", opening Settings, with `cannotInstall` as its tooltip (`src/lib/update.ts:30`, `UpdateNotice.tsx:60`).
- **Acceptance:**
  - [ ] For `package: "aur"` and `"homebrew"`, every text of §5.1 shows word for word, and Copy copies `updateCommand`.
  - [ ] For any other package there's no command and no Copy.
  - [ ] Without `package`, Settings and the notice look exactly as today (the existing tests pass).
  - [ ] Tests in `src/lib/update.test.ts` and `UpdateNotice.test.tsx`, and a new `src/components/UpdateSettings.test.tsx` for the section's text and the Update field (there is none yet).
  - [ ] `npm test`, `npm run build` and `scripts/ui-test.sh` pass. The card's summary has a screenshot of Settings → Updates for a package, from `cage`.

**Contract PK1 → PK2.** No new command: `update_status`, `check_for_updates`, `set_update_auto_check`, `start_update`, `stop_update` and `restart_gizai` stay as they are (src/api.ts:307-317). `UpdateStatus` gets two fields.

In Rust (`src-tauri/src/update.rs`, `UpdateStatus`, serde camelCase):

```rust
    /// The package manager that built this Gizai ("aur", "homebrew", or another name GIZAI_PACKAGE gave it), or
    /// None: an install script's Gizai, or a dev build.
    pub package: Option<String>,
    /// The command that updates it: "yay -Syu" for "aur", "brew update && brew upgrade gizai" for "homebrew"; None
    /// for any other package, and for a Gizai that isn't a package's.
    pub update_command: Option<String>,
```

In TypeScript (`src/types.ts`, `UpdateStatus`):

```ts
  /** The package manager that built this Gizai ("aur", "homebrew", …), or null: an install script's Gizai or a dev build. */
  package?: string | null;
  /** The command that updates a package's Gizai ("yay -Syu"), or null. */
  updateCommand?: string | null;
```

For a package's Gizai, `installTo` is null, and `cannotInstall` is the text of §5.1 for its package, word for word:

- `aur`: "Gizai came from the AUR, so it doesn't update itself. Update it with yay -Syu, then restart Gizai."
- `homebrew`: "Gizai came from Homebrew, so it doesn't update itself. Update it with brew update && brew upgrade gizai, then restart Gizai."
- any other value: "Gizai came from a package, so it doesn't update itself. Update it with the package manager that installed it, then restart Gizai."

`start_update` fails with that same text.

### PK3. AUR: build, check and publish the gizai package

- **Column:** To do. **Role:** backend (the DevOps Agent can take it). **Builds on:** a release with PK1 in it. Needs the AUR account (Q1) and Jefsev's OK to publish. It can run at the same time as PK4.
- **Builds:** the PKGBUILD and .SRCINFO of §5.3 for that release, tested as in §5.9, with every fix written down. Then the AUR package itself. The AUR takes pushes to `master` only and has no pull requests [V], so a card's branch can't go there. So this card's pull request, here, puts the tested files and the test results into §5.3 of this document. Jefsev publishes with the exact `git push` that the card puts in `run_for_me`.
- **Acceptance:**
  - [ ] `makepkg` builds the release as a normal user, with nothing installed into `/usr`, and `check()` passes.
  - [ ] namcap's findings are fixed or explained in the summary; `depends` matches what namcap finds linked.
  - [ ] The package's files match §5.3's table: gizai, gizai-mcp, the launcher, `/usr/bin/gizai`, the five icons and the SVG, the desktop entry, and NOTICE.
  - [ ] Headless in `cage`, Settings → Updates of the packaged program shows the AUR text of §5.1.
  - [ ] After Jefsev's install, the launcher entry starts Gizai with the tray icon, and `gizai --version` says the release.
  - [ ] Once published, https://aur.archlinux.org/packages/gizai shows the version.
  - [ ] §5.3 of this document holds the published PKGBUILD and .SRCINFO, word for word.

### PK4. Homebrew: the tap Yotech-AI/homebrew-tap with the gizai formula

- **Column:** To do. **Role:** backend. **Builds on:** a release with PK1 in it, and the tap repository, which Jefsev creates and adds to Gizai as a project (Q2). It can run at the same time as PK3.
- **Builds:** a pull request in the tap repository with `Formula/gizai.rb` (§5.4), `README.md`, `LICENSE`, and `.github/workflows/test.yml`, which builds from source on `macos-14` and runs `brew test`, `brew audit` and `codesign --verify`. No bottle workflow, no `brew pr-pull`.
- **Acceptance:**
  - [ ] The tap's CI builds the formula from source on Apple silicon, and `brew test` passes.
  - [ ] The app has `gizai-mcp` inside and verifies with `codesign --verify --deep --strict`.
  - [ ] `gizai --version` says the release. Gizai.app is in `$(brew --prefix)/opt/gizai/`, with nothing under `lib/gizai`.
  - [ ] The caveats show the `ln -sfn` line and `brew update && brew upgrade gizai`.
  - [ ] Nothing is built for download: no bottle block, no bottle publishing.
  - [ ] The summary has a check list for Jefsev's Mac (§5.9): Dock, Spotlight, and an upgrade to the next release. If the Dock's item breaks at the upgrade, a follow-up pull request switches the formula to the stable copy of §5.4 (Q11).

### PK5. Install docs and the release step for the packages

- **Column:** To do. **Role:** backend (docs). **Builds on:** PK3 and PK4, published.
- **Builds:** the docs/install.md part, the README line and the Updates sentence (§5.7). Step 5 and the "Never" line in docs/RELEASING.md (§5.6). docs/PLATFORMS.md's check-list line for Homebrew (§5.9). If Jefsev uses the AUR package on this PC, CLAUDE.md's Never list names `/usr/bin/gizai` and `/usr/lib/gizai` next to `~/.local/bin/gizai`.
- **Acceptance:**
  - [ ] The commands in the docs are the ones PK3 and PK4 tested, word for word.
  - [ ] Step 5 names the DevOps Agent, the checksum, `pkgver`, `.SRCINFO`, the tap's pull request and the checks.
  - [ ] The one line stays the first install route in README.md and docs/install.md.
  - [ ] The README's link to `docs/install.md#with-a-package-manager` opens that part on GitHub.

---

## 7. Open questions for Jefsev

1. **The AUR account (needs your OK).** Who owns it (you, or a YoTech account), and which name and address go in the PKGBUILD's Maintainer line and on the AUR's public page? *Recommendation:* a YoTech account, with its SSH key only on your PC. Create it soon: the name `gizai` is free today, but nothing reserves it.
2. **The tap repository (needs your OK).** Create `Yotech-AI/homebrew-tap`: public, with a protected main branch, and added to Gizai as a project so cards can open pull requests there. *Recommendation:* yes, as PK4 describes.
3. **The updater change (needs your OK).** Approve `GIZAI_PACKAGE` (§5.2, option A) and the texts of §5.1? *Recommendation:* yes, as PK1 and PK2, released before the first package goes public.
4. **Also make the install scripts mark their own installs** (§5.2, option C), so a package someone else makes can't fall into the trap either? *Recommendation:* not now.
5. **`gizai-git`?** *Recommendation:* no (§3.2 Q1).
6. **Should Gizai notice a package's upgrade and offer "Restart to use X.Y.Z"?** *Recommendation:* later, as its own card.
7. **`libayatana-appindicator` in `depends` (as your brief has it) or in `optdepends` (Gizai runs without it)?** *Recommendation:* `depends`, so the tray just works on Omarchy.
8. **The launcher and the desktop entry exist twice** (install.sh and the PKGBUILD). Give them one source in the repository (for example `packaging/linux/`) that both use? *Recommendation:* not now; step 5 checks install.sh's diff each release.
9. **A licence for the packaging files** (the AUR asks for one and suggests 0BSD): 0BSD for the AUR repository and the tap? *Recommendation:* yes, 0BSD for both.
10. **Homebrew on Linux, and Scoop?** *Recommendation:* neither now (§3.2 Q2 and Q5). The website's install page (another repository) gets the two commands once the packages are live.
11. **Homebrew and the Dock: a plain link, or a stable copy of the app** (§5.4)? A link is Homebrew's usual way, but a Gizai kept in the Dock probably breaks at every upgrade. A stable copy survives upgrades, but `brew uninstall` leaves it behind. *Recommendation:* PK4 tries the link on your Mac over one upgrade, and switches to the stable copy if the Dock item breaks.

---

## 8. Sources

All read on 2026-10-13.

| Source | Link |
|---|---|
| The AUR's git server: `git fetch --dry-run` of `gizai.git`, `gizai-git.git` (no `HEAD`) and `yay.git` (found); yay's PKGBUILD and .SRCINFO (13.0.1-1), read with `git fetch` and `git show FETCH_HEAD:PKGBUILD` | https://aur.archlinux.org/gizai.git, https://aur.archlinux.org/gizai-git.git, https://aur.archlinux.org/yay.git |
| The AUR's API and page (answered with Anubis's bot check) | https://aur.archlinux.org/rpc/v5/info?arg[]=gizai&arg[]=gizai-git, https://aur.archlinux.org/packages/gizai |
| Arch's package search (gizai: 0 results; rust, rustup, nodejs, npm, webkit2gtk-4.1, libayatana-appindicator, hicolor-icon-theme, libdbus: one name per query) | https://archlinux.org/packages/search/json/?name=gizai |
| The pacman hooks in desktop-file-utils 0.28-1 and gtk-update-icon-cache | https://archlinux.org/packages/extra/x86_64/desktop-file-utils/files/, https://archlinux.org/packages/extra/x86_64/gtk-update-icon-cache/files/ |
| PKGBUILD(5), pacman's manual (last updated 2026-02-04) | https://pacman.archlinux.page/PKGBUILD.5.html |
| ArchWiki, through a Kiwix copy of its April 2026 dump (wiki.archlinux.org and its raw pages answered with Anubis): Rust package guidelines, Node.js package guidelines, VCS package guidelines, AUR submission guidelines, .SRCINFO, PKGBUILD | https://wikipedia.jakami.de/content/archlinux_en_all_maxi_2026-04/Rust_package_guidelines (and `/Node.js_package_guidelines`, `/VCS_package_guidelines`, `/AUR_submission_guidelines`, `/.SRCINFO`, `/PKGBUILD`) |
| ArchWiki's own address of the Rust guidelines (search result only) | https://wiki.archlinux.org/title/Rust_package_guidelines |
| Tauri 2, "AUR" (updated 2026-06-02): the from-source PKGBUILD's depends and makedepends | https://v2.tauri.app/distribute/aur/ |
| Tauri 2, CLI reference, `tauri build` (updated 2024-08-12) | https://v2.tauri.app/reference/cli/ |
| yay's README (`yay` = `yay -Syu`, `--devel`) | https://raw.githubusercontent.com/Jguer/yay/next/README.md |
| Homebrew, "How to Create and Maintain a Tap" | https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap |
| Homebrew, "Acceptable Formulae" (an `.app` formula is not eligible for core) | https://docs.brew.sh/Acceptable-Formulae |
| Homebrew, "Formula Cookbook" | https://docs.brew.sh/Formula-Cookbook |
| Homebrew, "Language-Specific Formulae" (Node: `std_npm_args`) | https://docs.brew.sh/Language-Specific-Formulae |
| Homebrew, "Homebrew on Linux" | https://docs.brew.sh/Homebrew-on-Linux |
| Homebrew, "brew livecheck" (`:github_latest`) | https://docs.brew.sh/Brew-Livecheck |
| Homebrew, the Manpage (`brew upgrade`, `brew tap`, `bump-formula-pr`) | https://docs.brew.sh/Manpage |
| Homebrew's source: `env_config.rb` (build network, auto-update, cleanup, the Linux sandbox), `sandbox.rb`, `cmd/shellenv.sh`, `requirements/arch_requirement.rb`, `extend/pathname.rb` (`write_exec_script`) | https://github.com/Homebrew/brew/tree/main/Library/Homebrew |
| Homebrew/brew#1808, "Deprecate brew (un)linkapps." (merged 2017-01-11), and Homebrew 1.2.0's notes that list it (2017-05-01) | https://github.com/Homebrew/brew/pull/1808, https://brew.sh/2017/05/01/homebrew-1.2.0/ |
| formulae.brew.sh: `webkitgtk` 2.54.1, `libayatana-appindicator` 0.6.1, `gizai` (formula and cask: 404) | https://formulae.brew.sh/api/formula/webkitgtk.json, https://formulae.brew.sh/api/formula/libayatana-appindicator.json, https://formulae.brew.sh/api/formula/gizai.json, https://formulae.brew.sh/api/cask/gizai.json |
| homebrew-core: `webkitgtk.rb` (`USE_GTK4=OFF`, tests `webkit2gtk-4.1`), `macvim.rb` (an app in its keg) | https://github.com/Homebrew/homebrew-core/blob/main/Formula/w/webkitgtk.rb, https://github.com/Homebrew/homebrew-core/blob/main/Formula/m/macvim.rb |
| emacs-plus's tap, README, Usage (a formula's apps and Applications: copies, links, Spotlight, aliases and the versioned keg) | https://github.com/d12frosted/homebrew-emacs-plus |
| GitHub blog, "Update on the future stability of source code archives and hashes" (2023-02-21) | https://github.blog/open-source/git/update-on-the-future-stability-of-source-code-archives-and-hashes/ |
| GitHub docs, "Downloading source code archives" | https://docs.github.com/en/repositories/working-with-files/using-files/downloading-source-code-archives |
| Scoop wiki, "App Manifests" and "App Manifest Autoupdate" | https://github.com/ScoopInstaller/Scoop/wiki/App-Manifests, https://github.com/ScoopInstaller/Scoop/wiki/App-Manifest-Autoupdate |
| Microsoft Learn, "Install the Microsoft C++ (MSVC) Build Tools" (2026-08-20) | https://learn.microsoft.com/cpp/overview/acquire-msvc |
| Repology's API (refused: 403) | https://repology.org/api/v1/project/gizai |

Not read, so marked [I] where used: pacman's rule on unversioned provides; cargo's rebuild on `option_env!`; where cargo's cache goes in Homebrew's build sandbox; whether Homebrew's sandbox allows `lsregister` in `post_install`; whether the Dock keeps a link's or the version's path; Homebrew's cleaner and the execute bit; the licences of pacman and Homebrew.
