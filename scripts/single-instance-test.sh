#!/usr/bin/env bash
# Headless (cage): a second Gizai on the same data must not start a second app; the first keeps running.
# A Gizai on other data runs next to it. A Gizai in another session (no shared D-Bus) on the same data folder is
# refused by the data lock.
set -uo pipefail
cd "$(dirname "$0")/.."
GZ=$PWD; DEV=$GZ/.devdata; BIN=$GZ/target/release/gizai
# CLAUDE: an empty Claude Code account folder (CLAUDE_CONFIG_DIR) for the Gizais, so they don't import your own Claude
# Code memory notes (~/.claude) into this test's data when they start (GA-85).
DATA=$DEV/single; OTHER=$DEV/single-other; CLAUDE=$DEV/single-claude; rm -rf "$DATA" "$OTHER" "$CLAUDE"
mkdir -p "$DATA" "$OTHER" "$CLAUDE" "$DEV/run" "$DEV/xdg"; chmod 700 "$DEV/run"
OUT=$DEV/single-result.txt; rm -f "$OUT"
RUNNER=$DEV/single-runner.sh
cat > "$RUNNER" <<RUN
#!/usr/bin/env bash
"$BIN" & A=\$!
sleep 4
start=\$(date +%s%N)
timeout 10 "$BIN"; b=\$?
took=\$(( (\$(date +%s%N) - start) / 1000000 ))
kill -0 \$A 2>/dev/null && alive=yes || alive=no
GIZAI_DATA_DIR="$OTHER" "$BIN" & O=\$!
sleep 4
kill -0 \$O 2>/dev/null && other_data=runs || other_data=exited
kill \$O 2>/dev/null; wait \$O 2>/dev/null
# another session: no D-Bus shared with the first, so only the data lock can stop it
timeout 10 dbus-run-session -- "$BIN" 2> "$DEV/single-other.err"; c=\$?
echo "second_exit=\$b second_ms=\$took first_alive=\$alive other_data=\$other_data other_session_exit=\$c" > "$OUT"
kill \$A 2>/dev/null; wait \$A 2>/dev/null
RUN
chmod +x "$RUNNER"
env -i HOME="$HOME" PATH="$PATH" USER="${USER:-gizai}" LANG="${LANG:-C.UTF-8}" XDG_RUNTIME_DIR="$DEV/run" \
  WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128 \
  GIZAI_DATA_DIR="$DATA" XDG_DATA_HOME="$DEV/xdg/data" XDG_CACHE_HOME="$DEV/xdg/cache" XDG_CONFIG_HOME="$DEV/xdg/config" \
  CLAUDE_CONFIG_DIR="$CLAUDE" \
  __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
  timeout -k 5 60 dbus-run-session -- cage -- "$RUNNER" > "$DEV/single.log" 2>&1
cat "$OUT" 2>/dev/null || { echo "SINGLE FAIL: no result (see .devdata/single.log)"; exit 1; }
grep -q "second_exit=0 " "$OUT" && grep -q "first_alive=yes" "$OUT" && grep -q "other_data=runs" "$OUT" && grep -q "other_session_exit=1" "$OUT" \
  && grep -q "already running" "$DEV/single-other.err" && echo "SINGLE INSTANCE OK" || { echo "SINGLE FAIL"; cat "$DEV/single-other.err"; exit 1; }
