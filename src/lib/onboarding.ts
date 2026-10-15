// The first start's onboarding (GA-164, from GA-168's design: design/onboarding/README.md, which has every text): its
// steps, the texts that depend on what it found, and step 2's coding CLIs, which Next adds to Settings → Coding CLIs.
// The screens are src/pages/OnboardingPage.tsx; the commands are GA-162's (api.ts).
import { CLAUDE_CODE } from "./clis";
import type { Cli, CliStatus } from "../types";

/** The step indicator's names, in order. */
export const STEPS = ["You and your team", "Coding CLIs", "MCP servers", "Ready"] as const;

/** The welcome chat's first message, yours, word for word: finishing sends it (`gizai_core::onboarding::WELCOME`), and
 *  step 4 shows it first, so nothing is sent in your name unseen. */
export const WELCOME = "Hi! I just installed GizAi and set up my team. Please welcome me, tell me in a few lines what you and the team can do, and ask whether I'd like help setting up agents, clients, projects, tasks or anything else.";

/** Where to read how to install Claude Code (the page docs/install.md links). */
export const CLAUDE_CODE_DOCS = "https://docs.claude.com/en/docs/claude-code";

/** The coding CLIs GizAi knows how to find besides the built-in Claude Code: `KNOWN` in src-tauri/src/clis.rs. */
export const KNOWN_CLIS = ["Codex", "Gemini", "OpenCode", "Cursor Agent", "Crush"];

/** "a", "a and b", "a, b and c". */
export function joinAnd(names: string[]): string {
  return names.length < 2 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** A CLI's key in step 2: its name in lower case (Settings → Coding CLIs gives each its own name). */
export const cliKey = (c: { name: string }) => c.name.trim().toLowerCase();

const bare = (c: CliStatus): Cli => ({ id: c.id, name: c.name, kind: c.kind, command: c.command, env: c.env, args: c.args });

/** A row of step 2's list: a CLI found on this computer. */
export type CliRow = {
  key: string;
  cli: Cli;
  /** The program found. */
  program: string;
  /** Claude Code: always in Settings → Coding CLIs, so its tick stays on. */
  builtIn: boolean;
  /** In Settings → Coding CLIs before the onboarding (Codex, which the seed adds when Claude Code isn't installed): it
   *  stays, so its tick stays on too. */
  kept: boolean;
  /** Why it can't run although it is found (on Windows, Claude Code without Git Bash). */
  problem?: string | null;
};

/** Step 2's rows: the CLIs in Settings → Coding CLIs whose program is found (`listed`: list_clis) and the ones found but not
 *  listed yet (`found`: find_clis). `before`: the ids listed before the onboarding. Claude Code first, then those listed
 *  before, then the others in `KNOWN_CLIS`' order, so a row stays where it was when Next has added it. */
export function cliRows(listed: CliStatus[], found: Cli[], before: string[]): CliRow[] {
  const rows: CliRow[] = listed.filter((c) => !!c.path).map((c) => ({
    key: cliKey(c), cli: bare(c), program: c.path ?? c.command, builtIn: c.id === CLAUDE_CODE,
    kept: c.id !== CLAUDE_CODE && before.includes(c.id), problem: c.problem,
  }));
  for (const c of found) if (!rows.some((r) => r.key === cliKey(c))) rows.push({ key: cliKey(c), cli: c, program: c.command, builtIn: false, kept: false });
  const order = (r: CliRow) => (r.builtIn ? -2 : r.kept ? -1 : KNOWN_CLIS.findIndex((n) => n.toLowerCase() === r.key) + 1 || KNOWN_CLIS.length + 1);
  return rows.sort((a, b) => order(a) - order(b));
}

/** Whether a row is ticked: Claude Code and the CLIs listed before always; the others unless you unticked them. */
export const isTicked = (row: CliRow, unticked: string[]) => row.builtIn || row.kept || !unticked.includes(row.key);

/** Whether Claude Code is installed: list_clis found its program. */
export const claudeCodeFound = (listed: CliStatus[]) => !!listed.find((c) => c.id === CLAUDE_CODE)?.path;

/** What Next on step 2 saves in Settings → Coding CLIs (save_clis takes the whole list): what is listed now, but the ones
 *  this onboarding added and you unticked since, plus the ticked ones found. Null when that changes nothing. */
export function clisToSave(listed: CliStatus[], found: Cli[], before: string[], unticked: string[]): Cli[] | null {
  const now = listed.filter((c) => c.id !== CLAUDE_CODE);
  const keep = now.filter((c) => before.includes(c.id) || !unticked.includes(cliKey(c))).map(bare);
  const add = found.filter((c) => !unticked.includes(cliKey(c)) && !now.some((l) => cliKey(l) === cliKey(c)));
  return add.length === 0 && keep.length === now.length ? null : [...keep, ...add];
}

/** Step 2's CLIs once Next has saved the list (`listed`, as save_clis gives it): one it took out again, which you unticked,
 *  is still found, to tick again. */
export function afterSave(prev: { listed: CliStatus[]; found: Cli[] }, listed: CliStatus[]): { listed: CliStatus[]; found: Cli[] } {
  const out = prev.listed.filter((c) => c.id !== CLAUDE_CODE && !listed.some((n) => n.id === c.id) && !prev.found.some((f) => cliKey(f) === cliKey(c)));
  return { listed, found: [...prev.found, ...out.map((c) => ({ ...bare(c), id: "" }))] };
}

/** The CLI the agents get unless you pick another: Claude Code when it is found, else the first one ticked. */
export function defaultAgentsCli(rows: CliRow[], unticked: string[]): string | null {
  const ticked = rows.filter((r) => isTicked(r, unticked));
  return (ticked.find((r) => r.builtIn) ?? ticked[0])?.key ?? null;
}

/** Under step 2's list: the known CLIs not found, and where to add one later. */
export function notFoundLine(rows: CliRow[]): string {
  const missing = KNOWN_CLIS.filter((name) => !rows.some((r) => r.key === name.toLowerCase()));
  const later = "You can add a CLI later in Settings → Coding CLIs.";
  return missing.length ? `Not found: ${joinAnd(missing)}. ${later}` : later;
}

/** Beside step 3's list head: how many servers are ticked and what Next does with them. */
export function tickedLine(n: number): string {
  return n === 0 ? "None ticked: Next goes on" : n === 1 ? "1 ticked: Next imports it" : `${n} ticked: Next imports them`;
}

/** Why there is no Team Lead chat after finishing (finish_onboarding gave no chat), shown above the board. */
export function noChatNote(claudeCode: boolean): string {
  return claudeCode
    ? "The Team Lead's chat couldn't start. You can start one on the Chat page."
    : "There's no Team Lead chat yet: it needs Claude Code, which isn't installed. Once it is, start one on the Chat page.";
}
