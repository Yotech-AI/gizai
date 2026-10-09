// GA-60 against GA-59 as merged: the UI's Bitbucket calls, types, scopes and provider helper match the Rust side, read
// as text (Vite's ?raw), so a change on one side without the other fails here.
import { describe, expect, it, vi } from "vitest";
import type { BitbucketStatus } from "./types";
import type { RepoCheck } from "./api";
import { TOKEN_SCOPES } from "./lib/bitbucket";
import { HOST_NAMES, providerOf } from "./lib/provider";
import libRs from "../src-tauri/src/lib.rs?raw";
import commandsRs from "../src-tauri/src/commands.rs?raw";
import appBitbucketRs from "../src-tauri/src/bitbucket.rs?raw";
import gitRs from "../src-tauri/src/git.rs?raw";
import agentsBitbucketRs from "../crates/gizai-agents/src/bitbucket.rs?raw";
import repoUrlRs from "../crates/gizai-core/src/repo_url.rs?raw";

const invoked: [string, unknown][] = [];
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => { invoked.push([cmd, args]); return Promise.resolve(null); } }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

const api = await import("./api");

/** The fields of a Rust struct, camelCased as serde's rename_all = "camelCase" sends them. */
function serdeFields(rs: string, name: string): string[] {
  const body = new RegExp(`#\\[serde\\(rename_all = "camelCase"\\)\\]\\s*pub struct ${name} \\{([\\s\\S]*?)\\n\\}`).exec(rs);
  if (!body) throw new Error(`no camelCase struct ${name}`);
  return [...body[1].matchAll(/^\s*pub (\w+):/gm)].map((m) => m[1].replace(/_(\w)/g, (_, c: string) => c.toUpperCase())).sort();
}

describe("the commands Settings → Bitbucket calls are GA-59's", () => {
  it("are registered with Tauri", async () => {
    invoked.length = 0;
    await api.bitbucketStatus();
    await api.bitbucketSaveLogin("jef@example.com", "ATATT3x");
    await api.bitbucketRemoveLogin();
    await api.bitbucketCheck();
    expect(invoked.map(([cmd]) => cmd)).toEqual(["bitbucket_status", "bitbucket_save_login", "bitbucket_remove_login", "bitbucket_check"]);
    const handler = /generate_handler!\[([\s\S]*?)\]\)/.exec(libRs)?.[1] ?? "";
    for (const [cmd] of invoked) expect(handler, cmd).toMatch(new RegExp(`commands::${cmd}\\b`));
  });
  it("take the arguments the UI sends: bitbucket_save_login an email and a token, the others nothing", () => {
    // Without the app state Tauri passes itself
    const args = (cmd: string) => new RegExp(`pub async fn ${cmd}\\(([^)]*)\\)`).exec(commandsRs)?.[1]
      .replace(/^st: State<'_, AppState>,?/, "").split(",").map((a) => a.trim()).filter(Boolean);
    expect(args("bitbucket_save_login")).toEqual(["email: String", "token: String"]);
    expect(args("bitbucket_status")).toEqual([]);
    expect(args("bitbucket_remove_login")).toEqual([]);
    expect(args("bitbucket_check")).toEqual([]);
    expect(commandsRs).toMatch(/pub async fn bitbucket_check\([^)]*\) -> R<crate::github::ConnectionCheck>/);
  });
});

describe("the types are the JSON GA-59 sends", () => {
  it("BitbucketStatus has the Rust struct's fields, and nothing else", () => {
    // Required<> and the literal's excess-property check (tsc -b in npm run build) make these keys the type's own.
    const full: Required<BitbucketStatus> = { email: "jef@example.com", hasToken: true, account: "jefsev", accountProblem: { what: "x", fix: null } };
    expect(Object.keys(full).sort()).toEqual(serdeFields(appBitbucketRs, "BitbucketStatus"));
  });
  it("RepoCheck has the Rust struct's fields, bitbucket next to github", () => {
    const full: Required<RepoCheck> = { isGit: true, branch: "main", dirty: false, github: null, bitbucket: "https://bitbucket.org/acme/shop", suggestCopy: [] };
    expect(Object.keys(full).sort()).toEqual(serdeFields(gitRs, "RepoCheck"));
  });
});

describe("the texts and the provider helper agree with the backend", () => {
  it("the token line asks for the scopes gizai-agents' bitbucket::SCOPES needs, in its order", () => {
    const scopes = /pub const SCOPES: \[&str; \d+\] = \[([^\]]*)\]/.exec(agentsBitbucketRs)?.[1];
    expect(scopes).toBeTruthy();
    expect(TOKEN_SCOPES).toEqual([...scopes!.matchAll(/"([^"]+)"/g)].map((m) => m[1]));
  });
  it("providerOf reads the links the backend stores, and names them as the backend does", () => {
    const stored = [...repoUrlRs.matchAll(/url: format!\("(https:\/\/[^"]+)"/g)].map((m) => m[1].replace(/\{\w+\}/g, "acme"));
    expect(stored).toEqual(["https://github.com/acme/acme", "https://bitbucket.org/acme/acme"]);
    expect(stored.map(providerOf)).toEqual(["github", "bitbucket"]);
    const names = [...repoUrlRs.matchAll(/"(github|bitbucket)" => Some\("(\w+)"\)/g)].map((m) => [m[1], m[2]]);
    expect(Object.fromEntries(names)).toEqual(HOST_NAMES);
  });
});
