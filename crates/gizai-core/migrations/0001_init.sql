-- generated from DATA-MODEL.md, 2026-10-06

-- Organisation, people, agents -------------------------------------------

CREATE TABLE orgs (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  name TEXT NOT NULL,
  key TEXT NOT NULL,                 -- identifier prefix for project-less tasks, e.g. 'YT'
  legal_name TEXT, vat_number TEXT, coc_number TEXT,   -- own company details (BTW, KvK)
  default_currency TEXT NOT NULL DEFAULT 'EUR',
  default_vat_rate_bp INTEGER DEFAULT 2100,            -- basis points: 21.00 %
  next_task_number INTEGER NOT NULL DEFAULT 1
) STRICT;

CREATE TABLE actors (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  kind TEXT NOT NULL CHECK (kind IN ('person','agent')),
  name TEXT NOT NULL,                -- 'Jeffrey', 'Master Chief', 'CodexCoder'
  handle TEXT NOT NULL,              -- for @mentions: 'jeffrey', 'chief'
  title TEXT,                        -- 'CEO', 'CTO', 'Agent Manager'
  reports_to_id TEXT REFERENCES actors(id),
  is_manager INTEGER NOT NULL DEFAULT 0,
  email TEXT,
  avatar_file_id TEXT REFERENCES files(id),
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','paused','archived')),
  UNIQUE (org_id, handle)
) STRICT;

CREATE TABLE agent_configs (
  actor_id TEXT PRIMARY KEY REFERENCES actors(id),
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, version INTEGER NOT NULL DEFAULT 1,
  adapter TEXT NOT NULL,             -- 'claude_code' | 'codex' | 'cursor' | 'gemini' | 'acp:<name>'
  model TEXT,                        -- adapter-specific model id, NULL = adapter default
  instructions_md TEXT,              -- role / system prompt
  permission_mode TEXT NOT NULL DEFAULT 'ask',   -- 'ask' | 'edits' | 'full' (mapped per adapter)
  default_cwd TEXT,                  -- NULL = task's repo worktree
  mcp_extra_json TEXT,               -- extra MCP servers for this agent
  wakeup TEXT NOT NULL DEFAULT 'on_assign',     -- 'on_assign' | 'heartbeat' | 'manual'
  heartbeat_minutes INTEGER,
  max_concurrent_runs INTEGER NOT NULL DEFAULT 1,
  budget_usd_micros INTEGER,         -- agent RUN budget per period (model spend), NULL = none
  budget_period TEXT DEFAULT 'month' CHECK (budget_period IN ('day','week','month'))
) STRICT;

-- Teams and workflow (2026-10-06: Jeffrey's "Teams" with Frontend/Backend/QA agents) ------

CREATE TABLE teams (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  name TEXT NOT NULL,                -- 'Software team'
  description_md TEXT,
  lead_actor_id TEXT REFERENCES actors(id),       -- default: Master Chief
  color TEXT
) STRICT;

CREATE TABLE team_members (
  team_id TEXT NOT NULL REFERENCES teams(id),
  actor_id TEXT NOT NULL REFERENCES actors(id),    -- person or agent
  role_key TEXT NOT NULL,            -- 'lead','frontend','backend','qa','reviewer','designer', free text allowed
  is_lead INTEGER NOT NULL DEFAULT 0,
  max_concurrent_runs INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL, deleted_at INTEGER,
  PRIMARY KEY (team_id, actor_id)
) STRICT;

CREATE TABLE workflow_states (       -- the board columns of a team
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  team_id TEXT NOT NULL REFERENCES teams(id),
  name TEXT NOT NULL,                -- 'Backlog','To do','In progress','Testing','Review','Done'
  category TEXT NOT NULL             -- fixed logic, editable names: Cordon's gates key off the category
    CHECK (category IN ('backlog','ready','in_progress','testing','review','done','cancelled')),
  owner_role TEXT,                   -- who works this column: 'implementer' (by label), 'qa', 'human', NULL = nobody
  wip_limit INTEGER,                 -- e.g. at most 2 cards in Testing
  color TEXT,
  sort_key TEXT NOT NULL
) STRICT;

CREATE TABLE routing_rules (         -- "label frontend -> role frontend", "column Testing -> role qa"
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  team_id TEXT NOT NULL REFERENCES teams(id),
  kind TEXT NOT NULL CHECK (kind IN ('label','column')),
  match_label_id TEXT REFERENCES labels(id),
  match_state_id TEXT REFERENCES workflow_states(id),
  target_role TEXT,                  -- first idle team member with this role_key...
  target_actor_id TEXT REFERENCES actors(id),        -- ...or one fixed actor (a pin on the card always wins)
  priority INTEGER NOT NULL DEFAULT 100,              -- lowest number wins
  enabled INTEGER NOT NULL DEFAULT 1,
  CHECK ((kind = 'label' AND match_label_id IS NOT NULL) OR (kind = 'column' AND match_state_id IS NOT NULL)),
  CHECK (target_role IS NOT NULL OR target_actor_id IS NOT NULL)
) STRICT;
-- Auto-dispatch is off by default: a rule assigns, the Run button (or an opt-in setting) starts the run.

-- Clients ---------------------------------------------------------------

CREATE TABLE clients (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  name TEXT NOT NULL,                -- display name
  legal_name TEXT,
  kind TEXT NOT NULL DEFAULT 'company' CHECK (kind IN ('company','person')),
  vat_number TEXT,                   -- BTW-nummer / VAT id (NL…B01 format validated in UI, not DB)
  coc_number TEXT,                   -- KvK-nummer / company registration
  iban TEXT,
  email TEXT, phone TEXT, website TEXT,
  street TEXT, postal_code TEXT, city TEXT, country TEXT DEFAULT 'NL',   -- ISO 3166-1 alpha-2
  currency TEXT NOT NULL DEFAULT 'EUR',
  vat_rate_bp INTEGER,               -- NULL = org default (21 %); 0 for reverse-charge clients
  payment_terms_days INTEGER DEFAULT 30,
  language TEXT DEFAULT 'nl',        -- for documents/mail drafts
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('lead','active','inactive')),
  notes_md TEXT,
  sort_key TEXT
) STRICT;

CREATE TABLE contacts (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  client_id TEXT NOT NULL REFERENCES clients(id),
  name TEXT NOT NULL, role TEXT, email TEXT, phone TEXT,
  is_primary INTEGER NOT NULL DEFAULT 0,
  notes_md TEXT
) STRICT;

-- Projects ----------------------------------------------------------------

CREATE TABLE projects (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  client_id TEXT REFERENCES clients(id),         -- NULL = internal project
  number TEXT NOT NULL,              -- project nr, format from settings, e.g. '2026-014'
  key TEXT NOT NULL,                 -- task prefix, e.g. 'EUQR'
  name TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active'
    CHECK (status IN ('planned','active','paused','done','archived')),
  color TEXT,                        -- sidebar dot
  goal_md TEXT,
  lead_actor_id TEXT REFERENCES actors(id),
  team_id TEXT REFERENCES teams(id),             -- the team (and so the board workflow) that works on it
  starts_on TEXT, due_on TEXT,       -- ISO dates
  budget_amount_minor INTEGER,       -- PROJECT budget: money for the client (cents)
  budget_currency TEXT DEFAULT 'EUR',
  budget_hours REAL,
  hourly_rate_minor INTEGER,
  next_task_number INTEGER NOT NULL DEFAULT 1,
  folder_path TEXT,                  -- optional local folder for docs mirror / artifacts
  sort_key TEXT,
  UNIQUE (org_id, number), UNIQUE (org_id, key)
) STRICT;

CREATE TABLE repos (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  project_id TEXT NOT NULL REFERENCES projects(id),
  provider TEXT NOT NULL DEFAULT 'github' CHECK (provider IN ('github','git')),
  remote_url TEXT NOT NULL,          -- git@github.com:owner/name.git
  owner TEXT, name TEXT,
  default_branch TEXT DEFAULT 'main',
  local_path TEXT                    -- main checkout on this machine; worktrees go beside it
) STRICT;

CREATE TABLE docs (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  project_id TEXT REFERENCES projects(id),
  client_id TEXT REFERENCES clients(id),
  parent_id TEXT REFERENCES docs(id),            -- simple doc tree
  title TEXT NOT NULL,
  body_md TEXT NOT NULL DEFAULT '',
  mirror_path TEXT,                  -- if mirrored to a .md file (see §D)
  current_version INTEGER NOT NULL DEFAULT 1,
  sort_key TEXT
) STRICT;

CREATE TABLE doc_versions (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL,
  doc_id TEXT NOT NULL REFERENCES docs(id),
  version INTEGER NOT NULL,
  body_md TEXT NOT NULL,
  author_actor_id TEXT REFERENCES actors(id),
  run_id TEXT REFERENCES runs(id),
  UNIQUE (doc_id, version)
) STRICT;                            -- append-only: no updated_at/deleted_at needed

CREATE TABLE doc_links (             -- derived index: rebuilt from Markdown on every save
  source_type TEXT NOT NULL CHECK (source_type IN ('doc','task','comment','chat_message')),
  source_id TEXT NOT NULL,
  target_type TEXT NOT NULL CHECK (target_type IN ('task','project','client','doc','actor','file','artifact')),
  target_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('link','mention','embed')),
  PRIMARY KEY (source_type, source_id, target_type, target_id, kind)
) STRICT;                            -- local-only (derivable), never synced
CREATE INDEX doc_links_target ON doc_links(target_type, target_id);

-- Files and artifacts -------------------------------------------------------

CREATE TABLE files (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  owner_type TEXT NOT NULL CHECK (owner_type IN ('client','project','task','comment','doc','chat_message','actor')),
  owner_id TEXT NOT NULL,
  name TEXT NOT NULL, mime TEXT, size_bytes INTEGER NOT NULL,
  sha256 TEXT NOT NULL,              -- blob at data/files/<sha[0:2]>/<sha>
  uploaded_by_actor_id TEXT REFERENCES actors(id),
  run_id TEXT REFERENCES runs(id)    -- set for evidence: QA screenshots, traces, test reports
) STRICT;
CREATE INDEX files_owner ON files(owner_type, owner_id);

CREATE TABLE artifacts (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  project_id TEXT REFERENCES projects(id),
  task_id TEXT REFERENCES tasks(id),
  title TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'html' CHECK (kind IN ('html','chart','slides','design','report')),
  current_version INTEGER NOT NULL DEFAULT 1,
  allow_network INTEGER NOT NULL DEFAULT 0,      -- off by default (see §C)
  created_by_actor_id TEXT REFERENCES actors(id)
) STRICT;

CREATE TABLE artifact_versions (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL,
  artifact_id TEXT NOT NULL REFERENCES artifacts(id),
  version INTEGER NOT NULL,
  entry TEXT NOT NULL DEFAULT 'index.html',      -- data/artifacts/<artifact>/v<version>/<entry>
  sha256 TEXT NOT NULL,              -- hash of the folder manifest
  size_bytes INTEGER NOT NULL,
  note TEXT,                         -- "v3: darker header"
  thumbnail_sha256 TEXT,             -- PNG in files store
  author_actor_id TEXT REFERENCES actors(id),
  run_id TEXT REFERENCES runs(id),
  UNIQUE (artifact_id, version)
) STRICT;

-- Tasks -------------------------------------------------------------------

CREATE TABLE tasks (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  project_id TEXT REFERENCES projects(id),
  client_id TEXT REFERENCES clients(id),          -- usually the project's client; settable alone
  parent_id TEXT REFERENCES tasks(id),            -- sub-tasks
  identifier TEXT NOT NULL,          -- 'EUQR-12', frozen at creation
  title TEXT NOT NULL,
  description_md TEXT NOT NULL DEFAULT '',
  state_id TEXT NOT NULL REFERENCES workflow_states(id),   -- the board column (team's workflow)
  state_category TEXT NOT NULL DEFAULT 'backlog',          -- copy of the column's category, for indexes
  owner_person_id TEXT REFERENCES actors(id),              -- the responsible human: gets holds and escalations
  implementer_actor_id TEXT REFERENCES actors(id),         -- the agent that wrote the code; QA failures go back to it
  pinned_actor_id TEXT REFERENCES actors(id),              -- a pin beats routing rules
  acceptance_md TEXT,                -- acceptance criteria; QA checks these one by one
  hold TEXT CHECK (hold IN ('needs_decision','stalled','merge_conflict','waiting_approval','rate_limited','blocked')),
  hold_reason TEXT,                  -- a hold is a red flag on any column; it stops dispatch until a human clears it
  bounce_count INTEGER NOT NULL DEFAULT 0,                 -- QA fails; 3 = hold
  fail_count INTEGER NOT NULL DEFAULT 0,                   -- failed runs; 3 = hold
  priority INTEGER NOT NULL DEFAULT 0,            -- 0 none, 1 urgent, 2 high, 3 medium, 4 low
  assignee_actor_id TEXT REFERENCES actors(id),   -- person OR agent
  creator_actor_id TEXT REFERENCES actors(id),
  due_on TEXT,
  estimate_hours REAL,
  logged_hours REAL NOT NULL DEFAULT 0,          -- simple time log; feeds project budget use
  started_at INTEGER, completed_at INTEGER,
  sort_key TEXT NOT NULL,            -- order within status column / list
  claimed_by_run_id TEXT REFERENCES runs(id),     -- atomic claim (lease)
  lease_expires_at INTEGER,
  branch TEXT, pr_url TEXT,          -- git: worktree branch 'gizai/<key>-<slug>' and PR link
  UNIQUE (org_id, identifier)
) STRICT;
CREATE INDEX tasks_board ON tasks(org_id, project_id, state_id, sort_key) WHERE deleted_at IS NULL;
CREATE INDEX tasks_dispatch ON tasks(state_category, hold) WHERE deleted_at IS NULL;
CREATE INDEX tasks_assignee ON tasks(assignee_actor_id, state_id) WHERE deleted_at IS NULL;

CREATE TABLE labels (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  name TEXT NOT NULL, color TEXT,
  UNIQUE (org_id, name)
) STRICT;

CREATE TABLE task_labels (
  task_id TEXT NOT NULL REFERENCES tasks(id),
  label_id TEXT NOT NULL REFERENCES labels(id),
  created_at INTEGER NOT NULL, deleted_at INTEGER,
  PRIMARY KEY (task_id, label_id)
) STRICT;

CREATE TABLE comments (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  task_id TEXT NOT NULL REFERENCES tasks(id),
  author_actor_id TEXT NOT NULL REFERENCES actors(id),
  body_md TEXT NOT NULL,
  run_id TEXT REFERENCES runs(id)    -- set when an agent wrote it during a run
) STRICT;

-- The activity feed on a task or project is a query over `changes` (below):
-- e.g. all changes whose row_id is the task, its comments or its runs, newest first.

-- Agents at work ----------------------------------------------------------

CREATE TABLE runs (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  agent_actor_id TEXT NOT NULL REFERENCES actors(id),
  task_id TEXT REFERENCES tasks(id),
  chat_thread_id TEXT REFERENCES chat_threads(id),
  trigger TEXT NOT NULL CHECK (trigger IN ('assigned','routed','chat','manual','approval','nudge')),
  role_key TEXT,                     -- the role the agent acted in: frontend, backend, qa, lead
  outcome TEXT CHECK (outcome IN ('ready_for_testing','qa_pass','qa_fail','needs_decision','no_result','error')),
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
CREATE INDEX runs_task ON runs(task_id, created_at);
CREATE INDEX runs_agent_period ON runs(agent_actor_id, started_at);

CREATE TABLE approvals (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  run_id TEXT REFERENCES runs(id),
  agent_actor_id TEXT NOT NULL REFERENCES actors(id),
  kind TEXT NOT NULL,                -- 'merge','git_push','delete','budget_raise','decision','tool'
  summary TEXT NOT NULL,             -- what the agent wants, in one line
  payload_json TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','approved','denied','expired')),
  decided_by_actor_id TEXT REFERENCES actors(id),
  decided_at INTEGER
) STRICT;

-- Master Chief chat -------------------------------------------------------

CREATE TABLE chat_threads (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  org_id TEXT NOT NULL REFERENCES orgs(id),
  agent_actor_id TEXT NOT NULL REFERENCES actors(id),   -- default: Master Chief
  title TEXT,
  session_id TEXT                    -- adapter session to resume the conversation
) STRICT;

CREATE TABLE chat_messages (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, deleted_at INTEGER,
  thread_id TEXT NOT NULL REFERENCES chat_threads(id),
  role TEXT NOT NULL CHECK (role IN ('user','agent','tool','system')),
  author_actor_id TEXT REFERENCES actors(id),
  body_md TEXT,
  run_id TEXT REFERENCES runs(id),
  tool_name TEXT, tool_json TEXT,    -- tool-call cards
  approval_id TEXT REFERENCES approvals(id)
) STRICT;

-- Settings, tokens, sync (browser profiles: left out of v1, 2026-10-06) ----------
CREATE TABLE api_tokens (            -- per-agent tokens for the MCP / CLI surface
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL,
  actor_id TEXT NOT NULL REFERENCES actors(id),
  token_sha256 TEXT NOT NULL UNIQUE,
  scopes_json TEXT NOT NULL,
  expires_at INTEGER, revoked_at INTEGER
) STRICT;

CREATE TABLE settings (
  key TEXT NOT NULL,
  org_id TEXT NOT NULL DEFAULT '',   -- '' = app-wide (no FK, so the primary key stays unique)
  value_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (key, org_id)
) STRICT;
-- e.g. 'project_number_format' = "{YYYY}-{seq:03}", 'theme' = "dark", 'locale' = "en"
-- Secrets (API keys, GitHub tokens) are NOT here: they live in the OS keychain;
-- settings only hold the keychain entry name.

CREATE TABLE devices (               -- this machine and, later, others
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL,
  name TEXT NOT NULL, is_self INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE changes (               -- append-only; activity feed + audit + undo + sync oplog
  seq INTEGER PRIMARY KEY,           -- local order only, never a global key
  id TEXT NOT NULL UNIQUE,           -- UUIDv7, so a change keeps its identity across machines
  hlc TEXT NOT NULL,                 -- hybrid logical clock: '<unix_ms>-<counter>-<device>'
  device_id TEXT NOT NULL REFERENCES devices(id),
  actor_id TEXT REFERENCES actors(id),
  run_id TEXT REFERENCES runs(id),   -- set when an agent run made the change
  table_name TEXT NOT NULL,
  row_id TEXT NOT NULL,
  op TEXT NOT NULL CHECK (op IN ('insert','update','delete')),
  diff_json TEXT,                    -- {"status":["todo","in_progress"]}: old and new per field
  schema_version INTEGER NOT NULL,
  pushed_at INTEGER                  -- NULL until a sync backend acknowledges it
) STRICT;
CREATE INDEX changes_row ON changes(row_id, seq);

-- Search ------------------------------------------------------------------

CREATE VIRTUAL TABLE search USING fts5(
  kind UNINDEXED, ref_id UNINDEXED, org_id UNINDEXED,
  title, body,
  tokenize = 'unicode61 remove_diacritics 2'
);
-- Kept in sync by the app for tasks, comments, docs, clients, projects. Optional second table with
-- tokenize = 'trigram' for "contains" search (Dutch compounds, KvK/BTW/IBAN fragments).
