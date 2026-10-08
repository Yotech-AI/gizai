-- Refused in this run (GA-48, 2026-10-08): the tool calls a headless run's CLI refused because they needed an approval
-- nobody can give, as a JSON list such as [{"tool": "Bash", "input": "cat <<EOF ...", "reason": "..."}], one row per
-- run that had any. Claude Code reports them (permission_denials in its result line, and a permission_denied line at
-- each one); Codex, Gemini and other CLIs report none. A table of its own, made only when it is missing, so the runs
-- table keeps its shape and opening a database that already has it changes nothing.
CREATE TABLE IF NOT EXISTS run_refusals (
  run_id TEXT PRIMARY KEY REFERENCES runs(id) ON DELETE CASCADE,
  refused_json TEXT NOT NULL DEFAULT '[]'
) STRICT;
