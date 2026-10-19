-- Test runs at once (2026-10-19, GA-212): how many of the project's cards may be in a test run (a run on a card in a
-- column of kind testing) at the same time, 1 to 20. A test run builds and runs the whole test suite in the card's own
-- worktree; on a big project more than one at a time can fill the disk. The queue passes over a card in Testing while
-- the project has that many test runs live or starting, and a start by hand waits too. NULL: no limit of its own (the
-- default, so every project works as before); "Runs at once" and the agents' cards at once still hold.
ALTER TABLE projects ADD COLUMN test_runs_at_once INTEGER;
