#!/usr/bin/env bash
# Gizai installer for Linux and macOS. Builds Gizai from source and installs it for you alone (no sudo):
#   ~/.local/lib/gizai/     gizai, gizai-mcp (the helper chat uses) and gizai-launch; on macOS also Gizai.app
#   ~/.local/bin/gizai      the command
#   Linux: a desktop entry and an icon, so Gizai shows up in your app launcher
#   macOS: a link to Gizai.app in ~/Applications, so Gizai starts from Finder and the Dock
# Your data lives in ~/.local/share/gizai (macOS: ~/Library/Application Support/Gizai) and is never touched by install
# or uninstall (unless --purge). On Windows, use install.ps1 (see the README).
#
# Usage:
#   ./install.sh                 from a checkout: check, build and install
#   curl -fsSL <raw url of this file> | bash
#                                clone (or update) the source into ~/.local/share/gizai-src, then the same
#   ./install.sh --check         only report what is missing
#   ./install.sh --build-only    check and build, install nothing (Gizai's own Update runs this, then --skip-build)
#   ./install.sh --skip-build    install the binaries already built in target/release
#   ./install.sh --uninstall     remove Gizai (add --purge to delete your data too)
# Environment: GIZAI_REPO (the git URL to clone), GIZAI_BRANCH (default production: the released code; main is
# development), GIZAI_PREFIX (default ~/.local), GIZAI_DATA_DIR (macOS: the data folder, if not the usual one).
# It runs with macOS's own bash 3.2 and BSD tools too: no newer bash features or GNU-only options.
set -euo pipefail

DEFAULT_REPO="https://github.com/Yotech-AI/gizai.git"
REPO="${GIZAI_REPO:-$DEFAULT_REPO}"
PREFIX="${GIZAI_PREFIX:-$HOME/.local}"
SHARE="${XDG_DATA_HOME:-$HOME/.local/share}"
LIB="$PREFIX/lib/gizai"
BIN="$PREFIX/bin"
APPS="$SHARE/applications"
ICONS="$SHARE/icons/hicolor"
DATA="$SHARE/gizai"
SRC_CLONE="$SHARE/gizai-src"
OS="$(uname -s)"
if [ "$OS" = Darwin ]; then
  # macOS keeps an app's data in ~/Library/Application Support, and apps in ~/Applications.
  DATA="${GIZAI_DATA_DIR:-$HOME/Library/Application Support/Gizai}"
  APP_LINK="$HOME/Applications/Gizai.app"
fi

CHECK=0 BUILD_ONLY=0 SKIP_BUILD=0 UNINSTALL=0 PURGE=0
for arg in "$@"; do
  case "$arg" in
    --check) CHECK=1 ;;
    --build-only) BUILD_ONLY=1 ;;
    --skip-build) SKIP_BUILD=1 ;;
    --uninstall) UNINSTALL=1 ;;
    --purge) PURGE=1 ;;
    -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $arg (see --help)" >&2; exit 2 ;;
  esac
done
if [ "$BUILD_ONLY" = 1 ] && [ "$SKIP_BUILD" = 1 ]; then
  echo "--build-only and --skip-build don't go together (see --help)" >&2
  exit 2
fi

say() { printf '%s\n' "$*"; }
step() { printf '\n== %s\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }

case "$OS" in
  Linux|Darwin) ;;
  MINGW*|MSYS*|CYGWIN*) say "On Windows, install Gizai with install.ps1 in PowerShell (see the README)."; exit 1 ;;
  *) say "Gizai installs on Linux and macOS with this script, and on Windows with install.ps1."; exit 1 ;;
esac

# ---------- uninstall (macOS) ----------
if [ "$UNINSTALL" = 1 ] && [ "$OS" = Darwin ]; then
  step "Removing Gizai"
  rm -f "$BIN/gizai"
  # the link in ~/Applications only when it is this install's
  if [ -L "$APP_LINK" ] && [ "$(readlink "$APP_LINK")" = "$LIB/Gizai.app" ]; then rm -f "$APP_LINK"; fi
  rm -rf "$LIB"
  say "Removed the app and the command."
  if [ "$PURGE" = 1 ]; then
    rm -rf "$DATA" "$SRC_CLONE"
    say "Deleted your data ($DATA) and the source copy ($SRC_CLONE)."
  else
    say "Your data stays in $DATA (run with --uninstall --purge to delete it)."
  fi
  exit 0
fi

# ---------- uninstall ----------
if [ "$UNINSTALL" = 1 ]; then
  step "Removing Gizai"
  rm -f "$BIN/gizai" "$APPS/gizai.desktop" "$ICONS/scalable/apps/gizai.svg"
  for size in 32 64 128 256 512; do rm -f "$ICONS/${size}x${size}/apps/gizai.png"; done
  rm -rf "$LIB"
  have update-desktop-database && update-desktop-database "$APPS" >/dev/null 2>&1 || true
  # Refresh the icon cache only where one exists already (without one, GTK reads the folders directly).
[ -f "$ICONS/icon-theme.cache" ] && have gtk-update-icon-cache && gtk-update-icon-cache -q -t -f "$ICONS" >/dev/null 2>&1 || true
  say "Removed the app, the command, the desktop entry and the icon."
  if [ "$PURGE" = 1 ]; then
    rm -rf "$DATA" "$SRC_CLONE"
    say "Deleted your data ($DATA) and the source copy ($SRC_CLONE)."
  else
    say "Your data stays in $DATA (run with --uninstall --purge to delete it)."
  fi
  exit 0
fi

# ---------- check ----------
step "Checking what Gizai needs"
missing_cmds=() missing_libs=() notes=()
node_from="your package manager, mise or nvm"
if [ "$OS" = Darwin ]; then
  # git and a C compiler come with Apple's Command Line Tools (until then, git and cc only offer to install them).
  xcode-select -p >/dev/null 2>&1 || notes+=("Apple's Command Line Tools (git and a C compiler) are missing: xcode-select --install")
  node_from="Homebrew (brew install node), mise, nvm or the installer from nodejs.org"
else
  for c in git pkg-config cc; do have "$c" || missing_cmds+=("$c"); done
fi
if [ "$SKIP_BUILD" = 0 ]; then
  have cargo || notes+=("Rust is missing: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   (then open a new terminal)")
  if have node; then
    major="$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)"
    [ "$major" -ge 20 ] || notes+=("Node.js 20 or newer is needed (found $(node -v)); install it with $node_from")
  else
    notes+=("Node.js 20 or newer is missing; install it with $node_from")
  fi
  have npm || notes+=("npm is missing (it comes with Node.js)")
fi
if [ "$OS" = Linux ]; then
  for lib in webkit2gtk-4.1 gtk+-3.0 librsvg-2.0 openssl; do
    have pkg-config && pkg-config --exists "$lib" 2>/dev/null || missing_libs+=("$lib")
  done
fi

distro=""
if [ -r /etc/os-release ]; then
  # shellcheck disable=SC1091
  . /etc/os-release
  distro=" ${ID:-} ${ID_LIKE:-} "
fi
deps_command() {
  case "$distro" in
    *" arch "*|*" manjaro "*|*" endeavouros "*|*" omarchy "*) say "sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libayatana-appindicator librsvg git" ;;
    *" debian "*|*" ubuntu "*) say "sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev pkg-config git" ;;
    *" fedora "*|*" rhel "*) say "sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel git && sudo dnf group install c-development" ;;
    *" opensuse"*|*" suse "*) say "sudo zypper in webkit2gtk3-soup2-devel libopenssl-devel curl wget file libappindicator3-1 librsvg-devel git && sudo zypper in -t pattern devel_basis" ;;
    *) say "Install WebKitGTK 4.1, GTK 3, librsvg and OpenSSL development packages, a C compiler and pkg-config (see https://v2.tauri.app/start/prerequisites/)." ;;
  esac
}

ok=1
if [ "${#missing_cmds[@]}" -gt 0 ] || [ "${#missing_libs[@]}" -gt 0 ]; then
  ok=0
  # ${a[*]-} and ${a[@]+...}: bash before 4.4 (macOS has 3.2) calls an empty array unbound under set -u
  say "Missing system packages: ${missing_cmds[*]-} ${missing_libs[*]-}"
  say "Install them with:"
  say "  $(deps_command)"
fi
for n in ${notes[@]+"${notes[@]}"}; do ok=0; say "$n"; done
if have claude; then
  say "Claude Code: found ($(command -v claude)). Make sure you are logged in: run 'claude' once."
else
  say "Claude Code: not found. Gizai's agents and chat run it; install it from https://docs.claude.com/en/docs/claude-code and log in."
fi
# The tray icon loads libayatana-appindicator (or the older libappindicator) when Gizai starts; Gizai works without it.
if have ldconfig && ! ldconfig -p 2>/dev/null | grep -qE 'lib(ayatana-)?appindicator3\.so\.1'; then
  say "Tray icon: libayatana-appindicator is missing, so Gizai will have no tray icon (Arch and Omarchy: sudo pacman -S libayatana-appindicator; Debian and Ubuntu: sudo apt install libayatana-appindicator3-1)."
fi
if [ "$OS" = Darwin ] && [ "$(uname -m)" != arm64 ]; then
  say "Note: Gizai is made for Macs with Apple silicon, and this shell runs as $(uname -m) (an Intel Mac, or Rosetta). The build may work, but isn't tested."
fi
[ "$ok" = 1 ] && say "Everything Gizai needs to build is here."
if [ "$CHECK" = 1 ]; then exit $((1 - ok)); fi
if [ "$ok" = 0 ] && [ "$SKIP_BUILD" = 0 ]; then
  say ""
  say "Install the missing pieces above, then run this again."
  exit 1
fi

# ---------- source ----------
here="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || true)"
if [ -n "$here" ] && [ -f "$here/src-tauri/tauri.conf.json" ] && grep -q '"productName": "Gizai"' "$here/src-tauri/tauri.conf.json"; then
  SRC="$here"
else
  step "Getting the source"
  if [ -z "$REPO" ]; then
    say "Set GIZAI_REPO to the git URL of Gizai, or run this script from a checkout."
    exit 1
  fi
  if [ -d "$SRC_CLONE/.git" ]; then
    git -C "$SRC_CLONE" pull --ff-only
  else
    git clone --depth 1 --branch "${GIZAI_BRANCH:-production}" "$REPO" "$SRC_CLONE"
  fi
  SRC="$SRC_CLONE"
fi
say "Source: $SRC"

# ---------- build ----------
if [ "$SKIP_BUILD" = 0 ]; then
  step "Building (this takes a few minutes the first time)"
  (
    cd "$SRC"
    export TAURI_TELEMETRY_DISABLED=1 npm_config_update_notifier=false npm_config_fund=false npm_config_audit=false
    npm ci
    if [ "$OS" = Darwin ]; then
      # macOS: Gizai.app too (target/release/bundle/macos), so Gizai starts from the Dock and Finder. Nothing is signed.
      npm run tauri build -- --bundles app
    else
      npm run tauri build -- --no-bundle
    fi
    cargo build --release -p gizai-mcp
  )
fi
for b in gizai gizai-mcp; do
  if [ ! -x "$SRC/target/release/$b" ]; then
    say "$SRC/target/release/$b is missing: build first (run without --skip-build)."
    exit 1
  fi
done
APP_BUILT="$SRC/target/release/bundle/macos/Gizai.app"
if [ "$OS" = Darwin ] && [ ! -x "$APP_BUILT/Contents/MacOS/gizai" ]; then
  say "$APP_BUILT is missing: build first (run without --skip-build)."
  exit 1
fi
if [ "$BUILD_ONLY" = 1 ]; then
  step "Built: $("$SRC/target/release/gizai" --version 2>/dev/null || echo gizai), in $SRC/target/release"
  say "Nothing was installed. Install it with: $SRC/install.sh --skip-build"
  exit 0
fi

# ---------- back up ----------
# Before replacing a Gizai you use, snapshot its data (the new build opens it, and may upgrade it, on its next start).
if [ -f "$DATA/gizai.db" ]; then
  step "Backing up your data"
  if snap="$(GIZAI_DATA_DIR="$DATA" "$SRC/target/release/gizai" --backup before-install 2>&1)"; then
    say "Backed up your data to $snap"
  else
    say "Could not back up $DATA/gizai.db: $snap"
    say "So nothing was installed. Move that file aside (or fix it), then run this again."
    exit 1
  fi
fi

# ---------- install (macOS) ----------
# Gizai.app holds both programs (Gizai looks for gizai-mcp next to itself). lib/gizai/gizai and gizai-mcp link into it,
# and gizai-launch, behind the gizai command, starts the app's own program, so macOS shows it as Gizai.
if [ "$OS" = Darwin ]; then
  step "Installing"
  mkdir -p "$LIB" "$BIN"
  # The new Gizai.app is put together under a temporary name of this installer's own first, then replaces the old one
  # with renames: a copy that fails (a full disk) leaves the installed Gizai as it was.
  new_app="$LIB/.Gizai.app.new.$$" old_app="$LIB/.Gizai.app.old.$$" new_launch="$LIB/.gizai-launch.new.$$"
  trap 'rm -rf "$new_app" "$new_launch"' EXIT
  if ! ditto "$APP_BUILT" "$new_app" || ! install -m 755 "$SRC/target/release/gizai-mcp" "$new_app/Contents/MacOS/gizai-mcp"; then
    say "Could not copy Gizai into $LIB, so nothing was installed."
    exit 1
  fi
  # Sign the app ad hoc again, so its signature covers gizai-mcp too (Apple silicon runs only signed programs, and a file
  # added after the build leaves a stale seal). An ad hoc signature names no one: Gizai stays unsigned in that sense.
  codesign --force --deep --sign - "$new_app" >/dev/null 2>&1 || true
  cat > "$new_launch" <<LAUNCH
#!/bin/sh
# Starts Gizai: the program inside Gizai.app, so macOS shows it as the app. Not a link to it: macOS tells a program the
# path of the link it was started from, and Gizai looks for gizai-mcp next to that. Gizai reads your login shell's PATH
# itself, so its agents find claude, git, npm and cargo also when it starts from the Dock.
exec "$LIB/Gizai.app/Contents/MacOS/gizai" "\$@"
LAUNCH
  chmod 755 "$new_launch"
  if [ -e "$LIB/Gizai.app" ]; then mv "$LIB/Gizai.app" "$old_app"; fi
  if ! mv "$new_app" "$LIB/Gizai.app"; then
    if [ -e "$old_app" ]; then mv "$old_app" "$LIB/Gizai.app"; fi
    say "Could not put the new Gizai.app into $LIB, so nothing was installed."
    exit 1
  fi
  rm -rf "$old_app"
  mv -f "$new_launch" "$LIB/gizai-launch"
  ln -sfn Gizai.app/Contents/MacOS/gizai "$LIB/gizai"
  ln -sfn Gizai.app/Contents/MacOS/gizai-mcp "$LIB/gizai-mcp"
  ln -sfn "$LIB/gizai-launch" "$BIN/gizai"
  # LaunchServices learns the new app now, so its notifications come from Gizai (not Finder) from the first start.
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$LIB/Gizai.app" >/dev/null 2>&1 || true
  # ~/Applications/Gizai.app for the usual install only: an install into another prefix (a test's) leaves yours alone.
  if [ "$PREFIX" = "$HOME/.local" ]; then
    if [ -L "$APP_LINK" ] || [ ! -e "$APP_LINK" ]; then
      mkdir -p "$HOME/Applications"
      ln -sfn "$LIB/Gizai.app" "$APP_LINK"
    else
      say "Note: $APP_LINK is there already and isn't a link, so it was left as it is. Gizai.app is in $LIB."
    fi
  fi

  version="$("$LIB/gizai" --version 2>/dev/null || echo "gizai")"
  step "Done: $version"
  if [ -L "$APP_LINK" ] && [ "$(readlink "$APP_LINK")" = "$LIB/Gizai.app" ]; then
    say "Start Gizai from Applications in your home folder (Finder: Go > Home), and keep it in the Dock. Or run: gizai"
  else
    say "Start Gizai by opening $LIB/Gizai.app, or run: gizai"
  fi
  case ":$PATH:" in *":$BIN:"*) ;; *) say "Note: $BIN is not on your PATH; add it to use the gizai command." ;; esac
  say "Your data: $DATA"
  say "Update: Gizai offers new releases above Company in its sidebar (or run this installer again). Remove: run it with --uninstall."
  exit 0
fi

# ---------- install ----------
step "Installing"
mkdir -p "$LIB" "$BIN" "$APPS" "$ICONS/scalable/apps"
# Each program goes in under a temporary name of this installer's own first, then replaces the old one in one step (a
# rename): a copy that fails (a full disk) leaves the installed Gizai as it was.
new_gizai="$LIB/.gizai.new.$$" new_mcp="$LIB/.gizai-mcp.new.$$" new_launch="$LIB/.gizai-launch.new.$$"
trap 'rm -f "$new_gizai" "$new_mcp" "$new_launch"' EXIT
if ! install -m 755 "$SRC/target/release/gizai" "$new_gizai" || ! install -m 755 "$SRC/target/release/gizai-mcp" "$new_mcp"; then
  say "Could not copy Gizai into $LIB, so nothing was installed."
  exit 1
fi
cat > "$new_launch" <<LAUNCH
#!/bin/sh
# Starts Gizai with your login shell's PATH, so its agents find claude, git, npm and cargo even when
# Gizai is started from the app launcher. On machines without an NVIDIA GPU, WebKitGTK needs Mesa's EGL.
if ! lspci 2>/dev/null | grep -qi nvidia && [ -f /usr/share/glvnd/egl_vendor.d/50_mesa.json ]; then
  export __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json
fi
if command -v bash >/dev/null 2>&1; then
  exec bash -lc 'exec "\$0" "\$@"' "$LIB/gizai" "\$@"
fi
exec "$LIB/gizai" "\$@"
LAUNCH
chmod 755 "$new_launch"
mv -f "$new_gizai" "$LIB/gizai"
mv -f "$new_mcp" "$LIB/gizai-mcp"
mv -f "$new_launch" "$LIB/gizai-launch"
ln -sfn "$LIB/gizai-launch" "$BIN/gizai"
# The app icon in every size launchers ask for, plus the SVG it is drawn from.
for pair in 32:32x32.png 64:64x64.png 128:128x128.png 256:128x128@2x.png 512:icon.png; do
  size="${pair%%:*}"; file="${pair#*:}"
  mkdir -p "$ICONS/${size}x${size}/apps"
  install -m 644 "$SRC/src-tauri/icons/$file" "$ICONS/${size}x${size}/apps/gizai.png"
done
install -m 644 "$SRC/src-tauri/icons/gizai.svg" "$ICONS/scalable/apps/gizai.svg"
cat > "$APPS/gizai.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Gizai
Comment=Clients, projects and tasks, worked on by local AI agents
Exec=$LIB/gizai-launch
Icon=gizai
Terminal=false
Categories=Development;ProjectManagement;
Keywords=tasks;kanban;agents;claude;projects;clients;
StartupWMClass=gizai
DESKTOP
have update-desktop-database && update-desktop-database "$APPS" >/dev/null 2>&1 || true
# Refresh the icon cache only where one exists already (without one, GTK reads the folders directly).
[ -f "$ICONS/icon-theme.cache" ] && have gtk-update-icon-cache && gtk-update-icon-cache -q -t -f "$ICONS" >/dev/null 2>&1 || true

version="$("$LIB/gizai" --version 2>/dev/null || echo "gizai")"
step "Done: $version"
say "Start Gizai from your app launcher, or run: gizai"
case ":$PATH:" in *":$BIN:"*) ;; *) say "Note: $BIN is not on your PATH; add it to use the gizai command." ;; esac
say "Your data: $DATA"
say "Update: Gizai offers new releases above Company in its sidebar (or run this installer again). Remove: run it with --uninstall."
