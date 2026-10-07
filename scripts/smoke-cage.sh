#!/usr/bin/env bash
# Headless smoke test. Starts Gizai inside a HEADLESS cage (never on Jeffrey's screen),
# with its own runtime dir and data dir, and waits for the UI's self-test report.
# Env: DATA=<data dir> (default .devdata/data), ROUTE=<start route> (ROUTE=board also runs the drag probe),
# MODE=<extra self-test> (MODE=run drives an agent run on a task route).
set -uo pipefail
cd "$(dirname "$0")/.."
GZ=$PWD
BIN=${1:-$GZ/target/release/gizai}
DEV=$GZ/.devdata
DATA=${DATA:-$DEV/data}; ROUTE=${ROUTE:-}; MODE=${MODE:-}
mkdir -p "$DEV/run" "$DATA" "$DEV/xdg"; chmod 700 "$DEV/run"
REPORT=$DEV/selftest.json; rm -f "$REPORT"
env -i HOME="$HOME" PATH="$PATH" USER="${USER:-gizai}" LANG="${LANG:-C.UTF-8}" \
  XDG_RUNTIME_DIR="$DEV/run" \
  WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128 \
  GIZAI_DATA_DIR="$DATA" GIZAI_SELFTEST="$REPORT" GIZAI_ROUTE="$ROUTE" GIZAI_SELFTEST_MODE="$MODE" \
  XDG_DATA_HOME="$DEV/xdg/data" XDG_CACHE_HOME="$DEV/xdg/cache" XDG_CONFIG_HOME="$DEV/xdg/config" \
  __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
  timeout -k 5 40 dbus-run-session -- cage -- "$BIN" > "$DEV/smoke.log" 2>&1
python3 - "$REPORT" <<'PY'
import json, sys
try:
    r = json.load(open(sys.argv[1]))
except Exception as e:
    print("SMOKE FAIL: no report (", e, ") — see .devdata/smoke.log"); sys.exit(1)
if r.get("ready") and not r.get("errors"):
    print("SMOKE OK", json.dumps({k: v for k, v in r.items() if k != "errors"}))
else:
    print("SMOKE FAIL", json.dumps(r)); sys.exit(1)
PY
