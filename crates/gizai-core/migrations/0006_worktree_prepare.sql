-- Worktrees (2026-10-07): how a new worktree of the project is prepared before an agent starts in it. Paths copied from
-- the main checkout (a JSON list such as [".env", "node_modules/"]), whether missing dependencies are installed
-- (composer install, npm ci; on unless switched off) and a setup command run after the install.
ALTER TABLE projects ADD COLUMN worktree_copy_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE projects ADD COLUMN worktree_install INTEGER NOT NULL DEFAULT 1;
ALTER TABLE projects ADD COLUMN worktree_setup TEXT;
