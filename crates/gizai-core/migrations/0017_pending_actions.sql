-- Pending actions (2026-10-12, GA-102): a call of one of the Team Lead's tools that act (tools::NOT_AFTER_OUTSIDE in the
-- app), refused in a chat answer because that answer used a tool from outside Gizai (an MCP server of its own, the web,
-- the browser). It waits in its chat, under that answer, until you approve it (Gizai runs exactly that call, with every
-- other rule still in force) or discard it. Only you decide: no tool of the Team Lead's lists, approves or discards one,
-- and a board check never makes one. A newer memory_write or memory_append for the same note replaces the older one.
-- The Team Lead hears each decision at the start of its next answer in the chat (told_at). See docs/agent-tools.md.
CREATE TABLE pending_actions (
  id TEXT PRIMARY KEY,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  thread_id TEXT NOT NULL REFERENCES chat_threads(id),
  run_id TEXT,                          -- the chat answer that tried it (a chat run); NULL for a call no answer made
  actor_id TEXT NOT NULL REFERENCES actors(id),  -- the Team Lead: the call runs as it
  tool TEXT NOT NULL,                   -- the tool, like memory_write
  args_json TEXT NOT NULL,              -- its arguments, exactly as called
  outside_tool TEXT NOT NULL,           -- the tool from outside Gizai the answer used first, like mcp__otus__search
  note_key TEXT,                        -- memory_write and memory_append: the note it is for (its id, else its path in lower case)
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'running', 'approved', 'failed', 'discarded', 'replaced')),
  decided_at INTEGER,                   -- when you approved or discarded it (or a newer call replaced it)
  decided_by TEXT REFERENCES actors(id),
  result_md TEXT,                       -- what happened: what the approved call did, its error, or what was discarded
  told_at INTEGER                       -- the Team Lead's next answer heard the decision
) STRICT;
CREATE INDEX pending_actions_thread ON pending_actions(thread_id, status);
