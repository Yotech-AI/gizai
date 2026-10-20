#!/usr/bin/env python3
"""Stands in for `claude` in chat tests. Reads its flags the way Claude Code does (--session-id / --resume,
--mcp-config), starts the MCP server named in the config (the real gizai-mcp shim), lists the tools, calls one
and prints a stream-json conversation with partial messages, like `claude -p --include-partial-messages`.

The prompt (stdin) decides what happens:
  "create task <title> in <KEY>"  calls create_task; anything else calls get_overview
  FAKE_CHAT_HANG                  hangs after init until SIGINT (exit 130)
  FAKE_CHAT_CRASH                 exits 1 with an error on stderr, before init
  FAKE_LOST_SESSION + --resume    exits 1 before init, like Claude Code when the session file is gone
  FAKE_CHAT_WAIT                  after init, writes fake-waiting and waits for a fake-go file (both next to the MCP config)
  FAKE_IGNORE_STOP (with WAIT)    ignores SIGINT and, once fake-go is there, finishes a short answer normally
  FAKE_CHAT_SLOW                  after init, takes 3 s before it answers (the UI test queues a message meanwhile)
  FAKE_CHAT_FAIL                  after init (and the wait), ends with an error result and exits 1
  FAKE_CHAT_LIMIT                 after init (and the wait), Claude Code's usage-limit answer (fixtures/chat-limit.jsonl)
                                  and exit 1, unless FAKE_HAS_USAGE=1 is in the environment (another account)
  FAKE_ATTACH_TO <task>           calls attach_file with the first file the prompt names ("- name: /path", the files added
                                  to a message) and that task, instead of get_overview
  FAKE_READ_FILES                 reads every file the prompt names that way; the last answer is "Read: <their text>"
  FAKE_GET_PROJECT <key>          calls get_project for that project instead of get_overview; with FAKE_READ_FILES it reads
                                  the file at each of its files' paths instead (GA-207)
  FAKE_REMEMBER <text>            calls memory_append to add "- <text>" (the rest of that line) under Working agreements in
                                  Team Lead/Notes (GA-19), instead of get_overview
  FAKE_CHAT_STREAM                the last answer is about 20 KB of Markdown (headings, lists, tables, code blocks), streamed
                                  in pieces of 20 to 40 characters, one every 10 ms; exits 130 on SIGINT (GA-188)
Each run appends {argv, prompt, account, cwd, auto_memory} to fake-calls.jsonl next to its MCP config, for the tests to
read; `account` is its CLAUDE_CONFIG_DIR, `auto_memory` its CLAUDE_CODE_DISABLE_AUTO_MEMORY (null when unset, GA-85). Accounts: a session belongs to the account that started it (kept in fake-sessions.json next
to the MCP config, never in the account's folder), and --resume from another account exits 1 before init, like Claude
Code when the session file isn't in its CLAUDE_CONFIG_DIR.
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
account = os.environ.get("CLAUDE_CONFIG_DIR", "")
here = os.path.dirname(config) if config else None
if config:
    with open(os.path.join(here, "fake-calls.jsonl"), "a") as f:
        f.write(json.dumps({"argv": argv, "prompt": prompt, "account": account, "cwd": os.getcwd(),
                            "auto_memory": os.environ.get("CLAUDE_CODE_DISABLE_AUTO_MEMORY")}) + "\n")

def sessions():
    try:
        return json.load(open(os.path.join(here, "fake-sessions.json")))
    except Exception:
        return {}

def remember(session):
    if here and session not in sessions():
        s = sessions()
        s[session] = account
        json.dump(s, open(os.path.join(here, "fake-sessions.json"), "w"))

if "FAKE_NOT_LOGGED_IN" in prompt:
    # What Claude Code 2.1.289 prints without a login: a synthetic assistant text, then an error result.
    out({"type": "system", "subtype": "init", "session_id": sid, "model": "fake-model", "mcp_servers": []})
    out({"type": "assistant", "message": {"model": "<synthetic>", "role": "assistant", "content": [{"type": "text", "text": "Not logged in · Please run /login"}]}, "session_id": sid})
    out({"type": "result", "subtype": "success", "is_error": True, "result": "Not logged in · Please run /login", "total_cost_usd": 0, "num_turns": 1, "session_id": sid, "terminal_reason": "api_error"})
    sys.exit(1)
if "FAKE_CHAT_CRASH" in prompt:
    print("error: the fake was told to crash", file=sys.stderr)
    sys.exit(1)
if resume and ("FAKE_LOST_SESSION" in prompt or sessions().get(resume, account) != account):
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
remember(sid)
out({"type": "system", "subtype": "init", "session_id": sid, "model": "fake-model",
     "tools": ["Read"] + ["mcp__gizai__" + n for n in names], "mcp_servers": [{"name": "gizai", "status": status}]})

def end_mcp():
    try:
        mcp.stdin.close()
        mcp.wait(timeout=5)
    except Exception:
        pass

if "FAKE_CHAT_WAIT" in prompt:
    signal.signal(signal.SIGINT, signal.SIG_IGN if "FAKE_IGNORE_STOP" in prompt else (lambda *_: sys.exit(130)))
    open(os.path.join(here, "fake-waiting"), "w").close()
    t0 = time.time()
    while not os.path.exists(os.path.join(here, "fake-go")) and time.time() - t0 < 30:
        time.sleep(0.05)
if "FAKE_CHAT_SLOW" in prompt:
    time.sleep(3)
if "FAKE_IGNORE_STOP" in prompt:
    out({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "Finished just in time."}]}, "session_id": sid})
    end_mcp()
    out({"type": "result", "subtype": "success", "is_error": False, "result": "Finished just in time.", "total_cost_usd": 0.01, "num_turns": 1,
         "session_id": sid, "usage": {"input_tokens": 1000, "output_tokens": 100}})
    sys.exit(0)
if "FAKE_CHAT_FAIL" in prompt:
    end_mcp()
    out({"type": "result", "subtype": "success", "is_error": True, "result": "API Error: 500 the fake failed", "total_cost_usd": 0, "num_turns": 1, "session_id": sid})
    sys.exit(1)
if "FAKE_CHAT_LIMIT" in prompt and os.environ.get("FAKE_HAS_USAGE") != "1":
    end_mcp()
    for line in open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures", "chat-limit.jsonl")):
        ev = json.loads(line.replace("SESSION_ID", sid))
        if ev.get("type") != "system":
            out(ev)
    sys.exit(1)

if "FAKE_CHAT_HANG" in prompt:
    signal.signal(signal.SIGINT, lambda *_: sys.exit(130))
    out({"type": "stream_event", "event": {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}, "session_id": sid})
    out({"type": "stream_event", "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Thinking about "}}, "session_id": sid})
    time.sleep(600)
    sys.exit(0)

def say(text, index=0, pieces=None):
    out({"type": "stream_event", "event": {"type": "content_block_start", "index": index, "content_block": {"type": "text", "text": ""}}, "session_id": sid})
    half = len(text) // 2
    t0 = time.monotonic()
    for n, part in enumerate(pieces or (text[:half], text[half:]), 1):
        out({"type": "stream_event", "event": {"type": "content_block_delta", "index": index, "delta": {"type": "text_delta", "text": part}}, "session_id": sid})
        # FAKE_CHAT_STREAM's pieces: one every 10 ms, timed from the start, so the time printing takes doesn't add up.
        time.sleep(max(0.0, t0 + n * 0.01 - time.monotonic()) if pieces else 0.02)
    out({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}]}, "session_id": sid})

def in_pieces(text):
    """The text in pieces of 20 to 40 characters, the same every run (FAKE_CHAT_STREAM)."""
    cuts = [0]
    while cuts[-1] < len(text):
        cuts.append(cuts[-1] + 20 + len(cuts) * 8 % 21)
    return [text[a:b] for a, b in zip(cuts, cuts[1:])]

def long_answer():
    """FAKE_CHAT_STREAM's answer (GA-188): about 20 KB of Markdown, the same every run, with what a long Team Lead answer
    has: headings, paragraphs, bullet lists, tables and code blocks."""
    areas = ["board", "chat", "run log", "Memory", "editor", "Inbox"]
    parts = ["# Where KADE stands\n\nI went through the board, this week's runs and the notes. For each area: what is done, "
             "what waits, and what I would do next.\n"]
    n = 0
    while sum(map(len, parts)) < 20000:
        n += 1
        area = areas[n % len(areas)]
        parts.append(f"\n## {n}. The {area}\n\n"
                     f"KADE-{n} changed how the {area} loads: it asks for **one list** instead of three, and the page draws "
                     f"in about {40 + n} ms now. Two cards still wait for a review, and *KADE-{n + 100}* needs your decision "
                     "before it can start.\n\n"
                     f"- **Done:** `list_{n}` keeps its order and leaves out archived cards\n"
                     f"- **Waiting:** KADE-{n + 1} for a review, KADE-{n + 2} for a test that fails now and then\n"
                     f"- **Next:** a write reloads only what it changed (see [the plan](docs/plan-{n}.md))\n")
        if n % 3 == 0:
            parts.append(f"\n```ts\nconst cards = await invoke(\"list_tasks\", {{ project: \"KADE\", limit: {n * 10} }});\n"
                         "setCards(cards.filter((c) => c.state !== \"done\"));\n```\n")
        if n % 4 == 0:
            parts.append("\n| Card | State | Runs | Cost |\n|---|---|---:|---:|\n"
                         + "".join(f"| KADE-{n * 10 + i} | {('To do', 'In progress', 'Review')[i % 3]} | {i + 1} | ${0.42 * (i + 1):.2f} |\n"
                                   for i in range(5)))
    return "".join(parts)

say("Sure, on it.")
m = re.search(r"create task (.+?) in ([A-Za-z0-9]+)", prompt)
added = re.findall(r"^- .+?: (/.+)$", prompt, re.M)
attach = re.search(r"FAKE_ATTACH_TO ([A-Za-z0-9-]+)", prompt)
fact = re.search(r"FAKE_REMEMBER (.+)$", prompt, re.M)
project = re.search(r"FAKE_GET_PROJECT ([A-Za-z0-9]+)", prompt)
if fact:
    name, args = "memory_append", {"note": "Team Lead/Notes", "heading": "Working agreements", "text": "- " + fact.group(1).strip()}
elif attach and added:
    name, args = "attach_file", {"path": added[0], "task": attach.group(1)}
elif project:
    name, args = "get_project", {"project": project.group(1)}
else:
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
if "FAKE_READ_FILES" in prompt:
    if project:
        added = [f["path"] for f in json.loads(text)["files"]]
    final = "Read: " + " | ".join(open(p).read() for p in added)
if "FAKE_CHAT_STREAM" in prompt:
    signal.signal(signal.SIGINT, lambda *_: sys.exit(130))   # a Stop while it streams, as FAKE_CHAT_HANG
    final = long_answer()
    say(final, pieces=in_pieces(final))
else:
    say(final)
mcp.stdin.close()
mcp.wait(timeout=5)
n = 2 if resume else 1
out({"type": "result", "subtype": "success", "is_error": False, "result": final, "total_cost_usd": 0.01 * n, "num_turns": 2, "session_id": sid,
     "usage": {"input_tokens": 1000 * n, "output_tokens": 100 * n}})
