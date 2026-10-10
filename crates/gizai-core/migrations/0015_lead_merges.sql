-- The Team Lead merges pull requests (2026-10-10, GA-86): a project's "Team Lead may merge" switch, off by default. On,
-- the Team Lead's merge_pull_request may merge a card's pull request that QA passed, once its checks are green. Only a
-- person sets it, in the app (the project form); the Team Lead's tools can't (`projects::update` refuses an agent).
ALTER TABLE projects ADD COLUMN lead_may_merge INTEGER NOT NULL DEFAULT 0;
