#!/usr/bin/env bash
# Headless (cage): Gizai quits cleanly on a signal, and WebKit's page process (WebKitWebProcess) is gone by the time
# Gizai is, so it can't crash in its own teardown (GA-29). Each case starts Gizai on its own demo data:
#  1-3. SIGTERM, SIGINT and SIGHUP quit with exit 0 and say why on stderr.
#  4-5. A signal ignored when Gizai started stays ignored: SIGHUP under nohup, SIGINT in a background job of a
#     script. SIGTERM still quits.
#  6. With an agent run at work, SIGTERM stops the run first (its process group ends, the run is cancelled: "Stopped
#     because Gizai quit."), then quits.
#  7. A second SIGTERM while a run is still being stopped quits at once, and the run's process group is gone by then
#     (SIGKILL as Gizai exits); the run is recorded as stopped because Gizai quit too.
#  8. Logging out or shutting down: SIGTERM to Gizai and to its agent at once. The run is stopped because Gizai quit
#     (not failed) and no agent process is left.
#  9. A Gizai refused because another one holds the data folder exits 1 and leaves no page process behind.
# None of them may leave a WebKitWebProcess core dump.
# usage: scripts/quit-test.sh (build first: npm run tauri build -- --no-bundle)
set -uo pipefail
cd "$(dirname "$0")/.."
GZ=$PWD; DEV=$GZ/.devdata; BIN=$GZ/target/release/gizai; Q=$DEV/quittest
[ -x "$BIN" ] || { echo "QUIT FAIL: no $BIN (build first)"; exit 1; }
rm -rf "$Q"; mkdir -p "$Q" "$DEV/run" "$DEV/xdg"; chmod 700 "$DEV/run"
FAKE=$GZ/crates/gizai-agents/tests/fake-claude.sh
# Demo data. The run data adds a Backend Agent on a heartbeat with KADE-1 assigned, so a run starts as soon as Gizai
# does: run1 and run3 with the fake Claude Code on FAKE_HANG (it ends on Gizai's SIGINT, or on SIGTERM), run2 with a
# fake that ignores SIGINT, so stopping it takes Gizai's 5 s grace before SIGTERM. Each has its own repo (a run makes a
# branch there).
( source scripts/env.sh && cargo run -q -p gizai-core --example demo -- "$Q/data" > /dev/null ) || { echo "QUIT FAIL: no demo data"; exit 1; }
printf '#!/usr/bin/env bash\ntrap "" INT\nwhile :; do sleep 1; done\n' > "$Q/stubborn-claude.sh"; chmod +x "$Q/stubborn-claude.sh"
run_data() { local dir=$Q/$1 fake=$2
  cp -r "$Q/data" "$dir"
  git init -q -b main "$dir-repo" && git -C "$dir-repo" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
  ( source scripts/env.sh && cargo run -q -p gizai-core --example prep_run -- "$dir" "$dir-repo" "$fake" > /dev/null ) || return 1
  python3 - "$dir/gizai.db" <<'PY'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
agent = c.execute("select id from actors where name='Backend Agent' and kind='agent'").fetchone()[0]
c.execute("update agent_configs set wakeup='heartbeat', heartbeat_minutes=1, last_heartbeat_at=null where actor_id=?", (agent,))
c.execute("update tasks set assignee_actor_id=? where identifier='KADE-1'", (agent,))
c.commit()
PY
}
run_data run1 "$FAKE" && run_data run2 "$Q/stubborn-claude.sh" && run_data run3 "$FAKE" || { echo "QUIT FAIL: no run data"; exit 1; }

RUNNER=$Q/runner.sh
cat > "$RUNNER" <<'RUN'
#!/usr/bin/env bash
# Job control on, so a background Gizai gets SIGINT like one started from a terminal (case 4 turns it off).
set -m
B=$QT_BIN; Q=$QT_DIR; R=$Q/result.txt; : > "$R"
web_of() { for k in $(pgrep -P "$1"); do [ "$(cat /proc/$k/comm 2>/dev/null)" = WebKitWebProces ] && echo $k; done | head -n1; }
# A child of Gizai that leads its own process group: an agent run.
run_of() { for k in $(pgrep -P "$1"); do [ "$(ps -o pgid= -p $k | tr -d ' ')" = "$k" ] && echo $k; done | head -n1; }
gone() { local s; s=$(awk '/^State/{print $2}' /proc/$1/status 2>/dev/null); [ -z "$s" ] || [ "$s" = Z ]; }
# start <case> <data dir> [command before gizai, e.g. nohup]: starts Gizai and waits for its page process.
start() { local name=$1 data=$2; shift 2
  ERR=$Q/$name.err
  GIZAI_DATA_DIR=$data "$@" "$B" 2> "$ERR" & APP=$!
  WEB=; for _ in $(seq 100); do WEB=$(web_of $APP); [ -n "$WEB" ] && break; sleep 0.1; done
  echo "$WEB" >> "$Q/web-pids"
  sleep 3
}
# quit <case> <signal> <seconds> [process group]: sends the signal (also to the process group, right after Gizai) and
# waits up to <seconds> for Gizai to end. The page process is checked the moment Gizai is gone.
quit() { local name=$1 sig=$2 secs=$3 group=${4:-} t0 ms code web
  t0=$(date +%s%N); kill -$sig $APP; [ -n "$group" ] && kill -$sig -- -$group
  for _ in $(seq $((secs * 100))); do kill -0 $APP 2>/dev/null || break; sleep 0.01; done
  ms=$(( ($(date +%s%N) - t0) / 1000000 ))
  if kill -0 $APP 2>/dev/null; then
    echo "$name running_after_${secs}s" >> "$R"; kill -TERM $APP; wait $APP; return
  fi
  wait $APP; code=$?
  gone "$WEB" && web=gone || web=left
  echo "$name exit=$code ms=$ms web=$web web_pid=${WEB:-none} said=$(grep -c "gizai: quitting on $sig" "$ERR")" >> "$R"
}
# still <case> <signal>: sends a signal Gizai should ignore; it must still run 2 s later.
still() { kill -$2 $APP; sleep 2; kill -0 $APP 2>/dev/null && echo "$1 alive=yes" >> "$R" || echo "$1 alive=no" >> "$R"; }

start term "$Q/data"; quit term SIGTERM 10
start int "$Q/data"; quit int SIGINT 10
start hup "$Q/data"; quit hup SIGHUP 10

start nohup "$Q/data" nohup; still nohup-hup SIGHUP; quit nohup-term SIGTERM 10
set +m; start bg "$Q/data"; set -m; still bg-int SIGINT; quit bg-term SIGTERM 10

start run "$Q/run1"
RUNPG=; for _ in $(seq 200); do RUNPG=$(run_of $APP); [ -n "$RUNPG" ] && break; sleep 0.1; done
echo "run-started group=${RUNPG:-none}" >> "$R"
quit run SIGTERM 25
[ -n "$RUNPG" ] && { left=$(pgrep -g $RUNPG | wc -l); echo "run-group left=$left" >> "$R"; [ "$left" = 0 ] || kill -KILL -- -$RUNPG; }

start twice "$Q/run2"
RUNPG=; for _ in $(seq 200); do RUNPG=$(run_of $APP); [ -n "$RUNPG" ] && break; sleep 0.1; done
echo "twice-started group=${RUNPG:-none}" >> "$R"
kill -TERM $APP; sleep 1
kill -0 $APP 2>/dev/null && echo "twice-first alive=yes" >> "$R" || echo "twice-first alive=no" >> "$R"
quit twice SIGTERM 3
# Gizai quit without waiting for the run's grace, but ended its group as it exited.
[ -n "$RUNPG" ] && { left=$(pgrep -g $RUNPG | wc -l); echo "twice-group left=$left" >> "$R"; [ "$left" = 0 ] || kill -KILL -- -$RUNPG; }

# Logging out (after the window has closed) or shutting down: systemd sends SIGTERM to Gizai and its agents at once.
start logout "$Q/run3"
RUNPG=; for _ in $(seq 200); do RUNPG=$(run_of $APP); [ -n "$RUNPG" ] && break; sleep 0.1; done
echo "logout-started group=${RUNPG:-none}" >> "$R"
quit logout SIGTERM 25 "$RUNPG"
[ -n "$RUNPG" ] && { left=$(pgrep -g $RUNPG | wc -l); echo "logout-group left=$left" >> "$R"; [ "$left" = 0 ] || kill -KILL -- -$RUNPG; }

# Another session (no D-Bus shared with the first Gizai) on the same data folder: only the data lock stops it.
start held-first "$Q/data"
dbus-run-session -- env GIZAI_DATA_DIR="$Q/data" "$B" 2> "$Q/held.err" & DS=$!
HB=; HW=
for _ in $(seq 1000); do
  [ -z "$HB" ] && HB=$(pgrep -P $DS -x gizai)
  [ -n "$HB" ] && [ -z "$HW" ] && HW=$(web_of $HB)
  kill -0 $DS 2>/dev/null || break; sleep 0.01
done
wait $DS; code=$?
gone "$HW" && web=gone || web=left
echo "$HW" >> "$Q/web-pids"
echo "held exit=$code web=$web web_pid=${HW:-none} said=$(grep -c 'already running' "$Q/held.err")" >> "$R"
quit held-first SIGTERM 10
RUN
chmod +x "$RUNNER"
T0=$(date +%s)
env -i HOME="$HOME" PATH="$PATH" USER="${USER:-gizai}" LANG="${LANG:-C.UTF-8}" XDG_RUNTIME_DIR="$DEV/run" \
  WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128 \
  XDG_DATA_HOME="$DEV/xdg/data" XDG_CACHE_HOME="$DEV/xdg/cache" XDG_CONFIG_HOME="$DEV/xdg/config" \
  __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json QT_BIN="$BIN" QT_DIR="$Q" \
  timeout -k 5 180 dbus-run-session -- cage -- "$RUNNER" > "$Q/cage.log" 2>&1
R=$Q/result.txt
[ -s "$R" ] || { echo "QUIT FAIL: no result (see $Q/cage.log)"; exit 1; }
cat "$R"
# The runs Gizai stopped are recorded as cancelled, stopped because Gizai quit: "status/error", one per run.
runs_of() { python3 -c "import sqlite3,sys; print('; '.join(f'{s}/{e}' for s, e in sqlite3.connect(sys.argv[1]).execute('select status, error from runs')))" "$Q/$1/gizai.db"; }
RUN1=$(runs_of run1); RUN2=$(runs_of run2); RUN3=$(runs_of run3)
echo "run1-runs $RUN1"; echo "run2-runs $RUN2"; echo "run3-runs $RUN3"
# Core dumps of this test's page processes (give systemd-coredump a moment to write one).
sleep 3
DUMPS=$(coredumpctl list --since=@$T0 --no-pager --no-legend 2>/dev/null | grep WebKitWebProcess | awk '{print $5}')
OURS=$(for p in $DUMPS; do grep -qx "$p" "$Q/web-pids" && echo $p; done)
echo "core-dumps ${OURS:-none}"

fail=0
ok() { grep -qE "$1" "$R" || { echo "FAIL: expected '$1'"; fail=1; }; }
for c in term:SIGTERM int:SIGINT hup:SIGHUP nohup-term:SIGTERM bg-term:SIGTERM; do
  ok "^${c%%:*} exit=0 ms=[0-9]+ web=gone web_pid=[0-9]+ said=1$"
done
ok "^nohup-hup alive=yes$"; ok "^bg-int alive=yes$"
ok "^run-started group=[0-9]+$"; ok "^run exit=0 ms=[0-9]+ web=gone web_pid=[0-9]+ said=1$"; ok "^run-group left=0$"
QUIT_RUN="cancelled/Stopped because Gizai quit."
for r in run1:"$RUN1" run2:"$RUN2" run3:"$RUN3"; do
  [ "${r#*:}" = "$QUIT_RUN" ] || { echo "FAIL: ${r%%:*} is not '$QUIT_RUN' (${r#*:})"; fail=1; }
done
ok "^twice-started group=[0-9]+$"; ok "^twice-first alive=yes$"
# The second SIGTERM quits within 3 s instead of waiting out the run's 5 s grace, and the run's group is gone by then.
ok "^twice exit=0 ms=[0-9]+ web=gone web_pid=[0-9]+ said=2$"; ok "^twice-group left=0$"
ok "^logout-started group=[0-9]+$"; ok "^logout exit=0 ms=[0-9]+ web=gone web_pid=[0-9]+ said=1$"; ok "^logout-group left=0$"
ok "^held exit=1 web=gone web_pid=[0-9a-z]+ said=1$"; ok "^held-first exit=0 ms=[0-9]+ web=gone"
[ -z "$OURS" ] || { echo "FAIL: WebKitWebProcess core dumps: $OURS"; fail=1; }
[ $fail = 0 ] && echo "QUIT OK" || echo "QUIT FAIL (see $Q)"
exit $fail
