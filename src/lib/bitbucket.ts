// Settings → Bitbucket in plain words: the account your API token belongs to, how to make the token, and how pushes go.
import type { BitbucketStatus } from "../types";
import { problemText, type StatusLine } from "./github";

/** Where you make an API token: your Atlassian account's security page. */
export const TOKEN_PAGE = "https://id.atlassian.com/manage-profile/security/api-tokens";

/** The scopes the token needs, as the backend asks for them (gizai-agents' `bitbucket::SCOPES`; change both together): who
 *  it belongs to, and reading and opening pull requests. Pushes go over SSH, so it needs no access to push. */
export const TOKEN_SCOPES = ["read:user:bitbucket", "read:pullrequest:bitbucket", "write:pullrequest:bitbucket"];

/** How to make the API token and the scopes it needs, in one line. */
export const TOKEN_HOW_TO = "Make the token in your Atlassian account: Security → Create and manage API tokens → Create API token with scopes, "
  + `pick Bitbucket, and give it the scopes ${TOKEN_SCOPES.slice(0, -1).join(", ")} and ${TOKEN_SCOPES[TOKEN_SCOPES.length - 1]}.`;

/** How pushes go, in one line. */
export const SSH_LINE = "Pushes go over SSH with your own keys: add your public key in Bitbucket → Personal settings → SSH keys.";

/** The account the saved API token belongs to, or why there is none and what to do. */
export function accountLine(s: BitbucketStatus): StatusLine {
  if (s.account) return { mark: "ok", text: `Logged in as ${s.account}` };
  if (s.accountProblem) return { mark: "failed", text: problemText(s.accountProblem) };
  if (!s.hasToken) return { mark: "skipped", text: "No API token saved yet: enter your Atlassian email and an API token, then Save." };
  return { mark: "failed", text: "Bitbucket didn't say whose token this is. Check connection says more." };
}

/** The token field's placeholder: a saved token is never shown again. */
export function tokenPlaceholder(s?: BitbucketStatus | null): string {
  return s?.hasToken ? "Saved in your keychain: type to replace" : "Paste the API token";
}

/** Why Save can't be used yet, or null: Bitbucket checks the email and the token together, so both are needed. */
export function saveBlocked(email: string, token: string): string | null {
  if (!email.trim()) return "Enter the email of your Atlassian account.";
  if (!/^[^\s@]+@[^\s@]+$/.test(email.trim())) return `${email.trim()} isn't an email address.`;
  if (!token.trim()) return "Paste the API token too: Bitbucket checks the email and the token together.";
  return null;
}

/** Whether Remove is offered: there is an email or a token to remove. */
export function canRemove(s?: BitbucketStatus | null): boolean {
  return !!s && (s.hasToken || !!s.email?.trim());
}
