#!/usr/bin/env bash
# Gizai's own update, end to end in a HEADLESS cage (never on Jeffrey's screen), without GitHub and without touching
# the Gizai he uses (docs/RELEASING.md → Testing an update). Everything lives in one scratch folder:
#   - a scratch install of this checkout's build (install.sh --skip-build with HOME in the scratch folder);
#   - a fake release v9.9.9: a local repository whose install.sh is a quick stub, and a latest.json shaped like GitHub's
#     answer;
#   - that installed Gizai, started with HOME, XDG_DATA_HOME and its data in the scratch folder.
# Run 1 makes the stub's build fail: the notice must say the update failed, Settings → Updates must say why, and the
# installed Gizai must stay as it was. Run 2: the update must install 9.9.9, back up the data and offer the restart.
# Neither presses Check now: in run 1 the release check that runs 20 seconds after start must find the release by
# itself; run 2 starts with what that check found (it is kept, and the next check is due six hours later).
# Screenshots go to .devdata/update-test/. Builds the app when it is missing or stale.
# usage: scripts/update-test.sh
set -uo pipefail
cd "$(dirname "$0")/.."
GZ=$PWD
source scripts/app-ready.sh
gz_app || { echo "could not build the app"; exit 1; }
(source scripts/env.sh && cargo build --release -q -p gizai-mcp) || { echo "could not build gizai-mcp"; exit 1; }
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
OUT=$GZ/.devdata/update-test; rm -rf "$OUT"; mkdir -p "$OUT"
fail() { echo "UPDATE TEST FAIL: $*"; exit 1; }
H=$T/home; P=$H/.local; mkdir -p "$H" "$T/control" "$T/xdg" "$T/run"; chmod 700 "$T/run"
current="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"

# the scratch install of this build
env -i HOME="$H" PATH="$PATH" USER="${USER:-gizai}" bash ./install.sh --skip-build > "$T/install.txt" 2>&1 || { cat "$T/install.txt"; fail "scratch install"; }
[ "$("$P/lib/gizai/gizai" --version)" = "gizai $current" ] || fail "the scratch install isn't $current"

# the fake release v9.9.9
R=$T/fake-repo; mkdir -p "$R"
git -C "$R" init -q -b production
printf '[workspace]\nmembers = []\n\n[workspace.package]\nversion = "9.9.9"\n' > "$R/Cargo.toml"
printf 'target/\n' > "$R/.gitignore"
cat > "$R/install.sh" <<STUB
set -e
case "\$1" in
  --build-only)
    echo "Building the fake 9.9.9"; sleep 4
    if [ -f "$T/control/fail-build" ]; then echo "error[E0425]: cannot find value (a fake build failure)" >&2; exit 101; fi
    mkdir -p target/release
    printf '#!/bin/sh\n[ "\$1" = --version ] && echo "gizai 9.9.9"\nexit 0\n' > target/release/gizai
    printf '#!/bin/sh\nexit 0\n' > target/release/gizai-mcp
    chmod 755 target/release/gizai target/release/gizai-mcp ;;
  --skip-build)
    L="\$GIZAI_PREFIX/lib/gizai"; mkdir -p "\$L"
    for b in gizai gizai-mcp; do cp "target/release/\$b" "\$L/.\$b.new.\$\$" && mv -f "\$L/.\$b.new.\$\$" "\$L/\$b"; done
    echo "Done: gizai 9.9.9" ;;
esac
STUB
git -C "$R" add -A && git -C "$R" -c user.email=t@t -c user.name=t commit -q -m "release 9.9.9" && git -C "$R" tag v9.9.9
cat > "$T/latest.json" <<'JSON'
{"tag_name": "v9.9.9", "name": "v9.9.9", "html_url": "https://github.com/Yotech-AI/gizai/releases/tag/v9.9.9", "published_at": "2026-10-07T12:00:00Z", "draft": false, "prerelease": false, "body": "- A fake release for the update test"}
JSON

# One headless Gizai session: Settings, a wait for the release check (20 s after start), then a click on the notice.
# Screenshots: the notice (offered), during the update, and at the end.
session() {
  local name=$1 steps="#/settings"
  for _ in $(seq 20); do steps="$steps;#/settings"; done
  steps="$steps;.update-notice"
  cat > "$T/runner.sh" <<RUN
#!/usr/bin/env bash
"$P/lib/gizai/gizai" & APP=\$!
sleep 23; grim "$OUT/$name-1-offered.png"
sleep 6; grim "$OUT/$name-2-updating.png"
sleep 13; grim "$OUT/$name-3-end.png"
kill \$APP 2>/dev/null; wait \$APP 2>/dev/null
RUN
  chmod +x "$T/runner.sh"
  env -i HOME="$H" PATH="$PATH" USER="${USER:-gizai}" LANG="${LANG:-C.UTF-8}" XDG_RUNTIME_DIR="$T/run" \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128 \
    XDG_DATA_HOME="$P/share" XDG_CACHE_HOME="$T/xdg/cache" XDG_CONFIG_HOME="$T/xdg/config" \
    GIZAI_DATA_DIR="$T/data" GIZAI_REPO="$R" GIZAI_RELEASES_URL="file://$T/latest.json" GIZAI_SELFTEST_MODE="steps:$steps" \
    __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
    timeout -k 5 75 dbus-run-session -- cage -- "$T/runner.sh" > "$OUT/$name.log" 2>&1
  for s in 1-offered 2-updating 3-end; do [ -s "$OUT/$name-$s.png" ] || fail "$name: no screenshot $s (see $OUT/$name.log)"; done
}

# run 1: the build fails
touch "$T/control/fail-build"
session failed
[ "$("$P/lib/gizai/gizai" --version)" = "gizai $current" ] || fail "a failed build changed the installed Gizai"
grep -q "fake build failure" "$T/data/update/update.log" || fail "the log doesn't have what the build said"
ls "$T/data/backups"/gizai-before-update-*.db > /dev/null 2>&1 && fail "a failed build still backed up and went on"
cp "$T/data/update/update.log" "$OUT/failed-update.log"

# run 2: the update installs 9.9.9 (the stub replaces the scratch install's programs)
rm -f "$T/control/fail-build"
session installed
[ "$("$P/lib/gizai/gizai" --version)" = "gizai 9.9.9" ] || fail "9.9.9 isn't installed"
ls "$T/data/backups"/gizai-before-update-*.db > /dev/null 2>&1 || fail "no backup of the data before the install"
grep -q "== Installed 9.9.9" "$T/data/update/update.log" || fail "the log doesn't say it installed 9.9.9"
cp "$T/data/update/update.log" "$OUT/installed-update.log"
echo "UPDATE TEST OK (screenshots in $OUT)"
