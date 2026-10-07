#!/usr/bin/env bash
# Makes docs/gizai.gif for the README: the demo data plus a team and a chat (example prep_readme), shown in a
# HEADLESS cage (never on your screen), one fresh copy of the data per frame. The Backend Agent works on KADE-1 with
# the fake Claude Code. Needs the release build (npm run tauri build -- --no-bundle) and ffmpeg.
# usage: scripts/readme-gif.sh
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh
GZ=$PWD; R=$GZ/.devdata/readme; FAKE=$GZ/crates/gizai-agents/tests/fake-claude.sh
# The data sits in Gizai's usual folder under shot-cage's XDG_DATA_HOME, so the screens show no "Test data" tag.
DATA=$GZ/.devdata/xdg/data/gizai
rm -rf "$R" && mkdir -p "$R/frames"
git init -q -b main "$R/repo" && git -C "$R/repo" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
cargo run -q -p gizai-core --example demo -- "$R/base" > /dev/null
THREAD=$(cargo run -q -p gizai-core --example prep_readme -- "$R/base" "$R/repo" "$FAKE")
KADE1=$(python3 -c "import sqlite3,sys; print(sqlite3.connect(sys.argv[1]).execute(\"select id from tasks where identifier='KADE-1'\").fetchone()[0])" "$R/base/gizai.db")
BACKEND=$(python3 -c "import sqlite3,sys; print(sqlite3.connect(sys.argv[1]).execute(\"select id from actors where name='Backend Agent'\").fetchone()[0])" "$R/base/gizai.db")
RUN='.run-actions .btn.primary'
n=0
frame() {  # <route> <steps> <wait>
  rm -rf "$DATA" && mkdir -p "$(dirname "$DATA")" && cp -r "$R/base" "$DATA"
  git -C "$R/repo" worktree prune
  n=$((n + 1))
  SHOT_WAIT=$3 MODE="steps:$2" scripts/shot-cage.sh "$1" "$R/frames/raw$n.png" "$DATA" > /dev/null
  # without the window's title bar
  ffmpeg -loglevel error -y -i "$R/frames/raw$n.png" -vf "crop=iw:ih-37:0:37" "$R/frames/f$(printf %02d $n).png"
}
frame "task/$KADE1" "$RUN;#/tasks" 6.5                # the board, KADE-1 being worked on
frame "task/$KADE1" "$RUN" 5                          # the card and its live run
frame "chat/$THREAD" "#/chat/$THREAD" 4.5             # the Team Lead
frame "task/$KADE1" "$RUN;#/team" 6.5                 # the org chart
frame "task/$KADE1" "$RUN;#/agent/$BACKEND" 6.5       # the agent's page
rm -rf "$DATA"
# 2.5 s per screen, one palette for all of them, looping
ffmpeg -loglevel error -y -framerate 0.4 -i "$R/frames/f%02d.png" \
  -vf "split[a][b];[a]palettegen=max_colors=128:stats_mode=full[p];[b][p]paletteuse=dither=none" -loop 0 docs/gizai.gif
ls -la docs/gizai.gif
