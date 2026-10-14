-- Indexes (2026-10-14, GA-180): these reads went by a column no index started with, so SQLite read the whole table
-- each time, and comments, chat messages and runs only grow. The board check holds the database while it reads. Only
-- indexes, each made when it is missing: a database that already has one keeps it. See docs/research/performance.md
-- (F3, P8).

-- A card's comments, oldest first (its page, a run's prompt), their ids for its activity and its last change (the
-- board check), and a person's answer after a hold, newest first. With id last, the comments come out in their order
-- (created_at, id) with no sort. Every comment, not only live ones: the activity and the last change read them all.
CREATE INDEX IF NOT EXISTS comments_task ON comments(task_id, created_at, id);

-- A chat's messages and their files. On thread_id alone: a chat is read in rowid order, which this index keeps (with
-- created_at in it, every chat would need a sort).
CREATE INDEX IF NOT EXISTS chat_messages_thread ON chat_messages(thread_id);

-- A chat's runs (its answers), in order. Card runs and board checks have no chat and stay out.
CREATE INDEX IF NOT EXISTS runs_chat ON runs(chat_thread_id, created_at) WHERE chat_thread_id IS NOT NULL;

-- The card a run claimed, freed when the run ends (runs::release_claim). Only claimed cards are in it.
CREATE INDEX IF NOT EXISTS tasks_claimed ON tasks(claimed_by_run_id) WHERE claimed_by_run_id IS NOT NULL;

-- The runs at work: the board check's busy agents, a column's runs, the runs a restart finds. Only queued, running and
-- waiting runs are in it, so it stays small. SQLite uses it only where a query names this exact list, as runs::ACTIVE
-- and the board check do.
CREATE INDEX IF NOT EXISTS runs_active ON runs(status) WHERE status IN ('queued','running','waiting_approval');

-- An agent's runs by time: its spend this month (its budget, at every board check), the Team Lead's last board check,
-- the Agent page's latest runs and runs per day. runs_agent_period is on started_at, which these don't read.
CREATE INDEX IF NOT EXISTS runs_agent_created ON runs(agent_actor_id, created_at);

-- Runs by time: the Usage page's period.
CREATE INDEX IF NOT EXISTS runs_created ON runs(created_at);

-- A project's cards: the project list's counts (the board and the sidebar) and its runs this month, and a release under
-- way (the board check). deleted_at is a column, not a WHERE: the counts read live cards from the index alone, and the
-- runs this month count archived cards' runs too.
CREATE INDEX IF NOT EXISTS tasks_project ON tasks(project_id, deleted_at, state_category);
