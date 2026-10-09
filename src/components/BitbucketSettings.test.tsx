// GA-60: Settings → Bitbucket: the account the API token belongs to (or what's wrong and what to do), the Atlassian email and
// API token (a saved token is never shown again) with Save and Remove, the two lines on the token and on SSH, and Check
// connection with the same result list as Settings → GitHub. Rendered to HTML on the server, so no data loads: the
// component's state is handed in, in the order of its useState calls (status, statusErr, email, token, doing, removing, err,
// check, checking). Buttons and inputs are used on the element tree with the hooks replaced, and the api is mocked to see
// what Save, Remove and Check connection send. GA-59's commands themselves are tested in the backend.
import { describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { BitbucketStatus, ConnectionCheck } from "../types";

const queue: unknown[] = [];
// Direct: components are called as functions; each useState's setter records what it is given, in `sets[i]`.
let sets: unknown[][] | null = null;
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => {
    const v = queue.length ? queue.shift() : typeof init === "function" ? (init as () => unknown)() : init;
    if (!sets) return R.useState(v);
    const mine: unknown[] = [];
    sets.push(mine);
    return [v, (x: unknown) => mine.push(x)];
  }) as typeof R.useState;
  const useEffect = ((f: () => void, deps?: unknown[]) => (sets ? undefined : R.useEffect(f, deps))) as typeof R.useEffect;
  const useRef = ((init: unknown) => (sets ? { current: init } : R.useRef(init))) as typeof R.useRef;
  return { ...R, default: { ...R, useState, useEffect, useRef }, useState, useEffect, useRef };
});

const calls: [string, unknown[]][] = [];
const answers: Record<string, (...a: unknown[]) => unknown> = {};
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) {
    out[k] = typeof v !== "function" ? v : (...a: unknown[]) => { calls.push([k, a]); return Promise.resolve().then(() => answers[k]?.(...a)); };
  }
  return out;
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (url: string) => { calls.push(["openUrl", [url]]); return Promise.resolve(); } }));

const { BitbucketSettings } = await import("./BitbucketSettings");
const { ConnectionChecks } = await import("./ConnectionChecks");
const { SSH_LINE, TOKEN_HOW_TO, TOKEN_PAGE } = await import("../lib/bitbucket");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, "<")
  .replace(/&gt;/g, ">").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
const settle = () => new Promise((r) => setTimeout(r, 0));

const SAVED: BitbucketStatus = { email: "jef@example.com", hasToken: true, account: "Jeffrey Sevinga (jefsev)" };
const NONE: BitbucketStatus = { email: null, hasToken: false, account: null, accountProblem: null };
const REFUSED: BitbucketStatus = { email: "jef@example.com", hasToken: true, accountProblem: {
  what: "Bitbucket refused the email and API token", fix: "Check that the email is your Atlassian account's and that the token hasn't expired or been revoked, or make a new API token." } };
const CHECK: ConnectionCheck = { ok: false, pushOver: "ssh", checks: [
  { name: "API token", result: "ok", text: "Logged in as Jeffrey Sevinga (jefsev)" },
  { name: "SSH to Bitbucket", result: "failed", text: "Bitbucket refused your SSH key", fix: "Add your public key in Bitbucket → Personal settings → SSH keys." },
  { name: "Push", result: "skipped", text: "Needs SSH to Bitbucket", repo: "acme/shop", projectId: "p1" },
] };

type State = { status?: BitbucketStatus | null; statusErr?: string | null; email?: string; token?: string; doing?: "save" | "remove" | null;
  removing?: boolean; err?: string | null; check?: ConnectionCheck | null; checking?: boolean };
const states = (s: State) => [s.status === undefined ? SAVED : s.status, s.statusErr ?? null, s.email ?? (s.status === undefined ? SAVED.email : ""),
  s.token ?? "", s.doing ?? null, s.removing ?? false, s.err ?? null, s.check ?? null, s.checking ?? false];
const S = { status: 0, statusErr: 1, email: 2, token: 3, doing: 4, removing: 5, err: 6, check: 7, checking: 8 };

function render(s: State = {}) {
  queue.push(...states(s));
  try { return renderToStaticMarkup(<BitbucketSettings say={() => {}} />); } finally { queue.length = 0; }
}

function expand(node: ReactNode): ReactNode {
  if (Array.isArray(node)) return node.map(expand);
  if (node && typeof node === "object" && "props" in node) {
    const el = node as ReactElement<Record<string, unknown>>;
    if (typeof el.type === "function") return expand((el.type as (p: unknown) => ReactNode)(el.props));
    return { ...el, props: { ...el.props, children: expand(el.props.children as ReactNode) } } as ReactNode;
  }
  return node;
}
function findAll(node: ReactNode, pred: (p: Record<string, unknown>) => boolean, out: Record<string, unknown>[] = []) {
  if (Array.isArray(node)) node.forEach((n) => findAll(n, pred, out));
  else if (node && typeof node === "object" && "props" in node) {
    const p = (node as ReactElement<Record<string, unknown>>).props;
    if (pred(p)) out.push(p);
    findAll(p.children as ReactNode, pred, out);
  }
  return out;
}
const label = (p: Record<string, unknown>) => [p.children].flat(Infinity).filter((c) => typeof c === "string").join("");
type Button = { onClick: (e?: unknown) => unknown; disabled?: boolean; title?: string };
type Input = { onChange: (e: unknown) => void; onKeyDown: (e: unknown) => void; value?: string; type?: string; placeholder?: string };
/** BitbucketSettings as an element tree with state `s`; `got[i]` gets what the i-th useState's setter was given. */
function tree(s: State) {
  sets = [];
  queue.push(...states(s));
  const said: [boolean, string][] = [];
  try {
    const t = expand(BitbucketSettings({ say: (ok, msg) => said.push([ok, msg]) }));
    const got = sets;
    const button = (name: string) => findAll(t, (p) => typeof p.onClick === "function" && label(p) === name) as Button[];
    const input = (id: string) => findAll(t, (p) => p.id === id)[0] as Input;
    return { t, got, said, button, input };
  } finally { sets = null; queue.length = 0; }
}
const reset = () => { calls.length = 0; for (const k of Object.keys(answers)) delete answers[k]; };

describe("the account line", () => {
  it("says Checking… until Bitbucket has answered", () => {
    const t = text(render({ status: null, email: "" }));
    expect(t).toContain("Account Checking…");
  });
  it("names the account the token belongs to, with what Gizai does as it", () => {
    const html = render();
    expect(html).toMatch(/<div class="gh-line ok">.*Logged in as Jeffrey Sevinga \(jefsev\)/);
    expect(text(html)).toContain("Gizai opens pull requests on Bitbucket as this account.");
  });
  it("asks for an email and a token in grey when none is saved", () => {
    const html = render({ status: NONE });
    expect(html).toMatch(/<div class="gh-line skipped">.*No API token saved yet: enter your Atlassian email and an API token, then Save\./);
    expect(text(html)).not.toContain("as this account");
  });
  it("shows what's wrong and what to do in red", () => {
    const html = render({ status: REFUSED });
    expect(html).toMatch(/<div class="gh-line failed">.*Bitbucket refused the email and API token\. Check that the email is your Atlassian account/);
    expect(text(html)).not.toContain("as this account");
  });
  it("shows why the status couldn't be asked in red (before GA-59's commands are there: the command's error)", () => {
    const html = render({ status: null, statusErr: "Command bitbucket_status not found" });
    expect(html).toMatch(/<div class="gh-line failed">.*Command bitbucket_status not found/);
    expect(text(html)).not.toContain("Checking…");
  });
});

describe("the email and API token", () => {
  it("fills the saved email, and never shows a saved token: the password field stays empty and says it is saved", () => {
    const { input } = tree({});
    expect(input("s-bb-email").value).toBe("jef@example.com");
    const tok = input("s-bb-token");
    expect(tok.type).toBe("password");
    expect(tok.value).toBe("");
    expect(tok.placeholder).toBe("Saved in your keychain: type to replace");
    const t = text(render());
    expect(t).toContain("A saved token is never shown again");
    expect(text(render({ status: NONE }))).toContain("Goes to your keychain, never to Gizai's database");
    expect(tree({ status: NONE }).input("s-bb-token").placeholder).toBe("Paste the API token");
  });
  it("keeps Save off, with why in its tooltip, until the email has an @ and the token is there", () => {
    expect(tree({ status: NONE }).button("Save")[0]).toMatchObject({ disabled: true, title: "Enter the email of your Atlassian account." });
    expect(tree({ status: NONE, email: "jef" , token: "x" }).button("Save")[0]).toMatchObject({ disabled: true, title: "jef isn't an email address." });
    expect(tree({ status: NONE, email: "jef@example.com" }).button("Save")[0]).toMatchObject({ disabled: true,
      title: "Paste the API token too: Bitbucket checks the email and the token together." });
    // a saved token still has to be typed again with a new email: Bitbucket checks the two together
    expect(tree({}).button("Save")[0].disabled).toBe(true);
    expect(tree({ status: NONE, email: "jef@example.com", token: "ATATT3x" }).button("Save")[0]).toMatchObject({ disabled: false, title: undefined });
  });
  it("Save sends the email and token, clears the token field, asks the status again and says who you are logged in as", async () => {
    reset();
    answers.bitbucketStatus = () => SAVED;
    const { button, got, said } = tree({ status: NONE, email: "  jef@example.com ", token: " ATATT3x " });
    await button("Save")[0].onClick();
    await settle();
    expect(calls.filter(([k]) => k.startsWith("bitbucket"))).toEqual([["bitbucketSaveLogin", ["jef@example.com", "ATATT3x"]], ["bitbucketStatus", []]]);
    expect(got[S.token]).toEqual([""]);
    expect(got[S.status]).toEqual([SAVED]);
    expect(got[S.err]).toEqual([null]);
    expect(got[S.doing]).toEqual(["save", null]);
    expect(said).toEqual([[true, "Logged in to Bitbucket as Jeffrey Sevinga (jefsev)."]]);
  });
  it("Enter in the email or token field saves too; with Save off it sends nothing", async () => {
    for (const id of ["s-bb-email", "s-bb-token"]) {
      reset();
      answers.bitbucketStatus = () => SAVED;
      const { input } = tree({ status: NONE, email: "jef@example.com", token: "ATATT3x" });
      input(id).onKeyDown({ key: "Enter", preventDefault: () => {} });
      await settle();
      expect(calls[0]).toEqual(["bitbucketSaveLogin", ["jef@example.com", "ATATT3x"]]);
    }
    reset();
    tree({ status: NONE, email: "jef@example.com" }).input("s-bb-token").onKeyDown({ key: "Enter", preventDefault: () => {} });
    await settle();
    expect(calls).toEqual([]);
  });
  it("a refused login shows Bitbucket's words in red under the buttons, and keeps the token typed", async () => {
    reset();
    answers.bitbucketSaveLogin = () => { throw "Bitbucket refused the email and API token: check them, or make a new API token."; };
    const { button, got, said } = tree({ status: NONE, email: "jef@example.com", token: "ATATT3x" });
    await button("Save")[0].onClick();
    await settle();
    expect(got[S.err]).toEqual([null, "Bitbucket refused the email and API token: check them, or make a new API token."]);
    expect(got[S.token]).toEqual([]);
    expect(said).toEqual([]);
    const html = render({ status: NONE, email: "jef@example.com", token: "ATATT3x", err: "Bitbucket refused the email and API token: check them, or make a new API token." });
    expect(html).toMatch(/<div class="gh-line failed" role="alert">.*Bitbucket refused the email and API token: check them, or make a new API token\./);
    expect(html.indexOf("Bitbucket refused the email")).toBeGreaterThan(html.indexOf(">Save</button>"));
  });
  it("offers Remove only when an email or token is saved, and asks first", async () => {
    expect(tree({ status: NONE }).button("Remove")).toHaveLength(0);
    expect(tree({ status: { hasToken: false, email: "jef@example.com" } }).button("Remove")).toHaveLength(1);
    const { button, got } = tree({});
    button("Remove")[0].onClick();
    expect(got[S.removing]).toEqual([true]);
    const confirm = text(render({ removing: true }));
    expect(confirm).toContain("Remove your Bitbucket email and API token? Gizai can't open or follow pull requests on Bitbucket until you save them again.");
    expect(confirm).toContain("Cancel Remove");
  });
  it("Remove removes the email and token, asks the status again and says so", async () => {
    reset();
    answers.bitbucketStatus = () => NONE;
    const { button, got, said } = tree({ removing: true });
    const buttons = button("Remove");
    expect(buttons).toHaveLength(1); // the confirm's: the first Remove hides while it asks
    await buttons[0].onClick();
    await settle();
    expect(calls.filter(([k]) => k.startsWith("bitbucket"))).toEqual([["bitbucketRemoveLogin", []], ["bitbucketStatus", []]]);
    expect(got[S.removing]).toEqual([false]);
    expect(got[S.status]).toEqual([NONE]);
    expect(got[S.email]).toEqual([""]);
    expect(said).toEqual([[true, "Removed your Bitbucket email and API token."]]);
  });
});

describe("the lines on the API token and on SSH", () => {
  it("say how to make the token and its scopes, link the token page, and say pushes go over SSH with your keys", () => {
    const t = text(render());
    expect(t).toContain(TOKEN_HOW_TO);
    expect(t).toContain("read:user:bitbucket, read:pullrequest:bitbucket and write:pullrequest:bitbucket");
    expect(t).toContain(SSH_LINE);
    expect(t).toContain("Bitbucket → Personal settings → SSH keys");
    expect(render()).toContain(`href="${TOKEN_PAGE}"`);
  });
  it("Open API tokens opens the Atlassian page in the browser", () => {
    reset();
    const { t } = tree({});
    const link = findAll(t, (p) => p.href === TOKEN_PAGE)[0] as { onClick: (e: unknown) => void };
    link.onClick({ preventDefault: () => {} });
    expect(calls).toEqual([["openUrl", [TOKEN_PAGE]]]);
  });
});

describe("Check connection", () => {
  it("asks Bitbucket and shows the result list", async () => {
    reset();
    answers.bitbucketCheck = () => CHECK;
    answers.bitbucketStatus = () => SAVED;
    const { button, got } = tree({});
    await button("Check connection")[0].onClick();
    await settle();
    expect(calls.map(([k]) => k)).toEqual(["bitbucketCheck", "bitbucketStatus"]);
    expect(got[S.check]).toEqual([CHECK]);
    expect(got[S.checking]).toEqual([true, false]);
    const html = render({ check: CHECK });
    expect(html).toContain('aria-label="Bitbucket connection checks"');
    const t = text(html);
    expect(t).toContain("Something needs fixing: see what to do below.");
    expect(t).toContain("SSH to Bitbucket Bitbucket refused your SSH key Add your public key in Bitbucket → Personal settings → SSH keys.");
    expect(t).toContain("Push acme/shop Needs SSH to Bitbucket");
    expect(t).not.toContain("No project has a Bitbucket link yet.");
  });
  it("says when Gizai can use Bitbucket, and when no project has a Bitbucket link yet", () => {
    const ok: ConnectionCheck = { ok: true, pushOver: "ssh", checks: [{ name: "API token", result: "ok", text: "Logged in as jefsev" }] };
    const t = text(render({ check: ok }));
    expect(t).toContain("Gizai can use Bitbucket.");
    expect(t).toContain("Projects No project has a Bitbucket link yet.");
  });
  it("shows why the check couldn't run in red", async () => {
    reset();
    answers.bitbucketCheck = () => { throw "Command bitbucket_check not found"; };
    const { button, got } = tree({});
    await button("Check connection")[0].onClick();
    await settle();
    expect(got[S.checking]).toEqual([true, false]);
    const errs = got[1 + S.checking]; // checkErr is the useState after checking
    expect(errs).toEqual([null, "Command bitbucket_check not found"]);
  });
  it("is the same list as Settings → GitHub's, with the host's name", () => {
    const gh = renderToStaticMarkup(<ConnectionChecks check={CHECK} host="GitHub" />);
    const bb = renderToStaticMarkup(<ConnectionChecks check={CHECK} host="Bitbucket" />);
    expect(bb).toBe(gh);
    const ok: ConnectionCheck = { ok: true, pushOver: "ssh", checks: [] };
    const ghOk = renderToStaticMarkup(<ConnectionChecks check={ok} host="GitHub" />);
    expect(ghOk).toMatch(/^<div class="gh-checks" aria-label="Connection checks"><div class="gh-summary ok">Gizai can use GitHub\.<\/div><div class="gh-check skipped"><svg/);
    expect(text(ghOk)).toBe("Gizai can use GitHub. Projects No project has a GitHub link yet.");
    expect(renderToStaticMarkup(<ConnectionChecks check={ok} host="Bitbucket" />)).toBe(ghOk.replace(/GitHub/g, "Bitbucket"));
  });
});
