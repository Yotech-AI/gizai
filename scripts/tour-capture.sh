#!/usr/bin/env bash
# The product tour's stills (GA-148), for the video (GA-149): Gizai's eight features in the website's order, scene by
# scene, in dark and in light, each scene in a HEADLESS cage (never on your screen) on a fresh copy of fake data
# (.devdata/tour), made up from start to end (crates/gizai-core/examples/prep_tour.rs). The screen is a
# 13-inch MacBook Air's: 2560x1664 pixels at scale 2 (1280x832 CSS pixels), without the window's title bar.
# Each scene opens a page and clicks one element per step; every step has a still before its click and one after, and
# video/capture/<theme>/scenes.json lists the scenes in order with their steps, stills and the box of what each step
# clicked (x, y, width and height in the stills' pixels). Nothing reads your ~/.claude or your Gizai's data: Gizai runs
# in a fake home in .devdata/tour, with its data, cache and config in it, so every path it shows reads ~/…; its projects'
# folders are in the fake ~/Projects, its only Editor folder; and Claude Code and Codex are fakes in its ~/.local/bin
# (scripts/tour/fake-claude.sh and fake-codex.sh).
# Needs the release build (made when it is missing or stale), cage, grim, wlr-randr and ffmpeg.
# usage: scripts/tour-capture.sh [dark|light|both] [scene number…]
#   scripts/tour-capture.sh              every scene, in both themes
#   scripts/tour-capture.sh dark 3 4     scenes 3 and 4 in dark (scenes.json lists every scene captured so far)
set -uo pipefail
cd "$(dirname "$0")/.."
GZ=$PWD
for tool in cage grim wlr-randr ffmpeg dbus-run-session python3 git; do
  command -v "$tool" > /dev/null || { echo "tour-capture needs $tool"; exit 1; }
done
THEMES=${1:-both}; shift || true
case "$THEMES" in both) THEMES="dark light" ;; dark|light) ;; *) echo "usage: scripts/tour-capture.sh [dark|light|both] [scene number…]"; exit 1 ;; esac
ONLY=" $* "

source scripts/app-ready.sh
gz_app || { echo "could not build the app"; exit 1; }

# The screen: 1280x832 CSS pixels of page under the window's title bar (37 px), at scale 2.
SCALE=2; PAGE_W=1280; PAGE_H=832; TITLE=37
MODE_SIZE="$((PAGE_W * SCALE))x$(((PAGE_H + TITLE) * SCALE))"
T=$GZ/.devdata/tour; BASE=$T/base; LIVE=$T/live; OUT=$GZ/video/capture
# The fake home, and Gizai's data folder in it. Claude Code and Codex are the tour's fakes, as `claude` and `codex` in
# its ~/.local/bin; the cage's PATH has that folder and the system's, none of your own, so no real CLI can start.
FAKE_HOME=$LIVE/home; DATA=$FAKE_HOME/.local/share/gizai
PATH_IN_CAGE=$FAKE_HOME/.local/bin:/usr/local/bin:/usr/bin:/bin

# --- the fake data, once per capture: made in $LIVE (its paths are the ones every scene uses), then kept as $BASE ---
prepare() {
  echo "making the fake data in .devdata/tour"
  # the projects' repositories go in the fake home's ~/Projects, so the Editor and the terminals show ~/Projects/…
  rm -rf "$T" && mkdir -p "$FAKE_HOME/.local/bin" "$FAKE_HOME/.local/share/claude" "$FAKE_HOME/.cache" "$FAKE_HOME/.config"
  cp crates/gizai-agents/tests/fixtures/models-init.jsonl "$FAKE_HOME/.local/share/claude/"
  cp scripts/tour/fake-claude.sh "$FAKE_HOME/.local/bin/claude" && chmod +x "$FAKE_HOME/.local/bin/claude"
  cp scripts/tour/fake-codex.sh "$FAKE_HOME/.local/bin/codex" && chmod +x "$FAKE_HOME/.local/bin/codex"
  (source scripts/env.sh &&
    cargo run -q -p gizai-core --example prep_tour -- "$DATA" "$FAKE_HOME" > "$T/ids.sh") || return 1
  # the terminals' shell in the fake home: a short prompt, and git without a pager
  printf '%s\n' "PS1='\\[\\e[1;34m\\]\\w\\[\\e[0m\\] \\$ '" 'export GIT_PAGER=cat' > "$FAKE_HOME/.bashrc"
  printf '%s\n' '[ -f ~/.bashrc ] && . ~/.bashrc' > "$FAKE_HOME/.bash_profile"
  cp -a "$LIVE" "$BASE"
}
prepare || { echo "could not make the fake data"; exit 1; }
# the ids the scenes open (NWS1=…, THREAD=…): prep_tour prints them as shell assignments
source "$T/ids.sh"

# --- one scene in one theme: a fresh copy of the data, Gizai in the cage, the steps, the stills cropped ---
DEV=$T/run; mkdir -p "$DEV" "$T/logs"; chmod 700 "$DEV"
capture() {  # <theme> <scene folder name> <start route> <steps…>
  local theme=$1 name=$2 route=$3; shift 3
  local dir=$OUT/$theme/$name steps
  steps="theme:$theme;$(IFS=';'; echo "$*")"
  rm -rf "$LIVE" && cp -a "$BASE" "$LIVE"
  rm -rf "$dir" && mkdir -p "$dir"
  local runner=$T/runner.sh
  cat > "$runner" <<RUN
#!/usr/bin/env bash
wlr-randr --output "\$(wlr-randr | head -n1 | cut -d' ' -f1)" --custom-mode $MODE_SIZE --scale $SCALE
"$GZ/target/release/gizai" & APP=\$!
# the steps write steps.json when they are done
for _ in \$(seq 600); do [ -s "$dir/steps.json" ] && break; kill -0 \$APP 2>/dev/null || break; sleep 0.2; done
# End the agent runs it started (their own process groups, found by parent PID), then quit the app with SIGTERM, as
# scripts/shot-cage.sh does.
me=\$(ps -o pgid= -p \$\$ | tr -d ' ')
for k in \$(pgrep -P \$APP); do pg=\$(ps -o pgid= -p \$k | tr -d ' '); [ -n "\$pg" ] && [ "\$pg" != "\$me" ] && kill -- -\$pg 2>/dev/null; done
kill \$APP 2>/dev/null; wait \$APP 2>/dev/null
RUN
  chmod +x "$runner"
  env -i HOME="$FAKE_HOME" SHELL=/bin/bash PATH="$PATH_IN_CAGE" USER=alex LANG=en_US.UTF-8 XDG_RUNTIME_DIR="$DEV" \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128 \
    GIZAI_DATA_DIR="$DATA" GIZAI_ROUTE="$route" GIZAI_SELFTEST_MODE="steps:$steps" GIZAI_TOUR_DIR="$dir" \
    XDG_DATA_HOME="$FAKE_HOME/.local/share" XDG_CACHE_HOME="$FAKE_HOME/.cache" XDG_CONFIG_HOME="$FAKE_HOME/.config" \
    __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
    timeout -k 5 150 dbus-run-session -- cage -- "$runner" > "$T/logs/$theme-$name.log" 2>&1
  [ -s "$dir/steps.json" ] || { echo "  $theme $name: no steps.json (see .devdata/tour/logs/$theme-$name.log)"; return 1; }
  # without the window's title bar: the page is the screen's lowest 2560x1664 pixels
  local png ok=0
  for png in "$dir"/*.png; do
    ffmpeg -loglevel error -y -i "$png" -vf "crop=$((PAGE_W * SCALE)):$((PAGE_H * SCALE)):0:ih-$((PAGE_H * SCALE))" \
      -compression_level 9 -pred mixed "$png.crop.png" \
      && mv "$png.crop.png" "$png" || ok=1
  done
  python3 - "$dir/steps.json" "$((PAGE_W * SCALE))" "$((PAGE_H * SCALE))" <<'PY' || ok=1
import json, sys
s = json.load(open(sys.argv[1]))
vw, vh = s["viewport"]["width"], s["viewport"]["height"]
problems = list(s.get("errors", []))
if (vw, vh) != (int(sys.argv[2]), int(sys.argv[3])):
    problems.append(f"the page is {vw}x{vh} pixels, not {sys.argv[2]}x{sys.argv[3]}")
for p in problems: print("  ", p)
sys.exit(1 if problems else 0)
PY
  echo "  $theme $name: $(ls "$dir"/*.png 2>/dev/null | wc -l) stills"
  return $ok
}

# --- scenes.json: every scene below captured in that theme, in order ---
scenes_json() {  # <theme>
  python3 - "$OUT/$1" "$1" "$T/titles" <<'PY'
import json, os, sys
root, theme = sys.argv[1], sys.argv[2]
titles = dict(l.split("\t", 1) for l in open(sys.argv[3]).read().splitlines() if "\t" in l)
scenes = []
for name in sorted(os.listdir(root)):
    f = os.path.join(root, name, "steps.json")
    if name not in titles or not os.path.isfile(f): continue  # only the scenes below, never an old folder
    s = json.load(open(f))
    scenes.append({"scene": int(name.split("-")[0]), "name": name, "title": titles.get(name, name),
        "steps": [{"name": st["name"], "before": f"{name}/{st['before']}", "after": f"{name}/{st['after']}", "box": st["box"], "selector": st["selector"]}
                  for st in s["steps"]]})
out = {"theme": theme, "screen": {"width": 2560, "height": 1664, "scale": 2}, "box": "x, y, width and height in the stills' pixels", "scenes": scenes}
json.dump(out, open(os.path.join(root, "scenes.json"), "w"), indent=2)
print(f"video/capture/{theme}/scenes.json: {len(scenes)} scenes, {sum(len(s['steps']) for s in scenes)} steps")
PY
}

# --- the scenes, in the website's order (GA-144) ---
# scene <number> <name> <title> <start route> <steps…>: a step "<name>=<selector>" is a click with a still before and after
# (see runSteps in src/selftest.ts); "#/route", "wait:<ms>", "term:<command>" and a bare selector set things up without
# stills. Every scene starts with "theme:<dark|light>".
mkdir -p "$OUT"; : > "$T/titles"
fail=0
scene() {
  local n=$1 name=$2 title=$3; shift 3
  local folder; folder=$(printf '%02d-%s' "$n" "$name")
  printf '%s\t%s\n' "$folder" "$title" >> "$T/titles"
  case "$ONLY" in "  ") ;; *" $n "*) ;; *) return 0 ;; esac
  local theme
  for theme in $THEMES; do capture "$theme" "$folder" "$@" || fail=1; done
}

# The website's eight features, a few clicks each.
scene 1 board "The board, and a quick look at a card's live run" board \
  "wait:7000" 'open-card@3000=[data-card="NWS-1"]'
scene 2 chat "The Team Lead chat: a request, the cards it made, Approve" "chat/$THREAD" \
  "wait:1500" 'approve@2500=.pending-actions .pa-card .pa-actions .btn.primary'
scene 3 clis "Bring your own CLI: Claude Code and Codex in Settings → Coding CLIs, then Usage → Subscription" settings/appearance \
  "wait:1000" 'agents-tab=.tabs[aria-label="Settings"] .tab:text(Agents and runs)' 'usage=.side .nav-item:text(Usage)'
scene 4 review "Tested before you see it: a card in Review with its pull request and QA's result" board \
  "wait:2000" 'open-card=[data-card="NWS-2"]' 'runs=.tabs button.tab:text(Runs)'
scene 5 editor "The Editor and Terminal: a project opens in the Editor, then its Terminal" board \
  "wait:1500" 'open-folder=.side [aria-label="Editor"] button.nav-item:text(Open a folder)' \
  'open-project@2000=.ed-picker .opt[title="~/Projects/northwind-shop"]' 'terminal@2500=.btn.term-btn'
scene 6 memory "Memory: a note with its links, then the graph" "memory/$NOTE" \
  "wait:1000" 'open-link=.mem-note .note-view a.wikilink:text(Deploy steps)' 'graph@4000=.seg.mem-views button:text(Graph)'
scene 7 tools "Tools per agent: an agent's form → Tools" "agent/$BACKEND" \
  "wait:1000" 'settings=.entity-head .actions .btn.ghost:text(Settings)' 'use-playwright=.drawer .ma-server label.check:text(playwright)'
scene 8 inbox "The Inbox: a question, and Run this for me" board \
  "wait:1000" 'inbox=.side .nav-item:text(Inbox)' 'done-continue@2500=.run-for-me-card .btn.sm.primary'

# --- scenes.json for each theme captured, and the stills' size checked ---
for theme in $THEMES; do
  scenes_json "$theme" || fail=1
  bad=$(find "$OUT/$theme" -name '*.png' -exec ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 {} \; | grep -vc '^2560,1664$')
  [ "$bad" = 0 ] || { echo "$bad stills in $theme are not 2560x1664"; fail=1; }
done
du -sh "$OUT" | cut -f1 | sed 's/^/video\/capture: /'
exit $fail
