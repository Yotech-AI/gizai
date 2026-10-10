#!/usr/bin/env bash
# Stands in for an MCP server in mcp_cleanup_test (started by fake-claude-mcp.py, as its child, in its process group).
# Like a server that drives a browser or a language server, it starts a helper in a process group of its own (setsid),
# so a signal to the run's group doesn't reach that helper. It ends the helper itself before it exits:
#   mode int   on SIGINT or SIGTERM (on SIGINT when both come, as on Stop: SIGINT, then SIGTERM once claude exited)
#   mode term  on SIGTERM only (SIGINT is ignored)
# It writes its own PID to server.pid and the helper's to helper.pid in the folder given as $1, and server.ready there
# once it waits (the fake claudes wait for that).
dir="$1"
mode="${2:-int}"
if command -v setsid >/dev/null 2>&1; then
  setsid sleep 600 </dev/null >/dev/null 2>&1 &
else
  # macOS has no setsid command: perl's POSIX::setsid does the same, then becomes the sleep
  perl -MPOSIX -e 'POSIX::setsid(); exec @ARGV' sleep 600 </dev/null >/dev/null 2>&1 &
fi
helper=$!
# the helper leads its own group a moment after it starts (perl starts first): wait for that (at most 2 s) before the
# test looks
i=0
while [ "$i" -lt 100 ] && [ "$(ps -o pgid= -p "$helper" 2>/dev/null | tr -d ' ')" != "$helper" ]; do
  sleep 0.02
  i=$((i + 1))
done
echo "$helper" > "$dir/helper.pid.tmp" && mv "$dir/helper.pid.tmp" "$dir/helper.pid"
end_helper() {
  kill -TERM "$helper" 2>/dev/null
  echo "ended helper on $1" >> "$dir/server.log"
  exit 0
}
# SIGTERM only notes that it came; the server ends its helper on it after the wait below (GA-89). Stop's SIGTERM comes a
# moment after its SIGINT (as soon as claude exited on that), and an INT trap that is still running, or still to run,
# goes first that way: a TERM trap that ended the server at once would cut it short.
term=
trap 'term=1' TERM
if [ "$mode" = "term" ]; then
  trap '' INT
else
  trap 'end_helper INT' INT
fi
echo "$$" > "$dir/server.pid.tmp" && mv "$dir/server.pid.tmp" "$dir/server.pid"
# A stdio server waits for requests; this one waits in a foreground child, perl, which writes server.ready and sleeps.
# Not `sleep 600 & wait $!`: when SIGINT and SIGTERM come together (bash didn't run in between), the kernel runs bash's
# SIGTERM handler first, which jumps out of `wait` before the SIGINT handler has noted its signal, and the SIGINT is lost.
# perl leaves SIGINT at its default and is in the run's group once server.ready is there, so Stop's SIGINT ends it (in
# mode term, the SIGTERM does), and bash then runs that signal's trap. (Not a bash subshell: it catches SIGINT like its
# parent, and one that comes as it `exec`s a sleep is lost; see fake-claude.sh.) 2>/dev/null: bash's "Terminated" when
# the SIGTERM ends perl.
while [ -z "$term" ]; do
  { perl -e 'open(my $f, ">", $ARGV[0]) or die "$ARGV[0]: $!\n"; close $f; sleep 600' "$dir/server.ready"; } 2>/dev/null
done
end_helper TERM
