// GA-39: Settings → MCP servers: each server's sign-in state with Sign in / Sign out, a saved secret value never shown
// (names only; typing replaces it), and Import from Claude Code (names of lines only, Already in the list, another name on
// a clash, and the note on claude.ai connectors and plugin servers). Rendered to HTML on the server, so no data loads: the
// component's state is handed in, in the order of its useState calls (list, err, msg, edit, busy, open, removing, scan,
// importing, then the import panel's picks). Buttons and inputs are used on the element tree with the hooks replaced, and the
// api is mocked to see what Save and Import send.
import { describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { McpCandidate, McpScan, McpServerInput, McpServerView, McpToolView } from "../types";

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
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});

const calls: [string, unknown[]][] = [];
const answers: Record<string, (...a: unknown[]) => unknown> = {};
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) {
    out[k] = typeof v !== "function" ? v : (...a: unknown[]) => { calls.push([k, a]); return Promise.resolve(answers[k]?.(...a)); };
  }
  return out;
});

const { McpSettings } = await import("./McpSettings");
const { linesOf } = await import("../lib/mcp");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, "<")
  .replace(/&gt;/g, ">").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();

const tool = (name: string, risk: string): McpToolView => ({
  name, description: "", params: [], hints: { readOnly: false, destructive: false, idempotent: false, openWorld: false }, hintsSent: [], risk,
  summary: `${name}: ${risk}`, notes: [],
});
const server = (s: Partial<McpServerView> & { id: string; name: string }): McpServerView => ({
  transport: "http", command: "", args: [], envNames: [], url: `https://${s.name}.example.com/mcp`, headerNames: [], clientId: "", source: "",
  signIn: "", problem: null, missing: [], listed: null, usedBy: [], ...s,
});
const otus = server({ id: "s-otus", name: "otus", signIn: "signed_in", usedBy: ["Backend Agent"], source: "Claude Code (user scope)",
  listed: { serverName: "otus", serverVersion: "1.2.0", listedAt: Date.now(), tools: [tool("search", "low"), tool("update_note", "medium"), tool("delete_note", "high")] } });
const acme = server({ id: "s-acme", name: "acme", signIn: "needs_sign_in", headerNames: ["X-Api-Key"], missing: ["X-Api-Key"],
  problem: "acme refused the sign-in: sign in again." });
const local = server({ id: "s-local", name: "local", transport: "stdio", command: "npx", args: ["-y", "@acme/mcp"], url: "", envNames: ["ACME_TOKEN", "ACME_URL"],
  missing: ["ACME_URL"] });
const list = [otus, acme, local];

type State = { list?: McpServerView[] | null; err?: string | null; msg?: string | null; edit?: unknown; busy?: Record<string, string>;
  open?: Record<string, boolean>; removing?: string | null; scan?: McpScan | null; importing?: boolean; picks?: Record<string, { on: boolean; name: string }> };
const states = (s: State) => {
  const v: unknown[] = [s.list === undefined ? list : s.list, s.err ?? null, s.msg ?? null, s.edit ?? null, s.busy ?? {}, s.open ?? {}, s.removing ?? null,
    s.scan ?? null, s.importing ?? false];
  if (s.picks) v.push(s.picks);
  return v;
};
function render(s: State = {}) {
  queue.push(...states(s));
  try { return renderToStaticMarkup(<McpSettings />); } finally { queue.length = 0; }
}
const row = (html: string, name: string) => {
  const at = html.indexOf(`<b>${name}</b>`);
  const next = html.indexOf('<li class="mcp-server"', at);
  return html.slice(html.lastIndexOf("<li", at), next < 0 ? html.indexOf("</ul>", at) : next);
};

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
/** McpSettings as an element tree with state `s`; `sets[i]` gets what the i-th useState's setter was given. */
function tree(s: State) {
  sets = [];
  queue.push(...states(s));
  try {
    const t = expand(McpSettings());
    const got = sets;
    const button = (name: string) => findAll(t, (p) => typeof p.onClick === "function" && label(p) === name) as { onClick: () => unknown; disabled?: boolean }[];
    const input = (name: string) => findAll(t, (p) => p["aria-label"] === name)[0] as { onChange: (e: unknown) => void; value?: string };
    return { t, got, button, input };
  } finally { sets = null; queue.length = 0; }
}
const EDIT = 3; // the index of `edit` among McpSettings' useState calls

describe("the list of servers", () => {
  it("says there are none yet, with Add MCP server and Import from Claude Code", () => {
    const t = text(render({ list: [] }));
    expect(t).toContain("No MCP servers yet.");
    expect(t).toContain("Add MCP server Import from Claude Code");
  });

  it("shows a signed-in server as Signed in with Sign out, and that its tools act as you there", () => {
    const o = row(render(), "otus");
    expect(o).toContain('<span class="badge ok">Signed in</span>');
    const t = text(o);
    expect(t).toContain("Sign out");
    expect(t).not.toContain("Sign in ");
    expect(t).toContain("Its tools act as you in otus.");
    expect(t).toContain("Imported from Claude Code (user scope)");
    expect(t).toContain("On for Backend Agent");
    expect(t).toContain("3 tools: 1 only read, 1 change things, 1 may delete or overwrite. High risk.");
  });

  it("shows a server that needs sign-in as Needs sign-in with a Sign in button, its problem and a missing value", () => {
    const a = row(render(), "acme");
    expect(a).toContain('<span class="badge needs">Needs sign-in</span>');
    expect(a).toContain('<button class="btn primary sm">Sign in</button>');
    const t = text(a);
    expect(t).not.toContain("Sign out");
    expect(t).not.toContain("act as you");
    expect(t).toContain("No value saved for X-Api-Key: Edit to enter it.");
    expect(t).toContain("acme refused the sign-in: sign in again.");
  });

  it("shows a command server without a sign-in, with its command line", () => {
    const l = text(row(render(), "local"));
    expect(l).toContain("local · Command");
    expect(l).toContain("npx -y @acme/mcp");
    expect(l).not.toMatch(/Sign(ed)? (in|out)/);
    expect(l).not.toContain("Needs sign-in");
  });

  it("says to sign in in the browser while Gizai waits", () => {
    const a = row(render({ busy: { "s-acme": "signin" } }), "acme");
    expect(text(a)).toContain("Sign in in your browser: Gizai waits up to 10 minutes for it.");
    expect(a).toContain('<button class="btn primary sm" disabled="">Waiting…</button>');
  });

  it("asks before removing, saying it signs out and its values leave the keychain", () => {
    expect(text(row(render({ removing: "s-otus" }), "otus")))
      .toContain("Remove otus? Gizai signs out of it, its values leave your keychain, and Backend Agent no longer get it.");
  });
});

describe("a saved secret value is never shown", () => {
  it("Edit opens the form with the names only, no value, each saved unless missing", () => {
    const { got, button } = tree({});
    button("Edit")[2].onClick(); // local
    expect(got[EDIT]).toEqual([{ id: "s-local", name: "local", transport: "stdio", command: "npx", args: "-y\n@acme/mcp", url: "", clientId: "", source: "",
      env: [{ name: "ACME_TOKEN", value: "", saved: true }, { name: "ACME_URL", value: "", saved: false }], headers: [] }]);
    button("Edit")[1].onClick(); // acme
    expect((got[EDIT][1] as { headers: unknown }).headers).toEqual([{ name: "X-Api-Key", value: "", saved: false }]);
  });

  it("the form shows each line's name, an empty password box, and that a value is saved", () => {
    const edit = { id: "s-local", name: "local", transport: "stdio", command: "npx", args: "-y\n@acme/mcp", url: "", headers: [], clientId: "", source: "",
      env: linesOf(local.envNames, local.missing) };
    const html = render({ edit });
    expect(html).toContain('aria-label="Edit local"');
    expect(html).toContain('aria-label="Environment line 1 name" placeholder="ACME_TOKEN" value="ACME_TOKEN"/>');
    expect(html).toContain('<input class="input mono" type="password" autoComplete="off" aria-label="Environment line 1 value" placeholder="Saved in your keychain: type to replace" value=""/>');
    expect(html).toContain('<input class="input mono" type="password" autoComplete="off" aria-label="Environment line 2 value" placeholder="Value" value=""/>');
    expect(text(html)).toContain("Values go to your keychain, never to Gizai's database, and aren't shown again once saved.");
  });

  it("Save sends no value for a line left alone, so the saved one stays", async () => {
    const edit = { id: "s-local", name: "local", transport: "stdio", command: "npx", args: "-y\n@acme/mcp", url: "", headers: [], clientId: "", source: "",
      env: linesOf(local.envNames, local.missing) };
    answers.saveMcpServer = () => local;
    calls.length = 0;
    await tree({ edit }).button("Save server")[0].onClick();
    const input = calls.find(([k]) => k === "saveMcpServer")![1][0] as McpServerInput;
    expect(input.env).toEqual([{ name: "ACME_TOKEN", value: null }, { name: "ACME_URL", value: null }]);
    expect(input.headers).toEqual([]);
    expect(input.server).toMatchObject({ id: "s-local", name: "local", transport: "stdio", command: "npx", args: ["-y", "@acme/mcp"], url: "" });
  });

  it("typing in a line replaces its saved value", async () => {
    const edit = { id: "s-acme", name: "acme", transport: "http", command: "", args: "", url: "https://acme.example.com/mcp", env: [], clientId: "", source: "",
      headers: linesOf(["X-Api-Key", "X-Org"]) };
    const first = tree({ edit });
    first.input("Header 1 value").onChange({ target: { value: "new-key" } });
    const typed = (first.got[EDIT][0] as (x: unknown) => unknown)(edit);
    expect(typed).toMatchObject({ headers: [{ name: "X-Api-Key", value: "new-key", saved: true }, { name: "X-Org", value: "", saved: true }] });
    answers.saveMcpServer = () => acme;
    calls.length = 0;
    await tree({ edit: typed }).button("Save server")[0].onClick();
    const input = calls.find(([k]) => k === "saveMcpServer")![1][0] as McpServerInput;
    expect(input.headers).toEqual([{ name: "X-Api-Key", value: "new-key" }, { name: "X-Org", value: null }]);
    expect(input.env).toEqual([]);
  });
});

describe("Import from Claude Code", () => {
  const cand = (c: Partial<McpCandidate> & { key: string; name: string }): McpCandidate => ({
    account: "Claude Code", scope: "user", folder: null, transport: "stdio", command: "", args: [], url: "", envNames: [], headerNames: [],
    already: false, clash: null, ...c,
  });
  const scan: McpScan = {
    servers: [
      cand({ key: "cc|user||otus", name: "otus", transport: "http", url: "https://otus.example.com/mcp", headerNames: ["Authorization"], already: true,
        clash: "there is already an MCP server called otus: give this one another name" }),
      cand({ key: "cc|project|/home/u/app|github", name: "github", scope: "project", folder: "/home/u/app", command: "npx",
        args: ["-y", "@modelcontextprotocol/server-github"], envNames: ["GITHUB_TOKEN", "GITHUB_HOST"] }),
      cand({ key: "work|user||github", name: "github", account: "Claude Code (work)", command: "github-mcp" }),
    ],
    problems: ["Couldn't read /home/u/.claude.json: not JSON"],
  };
  const off = { "cc|user||otus": { on: false, name: "otus" }, "cc|project|/home/u/app|github": { on: false, name: "github" }, "work|user||github": { on: false, name: "github" } };

  it("lists each server with the names of its lines only, and marks one already in the list", () => {
    const html = render({ scan, picks: off });
    expect(html).toContain('aria-label="Import from Claude Code"');
    const t = text(html);
    expect(t).toContain("otus · Claude Code · user scope · Address (HTTP) Already in the list https://otus.example.com/mcp Headers: Authorization (values go to your keychain)");
    expect(t).toContain("github · Claude Code · project scope: /home/u/app · Command npx -y @modelcontextprotocol/server-github Environment: GITHUB_TOKEN, GITHUB_HOST (values go to your keychain)");
    expect(html.match(/Already in the list/g)).toHaveLength(1);
    expect(t).toContain("Couldn't read /home/u/.claude.json: not JSON");
  });

  it("says under the list that claude.ai connectors and plugin servers aren't in Claude Code's files: add those by address", () => {
    const t = text(render({ scan, picks: off }));
    expect(t).toContain("Connectors from your claude.ai account and servers from plugins aren't in these files: add those by address.");
    expect(t).toContain("Read only: Gizai doesn't change Claude Code's files, doesn't read its sign-ins and starts no server to find these.");
    expect(t.indexOf("add those by address")).toBeGreaterThan(t.indexOf("github-mcp"));
  });

  it("asks for another name when a ticked server's name is taken, and won't import until then", () => {
    const html = render({ scan, picks: { ...off, "cc|user||otus": { on: true, name: "otus" } } });
    expect(html).toContain('aria-label="New name for otus" value="otus"');
    expect(text(html)).toContain("There is already an MCP server called otus: give this one another name");
    expect(html).toContain('<button class="btn primary sm" disabled="">Import 1</button>');
  });

  it("imports under the new name once it is free", async () => {
    const picks = { ...off, "cc|user||otus": { on: true, name: " otus-copy " } };
    const html = render({ scan, picks });
    expect(html).toContain('aria-label="New name for otus" value=" otus-copy "');
    expect(text(html)).not.toContain("There is already an MCP server called");
    expect(html).toContain('<button class="btn primary sm">Import 1</button>');
    answers.importMcpServers = () => [otus];
    answers.listMcpServers = () => list;
    calls.length = 0;
    await tree({ scan, picks }).button("Import 1")[0].onClick();
    expect(calls.find(([k]) => k === "importMcpServers")![1][0]).toEqual([{ key: "cc|user||otus", name: "otus-copy" }]);
  });

  it("asks for another name when two ticked servers have the same name", () => {
    const html = render({ scan, picks: { ...off, "cc|project|/home/u/app|github": { on: true, name: "github" }, "work|user||github": { on: true, name: "github" } } });
    expect(html.match(/There is already an MCP server called github: give this one another name/g)).toHaveLength(2);
    expect(html).toContain('aria-label="New name for github"');
    expect(html).toContain('<button class="btn primary sm" disabled="">Import 2</button>');
  });

  it("says when Claude Code's files have no servers", () => {
    expect(text(render({ scan: { servers: [], problems: [] }, picks: {} }))).toContain("No MCP servers in Claude Code's config files.");
  });
});

// Found in QA: renaming a saved line without typing a value. Saving deletes the old name's value from the keychain
// (src-tauri/src/mcp_servers.rs, save: names no longer kept are deleted) and the new name has none, so the form must not
// still say a value is saved for it.
describe("a renamed line", () => {
  it("no longer says a value is saved under its new name", () => {
    const edit = { id: "s-local", name: "local", transport: "stdio", command: "npx", args: "", url: "", headers: [], clientId: "", source: "",
      env: linesOf(["ACME_TOKEN"]) };
    const t = tree({ edit });
    t.input("Environment line 1 name").onChange({ target: { value: "ACME_KEY" } });
    const renamed = (t.got[EDIT][0] as (x: unknown) => unknown)(edit);
    const html = render({ edit: renamed });
    expect(html).toContain('aria-label="Environment line 1 name" placeholder="ACME_TOKEN" value="ACME_KEY"/>');
    expect(html).not.toContain('aria-label="Environment line 1 value" placeholder="Saved in your keychain: type to replace"');
  });
});
