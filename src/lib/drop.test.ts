import { describe, expect, it } from "vitest";
import { dropHits } from "./drop";
const rect = { left: 100, top: 50, right: 300, bottom: 150 };
describe("dropHits", () => {
  it("converts physical pixels to CSS pixels before testing the rectangle", () => {
    expect(dropHits({ x: 400, y: 200 }, 2, rect)).toBe(true);   // (200, 100) in CSS px
    expect(dropHits({ x: 400, y: 200 }, 1, rect)).toBe(false);  // (400, 200) is outside
  });
  it("counts the edges as inside and ignores a missing position", () => {
    expect(dropHits({ x: 100, y: 50 }, 1, rect)).toBe(true);
    expect(dropHits(null, 1, rect)).toBe(false);
  });
});
