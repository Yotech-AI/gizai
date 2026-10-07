import { generateKeyBetween } from "fractional-indexing";

/** A sort key strictly between a and b (null = open end). Same format as gizai-core's sortkey.rs. */
export function keyBetween(a: string | null, b: string | null): string {
  return generateKeyBetween(a, b);
}

/** Key for inserting at `index` into a column whose sorted keys are `keys` (the moving card left out).
 * If the neighbours share a key, the card goes just after them instead of throwing. */
export function keyForDrop(keys: string[], index: number): string {
  const prev = index > 0 ? keys[index - 1] : null;
  const next = index < keys.length ? keys[index] : null;
  if (prev !== null && next !== null && prev >= next) return keyBetween(prev, null);
  return keyBetween(prev, next);
}
