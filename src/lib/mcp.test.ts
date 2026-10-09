// GA-39: the helpers behind Settings → MCP servers and the agent form's Tools: risk lines, the npm/npx warning, the
// switches per server and tool, the last-run and sign-in badges, secret lines that never carry a saved value, and names.
import { describe, expect, it } from "vitest";
import {
  actsAsYou, cleanSwitches, lastRunLabel, linesOf, nameProblem, npmWarning, parseArgs, RISK_BADGE, RISK_LABEL, sameSwitches, secretLines,
  serverWhere, signInLabel, switchServer, switchTool, TAKEN_NAMES, toolsSummary, TRANSPORT_LABEL,
} from "./mcp";
import type { AgentServer, McpToolView } from "../types";
// The Rust side, as text (Vite's ?raw), to check the shared wording.
import appMcpRs from "../../src-tauri/src/mcp_servers.rs?raw";
import coreMcpRs from "../../crates/gizai-core/src/mcp_servers.rs?raw";

const tool = (name: string, risk: string): McpToolView => ({
  name, description: "", params: [], hints: { readOnly: false, destructive: false, idempotent: false, openWorld: false }, hintsSent: [],
  risk, summary: "", notes: [],
});
const SOURCES: Record<string, string> = { "src-tauri/src/mcp_servers.rs": appMcpRs, "crates/gizai-core/src/mcp_servers.rs": coreMcpRs };
const rust = (path: string) => SOURCES[path].replace(/\\\n\s*/g, "");

describe("toolsSummary", () => {
  it("says a server whose tools aren't listed yet is Not listed yet, and how to list them", () => {
    expect(toolsSummary([])).toEqual({ risk: "unknown", text: "Its tools aren't listed yet: List tools in Settings → MCP servers shows them." });
    expect(RISK_LABEL.unknown).toBe("Not listed yet");
  });

  it("says in one line how many tools, what they may do and the highest risk", () => {
    expect(toolsSummary([tool("search", "low")])).toEqual({ risk: "low", text: "1 tool: 1 only read. Low risk." });
    expect(toolsSummary([tool("a", "low"), tool("b", "low"), tool("c", "medium")]))
      .toEqual({ risk: "medium", text: "3 tools: 2 only read, 1 change things. Medium risk." });
    expect(toolsSummary([tool("a", "low"), tool("b", "medium"), tool("c", "high"), tool("d", "high")]))
      .toEqual({ risk: "high", text: "4 tools: 1 only read, 1 change things, 2 may delete or overwrite. High risk." });
  });

  it("gives every risk a label and a badge", () => {
    for (const r of ["low", "medium", "high", "unknown"]) {
      expect(RISK_LABEL[r], r).toBeTruthy();
      expect(RISK_BADGE[r], r).toBeTruthy();
    }
    expect([RISK_BADGE.low, RISK_BADGE.medium, RISK_BADGE.high]).toEqual(["ok", "warn", "fail"]);
  });

  it("uses the same words as summary in src-tauri/src/mcp_servers.rs", () => {
    const src = rust("src-tauri/src/mcp_servers.rs");
    expect(src).toContain(toolsSummary([]).text);
    for (const w of ["only read", "change things", "may delete or overwrite"]) expect(src).toContain(w);
  });
});

describe("npmWarning", () => {
  const TEXT = "This agent may run npm or npx, and an MCP server is on: a server's answer could try to make it run code. Take npm and npx out of its commands, or switch the server off.";

  it("warns when a server is on and the agent may run Bash(npm:*) or Bash(npx:*)", () => {
    expect(npmWarning(["Bash(npm:*)"], true)).toBe(TEXT);
    expect(npmWarning(["Bash(git:*)", "Bash(npx:*)"], true)).toBe(TEXT);
    expect(npmWarning(["Bash(npm)"], true)).toBe(TEXT);
    expect(npmWarning(["Bash( npm :*)"], true)).toBe(TEXT);
  });

  it("says nothing while every server is off", () => {
    expect(npmWarning(["Bash(npm:*)", "Bash(npx:*)"], false)).toBeNull();
  });

  it("says nothing for narrower npm commands or other commands", () => {
    expect(npmWarning(["Bash(npm run build:*)"], true)).toBeNull();
    expect(npmWarning(["Bash(git:*)"], true)).toBeNull();
    expect(npmWarning([], true)).toBeNull();
  });

  it("is the same text as npm_warning in src-tauri/src/mcp_servers.rs", () => {
    expect(rust("src-tauri/src/mcp_servers.rs")).toContain(TEXT);
  });
});

describe("switches per server and tool", () => {
  it("switches a server on for the first time with all its tools on, and off again keeping its tools", () => {
    const on = switchServer([], "s1", true);
    expect(on).toEqual([{ serverId: "s1", on: true, toolsOff: [] }]);
    const withOff = switchTool(on, "s1", "delete", false);
    expect(switchServer(withOff, "s1", false)).toEqual([{ serverId: "s1", on: false, toolsOff: ["delete"] }]);
  });

  it("leaves the other servers as they are", () => {
    const list: AgentServer[] = [{ serverId: "a", on: true, toolsOff: ["x"] }, { serverId: "b", on: false, toolsOff: [] }];
    expect(switchServer(list, "b", true)).toEqual([{ serverId: "a", on: true, toolsOff: ["x"] }, { serverId: "b", on: true, toolsOff: [] }]);
  });

  it("switches a tool off and on again, keeping the list sorted and without doubles", () => {
    let l = switchServer([], "s1", true);
    l = switchTool(l, "s1", "write", false);
    l = switchTool(l, "s1", "delete", false);
    l = switchTool(l, "s1", "delete", false);
    expect(l).toEqual([{ serverId: "s1", on: true, toolsOff: ["delete", "write"] }]);
    expect(switchTool(l, "s1", "delete", true)).toEqual([{ serverId: "s1", on: true, toolsOff: ["write"] }]);
  });

  it("a tool switched off on a server not in the list yet keeps the server off", () => {
    expect(switchTool([], "s1", "delete", false)).toEqual([{ serverId: "s1", on: false, toolsOff: ["delete"] }]);
  });

  it("saves only the switches that say something, tools off sorted", () => {
    expect(cleanSwitches([{ serverId: "a", on: false, toolsOff: [] }, { serverId: "b", on: true, toolsOff: ["z", "a"] }, { serverId: "c", on: false, toolsOff: ["q"] }]))
      .toEqual([{ serverId: "b", on: true, toolsOff: ["a", "z"] }, { serverId: "c", on: false, toolsOff: ["q"] }]);
  });

  it("sees no change for the same switches in another order or with servers off and empty", () => {
    const a: AgentServer[] = [{ serverId: "a", on: true, toolsOff: ["y", "x"] }, { serverId: "b", on: true, toolsOff: [] }];
    const b: AgentServer[] = [{ serverId: "z", on: false, toolsOff: [] }, { serverId: "b", on: true, toolsOff: [] }, { serverId: "a", on: true, toolsOff: ["x", "y"] }];
    expect(sameSwitches(a, b)).toBe(true);
    expect(sameSwitches([], [{ serverId: "a", on: false, toolsOff: [] }])).toBe(true);
    expect(sameSwitches([], switchServer([], "a", true))).toBe(false);
    expect(sameSwitches(a, switchTool(a, "b", "t", false))).toBe(false);
  });
});

describe("badges", () => {
  it("says the state in the agent's last run per server", () => {
    expect(lastRunLabel("connected")).toEqual({ text: "Connected in its last run", badge: "ok" });
    expect(lastRunLabel("failed")).toEqual({ text: "Failed to connect in its last run", badge: "fail" });
    expect(lastRunLabel("needs-auth")).toEqual({ text: "Needed sign-in in its last run", badge: "needs" });
    expect(lastRunLabel("pending")).toEqual({ text: "Still connecting when its last run started", badge: "warn" });
    expect(lastRunLabel("disabled")).toEqual({ text: "Failed to connect in its last run (disabled)", badge: "fail" });
    expect(lastRunLabel("")).toEqual({ text: "Failed to connect in its last run", badge: "fail" });
  });

  it("says Signed in or Needs sign-in, and nothing for a server without a sign-in", () => {
    expect(signInLabel("signed_in")).toEqual({ text: "Signed in", badge: "ok" });
    expect(signInLabel("needs_sign_in")).toEqual({ text: "Needs sign-in", badge: "needs" });
    expect(signInLabel("")).toBeNull();
  });

  it("says next to a signed-in server that its tools act as you in that service, as the backend's acts_as_you does", () => {
    expect(actsAsYou({ signIn: "signed_in", name: "otus" })).toBe("Signed in: its tools act as you in otus.");
    expect(actsAsYou({ signIn: "needs_sign_in", name: "otus" })).toBeNull();
    expect(actsAsYou({ signIn: "", name: "otus" })).toBeNull();
    expect(rust("src-tauri/src/mcp_servers.rs")).toContain('format!("Signed in: its tools act as you in {}.", v.server.name)');
  });

  it("names each kind of server", () => {
    expect(TRANSPORT_LABEL).toEqual({ stdio: "Command", http: "Address (HTTP)", sse: "Address (SSE)" });
  });
});

describe("the server form's lines", () => {
  it("shows a command with its arguments, or an address", () => {
    expect(serverWhere({ transport: "stdio", command: "npx", args: ["-y", "@acme/mcp"], url: "" })).toBe("npx -y @acme/mcp");
    expect(serverWhere({ transport: "http", command: "", args: [], url: "https://os.example.com/api/mcp" })).toBe("https://os.example.com/api/mcp");
  });

  it("takes one argument per line, trimmed, without empty lines", () => {
    expect(parseArgs(" -y \n\n@acme/mcp-server\n  ")).toEqual(["-y", "@acme/mcp-server"]);
    expect(parseArgs("")).toEqual([]);
  });

  it("loads a saved server's lines as names only: no value, saved unless missing", () => {
    expect(linesOf(["ACME_TOKEN", "ACME_URL"], ["ACME_URL"])).toEqual([
      { name: "ACME_TOKEN", value: "", saved: true }, { name: "ACME_URL", value: "", saved: false }]);
    expect(linesOf([])).toEqual([]);
  });

  it("saves a value only when typed in now: null keeps the saved one; an empty name is dropped", () => {
    expect(secretLines([
      { name: " ACME_TOKEN ", value: "", saved: true },
      { name: "X_NEW", value: "s3cret", saved: false },
      { name: "  ", value: "orphan", saved: false },
    ])).toEqual([{ name: "ACME_TOKEN", value: null }, { name: "X_NEW", value: "s3cret" }]);
  });

  it("typing replaces a saved value", () => {
    const [saved] = linesOf(["ACME_TOKEN"]);
    expect(secretLines([{ ...saved, value: "new-value" }])).toEqual([{ name: "ACME_TOKEN", value: "new-value" }]);
  });
});

describe("nameProblem", () => {
  it("takes letters, digits, - and _ up to 64", () => {
    expect(nameProblem("otus_os-2", [])).toBeNull();
    expect(nameProblem("  otus  ", [])).toBeNull();
    expect(nameProblem("a".repeat(64), [])).toBeNull();
    expect(nameProblem("a".repeat(65), [])).toMatch(/^A name takes letters, digits, - and _ \(at most 64\)/);
    expect(nameProblem("otus os", [])).toBe('A name takes letters, digits, - and _ (at most 64), not "otus os"');
    expect(nameProblem("", [])).toBe("A name takes letters, digits, - and _ (at most 64)");
  });

  it("keeps Gizai's own names, in any case, as in TAKEN in crates/gizai-core", () => {
    expect(TAKEN_NAMES).toEqual(["gizai", "chrome-devtools"]);
    expect(rust("crates/gizai-core/src/mcp_servers.rs")).toContain('pub const TAKEN: [&str; 2] = ["gizai", "chrome-devtools"];');
    expect(nameProblem("Gizai", [])).toBe("Gizai is Gizai's own: give the server another name");
    expect(nameProblem("chrome-devtools", [])).toBe("chrome-devtools is Gizai's own: give the server another name");
  });

  it("asks for another name when one in the list has it, in any case", () => {
    expect(nameProblem("Otus", ["otus"])).toBe("There is already an MCP server called Otus: give this one another name");
    expect(nameProblem("otus2", ["otus"])).toBeNull();
  });
});
