-- Chat (2026-10-08, GA-50): each chat's own Runs on and the coding CLI its session belongs to (another account can't
-- resume it), notes in a chat that carry what the Chat page offers with them (a switch, a usage limit), and messages
-- you send while the Team Lead answers, which wait in a queue until the answer is done.
ALTER TABLE chat_threads ADD COLUMN cli TEXT;          -- the chat's own Runs on (a coding CLI's id); NULL = the Team Lead's
ALTER TABLE chat_threads ADD COLUMN session_cli TEXT;  -- the coding CLI whose account holds session_id
-- An existing session belongs to the CLI of the last turn that ran in it.
UPDATE chat_threads SET session_cli = (
    SELECT r.adapter FROM runs r WHERE r.chat_thread_id = chat_threads.id AND r.session_id = chat_threads.session_id
     ORDER BY r.created_at DESC, r.rowid DESC LIMIT 1)
  WHERE session_id IS NOT NULL;

ALTER TABLE chat_messages ADD COLUMN meta_json TEXT;   -- a note's details, such as {"kind": "limit", "cli": …}

CREATE TABLE chat_queue (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
  thread_id TEXT NOT NULL REFERENCES chat_threads(id),
  author_actor_id TEXT REFERENCES actors(id),
  body_md TEXT NOT NULL,
  held_at INTEGER                    -- set: it waits for Send now (the answer before it stopped or failed, or Gizai restarted)
) STRICT;
CREATE INDEX chat_queue_thread ON chat_queue(thread_id);
