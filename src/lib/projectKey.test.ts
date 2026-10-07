import { describe, expect, it } from "vitest";
import { suggestKey } from "./projectKey";

describe("suggestKey", () => {
  it("uses initials for several words and the first four letters for one", () => {
    expect(suggestKey("Kade Logistics portal")).toBe("KLP");
    expect(suggestKey("Gizai")).toBe("GIZA");
    expect(suggestKey("Café über app")).toBe("CUA");
  });
  it("always starts with a letter and stays within six characters", () => {
    expect(suggestKey("2026 site")).toBe("P2S");
    expect(suggestKey("a b c d e f g h")).toBe("ABCDEF");
    expect(suggestKey("  ")).toBe("");
  });
});
