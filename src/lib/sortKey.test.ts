import { describe, expect, it } from "vitest";
import { keyBetween } from "./sortKey";
describe("keyBetween", () => {
  it("orders between neighbours", () => {
    const a = keyBetween(null, null), b = keyBetween(a, null), m = keyBetween(a, b);
    expect(a < m && m < b).toBe(true);
    expect(keyBetween(null, a) < a).toBe(true);
  });
  it("continues after keys the Rust core wrote", () => {
    // gizai-core's key_after(None) = "a0", key_after("a0") = "a1"
    expect(keyBetween("a0", null)).toBe("a1");
    expect(keyBetween("a0", "a1") > "a0").toBe(true);
  });
});

import { keyForDrop } from "./sortKey";
describe("keyForDrop", () => {
  it("places a card at the top, between two cards, or at the end", () => {
    const col = ["a0", "a1", "a2"];
    expect(keyForDrop(col, 0) < "a0").toBe(true);
    const mid = keyForDrop(col, 2);
    expect("a1" < mid && mid < "a2").toBe(true);
    expect(keyForDrop(col, 3) > "a2").toBe(true);
    expect(keyForDrop([], 0)).toBe("a0");
  });
  it("does not throw when neighbours share a key", () => {
    const k = keyForDrop(["a1", "a1"], 1);
    expect(k > "a1").toBe(true);
  });
});
