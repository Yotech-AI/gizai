import { describe, expect, it } from "vitest";
import { accountLine, canLogIn, ghLine, problemText, PUSH_OVER, pushOverHint } from "./github";
import type { GithubStatus } from "../types";

const status = (more: Partial<GithubStatus> = {}): GithubStatus => ({ pushOver: "ssh", ...more });
const found = { ghPath: "/usr/bin/gh", ghVersion: "2.62.0" };

describe("Settings → GitHub status", () => {
  it("says gh is found with its path and version, or why not and where to get it", () => {
    expect(ghLine(status(found))).toEqual({ mark: "ok", text: "Found at /usr/bin/gh, version 2.62.0" });
    expect(ghLine(status({ ghPath: "/usr/bin/gh" }))).toEqual({ mark: "ok", text: "Found at /usr/bin/gh" });
    const missing = status({ ghProblem: { what: "Not found", fix: "Install the GitHub CLI from cli.github.com, or set its path." } });
    expect(ghLine(missing)).toEqual({ mark: "failed", text: "Not found. Install the GitHub CLI from cli.github.com, or set its path." });
  });
  it("says which account is logged in, or that none is", () => {
    expect(accountLine(status({ ...found, account: "octocat" }))).toEqual({ mark: "ok", text: "Logged in as octocat" });
    const out = status({ ...found, accountProblem: { what: "The GitHub CLI isn't logged in", fix: "Use Log in with GitHub, or run gh auth login in a terminal." } });
    expect(accountLine(out)).toEqual({ mark: "failed", text: "The GitHub CLI isn't logged in. Use Log in with GitHub, or run gh auth login in a terminal." });
    expect(accountLine(status(found))).toEqual({ mark: "failed", text: "Not logged in." });
    expect(accountLine(status({ ghProblem: { what: "Not found" } }))).toEqual({ mark: "skipped", text: "Needs the GitHub CLI." });
  });
  it("offers Log in with GitHub only when gh is there and isn't logged in", () => {
    expect(canLogIn(status(found))).toBe(true);
    expect(canLogIn(status({ ...found, accountProblem: { what: "gh's login doesn't work" } }))).toBe(true);
    expect(canLogIn(status({ ...found, account: "octocat" }))).toBe(false);
    expect(canLogIn(status({ ghProblem: { what: "Not found" } }))).toBe(false);
  });
  it("writes a problem without a fix as it is", () => {
    expect(problemText({ what: "Login cancelled" })).toBe("Login cancelled");
    expect(problemText({ what: "Login cancelled", fix: null })).toBe("Login cancelled");
  });
});

describe("Push over", () => {
  it("is SSH (the default) or HTTPS with gh's login, and neither changes git config or asks for a password", () => {
    expect(PUSH_OVER.map((o) => o.value)).toEqual(["ssh", "https"]);
    expect(PUSH_OVER[0].label).toContain("default");
    expect(pushOverHint("ssh")).toContain("git@github.com:owner/name");
    expect(pushOverHint("https")).toContain("GitHub CLI's login");
    for (const p of ["ssh", "https"] as const) expect(pushOverHint(p)).toContain("Neither way changes your git config or remotes, or asks for a password.");
  });
});
