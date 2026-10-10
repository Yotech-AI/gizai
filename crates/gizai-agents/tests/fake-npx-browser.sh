#!/usr/bin/env bash
# Stands in for `npx` starting Chrome DevTools MCP in GA-55's browser tests (GIZAI_NPX points here, or a test's own MCP
# config names it), never the real one: it fetches nothing and starts no browser. It keeps its arguments (one per line) in
# $FAKE_BROWSER_PIDS/npx.argv and the CHROME_DEVTOOLS_MCP_* environment lines it got in npx.env, then becomes
# fake-mcp-server.sh: like Chrome DevTools MCP, whose Puppeteer starts Chrome in a process group of its own, it starts a
# helper (a `setsid sleep`, the "browser") out of the run's process group and ends it on SIGINT or SIGTERM
# (FAKE_BROWSER_MODE=term: on SIGTERM only). server.pid and helper.pid go to $FAKE_BROWSER_PIDS.
here="$(cd "$(dirname "$0")" && pwd)"
dir="${FAKE_BROWSER_PIDS:?FAKE_BROWSER_PIDS must name a folder}"
mkdir -p "$dir"
printf '%s\n' "$@" > "$dir/npx.argv"
env | grep '^CHROME_DEVTOOLS_MCP_' | sort > "$dir/npx.env"
exec bash "$here/fake-mcp-server.sh" "$dir" "${FAKE_BROWSER_MODE:-int}"
