-- Refused in this run (GA-48, 2026-10-08): the tool calls a headless run's CLI refused because they needed an approval
-- nobody can give, as a JSON list such as [{"tool": "Bash", "input": "cat <<EOF ..."}]. Claude Code reports them in its
-- result line (permission_denials); Codex, Gemini and other CLIs report none, so theirs stay '[]'.
ALTER TABLE runs ADD COLUMN refused_json TEXT NOT NULL DEFAULT '[]';
