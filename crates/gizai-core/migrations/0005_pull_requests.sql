-- Review on GitHub (2026-10-07): the state of a card's pull request (open, draft, merged, closed) as Gizai last saw it
-- on GitHub, next to its link (tasks.pr_url). A merge Gizai hasn't seen yet moves the card to Done.
ALTER TABLE tasks ADD COLUMN pr_state TEXT;
