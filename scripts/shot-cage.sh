#!/usr/bin/env bash
# Screenshot one Gizai screen inside a HEADLESS cage (never on Jeffrey's screen).
# SHOT_SIZE=<width>x<height> sets the headless screen's size (needs wlr-randr); the default is 1280x720.
# Makes .devdata/demo when it is missing (and no data dir is given), and builds the app when it is missing or stale.
# usage: [MODE=open:project|steps:<sel>;#/route] [SHOT_WAIT=s] [SHOT_SIZE=WxH] scripts/shot-cage.sh <route e.g. tasks|clients|task/<id>> <out.png> [data-dir]
set -uo pipefail
cd "$(dirname "$0")/.."
GZ=$PWD; ROUTE=${1:-tasks}; OUT=${2:-$GZ/.devdata/shot.png}; DATA=${3:-$GZ/.devdata/demo}
# A brand-new worktree has no demo data and no app yet; an app older than its sources is built again.
source scripts/app-ready.sh
if [ "$DATA" = "$GZ/.devdata/demo" ]; then gz_demo || { echo "could not make the demo data"; exit 1; }; fi
gz_app || { echo "could not build the app"; exit 1; }
DEV=$GZ/.devdata; mkdir -p "$DEV/run" "$DEV/xdg"; chmod 700 "$DEV/run"
RUNNER=$DEV/shot-runner.sh
cat > "$RUNNER" <<RUN
#!/usr/bin/env bash
${SHOT_SIZE:+wlr-randr --output "\$(wlr-randr | head -n1 | cut -d' ' -f1)" --custom-mode $SHOT_SIZE}
"$GZ/target/release/gizai" & APP=\$!
sleep ${SHOT_WAIT:-3.5}
grim "$OUT"
# End the agent runs it started (their own process groups, found by parent PID), then quit the app with SIGTERM:
# it quits the usual way, which ends WebKit's page process first. Not SIGKILL: killed outright, the app leaves
# that process to crash in its own teardown (WebKitWebProcess core dumps).
me=\$(ps -o pgid= -p \$\$ | tr -d ' ')
for k in \$(pgrep -P \$APP); do pg=\$(ps -o pgid= -p \$k | tr -d ' '); [ -n "\$pg" ] && [ "\$pg" != "\$me" ] && kill -- -\$pg 2>/dev/null; done
kill \$APP 2>/dev/null; wait \$APP 2>/dev/null
RUN
chmod +x "$RUNNER"
env -i HOME="$HOME" PATH="$PATH" USER="${USER:-gizai}" LANG="${LANG:-C.UTF-8}" XDG_RUNTIME_DIR="$DEV/run" \
  WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128 \
  GIZAI_DATA_DIR="$DATA" GIZAI_ROUTE="$ROUTE" GIZAI_SELFTEST_MODE="${MODE:-}" \
  XDG_DATA_HOME="$DEV/xdg/data" XDG_CACHE_HOME="$DEV/xdg/cache" XDG_CONFIG_HOME="$DEV/xdg/config" \
  __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
  timeout -k 5 30 dbus-run-session -- cage -- "$RUNNER" > "$DEV/shot.log" 2>&1
[ -s "$OUT" ] && echo "shot: $OUT" || { echo "no screenshot; see .devdata/shot.log"; exit 1; }
