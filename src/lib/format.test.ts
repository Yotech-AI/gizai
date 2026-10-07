import { describe, expect, it } from "vitest";
import { relTime } from "./format";

describe("relTime", () => {
  it("formats minutes, hours, days", () => {
    const now = 1_000_000_000_000;
    expect(relTime(now - 30_000, now)).toBe("just now");
    expect(relTime(now - 5 * 60_000, now)).toBe("5m ago");
    expect(relTime(now - 3 * 3_600_000, now)).toBe("3h ago");
    expect(relTime(now - 26 * 3_600_000, now)).toBe("yesterday");
  });
});

import { initials } from "./format";
describe("initials", () => {
  it("takes first and last word, uppercases, survives odd input", () => {
    expect(initials("Jeffrey")).toBe("JE");
    expect(initials("Marloes van der Visser")).toBe("MV");
    expect(initials("  zoë  ëlbers ")).toBe("ZË");
    expect(initials("")).toBe("?");
  });
});

import { companyInitials } from "./format";
describe("companyInitials", () => {
  it("uses the first two words and ignores legal forms", () => {
    expect(companyInitials("Kade Logistics B.V.")).toBe("KL");
    expect(companyInitials("De Groene Fiets")).toBe("DG");
    expect(companyInitials("Mees GmbH")).toBe("ME");
    expect(companyInitials("")).toBe("?");
  });
});

import { formatBytes } from "./format";
describe("formatBytes", () => {
  it("uses B, KB, MB and GB with one decimal under 10", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(20 * 1024)).toBe("20 KB");
    expect(formatBytes(5.25 * 1024 * 1024)).toBe("5.3 MB");
    expect(formatBytes(3 * 1024 ** 3)).toBe("3.0 GB");
  });
});
