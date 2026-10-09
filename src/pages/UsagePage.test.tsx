// GA-33: the Usage page and the Projects list's AI usage column, rendered to HTML on the server. useData hands over the
// data at once; useState's first value picks the tab and the period shown.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Project, Usage, UsageTotals } from "../types";

const state = vi.hoisted(() => ({ tab: "total", period: "month", data: null as unknown }));
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const pick = (init: unknown) => (init === "total" ? state.tab : init === "month" ? state.period : init);
  const useState = ((init: unknown) => R.useState(pick(init))) as typeof R.useState;
  return { ...R, default: { ...R, useState }, useState };
});
vi.mock("../lib/useData", () => ({ useData: () => ({ data: state.data, error: null, reload: () => {}, setData: () => {} }) }));

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
  it("has the Total, Agents and Projects tabs with the tabs' markup, and the period switch with This month on", () => {
    const html = render("total");
    expect(html).toContain('<div class="tabs" role="tablist">');
    expect(html.match(/<button class="tab"[^>]*>/g)).toHaveLength(3);
    expect(strip(html.slice(html.indexOf('class="tabs"'), html.indexOf('role="tabpanel"')))).toContain("|Total|Agents|Projects|");
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
