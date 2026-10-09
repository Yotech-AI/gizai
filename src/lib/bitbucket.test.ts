// GA-60: Settings → Bitbucket in plain words: the account line, the token field, Save and Remove, and the two lines on
// how to make the API token and how pushes go.
import { describe, expect, it } from "vitest";
import { accountLine, canRemove, saveBlocked, SSH_LINE, TOKEN_HOW_TO, TOKEN_PAGE, TOKEN_SCOPES, tokenPlaceholder } from "./bitbucket";
import type { BitbucketStatus } from "../types";

const status = (more: Partial<BitbucketStatus> = {}): BitbucketStatus => ({ hasToken: false, ...more });

describe("the account line", () => {
  it("names the account the saved token belongs to", () => {
    const s = status({ email: "jef@example.com", hasToken: true, account: "Jeffrey Sevinga (jefsev)" });
    expect(accountLine(s)).toEqual({ mark: "ok", text: "Logged in as Jeffrey Sevinga (jefsev)" });
  });
  it("says what's wrong and what to do, as the backend words it", () => {
    const s = status({ email: "jef@example.com", hasToken: true, accountProblem: {
      what: "Bitbucket refused the email and API token", fix: "Check that the email is your Atlassian account's and that the token hasn't expired or been revoked, or make a new API token." } });
    expect(accountLine(s)).toEqual({ mark: "failed",
      text: "Bitbucket refused the email and API token. Check that the email is your Atlassian account's and that the token hasn't expired or been revoked, or make a new API token." });
    expect(accountLine(status({ hasToken: true, accountProblem: { what: "Can't reach Bitbucket" } }))).toEqual({ mark: "failed", text: "Can't reach Bitbucket" });
  });
  it("asks for an email and a token when none is saved, in grey", () => {
    expect(accountLine(status())).toEqual({ mark: "skipped", text: "No API token saved yet: enter your Atlassian email and an API token, then Save." });
    expect(accountLine(status({ email: "jef@example.com" })).mark).toBe("skipped");
  });
  it("prefers the account to a problem, and a problem to the empty line", () => {
    expect(accountLine(status({ hasToken: true, account: "jefsev", accountProblem: { what: "x" } })).mark).toBe("ok");
    expect(accountLine(status({ hasToken: false, accountProblem: { what: "The Bitbucket login in the keychain can't be read" } })).mark).toBe("failed");
  });
  it("says so when a token is saved but Bitbucket didn't name its account", () => {
    expect(accountLine(status({ hasToken: true }))).toEqual({ mark: "failed", text: "Bitbucket didn't say whose token this is. Check connection says more." });
  });
});

describe("the email and token", () => {
  it("never shows a saved token: the field stays empty and says it is saved", () => {
    expect(tokenPlaceholder(status({ hasToken: true }))).toBe("Saved in your keychain: type to replace");
    expect(tokenPlaceholder(status())).toBe("Paste the API token");
    expect(tokenPlaceholder(null)).toBe("Paste the API token");
    expect(tokenPlaceholder()).toBe("Paste the API token");
  });
  it("keeps Save off, saying why, until there is an email address and a token", () => {
    expect(saveBlocked("", "tok")).toBe("Enter the email of your Atlassian account.");
    expect(saveBlocked("   ", "tok")).toBe("Enter the email of your Atlassian account.");
    expect(saveBlocked("jefsev", "tok")).toBe("jefsev isn't an email address.");
    expect(saveBlocked(" jef sev@example.com ", "tok")).toBe("jef sev@example.com isn't an email address.");
    expect(saveBlocked("jef@@example.com", "tok")).toBe("jef@@example.com isn't an email address.");
    expect(saveBlocked("jef@example.com", "")).toBe("Paste the API token too: Bitbucket checks the email and the token together.");
    expect(saveBlocked("jef@example.com", "   ")).toMatch(/^Paste the API token too/);
    expect(saveBlocked("jef@example.com", "ATATT3x")).toBeNull();
    expect(saveBlocked("  jef@example.com  ", " ATATT3x ")).toBeNull();
  });
  it("offers Remove only when there is an email or a token to remove", () => {
    expect(canRemove(status({ hasToken: true }))).toBe(true);
    expect(canRemove(status({ email: "jef@example.com" }))).toBe(true);
    expect(canRemove(status({ email: "  " }))).toBe(false);
    expect(canRemove(status())).toBe(false);
    expect(canRemove(null)).toBe(false);
    expect(canRemove()).toBe(false);
  });
});

describe("how to make the token and how pushes go", () => {
  it("asks for the three scopes GA-59's backend needs (gizai-agents' bitbucket::SCOPES)", () => {
    expect(TOKEN_SCOPES).toEqual(["read:user:bitbucket", "read:pullrequest:bitbucket", "write:pullrequest:bitbucket"]);
  });
  it("says in one line where to make the API token, to pick Bitbucket, and every scope", () => {
    expect(TOKEN_HOW_TO).toMatch(/^Make the token in your Atlassian account: Security → Create and manage API tokens/);
    expect(TOKEN_HOW_TO).toContain("pick Bitbucket");
    expect(TOKEN_HOW_TO).toContain("read:user:bitbucket, read:pullrequest:bitbucket and write:pullrequest:bitbucket.");
    expect(TOKEN_HOW_TO.split("\n")).toHaveLength(1);
    expect(TOKEN_PAGE).toBe("https://id.atlassian.com/manage-profile/security/api-tokens");
  });
  it("says pushes go over SSH with your keys, and where to add the public key", () => {
    expect(SSH_LINE).toBe("Pushes go over SSH with your own keys: add your public key in Bitbucket → Personal settings → SSH keys.");
  });
});
