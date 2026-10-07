import { describe, expect, it } from "vitest";
import { dropKey, groupByColumn } from "./board";

const t = (id: string, stateId: string, sortKey: string) => ({ id, stateId, sortKey, identifier: id });

describe("groupByColumn", () => {
  it("puts every column in, sorted by key, and keeps unknown columns", () => {
    const cols = groupByColumn([t("b", "s1", "a1"), t("a", "s1", "a0"), t("c", "s9", "a0")], ["s1", "s2"]);
    expect(cols).toEqual({ s1: ["a", "b"], s2: [], s9: ["c"] });
  });
  it("breaks key ties by identifier so the order is stable", () => {
    expect(groupByColumn([t("Y", "s", "a0"), t("X", "s", "a0")], ["s"]).s).toEqual(["X", "Y"]);
  });
});

describe("dropKey", () => {
  const keys: Record<string, string> = { a: "a0", b: "a1", c: "a2", m: "zz" };
  it("gives the moved card a key between its new neighbours", () => {
    const k = dropKey(["a", "m", "b", "c"], "m", (id) => keys[id]);
    expect("a0" < k && k < "a1").toBe(true);
  });
  it("handles the top and the end of a column", () => {
    expect(dropKey(["m", "a"], "m", (id) => keys[id]) < "a0").toBe(true);
    expect(dropKey(["a", "b", "c", "m"], "m", (id) => keys[id]) > "a2").toBe(true);
  });
});
