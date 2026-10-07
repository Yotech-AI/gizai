#!/usr/bin/env bash
# UI tests in a HEADLESS cage against a fresh copy of the demo data (.devdata/demo):
#  1. board: drags a To do card into In progress with pointer events; checks the database.
#  2. task page: opens the description editor, types, saves with Ctrl+Enter; checks the database.
#  3. doc page: saves with Ctrl+S, then forces a conflict and keeps our text.
#  4. team page: adds an agent with a heartbeat through the dialog and the usual routing rules.
#  5. agent run: Run on a task with the fake Claude Code, watch it live, Stop, Run again → card in Testing.
#  6. chat: set up the Team Lead from the Chat page, send a message; the fake Claude Code calls Gizai's tools
#     through the real gizai-mcp shim and socket, and the answer links the task it created.
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
exit $fail
