-- Folders (agent form → Permissions, 2026-10-08): folders besides its card's worktree that an agent's file tools may
-- use, as a JSON list such as [{"path": "/home/me/Herd/shared", "access": "read"}]. Access is "read" or "change" (read
-- and change). Only you set them, in the agent form; the Team Lead's tools can't.
ALTER TABLE agent_configs ADD COLUMN folders_json TEXT NOT NULL DEFAULT '[]';
