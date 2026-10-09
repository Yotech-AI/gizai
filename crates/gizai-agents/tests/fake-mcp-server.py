#!/usr/bin/env python3
"""A fake stdio MCP server for List tools tests (newline-delimited JSON-RPC on stdin/stdout).

    fake-mcp-server.py <mode> [--env NAME]...

Modes:
  hints     three tools: one read-only (Otus OS style: only readOnlyHint, no title), one acting (readOnlyHint false),
            one with all four hints and a title in its annotations
  nohints   two tools without annotations
  paged     tools over three pages (nextCursor "page-2", then "page-3"); a wrong cursor is an error
  fail      prints the values of the --env variables (stdout, not JSON, and stderr) and exits 3 before answering
  rpcerror  answers tools/list with a JSON-RPC error that holds the values of the --env variables
  hang      prints the values of the --env variables to stderr, then never answers
  ignoreterm  like hints, but ignores SIGTERM and keeps running after its stdin closes (only SIGKILL ends it)

--env NAME  a variable the server needs: in hints, nohints and paged modes it exits (code 4) when it isn't set.

Environment (not secrets, for the tests):
  FAKE_MCP_PIDFILE        writes its own PID there
  FAKE_MCP_CHILD_PIDFILE  starts a child (sleep 300) in a session and process group of its own (setsid) and writes its
                          PID there; the child is ended on SIGTERM, SIGINT and when stdin closes
  FAKE_MCP_VERSION        the version in serverInfo (default 1.0.0)
"""
import json
import os
import signal
import subprocess
import sys
import time

args = sys.argv[1:]
mode = args[0] if args else "hints"
needed = [args[i + 1] for i, a in enumerate(args) if a == "--env" and i + 1 < len(args)]


def write_pid(var, pid):
    path = os.environ.get(var)
    if path:
        with open(path + ".tmp", "w") as f:
            f.write(str(pid))
        os.replace(path + ".tmp", path)


write_pid("FAKE_MCP_PIDFILE", os.getpid())

child = None


def end_child():
    global child
    if child is not None and child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=5)
        except Exception:
            child.kill()
            child.wait()
    child = None


def on_signal(signum, _frame):
    end_child()
    sys.exit(0)


if os.environ.get("FAKE_MCP_CHILD_PIDFILE"):
    child = subprocess.Popen(["sleep", "300"], start_new_session=True, stdin=subprocess.DEVNULL,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    write_pid("FAKE_MCP_CHILD_PIDFILE", child.pid)

if mode == "ignoreterm":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
else:
    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGINT, on_signal)

values = " ".join(f"{n}={os.environ.get(n, '<unset>')}" for n in needed)

if mode == "fail":
    print(f"starting with {values}", flush=True)
    print(f"error: the server refused the key ({values})", file=sys.stderr, flush=True)
    sys.exit(3)

if mode in ("hints", "nohints", "paged", "ignoreterm"):
    missing = [n for n in needed if not os.environ.get(n)]
    if missing:
        print(f"error: {', '.join(missing)} is not set", file=sys.stderr, flush=True)
        sys.exit(4)

if mode == "hang":
    print(f"connecting with {values}", file=sys.stderr, flush=True)

READ_NOTES = {
    "name": "read_notes",
    "description": "Reads the notes that match a query.",
    "inputSchema": {
        "type": "object",
        "properties": {
            "query": {"type": "string", "description": "Words to look for"},
            "limit": {"type": "integer", "description": "How many notes at most"},
            "tags": {"type": "array", "items": {"type": "string"}},
        },
        "required": ["query"],
    },
    "annotations": {"readOnlyHint": True},
}
SEND_MAIL = {
    "name": "send_mail",
    "description": "Sends an email.",
    "inputSchema": {
        "type": "object",
        "properties": {
            "to": {"type": "string", "description": "Address"},
            "body": {"type": ["string", "null"]},
            "priority": {"enum": ["low", "high"]},
        },
        "required": ["to"],
    },
    "annotations": {"readOnlyHint": False},
}
TAG_NOTE = {
    "name": "tag_note",
    "description": "Adds a tag to a note.",
    "inputSchema": {"type": "object", "properties": {"note": {"type": "string"}, "tag": {"type": "string"}}, "required": ["note", "tag"]},
    "annotations": {"title": "Tag a note", "readOnlyHint": False, "destructiveHint": False, "idempotentHint": True, "openWorldHint": False},
}
NO_HINTS = [
    {"name": "delete_everything", "description": "Deletes all the things.", "inputSchema": {"type": "object", "properties": {}}},
    {"name": "lookup", "description": "Looks something up.", "inputSchema": {"type": "object", "properties": {"q": {"type": "string"}}}},
]
PAGES = {
    None: ([{"name": "p1_a", "description": "page one, first"}, {"name": "p1_b", "description": "page one, second"}], "page-2"),
    "page-2": ([{"name": "p2_a", "description": "page two"}], "page-3"),
    "page-3": ([{"name": "p3_a", "description": "page three"}], None),
}


def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def answer(req):
    method = req.get("method")
    rid = req.get("id")
    if rid is None:
        return  # a notification
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": rid, "result": {
            "protocolVersion": req.get("params", {}).get("protocolVersion", "2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "fake-mcp", "version": os.environ.get("FAKE_MCP_VERSION", "1.0.0")},
        }})
    elif method == "tools/list":
        if mode == "rpcerror":
            send({"jsonrpc": "2.0", "id": rid, "error": {"code": -32000, "message": f"bad credentials: {values}"}})
        elif mode == "hang":
            pass
        elif mode == "paged":
            cursor = (req.get("params") or {}).get("cursor")
            if cursor not in PAGES:
                send({"jsonrpc": "2.0", "id": rid, "error": {"code": -32602, "message": f"unknown cursor {cursor}"}})
                return
            tools, nxt = PAGES[cursor]
            result = {"tools": tools}
            if nxt:
                result["nextCursor"] = nxt
            send({"jsonrpc": "2.0", "id": rid, "result": result})
        elif mode == "nohints":
            send({"jsonrpc": "2.0", "id": rid, "result": {"tools": NO_HINTS}})
        else:
            send({"jsonrpc": "2.0", "id": rid, "result": {"tools": [READ_NOTES, SEND_MAIL, TAG_NOTE]}})
    else:
        send({"jsonrpc": "2.0", "id": rid, "error": {"code": -32601, "message": "Method not found"}})


for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        answer(json.loads(line))
    except json.JSONDecodeError:
        pass

# stdin closed
if mode == "ignoreterm":
    while True:
        time.sleep(1)
end_child()
