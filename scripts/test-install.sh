#!/usr/bin/env bash
# Tests install.sh against a scratch HOME (never your real one): --check, an install of the binaries already
# in target/release (which must back up existing data first), the installed command, then --uninstall (which must
# keep the data folder).
set -euo pipefail
cd "$(dirname "$0")/.."
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
H="$T/home"; mkdir -p "$H"
run() { env -i HOME="$H" PATH="$PATH" USER="${USER:-gizai}" bash ./install.sh "$@"; }
fail() { echo "INSTALL TEST FAIL: $*"; exit 1; }
run --check > "$T/check.txt" 2>&1 || true
grep -q "Claude Code:" "$T/check.txt" || fail "--check printed no Claude Code line"
D="$H/.local/share/gizai"; mkdir -p "$D"
python3 -c "import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('create table t(x)'); c.execute('insert into t values (42)'); c.commit()" "$D/gizai.db"
run --skip-build > "$T/install.txt" 2>&1 || { cat "$T/install.txt"; fail "install failed"; }
ls "$D"/backups/gizai-before-install-*.db > /dev/null 2>&1 || { cat "$T/install.txt"; fail "no backup of the existing data before installing"; }
grep -q "Backed up your data" "$T/install.txt" || fail "the installer didn't say it backed up the data"
for f in .local/lib/gizai/gizai .local/lib/gizai/gizai-mcp .local/lib/gizai/gizai-launch .local/share/applications/gizai.desktop \
  .local/share/icons/hicolor/32x32/apps/gizai.png .local/share/icons/hicolor/128x128/apps/gizai.png .local/share/icons/hicolor/256x256/apps/gizai.png \
  .local/share/icons/hicolor/512x512/apps/gizai.png .local/share/icons/hicolor/scalable/apps/gizai.svg; do
  [ -e "$H/$f" ] || fail "missing $f"
done
[ -L "$H/.local/bin/gizai" ] || fail "no gizai command"
grep -q "^Exec=$H/.local/lib/gizai/gizai-launch$" "$H/.local/share/applications/gizai.desktop" || fail "desktop entry Exec"
grep -q "^StartupWMClass=gizai$" "$H/.local/share/applications/gizai.desktop" || fail "desktop entry StartupWMClass (Hyprland matches windows to it)"
if command -v desktop-file-validate >/dev/null; then desktop-file-validate "$H/.local/share/applications/gizai.desktop" || fail "desktop entry is not valid"; fi
v="$(env -i HOME="$H" PATH="$PATH" "$H/.local/bin/gizai" --version)"
[ "$v" = "gizai $(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)" ] || fail "gizai --version said: $v"
grep -q "Done: gizai" "$T/install.txt" || fail "no Done line"
# data that can't be backed up stops the install before anything is replaced
before="$(stat -c %Y.%s "$H/.local/lib/gizai/gizai")"; sleep 1.1
echo "not a database" > "$D/gizai.db"
if run --skip-build > "$T/install2.txt" 2>&1; then fail "installed over data it could not back up"; fi
grep -q "nothing was installed" "$T/install2.txt" || { cat "$T/install2.txt"; fail "no reason given"; }
[ "$(stat -c %Y.%s "$H/.local/lib/gizai/gizai")" = "$before" ] || fail "the binary was replaced after a failed backup"
echo keep > "$D/gizai.db"
run --uninstall > "$T/uninstall.txt" 2>&1 || fail "uninstall failed"
[ ! -e "$H/.local/lib/gizai" ] && [ ! -e "$H/.local/bin/gizai" ] && [ ! -e "$H/.local/share/applications/gizai.desktop" ] || fail "uninstall left files"
[ -z "$(find "$H/.local/share/icons" -name 'gizai.*' 2>/dev/null)" ] || fail "uninstall left icons"
[ -f "$H/.local/share/gizai/gizai.db" ] || fail "uninstall deleted the data"
run --uninstall --purge > /dev/null 2>&1
[ ! -e "$H/.local/share/gizai" ] || fail "--purge kept the data"
echo "INSTALL TEST OK ($v)"
