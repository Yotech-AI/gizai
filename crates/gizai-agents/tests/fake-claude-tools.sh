#!/usr/bin/env bash
# Stands in for `claude` in GA-55's Ask Claude Code again tests (crates/gizai-agents/tests/tool_catalog_test.rs), never the
# real one. Like Claude Code without a login, it prints its init line (with its tools), then a "Not logged in" result,
# and exits 1. First it keeps what it got in $HOME/seen: its arguments (argv, one per line), the prompt it read (prompt),
# its environment (env) and its PID (pid).
# $HOME/mode (the test writes it in the scratch home before it asks):
#   hang  after its init line it waits (its child, a sleep, in its process group; its PID in child.pid) until it is ended
#   none  no init line, only the error
mkdir -p "$HOME/seen"
printf '%s\n' "$@" > "$HOME/seen/argv"
env > "$HOME/seen/env"
echo $$ > "$HOME/seen/pid"
mode="$(cat "$HOME/mode" 2>/dev/null)"
prompt="$(cat)"
printf '%s' "$prompt" > "$HOME/seen/prompt"
if [ "$mode" = "hang" ]; then
  # its child first: Gizai may end the group as soon as it reads the init line
  sleep 600 &
  child=$!
  echo "$child" > "$HOME/seen/child.pid"
fi
if [ "$mode" != "none" ]; then
  echo '{"type":"system","subtype":"init","cwd":"/x","session_id":"S","model":"claude-opus-5-5","tools":["Task","Bash","Read","WebSearch","WebFetch","FancyNewTool","mcp__gizai__get_overview"],"mcp_servers":[]}'
fi
if [ "$mode" = "hang" ]; then
  wait "$child"
  exit 0
fi
echo '{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login","session_id":"S"}'
exit 1
