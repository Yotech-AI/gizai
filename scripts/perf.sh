#!/usr/bin/env bash
# The benchmark harness (GA-188; docs/research/performance.md §7). Runs Gizai's perf scenarios in a HEADLESS cage (never on
# Jeffrey's screen), each RUNS times (default 5) on a fresh copy of prep_perf's data, as scripts/smoke-cage.sh starts Gizai,
# with GIZAI_SELFTEST_MODE=perf:<scenario>: the self-test's perf probe measures the scenario (src/selftest.ts, perfProbe)
# and the counter (src/lib/perf.ts) counts every backend call and event from the first one. Meanwhile, every 500 ms, the
# runner reads the memory (VmRSS) and CPU time of the app and of every process under it (WebKit's web and network
# processes, agent runs) from /proc. Then it prints the medians and writes .devdata/perf/<commit>.json, with the bundle's
# sizes (scripts/bundle-size.mjs) next to them.
# The first run of a scenario starts with WebKit's cache and storage empty (a new XDG folder): that one counts as cold, the
# other runs as warm, and the medians are the warm runs'. "Cold" leaves the system's file cache warm: a start straight from
# disk, as after a reboot, needs root to empty that cache, so take it by hand with RUNS=1 after a reboot.
# Every performance card runs it on main and on its branch, on the same machine with nothing else running, and puts both
# tables in its pull request (--compare). Its numbers are for comparing before and after: the cage draws in a 1280 × 720
# headless output, not on your screen.
# Scenarios: start board chat memory runlog idle editor lead (the default: all eight; see perfProbe for what each measures).
# Builds the app when it is missing or stale (scripts/app-ready.sh) and gizai-mcp next to it (the chat's fake Team Lead
# calls Gizai's tools), and makes the data once in .devdata/perf/base (prep_perf, with a git repository for KADE's run and
# one of 20,000 files for the Editor); again when prep_perf.rs is newer than it, or after rm -rf .devdata/perf/base.
# Env: RUNS=<n> runs per scenario (default 5); WEBKIT_ENV='NAME=value …' extra environment for the app, e.g. to compare
# WebKitGTK's drawing modes (WEBKIT_ENV='WEBKIT_DISABLE_COMPOSITING_MODE=1'); its result file gets a -webkit suffix.
# usage: scripts/perf.sh [scenario …]
#        scripts/perf.sh --compare a.json b.json   two results side by side, with the change in %
set -uo pipefail
cd "$(dirname "$0")/.."
GZ=$PWD
if [ "${1:-}" = "--compare" ]; then
  [ $# -eq 3 ] || { echo "usage: scripts/perf.sh --compare a.json b.json"; exit 2; }
  exec node scripts/perf-report.mjs --compare "$2" "$3"
fi
ALL="start board chat memory runlog idle editor lead"
SCENARIOS=${*:-$ALL}
for s in $SCENARIOS; do
  case " $ALL " in *" $s "*) ;; *) echo "unknown scenario: $s (the scenarios: $ALL)"; exit 2 ;; esac
done
RUNS=${RUNS:-5}
source scripts/app-ready.sh
gz_app || { echo "could not build the app"; exit 1; }
(source scripts/env.sh && cargo build --release -q -p gizai-mcp) || { echo "could not build gizai-mcp"; exit 1; }

PERF=$GZ/.devdata/perf
BASE=$PERF/base; IDS=$PERF/base.ids; REPO=$PERF/repo; EDITOR=$PERF/editor
FAKE=$GZ/crates/gizai-agents/tests/fake-claude.sh
mkdir -p "$PERF"
if [ ! -s "$IDS" ] || [ ! -f "$BASE/gizai.db" ] || [ crates/gizai-core/examples/prep_perf.rs -nt "$IDS" ]; then
  echo "making the benchmark data in $BASE (prep_perf)"
  rm -rf "$BASE" "$IDS" "$REPO" && mkdir -p "$BASE"
  (source scripts/env.sh && cargo run --release -q -p gizai-core --example prep_perf -- "$BASE" "$FAKE" --repo "$REPO" --editor "$EDITOR") > "$IDS.new" \
    && [ -s "$IDS.new" ] && mv "$IDS.new" "$IDS" || { echo "could not make the benchmark data"; exit 1; }
fi
IDLINE=$(tail -n1 "$IDS")

COMMIT=$(git rev-parse --short HEAD)
[ -n "$(git status --porcelain -- src src-tauri crates index.html package.json vite.config.ts)" ] && COMMIT=$COMMIT-dirty
[ -n "${WEBKIT_ENV:-}" ] && COMMIT=$COMMIT-webkit
OUT=$PERF/runs/$COMMIT
rm -rf "$OUT" && mkdir -p "$OUT"
DATA=$PERF/run; XDG=$PERF/xdg; RT=$PERF/rt
mkdir -p "$RT" && chmod 700 "$RT"

# The runner, inside the cage: notes the launch (ms since 1970), starts Gizai, and every 500 ms writes one line per process
# of the app and everything under it, however deep: "<ms> <pid> <name> <VmRSS KiB> <utime + stime in clock ticks>".
RUNNER=$PERF/runner.sh
cat > "$RUNNER" <<'RUN'
#!/usr/bin/env bash
dir=$1; bin=$2
tree() { echo "$1"; local c; for c in $(pgrep -P "$1"); do tree "$c"; done; }
date +%s%3N > "$dir/launch_ms"
"$bin" & app=$!
while kill -0 "$app" 2>/dev/null; do
  now=$(date +%s%3N)
  for p in $(tree "$app"); do
    stat=$(cat "/proc/$p/stat" 2>/dev/null) || continue
    set -- ${stat##*) }   # the fields after the name: $12 is utime, $13 stime
    rss=$(grep -m1 '^VmRSS:' "/proc/$p/status" 2>/dev/null | tr -s ' ' | cut -d' ' -f2)
    name=$(tr ' ' '_' < "/proc/$p/comm" 2>/dev/null)
    echo "$now $p ${name:-?} ${rss:-0} $(( ${12} + ${13} ))"
  done >> "$dir/samples.txt"
  sleep 0.5
done
wait "$app"; echo $? > "$dir/exit_code"
RUN
chmod +x "$RUNNER"

# How long a scenario may take (s), and the page it starts on: the board for its own scenarios, People (a light page) for
# the ones that open their page themselves, so the start's work is done before they measure.
limit() { case $1 in start) echo 40 ;; board|idle) echo 120 ;; chat|memory) echo 120 ;; *) echo 200 ;; esac; }
route() { case $1 in start) echo "" ;; board|idle) echo board ;; *) echo users ;; esac; }

for s in $SCENARIOS; do
  for n in $(seq "$RUNS"); do
    dir=$OUT/$s-$n; mkdir -p "$dir"
    rm -rf "$DATA" && cp -a "$BASE" "$DATA"
    # the first run of each scenario: WebKit's cache and storage empty (cold)
    [ "$n" = 1 ] && rm -rf "$XDG"
    mkdir -p "$XDG"
    # KADE-1's run makes a worktree and a branch in KADE's repository: a new one for every run
    if [ "$s" = runlog ]; then
      rm -rf "$REPO" && git init -q -b main "$REPO" && git -C "$REPO" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
    fi
    printf '%s run %s of %s … ' "$s" "$n" "$RUNS"
    # shellcheck disable=SC2086 # WEBKIT_ENV is a list of NAME=value words
    env -i HOME="$HOME" PATH="$PATH" USER="${USER:-gizai}" LANG="${LANG:-C.UTF-8}" XDG_RUNTIME_DIR="$RT" \
      WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128 \
      GIZAI_DATA_DIR="$DATA" GIZAI_SELFTEST="$dir/report.json" GIZAI_ROUTE="$(route "$s")" GIZAI_SELFTEST_MODE="perf:$s $IDLINE" \
      GIZAI_SKIP_ONBOARDING=1 \
      XDG_DATA_HOME="$XDG/data" XDG_CACHE_HOME="$XDG/cache" XDG_CONFIG_HOME="$XDG/config" \
      __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json ${WEBKIT_ENV:-} \
      timeout -k 5 "$(limit "$s")" dbus-run-session -- cage -- "$RUNNER" "$dir" "$GZ/target/release/gizai" > "$dir/cage.log" 2>&1
    if [ -s "$dir/report.json" ]; then echo done; else echo "no report (see $dir/cage.log)"; fi
  done
done
rm -rf "$DATA"

BUNDLE=$PERF/bundle.json
node scripts/bundle-size.mjs --json > "$BUNDLE" 2>/dev/null && [ -s "$BUNDLE" ] || { rm -f "$BUNDLE"; BUNDLE=; }
node scripts/perf-report.mjs --collect "$OUT" "$PERF/$COMMIT.json" --commit "$COMMIT" --runs "$RUNS" --clk-tck "$(getconf CLK_TCK)" \
  --webkit-env "${WEBKIT_ENV:-}" ${BUNDLE:+--bundle "$BUNDLE"}
