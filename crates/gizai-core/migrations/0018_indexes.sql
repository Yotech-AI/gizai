-- Indexes (2026-10-14, GA-180): reads by a column no index started with, so SQLite read the whole table each time, and
-- comments, chat messages and runs only grow. The board check holds the database while it reads. Only indexes, made
-- when missing: a database that already has one keeps it. See docs/research/performance.md (F3, P8).

-- A card's comments (comments::list, oldest first: its page, a run's prompt), its activity and its last change (the
-- comments' ids), and a person's answer after a hold (newest first). Every comment, not only live ones: the activity and
-- the last change read them all.
CREATE INDEX IF NOT EXISTS comments_task ON comments(task_id, created_at, id);

-- A chat's messages and their files. On thread_id alone: a chat is read in rowid order, which this index keeps.
CREATE INDEX IF NOT EXISTS chat_messages_thread ON chat_messages(thread_id);

-- A chat's runs (its answers), in order. Card runs and board checks have no chat and stay out.
CREATE INDEX IF NOT EXISTS runs_chat ON runs(chat_thread_id, created_at) WHERE chat_thread_id IS NOT NULL;

-- The card a run claimed, freed when the run ends (runs::release_claim). Only claimed cards are in it.
CREATE INDEX IF NOT EXISTS tasks_claimed ON tasks(claimed_by_run_id) WHERE claimed_by_run_id IS NOT NULL;

-- The runs at work: the board check's busy agents, a column's runs, the runs a restart finds. Only queued and running
-- runs are in it. SQLite uses it only for this exact list, as runs::ACTIVE and the queries write it.
CREATE INDEX IF NOT EXISTS runs_active ON runs(status) WHERE status IN ('queued','running','waiting_approval');

-- An agent's runs by time: its spend this month (its budget, at every board check), the Team Lead's last board check,
-- the Agent page's latest runs and runs per day. runs_agent_period is on started_at, which these don't read.
CREATE INDEX IF NOT EXISTS runs_agent_created ON runs(agent_actor_id, created_at);

-- Runs by time: the Usage page's period.
CREATE INDEX IF NOT EXISTS runs_created ON runs(created_at);

-- A project's cards: the project list's counts (on the board and in the sidebar) and its runs this month, archived
-- cards included (so not only deleted_at IS NULL), and a release under way (the board check).
CREATE INDEX IF NOT EXISTS tasks_project ON tasks(project_id, deleted_at, state_category);
