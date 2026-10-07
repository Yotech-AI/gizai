import { describe, expect, it } from "vitest";
import { searchItems, type PaletteItem } from "./palette";

const items: PaletteItem[] = [
  { kind: "task", id: "1", label: "Export invoices as CSV", hint: "KADE-41" },
  { kind: "task", id: "2", label: "Café menu translations", hint: "HAV-3" },
  { kind: "project", id: "3", label: "Kade portal", hint: "2026-011" },
  { kind: "client", id: "4", label: "Kade Logistics B.V.", hint: "Amsterdam" },
];

describe("searchItems", () => {
  it("matches label or hint, case- and accent-insensitive, every word", () => {
    expect(searchItems("kade", items).map((i) => i.id)).toEqual(["3", "4", "1"]);
    expect(searchItems("cafe", items).map((i) => i.id)).toEqual(["2"]);
    expect(searchItems("kade 41", items).map((i) => i.id)).toEqual(["1"]);
  });
  it("returns the first items for an empty query, capped", () => {
    expect(searchItems("", items, 2)).toHaveLength(2);
  });
});
