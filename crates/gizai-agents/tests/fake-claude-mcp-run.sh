#!/usr/bin/env bash
# Stands in for `claude` in the GA-39 MCP run tests (src-tauri/tests/mcp_run_flow_test.rs), never the real one. It finishes
# like fake-claude.sh's plain run (the run-ok fixture as a stream), and before it prints anything it keeps what it was
# given in the folder $FAKE_MCP_OUT (one of the CLI's environment lines in Settings → Coding CLIs):
#   argv       its arguments, one per line
#   prompt     the prompt it read from stdin
#   mcp.json   a copy of the --mcp-config file, when it got one, with mcp.mode its permission bits (stat -c %a) and
#              mcp.path its path, all taken while the run lives
# FAKE_MCP_INIT=<file>: the first line of that file is its init line (one with mcp_servers, like
# fixtures/run-mcp-init.jsonl), instead of run-ok's. FAKE_MCP_HANG in the prompt: see below.
here="$(cd "$(dirname "$0")" && pwd)"
# Asked for the model list (stream-json input): answer the initialize request, then exit when stdin closes.
case " $* " in *" --input-format stream-json "*)
  IFS= read -r _request
  cat "$here/fixtures/models-init.jsonl"
  cat > /dev/null
  exit 0 ;;
esac
prompt="$(cat)"   # the prompt comes in on stdin, like claude -p
out="${FAKE_MCP_OUT:?FAKE_MCP_OUT must name a folder}"
mkdir -p "$out"
printf '%s\n' "$@" > "$out/argv"
printf '%s' "$prompt" > "$out/prompt"
config=""
prev=""
for a in "$@"; do
  if [ "$prev" = "--mcp-config" ]; then config="$a"; fi
  prev="$a"
done
if [ -n "$config" ]; then
  printf '%s' "$config" > "$out/mcp.path"
  # GNU stat on Linux, BSD stat on macOS
  stat -c %a "$config" > "$out/mcp.mode" 2>/dev/null || stat -f %Lp "$config" > "$out/mcp.mode"
  cp "$config" "$out/mcp.json"
fi
echo "fake claude (mcp) started" >&2
fixture="$here/fixtures/run-ok.jsonl"
init="${FAKE_MCP_INIT:-$fixture}"
# FAKE_MCP_HANG in the prompt (e.g. from the card's description): after its init line it waits until interrupted (and,
# like Claude Code, exits on SIGINT), so a test can look at the run while it lives and then Stop it. Its trap is set
# before the init line goes out, and the line comes from the foreground child that waits, perl, as in fake-claude.sh's
# print_and_wait (GA-89): Stop's SIGINT ends that child, and bash then runs its trap.
case "$prompt" in *FAKE_MCP_HANG*)
  trap 'exit 130' INT
  IFS= read -r first < "$init"
  perl -e '$| = 1; print "$ARGV[0]\n"; sleep 600' -- "$first"
  exit 0 ;;
esac
head -n1 "$init"
tail -n +2 "$fixture" | while IFS= read -r line; do
  printf '%s\n' "$line"
  sleep 0.02
done
echo "fake claude (mcp) done" >&2
exit 0
