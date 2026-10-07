-- Agent settings that 0001 lacked: Claude Code tool allow-list and the heartbeat clock.
ALTER TABLE agent_configs ADD COLUMN allowed_tools_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE agent_configs ADD COLUMN last_heartbeat_at INTEGER;
