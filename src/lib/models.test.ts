import { describe, expect, it } from "vitest";
import { EFFORTS, effortChoices, findModel, modelHint } from "./models";
import type { ModelOption } from "../types";

const all = ["low", "medium", "high", "xhigh", "max"];
const M: ModelOption[] = [
  { value: "default", resolvedModel: "claude-opus-5-5", displayName: "Default (recommended)", description: "Use the default model (currently Opus 5.5) · $4/$20 per Mtok", supportsEffort: true, effortLevels: all },
  { value: "opus", resolvedModel: "claude-opus-5-5", displayName: "Opus", description: "Opus 5.5 · Best for everyday, complex tasks · $4/$20 per Mtok", supportsEffort: true, effortLevels: all },
  { value: "sonnet", resolvedModel: "claude-sonnet-5-5", displayName: "Sonnet", description: "Sonnet 5.5 · Efficient for routine tasks · $2/$10 per Mtok", supportsEffort: true, effortLevels: all },
  { value: "haiku", resolvedModel: "claude-haiku-4-5-20251001", displayName: "Haiku", description: "Haiku 4.5 · Fastest for quick answers · $1/$5 per Mtok", supportsEffort: false, effortLevels: [] },
];

describe("findModel", () => {
  it("matches an alias or a full id, ignoring case; empty is Claude Code's default", () => {
    expect(findModel("opus", M)?.displayName).toBe("Opus");
    expect(findModel("Claude-Sonnet-5-5", M)?.value).toBe("sonnet");
    expect(findModel("", M)?.value).toBe("default");
    expect(findModel(null, M)?.value).toBe("default");
  });
  it("finds nothing for a name Claude Code doesn't offer", () => {
    expect(findModel("opus 5.5", M)).toBeUndefined();
    expect(findModel("sonnet 5.5", M)).toBeUndefined();
  });
});

describe("effortChoices", () => {
  it("are the model's own levels", () => {
    expect(effortChoices("opus", M)).toEqual(all);
    expect(effortChoices("haiku", M)).toEqual([]);
  });
  it("are all levels when the model or the list is unknown", () => {
    expect(effortChoices("something-new", M)).toEqual(EFFORTS);
    expect(effortChoices("opus", null)).toEqual(EFFORTS);
  });
});

describe("modelHint", () => {
  it("describes the chosen model, or says a saved name won't work", () => {
    expect(modelHint("sonnet", M)).toBe("Sonnet 5.5 · Efficient for routine tasks · $2/$10 per Mtok");
    expect(modelHint("opus 5.5", M)).toContain("Claude Code doesn't offer");
  });
});
