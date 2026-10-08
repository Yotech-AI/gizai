-- The Team Lead's board check (2026-10-08, GA-35): when a card's hold started (an answer is a person's comment after
-- it), the Team Lead's check interval and its pause after three failed checks, chats the Team Lead starts (their kind,
-- cards and whether they still wait for you), and a `board_check` run trigger with what each check saw. SQLite can't
-- change a CHECK, so runs is rebuilt with every row and id kept, as in 0007. Existing chats stay your own (kind NULL).
ALTER TABLE tasks ADD COLUMN hold_at INTEGER;      -- when the current hold was set; NULL without a hold
-- Existing holds: the last change that set one, else the card's last change.
UPDATE tasks SET hold_at = COALESCE(
    (SELECT max(CAST(substr(ch.hlc, 1, instr(ch.hlc, '-') - 1) AS INTEGER)) FROM changes ch
      WHERE ch.row_id = tasks.id AND ch.table_name = 'tasks' AND ch.diff_json LIKE '%"hold":"_%'),
    updated_at)
  WHERE hold IS NOT NULL;

ALTER TABLE agent_configs ADD COLUMN board_check_minutes INTEGER;   -- the Team Lead checks the board this often; NULL = off
ALTER TABLE agent_configs ADD COLUMN board_checked_at INTEGER;      -- when it last looked
ALTER TABLE agent_configs ADD COLUMN board_check_failures INTEGER NOT NULL DEFAULT 0;  -- failed checks in a row
ALTER TABLE agent_configs ADD COLUMN board_check_paused TEXT;       -- why the check stopped (three failures); NULL = not paused

ALTER TABLE chat_threads ADD COLUMN kind TEXT CHECK (kind IN ('question','approval'));  -- set: the Team Lead started it
ALTER TABLE chat_threads ADD COLUMN task_ids_json TEXT NOT NULL DEFAULT '[]';           -- the cards it is about
ALTER TABLE chat_threads ADD COLUMN answered_at INTEGER;   -- you sent a message in it
ALTER TABLE chat_threads ADD COLUMN dismissed_at INTEGER;  -- you dismissed it in the Inbox

CREATE TABLE runs_new (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  agent_actor_id TEXT NOT NULL REFERENCES actors(id),
  task_id TEXT REFERENCES tasks(id),
  chat_thread_id TEXT REFERENCES chat_threads(id),
  trigger TEXT NOT NULL CHECK (trigger IN ('assigned','routed','chat','manual','approval','nudge','board_check')),
  role_key TEXT,                     -- the role the agent acted in: frontend, backend, qa, lead
  outcome TEXT CHECK (outcome IN ('ready_for_testing','qa_pass','qa_fail','needs_decision','deployed','no_result','error')),
  outcome_json TEXT,                 -- the agent's structured result (numbered QA issues, summary, evidence ids)
  base_sha TEXT, head_sha TEXT,      -- what the run started from and ended at
  nudged INTEGER NOT NULL DEFAULT 0, -- one nudge when a run ends without a result, then a hold
  adapter TEXT NOT NULL, model TEXT,
  status TEXT NOT NULL DEFAULT 'queued'
    CHECK (status IN ('queued','running','waiting_approval','succeeded','failed','cancelled','timed_out')),
  cwd TEXT, worktree_path TEXT, branch TEXT,
  session_id TEXT,                   -- CLI session id for --resume
  pid INTEGER,
  started_at INTEGER, ended_at INTEGER,
  exit_code INTEGER, error TEXT,
  input_tokens INTEGER DEFAULT 0, output_tokens INTEGER DEFAULT 0,
  cache_read_tokens INTEGER DEFAULT 0, cache_write_tokens INTEGER DEFAULT 0,
  cost_usd_micros INTEGER DEFAULT 0,  -- rolls up into the AGENT budget, never the project budget
  log_path TEXT NOT NULL,            -- data/runs/<id>.jsonl (normalised event stream)
  summary_md TEXT,
  findings_json TEXT                 -- a board check: every finding it saw ([{key, stamp}]), new ones included
) STRICT;
INSERT INTO runs_new (id, created_at, updated_at, deleted_at, version, created_by, updated_by, org_id, agent_actor_id, task_id, chat_thread_id,
                      trigger, role_key, outcome, outcome_json, base_sha, head_sha, nudged, adapter, model, status, cwd, worktree_path, branch,
                      session_id, pid, started_at, ended_at, exit_code, error, input_tokens, output_tokens, cache_read_tokens,
                      cache_write_tokens, cost_usd_micros, log_path, summary_md)
  SELECT id, created_at, updated_at, deleted_at, version, created_by, updated_by, org_id, agent_actor_id, task_id, chat_thread_id,
         trigger, role_key, outcome, outcome_json, base_sha, head_sha, nudged, adapter, model, status, cwd, worktree_path, branch,
         session_id, pid, started_at, ended_at, exit_code, error, input_tokens, output_tokens, cache_read_tokens,
         cache_write_tokens, cost_usd_micros, log_path, summary_md
  FROM runs;
DROP TABLE runs;
ALTER TABLE runs_new RENAME TO runs;
CREATE INDEX runs_task ON runs(task_id, created_at);
CREATE INDEX runs_agent_period ON runs(agent_actor_id, started_at);
