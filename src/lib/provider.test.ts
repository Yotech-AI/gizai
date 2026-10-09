// GA-60: a project's provider from its (normalised) link: GitHub, Bitbucket, or a plain git link.
import { describe, expect, it } from "vitest";
import { hasPulls, HOST_NAMES, hostName, linkLabel, providerOf, pullHostOf } from "./provider";

const GH = "https://github.com/acme/shop";
const BB = "https://bitbucket.org/acme/shop";
const PLAIN = ["git@gitlab.com:acme/shop.git", "https://gitlab.com/acme/shop", "ssh://git@example.com/shop.git", "/home/you/repos/shop.git", "file:///srv/shop.git"];

describe("providerOf", () => {
  it("tells GitHub, Bitbucket and plain git links apart, and says null for no link", () => {
    expect(providerOf(GH)).toBe("github");
    expect(providerOf(BB)).toBe("bitbucket");
    for (const url of PLAIN) expect(providerOf(url)).toBe("git");
    expect(providerOf("")).toBeNull();
    expect(providerOf("   ")).toBeNull();
    expect(providerOf(null)).toBeNull();
    expect(providerOf(undefined)).toBeNull();
  });
  it("reads a pull request's link too, with spaces or capitals around it", () => {
    expect(providerOf("https://github.com/acme/shop/pull/12")).toBe("github");
    expect(providerOf("https://bitbucket.org/acme/shop/pull-requests/12")).toBe("bitbucket");
    expect(providerOf("  HTTPS://Bitbucket.org/Acme/Shop  ")).toBe("bitbucket");
    expect(providerOf(" https://GitHub.com/Acme/Shop ")).toBe("github");
  });
  it("goes by the host only, not by a name that merely looks like it", () => {
    expect(providerOf("https://github.com.example.com/acme/shop")).toBe("git");
    expect(providerOf("https://bitbucket.org.example.com/acme/shop")).toBe("git");
    expect(providerOf("https://example.com/github.com/acme/shop")).toBe("git");
    expect(providerOf("https://bitbucket.example.com/acme/shop")).toBe("git");
  });
});

describe("pull requests per provider", () => {
  it("opens and follows pull requests for GitHub and Bitbucket links only", () => {
    expect(hasPulls(GH)).toBe(true);
    expect(hasPulls(BB)).toBe(true);
    for (const url of PLAIN) expect(hasPulls(url)).toBe(false);
    expect(hasPulls(null)).toBe(false);
    expect(hasPulls("")).toBe(false);
  });
  it("puts a Bitbucket link's pull requests on Bitbucket, and any other on GitHub as before", () => {
    expect(pullHostOf(BB)).toBe("bitbucket");
    expect(pullHostOf(GH)).toBe("github");
    expect(pullHostOf(PLAIN[0])).toBe("github");
    expect(pullHostOf(null)).toBe("github");
    expect(hostName(BB + "/pull-requests/3")).toBe("Bitbucket");
    expect(hostName(GH + "/pull/3")).toBe("GitHub");
    expect(hostName(undefined)).toBe("GitHub");
    expect(HOST_NAMES).toEqual({ github: "GitHub", bitbucket: "Bitbucket" });
  });
});

describe("linkLabel (the project page)", () => {
  it("says GitHub or Bitbucket for their links, and Remote for another git link or none", () => {
    expect(linkLabel(GH)).toBe("GitHub");
    expect(linkLabel(BB)).toBe("Bitbucket");
    for (const url of PLAIN) expect(linkLabel(url)).toBe("Remote");
    expect(linkLabel(null)).toBe("Remote");
  });
});
