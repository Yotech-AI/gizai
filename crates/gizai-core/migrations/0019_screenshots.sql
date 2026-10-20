-- Screenshots (2026-10-15, GA-182, from GA-173's research: docs/research/screenshots.md §5.2). An agent that builds
-- something with a screen saves screenshots of it in its run's .gizai-tmp/screenshots/, and at the run's end Gizai keeps
-- them on the card: a files row with the run's id (files.run_id, made for this in 0001), a caption and its size. The
-- card's Attachments block lists only files without a run; its Screenshots tab the run's. Housekeeping clears old sets.

-- A run's screenshot: its caption (from captions.json, else made from its name) and its size in pixels. NULL for
-- what people attached.
ALTER TABLE files ADD COLUMN caption TEXT;
ALTER TABLE files ADD COLUMN width INTEGER;
ALTER TABLE files ADD COLUMN height INTEGER;
-- A run's screenshots (a card's sets, the clearing). Only run files are in it.
CREATE INDEX IF NOT EXISTS files_run ON files(run_id) WHERE run_id IS NOT NULL;

-- Set for a run that got the Screenshots section in its prompt, also one that saved none: JSON
-- {"leftOut": ["13-board.png: over the limit of 12 a run"], "preview": null}, where preview says why there was no
-- preview, when there wasn't. NULL: the run had no section, or its set was cleared.
ALTER TABLE runs ADD COLUMN screenshots_note TEXT;

-- The card's own Screenshots before Review switch: NULL follows the project's Screenshots switch; 0 or 1 is the card's.
ALTER TABLE tasks ADD COLUMN screenshots INTEGER;

-- The project's screenshots: {"on":true,"sizes":[{"name":"Desktop","width":1280,"height":800}],"bothThemes":false,
-- "fullPage":false}. NULL: the defaults (on; Desktop 1280 × 800 and Mobile 390 × 844; one theme; one window's height).
ALTER TABLE projects ADD COLUMN screenshots_json TEXT;

-- The project's preview: the command Gizai runs with bash -c in the card's worktree before an agent that takes
-- screenshots starts (NULL: no preview, so no screenshots), and how long it waits for its port to answer, in seconds
-- (NULL: 120).
ALTER TABLE projects ADD COLUMN preview_command TEXT;
ALTER TABLE projects ADD COLUMN preview_start_secs INTEGER;
