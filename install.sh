#!/usr/bin/env bash
# Gizai installer for Linux. Builds Gizai from source and installs it for you alone (no sudo):
#   ~/.local/lib/gizai/     gizai, gizai-mcp (the helper chat uses) and gizai-launch
#   ~/.local/bin/gizai      the command
#   a desktop entry and an icon, so Gizai shows up in your app launcher
# Your data lives in ~/.local/share/gizai and is never touched by install or uninstall (unless --purge).
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
# development), GIZAI_PREFIX (default ~/.local).
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

CHECK=0 BUILD_ONLY=0 SKIP_BUILD=0 UNINSTALL=0 PURGE=0
for arg in "$@"; do
  case "$arg" in
    --check) CHECK=1 ;;
    --build-only) BUILD_ONLY=1 ;;
    --skip-build) SKIP_BUILD=1 ;;
    --uninstall) UNINSTALL=1 ;;
    --purge) PURGE=1 ;;
    -h|--help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
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

if [ "$(uname -s)" != "Linux" ]; then
  say "Gizai installs on Linux for now. macOS and Windows builds come later."
  exit 1
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
for c in git pkg-config cc; do have "$c" || missing_cmds+=("$c"); done
if [ "$SKIP_BUILD" = 0 ]; then
  have cargo || notes+=("Rust is missing: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   (then open a new terminal)")
  if have node; then
    major="$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)"
    [ "$major" -ge 20 ] || notes+=("Node.js 20 or newer is needed (found $(node -v)); install it with your package manager, mise or nvm")
  else
    notes+=("Node.js 20 or newer is missing; install it with your package manager, mise or nvm")
  fi
  have npm || notes+=("npm is missing (it comes with Node.js)")
fi
for lib in webkit2gtk-4.1 gtk+-3.0 librsvg-2.0 openssl; do
  have pkg-config && pkg-config --exists "$lib" 2>/dev/null || missing_libs+=("$lib")
done

distro=""
if [ -r /etc/os-release ]; then
  # shellcheck disable=SC1091
  . /etc/os-release
  distro=" ${ID:-} ${ID_LIKE:-} "
fi
deps_command() {
  case "$distro" in
    *" arch "*|*" manjaro "*|*" endeavouros "*|*" omarchy "*) say "sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg git" ;;
    *" debian "*|*" ubuntu "*) say "sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev pkg-config git" ;;
    *" fedora "*|*" rhel "*) say "sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel git && sudo dnf group install c-development" ;;
    *" opensuse"*|*" suse "*) say "sudo zypper in webkit2gtk3-soup2-devel libopenssl-devel curl wget file libappindicator3-1 librsvg-devel git && sudo zypper in -t pattern devel_basis" ;;
    *) say "Install WebKitGTK 4.1, GTK 3, librsvg and OpenSSL development packages, a C compiler and pkg-config (see https://v2.tauri.app/start/prerequisites/)." ;;
  esac
}

ok=1
if [ "${#missing_cmds[@]}" -gt 0 ] || [ "${#missing_libs[@]}" -gt 0 ]; then
  ok=0
  say "Missing system packages: ${missing_cmds[*]} ${missing_libs[*]}"
  say "Install them with:"
  say "  $(deps_command)"
fi
for n in "${notes[@]}"; do ok=0; say "$n"; done
if have claude; then
  say "Claude Code: found ($(command -v claude)). Make sure you are logged in: run 'claude' once."
else
  say "Claude Code: not found. Gizai's agents and chat run it; install it from https://docs.claude.com/en/docs/claude-code and log in."
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
    npm run tauri build -- --no-bundle
    cargo build --release -p gizai-mcp
  )
fi
for b in gizai gizai-mcp; do
  if [ ! -x "$SRC/target/release/$b" ]; then
    say "$SRC/target/release/$b is missing: build first (run without --skip-build)."
    exit 1
  fi
done
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
