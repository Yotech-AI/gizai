#!/usr/bin/env bash
# Stands in for an MCP server in mcp_cleanup_test (started by fake-claude-mcp.py, as its child, in its process group).
# Like a server that drives a browser or a language server, it starts a helper in a process group of its own (setsid),
# so a signal to the run's group doesn't reach that helper. It ends the helper itself before it exits:
#   mode int   on SIGINT or SIGTERM
#   mode term  on SIGTERM only (SIGINT is ignored)
# It writes its own PID to server.pid and the helper's to helper.pid in the folder given as $1.
dir="$1"
mode="${2:-int}"
setsid sleep 600 </dev/null >/dev/null 2>&1 &
helper=$!
echo "$helper" > "$dir/helper.pid.tmp" && mv "$dir/helper.pid.tmp" "$dir/helper.pid"
end_helper() {
  kill -TERM "$helper" 2>/dev/null
  echo "ended helper on $1" >> "$dir/server.log"
  exit 0
}
trap 'end_helper TERM' TERM
if [ "$mode" = "term" ]; then
  trap '' INT
else
  trap 'end_helper INT' INT
fi
echo "$$" > "$dir/server.pid.tmp" && mv "$dir/server.pid.tmp" "$dir/server.pid"
# A stdio server waits for requests; this one just waits (wait returns when a trapped signal comes in).
while true; do
  sleep 600 &
  wait $!
done
