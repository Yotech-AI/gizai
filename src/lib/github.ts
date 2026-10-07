// Settings → GitHub in plain words: the GitHub CLI, the account it is logged in as, and how pushes go.
import type { GithubProblem, GithubStatus, PushOver } from "../types";

/** ok, failed, or skipped (not needed now, or it needs something that failed). */
export type Mark = "ok" | "failed" | "skipped";
export type StatusLine = { mark: Mark; text: string };

/** "What went wrong. What to do." */
export function problemText(p: GithubProblem): string {
  return p.fix ? `${p.what}. ${p.fix}` : p.what;
}

/** The GitHub CLI: found (its path and version), or why not and where to get it. */
export function ghLine(s: GithubStatus): StatusLine {
  if (s.ghProblem) return { mark: "failed", text: problemText(s.ghProblem) };
  return { mark: "ok", text: `Found at ${s.ghPath ?? "?"}${s.ghVersion ? `, version ${s.ghVersion}` : ""}` };
}

/** The account gh is logged in as on GitHub, or why there is none. */
export function accountLine(s: GithubStatus): StatusLine {
  if (s.ghProblem) return { mark: "skipped", text: "Needs the GitHub CLI." };
  if (s.account) return { mark: "ok", text: `Logged in as ${s.account}` };
  if (s.accountProblem) return { mark: "failed", text: problemText(s.accountProblem) };
  return { mark: "failed", text: "Not logged in." };
}

/** Whether Log in with GitHub is offered: gh is there and isn't logged in (or GitHub refuses its login). */
export function canLogIn(s: GithubStatus): boolean {
  return !s.ghProblem && !s.account;
}

/** The two ways a push can go, as Settings → GitHub offers them. */
export const PUSH_OVER: { value: PushOver; label: string; hint: string }[] = [
  { value: "ssh", label: "SSH, with your SSH keys (default)",
    hint: "Pushes to git@github.com:owner/name, also when the repository's remote is an https address." },
  { value: "https", label: "HTTPS, with gh's login",
    hint: "git uses the GitHub CLI's login for the push, and none of your other git logins." },
];

/** What the chosen way means, in one line. */
export function pushOverHint(p: PushOver): string {
  const way = PUSH_OVER.find((o) => o.value === p) ?? PUSH_OVER[0];
  return `${way.hint} Neither way changes your git config or remotes, or asks for a password.`;
}
