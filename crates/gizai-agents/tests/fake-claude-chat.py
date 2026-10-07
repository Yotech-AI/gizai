#!/usr/bin/env python3
"""Stands in for `claude` in chat tests. Reads its flags the way Claude Code does (--session-id / --resume,
--mcp-config), starts the MCP server named in the config (the real gizai-mcp shim), lists the tools, calls one
and prints a stream-json conversation with partial messages, like `claude -p --include-partial-messages`.

The prompt (stdin) decides what happens:
  "create task <title> in <KEY>"  calls create_task; anything else calls get_overview
  FAKE_CHAT_HANG                  hangs after init until SIGINT (exit 130)
  FAKE_CHAT_CRASH                 exits 1 with an error on stderr, before init
  FAKE_LOST_SESSION + --resume    exits 1 before init, like Claude Code when the session file is gone
Each run appends {argv, prompt} to fake-calls.jsonl next to its MCP config, for the tests to read.
Costs are cumulative per session, as Claude Code reports them: 0.01 on a new session, 0.02 when resumed."""
import json, os, re, signal, subprocess, sys, time

def out(obj):
    print(json.dumps(obj), flush=True)

argv = sys.argv[1:]

def flag(name):
    if name in argv:
        i = argv.index(name)
        return argv[i + 1] if i + 1 < len(argv) else ""
    return None

if "--input-format" in argv:
    # Asked for the model list: answer the initialize request, then exit when stdin closes.
    sys.stdin.readline()
    sys.stdout.write(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures", "models-init.jsonl")).read())
    sys.stdout.flush()
    sys.stdin.read()
    sys.exit(0)

resume = flag("--resume")
sid = resume or flag("--session-id") or "none"
config = flag("--mcp-config")
prompt = sys.stdin.read()
if config:
    with open(os.path.join(os.path.dirname(config), "fake-calls.jsonl"), "a") as f:
        f.write(json.dumps({"argv": argv, "prompt": prompt}) + "\n")

if "FAKE_NOT_LOGGED_IN" in prompt:
    # What Claude Code 2.1.289 prints without a login: a synthetic assistant text, then an error result.
    out({"type": "system", "subtype": "init", "session_id": sid, "model": "fake-model", "mcp_servers": []})
    out({"type": "assistant", "message": {"model": "<synthetic>", "role": "assistant", "content": [{"type": "text", "text": "Not logged in · Please run /login"}]}, "session_id": sid})
    out({"type": "result", "subtype": "success", "is_error": True, "result": "Not logged in · Please run /login", "total_cost_usd": 0, "num_turns": 1, "session_id": sid, "terminal_reason": "api_error"})
    sys.exit(1)
if "FAKE_CHAT_CRASH" in prompt:
    print("error: the fake was told to crash", file=sys.stderr)
    sys.exit(1)
if resume and "FAKE_LOST_SESSION" in prompt:
    print(f"No conversation found with session ID: {resume}", file=sys.stderr)
    sys.exit(1)

server = json.load(open(config))["mcpServers"]["gizai"]
env = dict(os.environ)
env.update(server.get("env", {}))
mcp = subprocess.Popen([server["command"], *server.get("args", [])], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=env, text=True, bufsize=1)

def send(msg):
    mcp.stdin.write(json.dumps(msg) + "\n")
    mcp.stdin.flush()

def rpc(i, method, params=None):
    send({"jsonrpc": "2.0", "id": i, "method": method, "params": params or {}})
    line = mcp.stdout.readline()
    return json.loads(line) if line else {"error": "closed"}

init = rpc(1, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "fake-claude", "version": "0"}})
status = "connected" if "result" in init else "failed"
if status == "connected":
    send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    names = [t["name"] for t in rpc(2, "tools/list").get("result", {}).get("tools", [])]
else:
    names = []
out({"type": "system", "subtype": "init", "session_id": sid, "model": "fake-model",
     "tools": ["Read"] + ["mcp__gizai__" + n for n in names], "mcp_servers": [{"name": "gizai", "status": status}]})

if "FAKE_CHAT_HANG" in prompt:
    signal.signal(signal.SIGINT, lambda *_: sys.exit(130))
    out({"type": "stream_event", "event": {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}, "session_id": sid})
    out({"type": "stream_event", "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Thinking about "}}, "session_id": sid})
    time.sleep(600)
    sys.exit(0)

def say(text, index=0):
    out({"type": "stream_event", "event": {"type": "content_block_start", "index": index, "content_block": {"type": "text", "text": ""}}, "session_id": sid})
    half = len(text) // 2
    for part in (text[:half], text[half:]):
        out({"type": "stream_event", "event": {"type": "content_block_delta", "index": index, "delta": {"type": "text_delta", "text": part}}, "session_id": sid})
        time.sleep(0.02)
    out({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}]}, "session_id": sid})

say("Sure, on it.")
m = re.search(r"create task (.+?) in ([A-Za-z0-9]+)", prompt)
name, args = ("create_task", {"project": m.group(2), "title": m.group(1)}) if m else ("get_overview", {})
out({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "tool_use", "id": "toolu_1", "name": "mcp__gizai__" + name, "input": args}]}, "session_id": sid})
res = rpc(3, "tools/call", {"name": name, "arguments": args}).get("result", {"content": [{"type": "text", "text": "no answer"}], "isError": True})
text = res["content"][0]["text"]
out({"type": "user", "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": [{"type": "text", "text": text}], "is_error": res.get("isError", False)}]}, "session_id": sid})
try:
    ident = json.loads(text).get("task", {}).get("identifier")
except Exception:
    ident = None
final = f"Done: {ident}." if ident else "Here is the overview."
say(final)
mcp.stdin.close()
mcp.wait(timeout=5)
n = 2 if resume else 1
out({"type": "result", "subtype": "success", "is_error": False, "result": final, "total_cost_usd": 0.01 * n, "num_turns": 2, "session_id": sid,
     "usage": {"input_tokens": 1000 * n, "output_tokens": 100 * n}})
