// GA-164 QA: the onboarding's rules for step 2's coding CLIs (the rows, what Next saves with save_clis, the agents' CLI)
// and its texts. The welcome message is the backend's word for word (gizai_core::onboarding::WELCOME, which
// finish_onboarding sends), the known CLIs are src-tauri's KNOWN, and the texts are GA-168's (design/onboarding/README.md).
// Files are read from disk; node:fs comes in through a computed specifier, as in band.test.ts.
import { describe, expect, it } from "vitest";
import type { Cli, CliStatus } from "../types";
import { afterSave, CLAUDE_CODE_DOCS, claudeCodeFound, cliRows, clisToSave, defaultAgentsCli, isTicked, joinAnd, KNOWN_CLIS, noChatNote,
  notFoundLine, STEPS, tickedLine, WELCOME } from "./onboarding";

const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const read = (path: string): string => fs.readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

const cli = (id: string, name: string, kind: Cli["kind"], command: string): Cli => ({ id, name, kind, command, env: [], args: "" });
const status = (c: Cli, path: string | null = c.command): CliStatus => ({ ...c, path, problem: null });
const CLAUDE = status(cli("claude_code", "Claude Code", "claude_code", "claude"), "/home/me/.local/bin/claude");
const NO_CLAUDE = status(cli("claude_code", "Claude Code", "claude_code", "claude"), null);
const CODEX = cli("", "Codex", "codex", "/bin/codex");
const GEMINI = cli("", "Gemini", "gemini", "/bin/gemini");
const CRUSH = cli("", "Crush", "other", "/bin/crush");

describe("step 2's rows", () => {
  it("Claude Code first and built in, then the CLIs listed before, then the ones found in KNOWN's order; a listed CLI not found is left out", () => {
    const listedCodex = status(cli("c1", "Codex", "codex", "/bin/codex"));
    const listedGone = status(cli("c2", "Gemini", "gemini", "gemini"), null);
    const rows = cliRows([CLAUDE, listedGone, listedCodex], [CRUSH, cli("", "OpenCode", "other", "/bin/opencode")], ["c1", "c2"]);
    expect(rows.map((r) => [r.cli.name, r.builtIn, r.kept, r.program])).toEqual([
      ["Claude Code", true, false, "/home/me/.local/bin/claude"],
      ["Codex", false, true, "/bin/codex"],
      ["OpenCode", false, false, "/bin/opencode"],
      ["Crush", false, false, "/bin/crush"],
    ]);
    // Claude Code and the ones listed before stay ticked; the others unless unticked
    expect(rows.map((r) => isTicked(r, ["codex", "crush", "claude code"]))).toEqual([true, true, true, false]);
  });

  it("without Claude Code there is no Claude Code row, and claudeCodeFound says so", () => {
    expect(cliRows([NO_CLAUDE], [CODEX], []).map((r) => r.cli.name)).toEqual(["Codex"]);
    expect(claudeCodeFound([NO_CLAUDE])).toBe(false);
    expect(claudeCodeFound([CLAUDE])).toBe(true);
    expect(claudeCodeFound([])).toBe(false);
  });

  it("the agents' CLI defaults to Claude Code when it is found, else the first ticked, else none", () => {
    expect(defaultAgentsCli(cliRows([CLAUDE], [CODEX, GEMINI], []), [])).toBe("claude code");
    expect(defaultAgentsCli(cliRows([NO_CLAUDE], [CODEX, GEMINI], []), [])).toBe("codex");
    expect(defaultAgentsCli(cliRows([NO_CLAUDE], [CODEX, GEMINI], []), ["codex"])).toBe("gemini");
    expect(defaultAgentsCli(cliRows([NO_CLAUDE], [CODEX], []), ["codex"])).toBeNull();
  });
});

describe("what Next on step 2 saves (save_clis takes the whole list, without the built-in Claude Code)", () => {
  it("the ticked ones found, added to what is listed", () => {
    const listed = [CLAUDE, status(cli("c1", "OpenCode", "other", "/bin/opencode"))];
    expect(clisToSave(listed, [CODEX, GEMINI], ["c1"], ["gemini"])).toEqual([cli("c1", "OpenCode", "other", "/bin/opencode"), CODEX]);
  });

  it("nothing when nothing changes: none found, or every one found unticked", () => {
    expect(clisToSave([CLAUDE], [], [], [])).toBeNull();
    expect(clisToSave([CLAUDE], [CODEX], [], ["codex"])).toBeNull();
  });

  it("a CLI the onboarding added and you unticked since goes out again; one listed before the onboarding stays", () => {
    const codex = status(cli("c9", "Codex", "codex", "/bin/codex"));
    const kept = status(cli("c1", "Gemini", "gemini", "/bin/gemini"));
    expect(clisToSave([CLAUDE, kept, codex], [], ["c1"], ["codex", "gemini"])).toEqual([cli("c1", "Gemini", "gemini", "/bin/gemini")]);
  });

  it("after a save that took one out, that one is found again with no id, to tick again", () => {
    const codex = status(cli("c9", "Codex", "codex", "/bin/codex"));
    const after = afterSave({ listed: [CLAUDE, codex], found: [] }, [CLAUDE]);
    expect(after.listed).toEqual([CLAUDE]);
    expect(after.found).toEqual([cli("", "Codex", "codex", "/bin/codex")]);
    expect(cliRows(after.listed, after.found, []).map((r) => r.cli.name)).toEqual(["Claude Code", "Codex"]);
  });
});

describe("the texts", () => {
  it("the welcome message is the one finish_onboarding sends (gizai_core::onboarding::WELCOME), and GA-164's word for word", () => {
    const rust = /pub const WELCOME: &str = "([^"]+)";/.exec(read("crates/gizai-core/src/onboarding.rs"))?.[1];
    expect(WELCOME).toBe(rust);
    expect(WELCOME).toBe("Hi! I just installed GizAi and set up my team. Please welcome me, tell me in a few lines what you and the team can do, and ask whether I'd like help setting up agents, clients, projects, tasks or anything else.");
  });

  it("the known CLIs are src-tauri's KNOWN, in its order", () => {
    const known = [...read("src-tauri/src/clis.rs").matchAll(/\("[\w-]+", "\w+", "([^"]+)", "[^"]*"\)/g)].map((m) => m[1]);
    expect(KNOWN_CLIS).toEqual(known);
  });

  it("the steps, the docs link and the lines that depend on what was found are GA-168's", () => {
    expect(STEPS).toEqual(["You and your team", "Coding CLIs", "MCP servers", "Ready"]);
    expect(CLAUDE_CODE_DOCS).toBe("https://docs.claude.com/en/docs/claude-code");
    expect(read("docs/install.md")).toContain(CLAUDE_CODE_DOCS);
    expect(notFoundLine(cliRows([CLAUDE], [cli("", "Codex", "codex", "/c"), cli("", "Gemini", "gemini", "/g"), cli("", "OpenCode", "other", "/o")], [])))
      .toBe("Not found: Cursor Agent and Crush. You can add a CLI later in Settings → Coding CLIs.");
    expect(notFoundLine(cliRows([NO_CLAUDE], [cli("", "Codex", "codex", "/c"), cli("", "Gemini", "gemini", "/g")], [])))
      .toBe("Not found: OpenCode, Cursor Agent and Crush. You can add a CLI later in Settings → Coding CLIs.");
    expect(notFoundLine(cliRows([CLAUDE], KNOWN_CLIS.map((n) => cli("", n, "other", `/${n}`)), []))).toBe("You can add a CLI later in Settings → Coding CLIs.");
    expect([0, 1, 2].map(tickedLine)).toEqual(["None ticked: Next goes on", "1 ticked: Next imports it", "2 ticked: Next imports them"]);
    expect([[], ["a"], ["a", "b"], ["a", "b", "c"]].map(joinAnd)).toEqual(["", "a", "a and b", "a, b and c"]);
    expect(noChatNote(false)).toContain("needs Claude Code");
  });

  it("every text the design marks GA-164's, New or Changed for the four steps is in the onboarding, word for word", () => {
    const page = read("src/pages/OnboardingPage.tsx") + read("src/lib/onboarding.ts");
    const readme = read("design/onboarding/README.md");
    // the texts tables of steps 1 to 4 and On every step: | Where | Text | Mark |, with the Text column taken word for word
    const from = readme.indexOf("## Every text, word for word");
    const section = readme.slice(from, readme.indexOf("### Right after: the Chat page", from));
    const texts = [...section.matchAll(/^\| ([^|]+) \| ([^|]+) \| (GA-164|New|Changed)[^|]*\|$/gm)]
      .filter(([, where]) => !/not drawn|\(2, 3, 4 likewise\)/.test(where))
      .map(([, , text]) => text.trim().replace(/\*\*/g, ""))
      // a row naming several texts, or showing an example's shape, is checked by the page's tests instead
      .filter((t) => !/^(1, 2, 3, 4|Step 1 of 4.*|2 ticked: Next imports them.*|Your name · Team · .*|Not found: .*|Your Claude Code has no MCP servers yet\..*|Claude Code · Built in|Back to step 1: You and your team)$/.test(t));
    expect(texts.length).toBeGreaterThan(25);
    const missing = texts.filter((t) => !page.includes(t));
    expect(missing).toEqual([]);
  });
});
