#!/usr/bin/env python3
"""Stands in for `claude` in the GA-39 chat tests (src-tauri/tests/mcp_run_flow_test.rs), never the real one. It starts
no MCP server: it keeps what the turn gave it and answers in one short message.

Each run appends {argv, prompt, path, mode, config} to fake-mcp-calls.jsonl next to its --mcp-config file: `config` is
that file's contents and `mode` its permission bits (like stat -c %a), both taken while the turn lives.
Its init line names every server in the config: `gizai` connected, another one connected unless the prompt holds
`mcp-status:<name>=<status>` for it (like mcp-status:docs=failed)."""
import json, os, re, stat, sys

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

sid = flag("--resume") or flag("--session-id") or "none"
config = flag("--mcp-config")
prompt = sys.stdin.read()
servers = {}
if config:
    here = os.path.dirname(config)
    servers = json.load(open(config)).get("mcpServers", {})
    mode = format(stat.S_IMODE(os.stat(config).st_mode), "o")
    with open(os.path.join(here, "fake-mcp-calls.jsonl"), "a") as f:
        f.write(json.dumps({"argv": argv, "prompt": prompt, "path": config, "mode": mode, "config": servers}) + "\n")

wanted = dict(re.findall(r"mcp-status:([A-Za-z0-9_-]+)=([a-z-]+)", prompt))
states = [{"name": n, "status": "connected" if n == "gizai" else wanted.get(n, "connected")} for n in servers]
out({"type": "system", "subtype": "init", "session_id": sid, "model": "fake-model", "tools": ["Read", "Glob", "Grep"], "mcp_servers": states})
out({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "Noted."}]}, "session_id": sid})
out({"type": "result", "subtype": "success", "is_error": False, "result": "Noted.", "total_cost_usd": 0.01, "num_turns": 1, "session_id": sid,
     "usage": {"input_tokens": 1000, "output_tokens": 100}})
