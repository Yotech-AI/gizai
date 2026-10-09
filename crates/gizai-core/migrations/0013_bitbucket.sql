-- Bitbucket (2026-10-09, GA-59): a project's link may be a Bitbucket Cloud repository, provider 'bitbucket'. SQLite
-- can't change a CHECK, so repos is rebuilt with every row and id kept, as in 0007. After this script, a Bitbucket
-- link saved earlier as a plain git URL (provider 'git': a page link like …/src/master/, or a clone address) gets its
-- tidy form https://bitbucket.org/<workspace>/<repository> and provider 'bitbucket' (db.rs, `bitbucket_links`).
CREATE TABLE repos_new (
  id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER, version INTEGER NOT NULL DEFAULT 1, created_by TEXT REFERENCES actors(id), updated_by TEXT REFERENCES actors(id),
  project_id TEXT NOT NULL REFERENCES projects(id),
  provider TEXT NOT NULL DEFAULT 'github' CHECK (provider IN ('github','bitbucket','git')),
  remote_url TEXT NOT NULL,          -- https://github.com/owner/name, https://bitbucket.org/workspace/repository or a git URL
  owner TEXT, name TEXT,             -- GitHub owner or Bitbucket workspace, and the repository's name
  default_branch TEXT DEFAULT 'main',
  local_path TEXT                    -- main checkout on this machine; worktrees go beside it
) STRICT;
INSERT INTO repos_new (id, created_at, updated_at, deleted_at, version, created_by, updated_by, project_id, provider, remote_url, owner, name,
                       default_branch, local_path)
  SELECT id, created_at, updated_at, deleted_at, version, created_by, updated_by, project_id, provider, remote_url, owner, name,
         default_branch, local_path
  FROM repos;
DROP TABLE repos;
ALTER TABLE repos_new RENAME TO repos;
