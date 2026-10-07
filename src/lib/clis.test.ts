// Coding CLIs in the UI (GA-3): what each kind offers in the agent form, and the Settings rows.
import { describe, expect, it } from "vitest";
import { draftFrom, inputFrom } from "./agents";
import { CLAUDE_CODE, cliName, cliSummary, defaultMode, EFFORTS_BY_KIND, kindOf, modeFor, parseEnv, PERMISSIONS, RISKY, usesAllowedTools } from "./clis";
import type { Cli, Member } from "../types";

const clis: Cli[] = [
  { id: CLAUDE_CODE, name: "Claude Code", kind: "claude_code", command: "", env: [], args: "" },
  { id: "c1", name: "Codex", kind: "codex", command: "codex", env: [], args: "" },
  { id: "c2", name: "Claude Code (2nd account)", kind: "claude_code", command: "claude", env: ["CLAUDE_CONFIG_DIR=~/.claude-2"], args: "" },
  { id: "c3", name: "OpenCode", kind: "other", command: "opencode", env: [], args: "run -m {model} {prompt}" },
];

describe("coding CLIs", () => {
  it("finds the kind of an agent's CLI; no CLI is Claude Code", () => {
    expect(kindOf(null, clis)).toBe("claude_code");
    expect(kindOf("c1", clis)).toBe("codex");
    expect(kindOf("c2", clis)).toBe("claude_code");
    expect(kindOf("c3", clis)).toBe("other");
    expect(kindOf(CLAUDE_CODE, null)).toBe("claude_code");
    expect(kindOf("gone", clis)).toBe("other");
  });

  it("offers each kind's own permission modes, the safe one first, and marks the risky ones", () => {
    expect(defaultMode("claude_code")).toBe("acceptEdits");
    expect(defaultMode("codex")).toBe("workspace-write");
    expect(defaultMode("gemini")).toBe("auto_edit");
    expect(defaultMode("other")).toBe("");
    for (const k of ["claude_code", "codex", "gemini"] as const) {
      expect(Object.keys(PERMISSIONS[k]).filter((m) => RISKY.has(m))).toHaveLength(1);
      expect(RISKY.has(defaultMode(k))).toBe(false);
    }
  });

  it("keeps a mode that fits the new CLI and else takes its default", () => {
    expect(modeFor("codex", "acceptEdits")).toBe("workspace-write");
    expect(modeFor("codex", "read-only")).toBe("read-only");
    expect(modeFor("claude_code", "workspace-write")).toBe("acceptEdits");
    expect(modeFor("gemini", "plan")).toBe("plan");
    expect(modeFor("other", "acceptEdits")).toBe("");
  });

  it("has the same effort levels as the backend", () => {
    expect(EFFORTS_BY_KIND.claude_code).toEqual(["low", "medium", "high", "xhigh", "max"]);
    expect(EFFORTS_BY_KIND.codex).toEqual(["minimal", "low", "medium", "high", "xhigh"]);
    expect(EFFORTS_BY_KIND.gemini).toEqual([]);
    expect(EFFORTS_BY_KIND.other).toEqual([]);
    expect(usesAllowedTools("gemini") && usesAllowedTools("claude_code")).toBe(true);
    expect(usesAllowedTools("codex") || usesAllowedTools("other")).toBe(false);
  });

  it("reads environment lines and says what a CLI runs", () => {
    expect(parseEnv(" CLAUDE_CONFIG_DIR=~/.claude-2 \n\n  A=b ")).toEqual(["CLAUDE_CONFIG_DIR=~/.claude-2", "A=b"]);
    expect(cliSummary({ ...clis[2], path: "/home/u/.local/bin/claude" })).toBe("CLAUDE_CONFIG_DIR=~/.claude-2 /home/u/.local/bin/claude");
    expect(cliSummary(clis[3])).toBe("opencode run -m {model} {prompt}");
    expect(cliSummary({ ...clis[1], args: "ignored" })).toBe("codex");
    expect(cliSummary(clis[0])).toBe("found when needed");
  });

  it("names an agent's CLI", () => {
    expect(cliName(null, clis)).toBe("Claude Code");
    expect(cliName("c2", clis)).toBe("Claude Code (2nd account)");
    expect(cliName("c1", null)).toBe("Coding CLI");
    expect(cliName("gone", clis)).toBe("Unknown CLI");
    expect(cliName(undefined, undefined)).toBe("Claude Code");
  });

  it("the agent form keeps the agent's CLI and sends it as its adapter", () => {
    expect(draftFrom(null).cli).toBe(CLAUDE_CODE);
    const m = { name: "Codex Agent", roleKey: "backend", adapter: "c1", allowedTools: [], permissionMode: "read-only" } as unknown as Member;
    const d = draftFrom(m);
    expect(d.cli).toBe("c1");
    expect(inputFrom(d).adapter).toBe("c1");
    expect(inputFrom({ ...d, cli: "" }).adapter).toBe(CLAUDE_CODE);
  });
});
