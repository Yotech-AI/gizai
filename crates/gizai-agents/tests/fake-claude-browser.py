#!/usr/bin/env python3
"""Stands in for `claude` in GA-55's browser cleanup tests (crates/gizai-agents/tests/browser_test.rs and
src-tauri/tests/web_browser_flow_test.rs), never the real one. Like Claude Code, it starts the command servers of its
--mcp-config file (their command, arguments and environment lines) as its own children, in its process group, prints an
init line naming them and works until it is interrupted: on SIGINT it exits 130 at once, without ending the servers
itself, so only Gizai's signals to the run's process group reach them.

Its environment holds FAKE_BROWSER_PIDS=<folder>: claude.pid goes there, and the servers get it too (fake-npx-browser.sh
writes server.pid and helper.pid there). TOOLCALLS=<n> in the prompt: once the servers are up it makes n browser tool calls
(assistant tool_use lines), for the cap on tool calls per run."""
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
pids = os.environ.get("FAKE_BROWSER_PIDS")
if not pids:
    print("fake-claude-browser: no FAKE_BROWSER_PIDS in the environment", file=sys.stderr)
    sys.exit(2)
os.makedirs(pids, exist_ok=True)
signal.signal(signal.SIGINT, lambda *_: os._exit(130))
config = argv[argv.index("--mcp-config") + 1] if "--mcp-config" in argv else None
servers = json.load(open(config)).get("mcpServers", {}) if config else {}
children = []
for name, s in servers.items():
    if s.get("type", "stdio") != "stdio" or name == "gizai":
        continue
    env = dict(os.environ)
    env.update(s.get("env") or {})
    children.append(subprocess.Popen([s["command"]] + list(s.get("args") or []), env=env, stdin=subprocess.PIPE))
with open(os.path.join(pids, "claude.pid.tmp"), "w") as f:
    f.write(str(os.getpid()))
os.rename(os.path.join(pids, "claude.pid.tmp"), os.path.join(pids, "claude.pid"))
t0 = time.time()
while children and not (os.path.exists(os.path.join(pids, "server.pid")) and os.path.exists(os.path.join(pids, "helper.pid"))) \
        and time.time() - t0 < 10:
    time.sleep(0.02)
print(json.dumps({"type": "system", "subtype": "init", "session_id": "S", "model": "fake-model",
                  "tools": ["Read", "Bash", "mcp__chrome-devtools__navigate_page", "mcp__chrome-devtools__click"],
                  "mcp_servers": [{"name": n, "status": "connected"} for n in servers]}), flush=True)
m = re.search(r"TOOLCALLS=(\d+)", prompt)
for i in range(int(m.group(1)) if m else 0):
    print(json.dumps({"type": "assistant", "session_id": "S", "message": {"role": "assistant", "content": [
        {"type": "tool_use", "id": f"tu{i}", "name": "mcp__chrome-devtools__navigate_page", "input": {"url": "https://kade.test"}}]}}), flush=True)
    time.sleep(0.05)
time.sleep(600)
