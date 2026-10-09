// GA-60: the UI's Bitbucket calls match GA-59's contract: the command names and their arguments, as Tauri gets them.
import { describe, expect, it, vi } from "vitest";
import type { BitbucketStatus, ConnectionCheck } from "./types";
import type { RepoCheck } from "./api";

const invoked: [string, unknown][] = [];
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => { invoked.push([cmd, args]); return Promise.resolve(null); } }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

const api = await import("./api");

describe("Bitbucket's commands (GA-59's contract)", () => {
  it("calls bitbucket_status, bitbucket_save_login with { email, token }, bitbucket_remove_login and bitbucket_check", async () => {
    invoked.length = 0;
    await api.bitbucketStatus();
    await api.bitbucketSaveLogin("jef@example.com", "ATATT3x");
    await api.bitbucketRemoveLogin();
    await api.bitbucketCheck();
    expect(invoked).toEqual([
      ["bitbucket_status", undefined],
      ["bitbucket_save_login", { email: "jef@example.com", token: "ATATT3x" }],
      ["bitbucket_remove_login", undefined],
      ["bitbucket_check", undefined],
    ]);
  });
  it("types the answers as the contract says", () => {
    // Type-checked by npm run build (tsc -b also checks the tests): a status with every field, and the least one.
    const full: BitbucketStatus = { email: "jef@example.com", hasToken: true, account: "jefsev", accountProblem: { what: "x", fix: "y" } };
    const least: BitbucketStatus = { hasToken: false };
    const check: Awaited<ReturnType<typeof api.bitbucketCheck>> = { ok: true, pushOver: "ssh", checks: [] } satisfies ConnectionCheck;
    const repo: RepoCheck = { isGit: true, dirty: false, github: null, bitbucket: "https://bitbucket.org/acme/shop" };
    const status: Awaited<ReturnType<typeof api.bitbucketStatus>> = full;
    expect([full, least, check, repo, status].length).toBe(5);
  });
});
