#!/usr/bin/env python3
"""Stands in for `claude` in mcp_cleanup_test (and the quit test in src-tauri/tests/team_lead_mcp_rules_test.rs): like Claude
Code, it starts an MCP server (fake-mcp-server.sh) as its own child, in its process group, prints an init line and works
until it is interrupted. On SIGINT it exits 130 at once, like Claude Code, without ending the server itself: only
Gizai's signals to the run's process group reach the server.

The prompt (stdin) holds PIDS=<folder> (where claude.pid, server.pid and helper.pid go: claude.pid once the server waits,
so a test that has all three PIDs can Stop the run) and MODE=int|term (the server's mode, see fake-mcp-server.sh; default
int)."""
import json, os, re, signal, subprocess, sys, time

argv = sys.argv[1:]
if "--input-format" in argv:
    # Asked for the model list: answer the initialize request, then exit when stdin closes.
    sys.stdin.readline()
    sys.stdout.write(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures", "models-init.jsonl")).read())
    sys.stdout.flush()
    sys.stdin.read()
    sys.exit(0)

prompt = sys.stdin.read()
m = re.search(r"PIDS=(\S+)", prompt)
if not m:
    print("fake-claude-mcp: no PIDS=<folder> in the prompt", file=sys.stderr)
    sys.exit(2)
pids = m.group(1)
mode = (re.search(r"MODE=(\w+)", prompt) or [None, "int"])[1]
os.makedirs(pids, exist_ok=True)
signal.signal(signal.SIGINT, lambda *_: os._exit(130))
here = os.path.dirname(os.path.abspath(__file__))
server = subprocess.Popen(["bash", os.path.join(here, "fake-mcp-server.sh"), pids, mode], stdin=subprocess.PIPE)
t0 = time.time()
# server.ready, not server.pid: the server writes it once it waits for Stop's signals (see fake-mcp-server.sh, GA-89)
while not (os.path.exists(os.path.join(pids, "server.ready")) and os.path.exists(os.path.join(pids, "helper.pid"))) and time.time() - t0 < 10:
    time.sleep(0.02)
with open(os.path.join(pids, "claude.pid.tmp"), "w") as f:
    f.write(str(os.getpid()))
os.rename(os.path.join(pids, "claude.pid.tmp"), os.path.join(pids, "claude.pid"))
print(json.dumps({"type": "system", "subtype": "init", "session_id": "S", "model": "fake-model",
                  "mcp_servers": [{"name": "otus", "status": "connected"}]}), flush=True)
time.sleep(600)
