-- How hard an agent's model thinks (Claude Code --effort: low, medium, high, xhigh, max); NULL = Claude Code's default.
ALTER TABLE agent_configs ADD COLUMN effort TEXT;
