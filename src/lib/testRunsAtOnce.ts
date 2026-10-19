// Test runs at once (GA-212, project form → New worktrees): how many of a project's cards are tested at the same time.
// The range and the message are the backend's (TEST_RUNS_AT_ONCE and test_runs_field in crates/gizai-core/src/projects.rs),
// so the form refuses a value before saving with the same why the backend would give.

/** 1 to 20 cards; an empty field is no limit of its own. */
export const TEST_RUNS = { low: 1, high: 20 };

/** Why the project form can't save Test runs at once as typed, or null: it is empty (null, or NaN while it is emptied),
 *  or a whole number from 1 to 20. */
export function testRunsProblem(n: number | null | undefined): string | null {
  if (n == null || Number.isNaN(n)) return null;
  if (Number.isInteger(n) && n >= TEST_RUNS.low && n <= TEST_RUNS.high) return null;
  return `Test runs at once must be from ${TEST_RUNS.low} to ${TEST_RUNS.high}, or empty for no limit.`;
}

/** What the project form saves: the number, or 0 for an empty field, which removes the limit. */
export const testRunsToSave = (n: number | null | undefined): number => (n == null || Number.isNaN(n) ? 0 : n);
