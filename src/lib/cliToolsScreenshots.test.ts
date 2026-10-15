// GA-182: the agent's Take screenshots of what it built (contract CP's CliTools.screenshots). Its Member carries the switch
// (its role's default until set); the agent form keeps it when the agent moves to another CLI, sees a change to it, and
// sends it only when it changed, so an agent that never set it keeps following its role's default.
import { describe, expect, it } from "vitest";
import { draftFrom } from "./agents";
import { cliToolsFrom, cliToolsToSave, fitCliTools, NO_CLI_TOOLS, sameCliTools } from "./cliTools";
import type { CliKind, CliTools, Member } from "../types";

const tools = (over: Partial<CliTools> = {}): CliTools => ({ ...NO_CLI_TOOLS, ...over });

describe("Take screenshots in the agent form's CLI tools", () => {
  it("is off in an empty set, and read as saved from the agent's Member", () => {
    expect(NO_CLI_TOOLS.screenshots).toBe(false);
    expect(cliToolsFrom(null).screenshots).toBe(false);
    expect(cliToolsFrom(tools({ screenshots: true })).screenshots).toBe(true);
    const member = (screenshots: boolean) => ({ actorId: "a1", name: "Frontend Agent", roleKey: "frontend", kind: "agent", isLead: false,
      allowedTools: [], adapter: "claude_code", cliTools: tools({ webSearch: true, screenshots }) }) as unknown as Member;
    expect(draftFrom(member(true)).cliTools).toEqual(tools({ webSearch: true, screenshots: true }));
    expect(draftFrom(member(false)).cliTools.screenshots).toBe(false);
    expect(draftFrom(null).cliTools.screenshots).toBe(false);
  });

  it("stays as it is when the agent moves to another CLI, as the browser waits there too", () => {
    for (const kind of ["claude_code", "codex", "gemini", "other"] as CliKind[]) {
      expect(fitCliTools(tools({ webSearch: true, screenshots: true }), kind).screenshots, kind).toBe(true);
      expect(fitCliTools(tools({ screenshots: false }), kind).screenshots, kind).toBe(false);
    }
    // an Other CLI drops everything else
    expect(fitCliTools(tools({ webSearch: true, builtin: ["Bash"], slashCommands: true, screenshots: true }), "other"))
      .toEqual(tools({ screenshots: true }));
  });

  it("counts as a change of the tools, so Save changes sends them", () => {
    expect(sameCliTools(tools({ screenshots: true }), tools({ screenshots: true }))).toBe(true);
    expect(sameCliTools(tools({ screenshots: true }), tools({ screenshots: false }))).toBe(false);
  });

  it("is sent only when it changed, so an agent that never set it keeps its role's default", () => {
    const was = tools({ webSearch: true, screenshots: true });
    // another switch changed: the CLI tools without screenshots
    const sent = cliToolsToSave(tools({ webSearch: true, webFetch: true, fetchDomains: ["docs.rs"], screenshots: true }), was);
    expect(sent).toEqual({ webSearch: true, webFetch: true, fetchDomains: ["docs.rs"], insecureCerts: false, builtin: [],
      slashCommands: false });
    expect("screenshots" in sent).toBe(false);
    // switched off, and on again from off
    expect(cliToolsToSave(tools({ webSearch: true, screenshots: false }), was).screenshots).toBe(false);
    expect(cliToolsToSave(tools({ screenshots: true }), tools({ screenshots: false })).screenshots).toBe(true);
  });

  it("sends the fetch domains only with fetching on, as before", () => {
    const was = tools();
    expect(cliToolsToSave(tools({ webFetch: false, fetchDomains: ["docs.rs"] }), was).fetchDomains).toEqual([]);
    expect(cliToolsToSave(tools({ webFetch: true, fetchDomains: ["docs.rs", "crates.io"] }), was).fetchDomains).toEqual(["docs.rs", "crates.io"]);
  });
});
