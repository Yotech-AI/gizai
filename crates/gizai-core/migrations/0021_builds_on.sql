-- Builds on (2026-10-26, GA-228): a card that needs another card's code builds on it, and waits until that card is
-- merged: in a column of kind deploy or done (an archived card came from Done, so it counts; a cancelled card never
-- does). The queue passes over a card that waits without moving or holding it, so it keeps its place and starts by
-- itself at the next pull once the last card it builds on is merged; a start by hand (Run, Continue, the Team Lead's
-- start_agent_run and continue_agent_run) is refused with why. Any card of the organisation may be named, another
-- project's too. A card builds on at most 20 cards, never on itself, and never on a card that already builds on it,
-- directly or through other cards (`builds_on::set_in` refuses those).
CREATE TABLE task_builds_on (
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  builds_on_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (task_id, builds_on_id),
  CHECK (task_id <> builds_on_id)
) STRICT;
-- Needed by: the cards that build on a card (the card page, the loop check).
CREATE INDEX task_builds_on_needed_by ON task_builds_on(builds_on_id);
