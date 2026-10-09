#!/usr/bin/env bash
# UI tests in a HEADLESS cage against a fresh copy of the demo data (.devdata/demo):
#  1. board: drags a To do card into In progress with pointer events; checks the database.
#  2. task page: opens the description editor, types, saves with Ctrl+Enter; checks the database.
#  3. doc page: saves with Ctrl+S, then forces a conflict and keeps our text.
#  4. team page (GA-53): Design first and one empty spot per branch; the Development spot opens the agent form with its
#     role and no wake-up; picking a role fills in its instructions and allowed commands until the list is edited (GA-63),
#     and the agent lands on To do and In progress; its card dragged from the chart onto Testing (and
#     not onto Review), "+ Agent" and ×; Manual and Auto; the next column
#     and the backend's refusals (a link to itself, Auto without a next column); Review; Add column; dragging a column by
#     its grip; the bins of the last Backlog and Done; the removal confirm and removing a column; a new label, a name in use
#     and removing a label; a new branch with its empty spot, removed again; "New label…" in Properties and the New task drawer.
#  5. agent run: Run on a task with the fake Claude Code, watch it live, Stop, Run again → card in Testing.
#  6. chat: set up the Team Lead from the Chat page, send a message; the fake Claude Code calls Gizai's tools
#     through the real gizai-mcp shim and socket, and the answer links the task it created. Then Runs on under the
#     text box: it lists the Claude Code accounts (Codex disabled, with why) and picking Claude Code 2 saves it; while a
#     slow answer is written, Enter queues a message, which shows as queued and goes by itself when the answer is done.
#  7. usage (GA-33): Usage in the sidebar's Company section above Team; the Total, Agents and Projects tabs and the period
#     switch against prep_usage's runs (an unknown cost, a chat turn, a run 20 days ago), and the tabs agree; then the
#     Projects list's AI usage column, sorted by its header.
# Makes .devdata/demo when it is missing, and builds the app when it is missing or stale (scripts/app-ready.sh).
# usage: scripts/ui-test.sh
set -uo pipefail
cd "$(dirname "$0")/.."
# A brand-new worktree has no demo data and no app yet; an app older than its sources is built again.
source scripts/app-ready.sh
gz_demo || { echo "could not make the demo data"; exit 1; }
gz_app || { echo "could not build the app"; exit 1; }
# The chat test needs gizai-mcp next to the app (a fresh checkout doesn't have it yet).
(source scripts/env.sh && cargo build --release -q -p gizai-mcp) || { echo "could not build gizai-mcp"; exit 1; }
# Every test copy points Claude Code at the fake, so the agent form's model list never starts the real claude.
fresh() { rm -rf .devdata/uitest && mkdir -p .devdata/uitest && cp -r .devdata/demo/. .devdata/uitest/
  python3 -c "import sqlite3,json,time,sys; c=sqlite3.connect('.devdata/uitest/gizai.db'); c.execute(\"insert or replace into settings(key,org_id,value_json,updated_at) values('claude_bin','',?,?)\", (json.dumps(sys.argv[1]), int(time.time()*1000))); c.commit()" "$PWD/crates/gizai-agents/tests/fake-claude.sh"; }
fail=0
fresh; DATA=$PWD/.devdata/uitest ROUTE=board scripts/smoke-cage.sh || fail=1
fresh
TASK=$(python3 -c "import sqlite3; print(sqlite3.connect('.devdata/uitest/gizai.db').execute(\"select id from tasks where identifier='KADE-1'\").fetchone()[0])")
DATA=$PWD/.devdata/uitest ROUTE="task/$TASK" scripts/smoke-cage.sh || fail=1
fresh
DOC=$(python3 -c "import sqlite3; print(sqlite3.connect('.devdata/uitest/gizai.db').execute(\"select id from docs where title='Requirements'\").fetchone()[0])")
DATA=$PWD/.devdata/uitest ROUTE="doc/$DOC" scripts/smoke-cage.sh || fail=1
fresh
DATA=$PWD/.devdata/uitest ROUTE=team scripts/smoke-cage.sh || fail=1
fresh
rm -rf .devdata/uitest-repo && mkdir -p .devdata/uitest-repo && git -C .devdata/uitest-repo init -q -b main \
  && git -C .devdata/uitest-repo -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
FAKE=$PWD/crates/gizai-agents/tests/fake-claude.sh
RUNTASK=$(source scripts/env.sh && cargo run -q -p gizai-core --example prep_run -- .devdata/uitest "$PWD/.devdata/uitest-repo" "$FAKE")
DATA=$PWD/.devdata/uitest ROUTE="task/$RUNTASK" MODE=run scripts/smoke-cage.sh || fail=1
fresh
(source scripts/env.sh && cargo run -q -p gizai-core --example prep_chat -- .devdata/uitest "$PWD/crates/gizai-agents/tests/fake-claude-chat.py")
DATA=$PWD/.devdata/uitest ROUTE=chat MODE=chat scripts/smoke-cage.sh || fail=1
fresh
(source scripts/env.sh && cargo run -q -p gizai-core --example prep_usage -- .devdata/uitest "$PWD/crates/gizai-agents/tests/fake-claude.sh")
DATA=$PWD/.devdata/uitest ROUTE=usage scripts/smoke-cage.sh || fail=1
exit $fail
