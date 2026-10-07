-- Chat with the Team Lead (2026-10-06): which agent answers on the Chat page, and each thread's Claude
-- session totals (resumed sessions report cumulative cost and tokens; a turn's own share is the difference).
ALTER TABLE agent_configs ADD COLUMN chat_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chat_threads ADD COLUMN cost_usd_micros INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chat_threads ADD COLUMN input_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chat_threads ADD COLUMN output_tokens INTEGER NOT NULL DEFAULT 0;
