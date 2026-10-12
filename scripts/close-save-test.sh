#!/usr/bin/env bash
# Headless (cage): quitting Gizai with unsaved text in the doc page's or the Memory page's editor (GA-122). Gizai is quit
# the way scripts/quit-test.sh quits it (SIGTERM, which takes the same path as Quit Gizai completely in the tray), and
# the database is read afterwards, as the next start would. The page's probe is closeSaveProbe in src/selftest.ts.
#  1. doc-type: a line typed in a doc and left unsaved; leaving the editor and the window makes no version; quitting
#     saves it as exactly one new version, then Gizai quits (exit 0).
#  2. doc-none: nothing typed: quitting makes no version.
#  3. memory-type: the same as 1 in a memory note on the Memory page.
#  4. memory-secret: an API key typed in a memory note, which Memory refuses to save: quitting asks "Close Gizai without
#     saving your changes to Use SQLite?" with Stay focused; Stay keeps Gizai running with the text and "Unsaved changes";
#     the next quit asks again, and Close quits without a version.
# usage: scripts/close-save-test.sh (build first: npm run tauri build -- --no-bundle)
set -uo pipefail
cd "$(dirname "$0")/.."
GZ=$PWD; DEV=$GZ/.devdata; BIN=$GZ/target/release/gizai; Q=$DEV/closesave
[ -x "$BIN" ] || { echo "CLOSE-SAVE FAIL: no $BIN (build first)"; exit 1; }
rm -rf "$Q"; mkdir -p "$Q" "$Q/claude" "$DEV/run" "$DEV/xdg"; chmod 700 "$DEV/run"
FAKE=$GZ/crates/gizai-agents/tests/fake-claude.sh
( source scripts/env.sh && cargo run -q -p gizai-core --example demo -- "$Q/demo" > /dev/null ) || { echo "CLOSE-SAVE FAIL: no demo data"; exit 1; }
cp -r "$Q/demo" "$Q/doc-type"; cp -r "$Q/demo" "$Q/doc-none"; cp -r "$Q/demo" "$Q/memory"
NOTE=$(source scripts/env.sh && cargo run -q -p gizai-core --example prep_memory -- "$Q/memory" "$FAKE") || { echo "CLOSE-SAVE FAIL: no memory data"; exit 1; }
NOTE=${NOTE% *}
cp -r "$Q/memory" "$Q/memory-type"; cp -r "$Q/memory" "$Q/memory-secret"
DOC=$(python3 -c "import sqlite3,sys; print(sqlite3.connect(sys.argv[1]).execute(\"select id from docs where title='Requirements'\").fetchone()[0])" "$Q/demo/gizai.db")

RUNNER=$Q/runner.sh
cat > "$RUNNER" <<'RUN'
#!/usr/bin/env bash
B=$CS_BIN; Q=$CS_DIR; R=$Q/result.txt; : > "$R"
# start <case> <route> <step>: starts Gizai on its own copy of the data, on that page, with the probe's step.
start() { NAME=$1; REP=$Q/$1.json; rm -f "$REP"
  GIZAI_DATA_DIR=$Q/$1 GIZAI_SELFTEST=$REP GIZAI_ROUTE=$2 GIZAI_SELFTEST_MODE=close-save:$3 "$B" 2> "$Q/$1.err" & APP=$!
}
# report <text> <seconds>: waits until the probe's report has <text>.
report() { for _ in $(seq $(($2 * 10))); do grep -qF "$1" "$REP" 2>/dev/null && return 0; sleep 0.1; done
  echo "$NAME no_report $1" >> "$R"; return 1; }
alive() { kill -0 $APP 2>/dev/null && echo yes || echo no; }
# quit [label] <seconds>: SIGTERM, then waits up to <seconds> for Gizai to end.
quit() { local label=$1 secs=$2 t0 ms code
  t0=$(date +%s%N); kill -TERM $APP
  for _ in $(seq $((secs * 100))); do kill -0 $APP 2>/dev/null || break; sleep 0.01; done
  ms=$(( ($(date +%s%N) - t0) / 1000000 ))
  if kill -0 $APP 2>/dev/null; then echo "$label running_after_${secs}s" >> "$R"; kill -KILL $APP; wait $APP; return; fi
  wait $APP; code=$?
  echo "$label exit=$code ms=$ms" >> "$R"
}

start doc-type "doc/$CS_DOC" type; report '"typed":true' 40 && sleep 0.5; quit doc-type 10
start doc-none "doc/$CS_DOC" none; report '"typed":false' 40 && sleep 0.5; quit doc-none 10
start memory-type "memory/$CS_NOTE" type; report '"typed":true' 40 && sleep 0.5; quit memory-type 10
start memory-secret "memory/$CS_NOTE" secret
if report '"typed":true' 40; then
  sleep 0.5; kill -TERM $APP
  report '"phase":"stayed"' 20; sleep 3
  echo "memory-secret alive_after_stay=$(alive)" >> "$R"
  report '"phase":"stayed"' 1 && quit memory-secret 20
else
  quit memory-secret 5
fi
RUN
chmod +x "$RUNNER"
env -i HOME="$HOME" PATH="$PATH" USER="${USER:-gizai}" LANG="${LANG:-C.UTF-8}" XDG_RUNTIME_DIR="$DEV/run" \
  WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128 \
  XDG_DATA_HOME="$DEV/xdg/data" XDG_CACHE_HOME="$DEV/xdg/cache" XDG_CONFIG_HOME="$DEV/xdg/config" CLAUDE_CONFIG_DIR="$Q/claude" \
  __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
  CS_BIN="$BIN" CS_DIR="$Q" CS_DOC="$DOC" CS_NOTE="$NOTE" \
  timeout -k 5 300 dbus-run-session -- cage -- "$RUNNER" > "$Q/cage.log" 2>&1
[ -s "$Q/result.txt" ] || { echo "CLOSE-SAVE FAIL: no result (see $Q/cage.log)"; exit 1; }
cat "$Q/result.txt"

python3 - "$Q" "$DOC" "$NOTE" <<'PY'
import json, re, sqlite3, sys
q, doc, note = sys.argv[1:4]
LINE, SECRET = "Typed before closing, GA-122.", "sk-a1B2c3D4e5F6g7H8i9J0kLmN"
result = open(f"{q}/result.txt").read()
fails = []
def check(name, ok, got=None):
    print(("ok   " if ok else "FAIL ") + name + ("" if ok or got is None else f": {got}"))
    if not ok: fails.append(name)
def rep(case):
    try: return json.load(open(f"{q}/{case}.json"))
    except Exception as e: return {"error": f"no report ({e})"}
def versions(case, doc_id):
    c = sqlite3.connect(f"{q}/{case}/gizai.db")
    current, body = c.execute("select current_version, (select body_md from doc_versions v where v.doc_id = d.id and v.version = d.current_version) from docs d where id = ?", (doc_id,)).fetchone()
    count = c.execute("select count(*) from doc_versions where doc_id = ?", (doc_id,)).fetchone()[0]
    return current, count, body
def quit_ok(case, ms=10000):
    m = re.search(rf"^{case} exit=(\d+) ms=(\d+)$", result, re.M)
    check(f"{case}: Gizai quit with exit 0", bool(m) and m.group(1) == "0" and int(m.group(2)) < ms, m.group(0) if m else result)

for case, doc_id in [("doc-type", doc), ("memory-type", note)]:
    r = rep(case)
    v0 = r.get("v0")
    check(f"{case}: the probe typed a line, status Unsaved changes", r.get("typed") is True and str(r.get("status_typed", "")).startswith("Unsaved changes"), r)
    check(f"{case}: leaving the editor and the window made no version", r.get("version_after_blur") == v0 and str(r.get("status_after_blur", "")).startswith("Unsaved changes"), r)
    quit_ok(case)
    current, count, body = versions(case, doc_id)
    base, base_count, _ = versions("demo" if case == "doc-type" else "memory", doc_id)
    check(f"{case}: exactly one new version, with the line", current == v0 + 1 and count == base_count + 1 and (body or "").rstrip().endswith(LINE), (v0, current, base_count, count, (body or "")[-80:]))

r = rep("doc-none")
check("doc-none: the probe typed nothing", r.get("typed") is False and r.get("v0") is not None, r)
quit_ok("doc-none", 3000)
current, count, _ = versions("doc-none", doc)
base, base_count, _ = versions("demo", doc)
check("doc-none: no version", current == r.get("v0") == base and count == base_count, (r.get("v0"), current, count, base_count))

r = rep("memory-secret")
current, count, body = versions("memory-secret", note)
base, base_count, _ = versions("memory", note)
check("memory-secret: the probe typed the key, status Unsaved changes", r.get("typed") is True and str(r.get("status_typed", "")).startswith("Unsaved changes"), r)
check("memory-secret: quitting asks first", r.get("question") == "Close Gizai without saving your changes to Use SQLite?", r.get("question"))
check("memory-secret: Stay is focused", r.get("stay_focused") is True, r.get("stay_focused"))
check("memory-secret: Stay closes the question, keeps the text and Unsaved changes", r.get("asked_gone") is True and r.get("text_kept") is True
      and str(r.get("status_stayed", "")).startswith("Unsaved changes") and r.get("version_stayed") == r.get("v0"), r)
check("memory-secret: the save's error shows", "doesn't keep secrets" in str(r.get("error_shown", "")), r.get("error_shown"))
check("memory-secret: Gizai still runs after Stay", "memory-secret alive_after_stay=yes" in result, result)
check("memory-secret: the next quit asks again", r.get("phase") == "asked-again" and r.get("question_again") == r.get("question"), r)
quit_ok("memory-secret", 20000)
check("memory-secret: Close made no version", current == base and count == base_count and SECRET not in (body or ""), (base, current, base_count, count))
print("CLOSE-SAVE OK" if not fails else f"CLOSE-SAVE FAIL ({len(fails)}; see {q})")
sys.exit(1 if fails else 0)
PY
