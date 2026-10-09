-- Memory (2026-10-09, GA-19): notes the Team Lead, the agents and people keep for later, as docs of kind 'memory' at
-- organisation level (no project), so versions, authorship, activity, live refresh and backups work as for docs. A note
-- has a path (folder and title, like 'Standards/Rust style'), a scope ('shared', or 'agent' for an agent's own folder
-- and the Team Lead's) and, in such a folder, its owner. Existing docs stay as they are (kind 'doc'); a project's doc
-- list shows only those. See docs/memory.md.
ALTER TABLE docs ADD COLUMN kind TEXT NOT NULL DEFAULT 'doc' CHECK (kind IN ('doc','memory'));
ALTER TABLE docs ADD COLUMN path TEXT;
ALTER TABLE docs ADD COLUMN scope TEXT CHECK (scope IN ('shared','agent'));
ALTER TABLE docs ADD COLUMN owner_actor_id TEXT REFERENCES actors(id);
CREATE UNIQUE INDEX docs_memory_path ON docs(org_id, path COLLATE NOCASE) WHERE kind = 'memory' AND deleted_at IS NULL;

-- An agent's "Use memory" switch (agent form; on by default): off, its runs get no Memory section and its learned lines
-- are not saved.
ALTER TABLE agent_configs ADD COLUMN use_memory INTEGER NOT NULL DEFAULT 1;

-- The memory notes a run's prompt was given, for the task page's Runs tab: [{"path": …, "chars": …, "shown": …}].
ALTER TABLE runs ADD COLUMN memory_json TEXT;
