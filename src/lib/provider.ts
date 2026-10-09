// Where a project's repository link points: GitHub, Bitbucket, or another git host (a plain git link). Gizai stores links
// normalised (https://github.com/owner/name, https://bitbucket.org/workspace/name), so the start of a link says which.

export type Provider = "github" | "bitbucket" | "git";
/** The hosts Gizai opens and follows pull requests on. */
export type PullHost = "github" | "bitbucket";

export const HOST_NAMES: Record<PullHost, string> = { github: "GitHub", bitbucket: "Bitbucket" };

/** The provider of a link; null for no link. A pull request's link works too. */
export function providerOf(url?: string | null): Provider | null {
  const s = (url ?? "").trim().toLowerCase();
  if (!s) return null;
  if (s.startsWith("https://github.com/")) return "github";
  if (s.startsWith("https://bitbucket.org/")) return "bitbucket";
  return "git";
}

/** Whether Gizai opens and follows pull requests for a project with this link: GitHub and Bitbucket, not a plain git link. */
export function hasPulls(url?: string | null): boolean {
  const p = providerOf(url);
  return p === "github" || p === "bitbucket";
}

/** Where a link's pull requests are: Bitbucket for a Bitbucket link, otherwise GitHub (as every text said before Bitbucket). */
export function pullHostOf(url?: string | null): PullHost {
  return providerOf(url) === "bitbucket" ? "bitbucket" : "github";
}

/** "GitHub" or "Bitbucket", for texts about a link's pull requests. */
export function hostName(url?: string | null): string {
  return HOST_NAMES[pullHostOf(url)];
}

/** What the project page calls a project's link: GitHub, Bitbucket, or Remote for another git link (or none). */
export function linkLabel(url?: string | null): string {
  const p = providerOf(url);
  return p === "github" || p === "bitbucket" ? HOST_NAMES[p] : "Remote";
}
