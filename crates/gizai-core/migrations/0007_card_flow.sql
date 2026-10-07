-- Card flow (2026-10-07, GA-32): a Testing switch on every card (on: the QA Agent tests it before Review; every
-- existing card is on), a Deploy column category (merged, not deployed yet: nothing routes it) and the DevOps
-- Agent's `deployed` outcome. SQLite can't change a CHECK, so workflow_states and runs are rebuilt with every row and
-- id kept: tasks.state_id, routing_rules.match_state_id and everything that points at a run still find their rows.
-- Gizai runs its migrations with foreign keys off (SQLite's way to rebuild a table), see `db::Db::init`.
ALTER TABLE tasks ADD COLUMN testing INTEGER NOT NULL DEFAULT 1;

CREATE TABLE workflow_states_new (   -- the board columns of a team
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  team_id TEXT NOT NULL REFERENCES teams(id),
  name TEXT NOT NULL,                -- 'Backlog','To do','In progress','Testing','Review','Deploy','Done'
  category TEXT NOT NULL             -- fixed logic, editable names: the gates key off the category
    CHECK (category IN ('backlog','ready','in_progress','testing','review','deploy','done','cancelled')),
  owner_role TEXT,                   -- who works this column: 'implementer' (by label), 'qa', 'human', NULL = nobody
  wip_limit INTEGER,                 -- e.g. at most 2 cards in Testing
  color TEXT,
  sort_key TEXT NOT NULL
) STRICT;
INSERT INTO workflow_states_new (id, created_at, updated_at, deleted_at, version, created_by, updated_by, team_id, name, category, owner_role,
                                 wip_limit, color, sort_key)
  SELECT id, created_at, updated_at, deleted_at, version, created_by, updated_by, team_id, name, category, owner_role, wip_limit, color, sort_key
  FROM workflow_states;
DROP TABLE workflow_states;
ALTER TABLE workflow_states_new RENAME TO workflow_states;

CREATE TABLE runs_new (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  agent_actor_id TEXT NOT NULL REFERENCES actors(id),
  task_id TEXT REFERENCES tasks(id),
  chat_thread_id TEXT REFERENCES chat_threads(id),
  trigger TEXT NOT NULL CHECK (trigger IN ('assigned','routed','chat','manual','approval','nudge')),
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
  summary_md TEXT
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
