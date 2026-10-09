// GA-33: the Usage page and the Projects list's AI usage column, rendered to HTML on the server. useData hands over the
// data at once; useState's first value picks the tab and the period shown. GA-62: the Subscription tab, first and the one
// the page opens on, gets its own data (state.limits) from the subscriptionLimits call.
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { CliLimits, Project, Usage, UsageTotals } from "../types";

const state = vi.hoisted(() => ({ tab: "subscription", period: "month", data: null as unknown, limits: null as unknown, LIMITS: Symbol("limits") }));
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const pick = (init: unknown) => (init === "subscription" ? state.tab : init === "month" ? state.period : init);
  const useState = ((init: unknown) => R.useState(pick(init))) as typeof R.useState;
  // The Subscription tab looks at the clock every minute: no timers on the server.
  const useEffect = (() => {}) as typeof R.useEffect;
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});
vi.mock("../api", async (orig) => ({ ...(await orig<typeof import("../api")>()), subscriptionLimits: () => state.LIMITS }));
vi.mock("../lib/useData", () => ({
  useData: (load: () => unknown) => {
    let got: unknown = null;
    try { got = load(); } catch { got = null; }
    if (got instanceof Promise) got.catch(() => {});
    return { data: got === state.LIMITS ? state.limits : state.data, error: null, reload: () => {}, setData: () => {} };
  },
}));

const { UsagePage } = await import("./UsagePage");
const { ProjectsPage } = await import("./ProjectsPage");

const DAY = 86_400_000;
const OCT_1 = Date.UTC(2026, 9, 1);
const t = (p: Partial<UsageTotals>): UsageTotals => ({ runs: 0, chatTurns: 0, inputTokens: 0, outputTokens: 0, costUsdMicros: 0, unknownCostRuns: 0, ...p });
const backend = t({ runs: 2, inputTokens: 16_000, outputTokens: 3_400, costUsdMicros: 570_000 });
const lead = t({ runs: 1, chatTurns: 1, inputTokens: 2_000, outputTokens: 100, costUsdMicros: 30_000 });
const codex = t({ runs: 1, inputTokens: 5_000, outputTokens: 500, unknownCostRuns: 1 });
const total = t({ runs: 4, chatTurns: 1, inputTokens: 23_000, outputTokens: 4_000, costUsdMicros: 600_000, unknownCostRuns: 1 });
const usage: Usage = {
  since: OCT_1, until: OCT_1 + 9 * DAY, total,
  days: Array.from({ length: 9 }, (_, i) => ({ dayStart: OCT_1 + i * DAY, totals: i === 8 ? total : t({}) })),
  agents: [
    { agentId: "a-be", name: "Backend Agent", roleKey: "backend", totals: backend },
    { agentId: "a-lead", name: "Team Lead", roleKey: "lead", totals: lead },
    { agentId: "a-codex", name: "Codex Agent", roleKey: "backend", totals: codex },
  ],
  projects: [
    { projectId: "p-kade", number: "P-1", key: "KADE", name: "Kade portal", color: "#e8c547",
      totals: t({ runs: 2, inputTokens: 17_000, outputTokens: 3_500, costUsdMicros: 420_000, unknownCostRuns: 1 }) },
    { projectId: "p-gfw", number: "P-2", key: "GFW", name: "Groene Fiets webshop", totals: t({ runs: 1, inputTokens: 4_000, outputTokens: 400, costUsdMicros: 150_000 }) },
  ],
  chat: lead, noProject: t({}),
};
const render = (tab: string, data: unknown = usage, period = "month") => {
  Object.assign(state, { tab, data, period });
  return renderToStaticMarkup(<UsagePage />);
};
const strip = (html: string) => html.replace(/<[^>]+>/g, "|").replace(/\|+/g, "|");
/** The text of the table's rows, cell by cell. */
const rows = (html: string, part: "tbody" | "tfoot") => {
  const body = html.slice(html.indexOf(`<${part}>`), html.indexOf(`</${part}>`));
  return body.split("<tr").slice(1).map((tr) => tr.split("<td").slice(1).map((td) => td.slice(td.indexOf(">") + 1).replace(/<[^>]+>/g, "")));
};

describe("the Usage page", () => {
  it("has the Subscription, Total, Agents and Projects tabs with the tabs' markup, and the period switch with This month on", () => {
    const html = render("total");
    expect(html).toContain('<div class="tabs" role="tablist">');
    expect(html.match(/<button class="tab"[^>]*>/g)).toHaveLength(4);
    expect(strip(html.slice(html.indexOf('class="tabs"'), html.indexOf('role="tabpanel"')))).toContain("|Subscription|Total|Agents|Projects|");
    expect(html).toMatch(/<button class="tab" role="tab" aria-selected="true">.*?Total<\/button>/);
    const chips = html.slice(html.indexOf('aria-label="Period"'), html.indexOf('class="spacer"'));
    expect(strip(chips)).toContain("|Today|7 days|30 days|This month|");
    expect(chips).toContain('aria-pressed="true">This month</button>');
    expect(html).toContain("<b>Usage</b><span class=\"faint\">1 Oct – 9 Oct</span>");
  });

  it("shows the Total: cost labelled as an estimate, input tokens with cache, output tokens, runs and a bar per day", () => {
    const html = render("total");
    const text = strip(html);
    expect(text).toContain("|API cost|An estimate at API prices, not a bill|$0.60|+ an unknown cost for 1 run|");
    expect(text).toContain("|Input tokens (incl. cache)|Cache reads and writes count as input|23K|");
    expect(text).toContain("|Output tokens|What the models wrote|4K|");
    expect(text).toContain("|Runs and chat turns|Of all agents|4|3 runs · 1 chat turn|");
    expect(html.match(/<div class="day"/g)).toHaveLength(18); // 9 days, a cost bar and a token bar each
    expect(html).toContain("API cost: what these tokens would cost at API prices");
    expect(html).toContain("Input tokens include cache reads and writes");
  });

  it("lists the agents, the Team Lead's chat turn included, with a total that is the Total's", () => {
    const html = render("agents");
    expect(html).toContain('aria-label="Usage per agent"');
    expect(rows(html, "tbody").map((r) => r.slice(0, 5))).toEqual([
      ["Backend Agent", "2 runs", "16K", "3.4K", "$0.57"],
      ["Team Lead", "1 chat turn", "2K", "100", "$0.03"],
      ["Codex Agent", "1 run", "5K", "500", "Unknown"],
    ]);
    expect(rows(html, "tfoot")[0].slice(0, 5)).toEqual(["Total", "3 runs · 1 chat turn", "23K", "4K", "$0.60 + unknown"]);
    expect(html).toContain('href="#/agent/a-lead"');
    expect(html).toContain("1 run reported no cost (Codex, Gemini and other CLIs don&#x27;t): its tokens count, its cost is unknown.");
  });

  it("lists the projects and the chat turns on their own line, Chat (no project)", () => {
    const html = render("projects");
    expect(html).toContain('aria-label="Usage per project"');
    expect(rows(html, "tbody").map((r) => r.slice(0, 5))).toEqual([
      ["Kade portalKADE", "2 runs", "17K", "3.5K", "$0.42 + unknown"],
      ["Groene Fiets webshopGFW", "1 run", "4K", "400", "$0.15"],
      ["Chat (no project)", "1 chat turn", "2K", "100", "$0.03"],
    ]);
    expect(rows(html, "tfoot")[0].slice(0, 5)).toEqual(["Total", "3 runs · 1 chat turn", "23K", "4K", "$0.60 + unknown"]);
    expect(html).not.toContain("Cards without a project");
  });

  it("adds a line for cards without a project only when there are runs on them", () => {
    const html = render("projects", { ...usage, noProject: t({ runs: 1, inputTokens: 10, costUsdMicros: 10_000 }) });
    expect(rows(html, "tbody").map((r) => r[0])).toEqual(["Kade portalKADE", "Groene Fiets webshopGFW", "Chat (no project)", "Cards without a project"]);
  });

  it("says when a period has no runs", () => {
    const empty: Usage = { ...usage, total: t({}), days: [{ dayStart: OCT_1, totals: t({}) }], agents: [], projects: [], chat: t({}) };
    expect(render("total", empty, "today")).toContain("<b>No runs today.</b>");
    expect(render("agents", empty)).toContain("<b>No runs this month.</b>");
    expect(render("total", empty, "7d")).toContain('aria-pressed="true">7 days</button>');
  });
});

describe("the Usage page's Subscription tab (GA-62)", () => {
  // Local times, so the labels are the same in any time zone: "now" is 9 Oct 2026, 15:00.
  const at = (day: number, h: number, m = 0) => new Date(2026, 9, day, h, m).getTime();
  const NOW = at(9, 15);
  afterEach(() => vi.restoreAllMocks());
  const agent = (agentId: string, name: string, roleKey: string, o: { status?: string; isLead?: boolean } = {}) =>
    ({ agentId, name, roleKey, status: o.status ?? "active", isLead: o.isLead ?? false });
  const limits: CliLimits[] = [
    { cliId: "claude_code", name: "Claude Code", kind: "claude_code", readable: true, accountDir: "~/.claude", leadChat: false, chats: 0,
      limits: [
        { key: "five_hour", name: "Session limit", windowMinutes: 300,
          reading: { key: "five_hour", usedPercent: 42, status: "allowed", resetsAt: at(9, 17), windowMinutes: 300, observedAt: at(9, 14, 2), runId: "r-1" } },
        { key: "seven_day", name: "Weekly limit", windowMinutes: 10_080,
          reading: { key: "seven_day", usedPercent: 85, resetsAt: at(13, 9), windowMinutes: 10_080, observedAt: at(9, 14, 2), runId: "r-1" } },
        { key: "seven_day_overage_included", name: "Fable limit", windowMinutes: 10_080, reading: null },
      ],
      agents: [agent("a-be", "Backend Agent", "backend")] },
    { cliId: "cli-2", name: "Claude Code 2", kind: "claude_code", readable: true, accountDir: "~/.claude-2", leadChat: true, chats: 2,
      limits: [
        { key: "five_hour", name: "Session limit", windowMinutes: 300,
          reading: { key: "five_hour", usedPercent: null, status: "rejected", resetsText: "3pm (Europe/Amsterdam)", observedAt: at(8, 14, 2), runId: "r-2" } },
        { key: "seven_day", name: "Weekly limit", windowMinutes: 10_080,
          reading: { key: "seven_day", usedPercent: 30, resetsAt: at(9, 12), observedAt: at(8, 9), runId: "r-2" } },
        { key: "seven_day_overage_included", name: "Fable limit", windowMinutes: 10_080,
          reading: { key: "seven_day_overage_included", usedPercent: 0.4, resetsAt: at(14, 9), observedAt: at(9, 14, 30), runId: "r-3" } },
      ],
      agents: [agent("a-lead", "Team Lead", "lead", { isLead: true }), agent("a-fe", "Frontend Agent", "frontend", { status: "paused" })] },
    { cliId: "cli-3", name: "Codex", kind: "codex", readable: true, accountDir: "~/.codex-work", leadChat: false, chats: 0,
      limits: [
        { key: "primary", name: "5-hour limit", windowMinutes: 300,
          reading: { key: "primary", usedPercent: 100, resetsAt: at(9, 18, 30), windowMinutes: 300, observedAt: at(9, 14, 1), runId: "r-4" } },
        { key: "secondary", name: "Weekly limit", windowMinutes: 10_080, reading: null },
      ],
      agents: [] },
    { cliId: "cli-4", name: "Gemini", kind: "gemini", readable: false, accountDir: null, leadChat: false, chats: 0, limits: [],
      agents: [agent("a-gem", "Gemini Agent", "design")] },
    { cliId: "cli-5", name: "Aider", kind: "other", readable: false, accountDir: null, leadChat: false, chats: 0, limits: [], agents: [] },
  ];
  const show = (data: CliLimits[] = limits) => {
    vi.spyOn(Date, "now").mockReturnValue(NOW);
    Object.assign(state, { tab: "subscription", data: usage, limits: data, period: "month" });
    return renderToStaticMarkup(<UsagePage />);
  };
  /** One block's HTML, by its coding CLI's name. */
  const block = (html: string, name: string) => {
    const from = html.indexOf(`<section class="panel limits-block" aria-label="${name}">`);
    expect(from, `a block for ${name}`).toBeGreaterThanOrEqual(0);
    return html.slice(from, html.indexOf("</section>", from));
  };

  it("is the first tab and the one the page opens on; the period switch and the UTC days are for the other tabs", () => {
    const html = show();
    expect(strip(html.slice(html.indexOf('class="tabs"'), html.indexOf('role="tabpanel"')))).toContain("|Subscription|Total|Agents|Projects|");
    expect(html).toMatch(/<button class="tab" role="tab" aria-selected="true">.*?Subscription<\/button>/);
    expect(html).not.toContain('aria-label="Period"');
    expect(html).not.toContain("UTC days");
    expect(html).toContain("<b>Usage</b></div>");
    expect(html).toContain("The newest numbers each coding CLI reported in your agents&#x27; runs and chat turns");
    expect(html).toContain("it never asks Anthropic or OpenAI");
    expect(html).not.toContain("API cost: what these tokens would cost");
  });

  it("has one block per coding CLI entry, in Settings' order, the two Claude Code accounts apart", () => {
    const html = show();
    const names = [...html.matchAll(/<section class="panel limits-block" aria-label="([^"]+)">/g)].map((m) => m[1]);
    expect(names).toEqual(["Claude Code", "Claude Code 2", "Codex", "Gemini", "Aider"]);
    expect(block(html, "Claude Code")).toContain("~/.claude<");
    expect(block(html, "Claude Code 2")).toContain("~/.claude-2<");
    // The kind shows only when the name doesn't say it already.
    expect(strip(block(html, "Claude Code"))).toMatch(/^\|Claude Code\|~\/\.claude\|/);
    expect(strip(block(html, "Claude Code 2"))).toMatch(/^\|Claude Code 2\|Claude Code\|~\/\.claude-2\|/);
  });

  it("shows a Claude Code account's session, weekly and Fable limits: used, resets, as of when; amber from 80%; one not reported says so", () => {
    const cc = block(show(), "Claude Code");
    expect(cc).toContain('aria-label="Claude Code limits"');
    expect(strip(cc.slice(cc.indexOf("<thead>"), cc.indexOf("</thead>")))).toBe("|Limit|Used|Resets|Updated|");
    expect(rows(cc, "tbody")).toEqual([
      ["Session limit", "42%", "17:00", "as of 14:02"],
      ["Weekly limit", "85%", "Tue 13 Oct, 09:00", "as of 14:02"],
      ["Fable limit", "Not reported yet", "", ""],
    ]);
    expect(cc.match(/<tr class="limit-(\w+)">/g)).toEqual(['<tr class="limit-ok">', '<tr class="limit-near">', '<tr class="limit-unread">']);
    expect(cc).toContain('style="width:42%"');
    expect(cc).toContain("Claude Code reports the Fable limit only for an account that has one");
    expect(cc).toMatch(/title="Friday,? 9 October 2026 at 14:02"/);
    expect(cc).toContain("Claude Code reports these limits in the runs and chat turns on Claude Code when they change, on a Claude subscription only.");
  });

  it("shows the second account's own numbers: a limit reached in words, a window that reset since, and under 1%", () => {
    const cc2 = block(show(), "Claude Code 2");
    expect(rows(cc2, "tbody")).toEqual([
      ["Session limit", "Limit reached", "3pm (Europe/Amsterdam)", "as of yesterday 14:02"],
      ["Weekly limit", "Reset at 12:00 · no newer number", "", "as of yesterday 09:00"],
      ["Fable limit", "<1%", "Wed 14 Oct, 09:00", "as of 14:30"],
    ].map((r) => r.map((c) => c.replace("<", "&lt;"))));
    expect(cc2.match(/<tr class="limit-(\w+)">/g)).toEqual(['<tr class="limit-reached">', '<tr class="limit-reset">', '<tr class="limit-ok">']);
    // The reset number isn't shown as if it still held.
    expect(cc2).not.toContain("30%");
  });

  it("lists the agents on each account, the Team Lead first, paused ones marked, and which chats run there", () => {
    const html = show();
    const cc = block(html, "Claude Code"), cc2 = block(html, "Claude Code 2");
    expect(strip(cc.slice(cc.indexOf('class="limits-agents"')))).toContain("|Agents on it:|Backend Agent|");
    expect(cc).toContain('href="#/agent/a-be"');
    expect(cc).not.toContain("Team Lead");
    expect(strip(cc2.slice(cc2.indexOf('class="limits-agents"')))).toContain("|Agents on it:|Team Lead|Frontend Agent| (paused)|");
    expect(cc2).toContain("The Team Lead&#x27;s chat runs here, and 2 chats picked it under Runs on.");
    expect(block(html, "Codex")).toContain("No agent runs on Codex.");
  });

  it("shows a Codex account's plan windows the same way, red when reached, and says where it reads them", () => {
    const codex = block(show(), "Codex");
    expect(codex).toContain('aria-label="Codex limits"');
    expect(rows(codex, "tbody")).toEqual([
      ["5-hour limit", "100%", "18:30", "as of 14:01"],
      ["Weekly limit", "Not read yet", "", ""],
    ]);
    expect(codex.match(/<tr class="limit-(\w+)">/g)).toEqual(['<tr class="limit-reached">', '<tr class="limit-unread">']);
    expect(codex).toContain("Codex writes these limits in its session log (~/.codex-work/sessions). Gizai reads the log of each run on Codex when the run ends.");
  });

  it("says Gizai can't read Gemini's and an Other CLI's limits yet, with no table and no number", () => {
    const html = show();
    const gemini = block(html, "Gemini"), other = block(html, "Aider");
    expect(gemini).toContain("Gizai can&#x27;t read Gemini&#x27;s limits yet.");
    expect(other).toContain("Gizai can&#x27;t read the limits of Aider yet: it reads Claude Code&#x27;s and Codex&#x27;s.");
    for (const b of [gemini, other]) {
      expect(b).not.toContain("<table");
      expect(b).not.toContain("%");
    }
    expect(gemini).toContain("Gemini Agent");
  });

  it("says no run has reported an account's limits yet when none has, and shows no number", () => {
    const [cc1] = limits;
    if (!cc1) throw new Error("no Claude Code block");
    const fresh: CliLimits = { ...cc1, limits: cc1.limits.map((l) => ({ ...l, reading: null })) };
    const cc = block(show([fresh]), "Claude Code");
    expect(rows(cc, "tbody").map((r) => r.slice(1).join("|"))).toEqual(["Not reported yet||", "Not reported yet||", "Not reported yet||"]);
    expect(cc).toContain("No run on Claude Code has reported its limits yet.");
    expect(cc).not.toContain("%");
  });
});

describe("the Projects list", () => {
  const project = (o: Partial<Project>): Project => ({
    id: "p", number: "P-1", key: "KADE", name: "Kade portal", status: "active", defaultBranch: "main", openTasks: 1, doneTasks: 1, updatedAt: 0,
    aiCostUsdMicros: 0, aiUnknownCostRuns: 0, worktreeCopy: [], worktreeInstall: false, ...o,
  });

  it("has a sortable AI usage column that says what the cost is", () => {
    state.data = [project({ aiCostUsdMicros: 1_420_000, aiUnknownCostRuns: 1 })];
    const html = renderToStaticMarkup(<ProjectsPage />);
    const th = html.match(/<th[^>]*title="API cost this month[^"]*"[^>]*>AI usage<\/th>/)?.[0];
    expect(th).toBeDefined();
    expect(th).toContain("cursor:pointer");
    expect(th).toContain("not a bill");
    expect(html).toContain("AI usage: the API cost this month, an estimate at API prices, not a bill.");
    expect(html).toContain('href="#/usage"');
  });
});
