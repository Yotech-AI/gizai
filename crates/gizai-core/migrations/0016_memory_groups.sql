-- Agents share one memory folder (2026-10-11, GA-96): an agent's "Shares memory with" (agent form → Memory). NULL, as for
-- every agent until it is set: its own folder, Agents/<its name>/. Else the agent whose folder it uses, the group's
-- owner: Agents/<owner name>/. Never an agent that shares itself (no chains: one owner per group) and never the Team Lead.
-- See docs/memory.md.
ALTER TABLE agent_configs ADD COLUMN shares_memory_with TEXT REFERENCES actors(id);
