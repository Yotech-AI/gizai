// GA-227 QA: the New team drawer's check of a template and Export's question, rendered to HTML on the server (nothing is
// clicked: scripts/ui-test.sh's templates run clicks them in the app), plus the file names and the save dialog's permission.
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";
import type { TemplatePlan, TemplateWarning } from "../types";
import { ExportWarnings, PAUSED_LINE, TemplateCheck, baseName, kindWord, templateFileName } from "./TeamTemplate";

/** The text a person reads: tags out, entities back. */
const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, "&")
  .replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/\s+/g, " ");
/** One element of the class's HTML, up to the next one of that class. */
const parts = (html: string, cls: string) => html.split(`class="${cls}"`).slice(1);

const fromFile: TemplatePlan = {
  teamName: "Mobile team 2",
  columns: [
    { name: "Backlog", kind: "backlog", auto: false, next: null, agents: [] },
    { name: "Build", kind: "in_progress", auto: true, next: "Check", agents: ["Backend Agent (Mobile team 2)", "Codex Agent"] },
    { name: "Check", kind: "testing", auto: false, next: "Shipped", agents: ["QA Bot"] },
    { name: "Shipped", kind: "done", auto: false, next: null, agents: [] },
  ],
  agents: [
    { name: "Backend Agent (Mobile team 2)", fileName: "Backend Agent", role: "backend", cliName: "Claude Code", model: "opus", effort: "high",
      permissionMode: "acceptEdits", extraTools: ["Bash(rm -rf:*)", "Bash(curl:*)"], missingServers: ["figma", "linear"], active: false },
    { name: "Codex Agent", fileName: "Codex Agent", role: "frontend", cliName: "Claude Code", model: null, effort: null, permissionMode: "acceptEdits",
      extraTools: [], missingServers: [], active: false },
    { name: "QA Bot", fileName: "QA Bot", role: "qa", cliName: "Codex", model: "gpt-5", effort: "medium", permissionMode: "workspace-write",
      extraTools: [], missingServers: [], active: false },
  ],
  labelsAdded: ["mobile", "ios"],
  notes: ["No Codex CLI is set up: Codex Agent runs on Claude Code.", "Cards at once for QA Bot is set to 10.", "A third note, as it comes."],
};
const builtin: TemplatePlan = {
  teamName: "Software team",
  columns: [
    { name: "Backlog", kind: "backlog", auto: false, next: null, agents: [] },
    { name: "To do", kind: "ready", auto: true, next: "In progress", agents: ["Backend Agent", "Frontend Agent"] },
    { name: "Done", kind: "done", auto: false, next: null, agents: [] },
  ],
  agents: [
    { name: "Backend Agent", fileName: "Backend Agent", role: "backend", cliName: "Claude Code", model: null, effort: null, permissionMode: "acceptEdits",
      extraTools: [], missingServers: [], active: true },
    { name: "Frontend Agent", fileName: "Frontend Agent", role: "frontend", cliName: "Claude Code", model: null, effort: null, permissionMode: "acceptEdits",
      extraTools: [], missingServers: [], active: true },
  ],
  labelsAdded: [],
  notes: [],
};

describe("the check of a template file", () => {
  const html = renderToStaticMarkup(<TemplateCheck plan={fromFile} fromFile />);
  const all = text(html);

  it("lists the columns in order, each with its kind, Auto or Manual, its next column and its agents", () => {
    const rows = [...html.matchAll(/<tr>(.*?)<\/tr>/g)].map((m) => [...m[1].matchAll(/<t[dh]>(.*?)<\/t[dh]>/g)].map((c) => text(c[1]).trim()));
    expect(rows).toEqual([
      ["Column", "Kind", "Auto or Manual", "Next column", "Agents"],
      ["Backlog", "Backlog", "Manual", "None", "None"],
      ["Build", "Work", "Auto", "Check", "Backend Agent (Mobile team 2), Codex Agent"],
      ["Check", "Testing", "Manual", "Shipped", "QA Bot"],
      ["Shipped", "Done", "Manual", "None", "None"],
    ]);
  });

  it("shows each agent's name, its name in the file when that differs, role, CLI, model, effort and permission mode", () => {
    const cards = parts(html, "tt-agent").map(text);
    expect(cards).toHaveLength(3);
    expect(cards[0]).toContain("Backend Agent (Mobile team 2) Backend Agent in the file Paused");
    expect(cards[0]).toContain("Role backend CLI Claude Code Model opus Effort high Permission mode acceptEdits");
    expect(cards[1]).not.toContain("in the file");
    expect(cards[1]).toContain("Model The CLI's default Effort The CLI's default");
    expect(cards[2]).toContain("Role qa CLI Codex Model gpt-5 Effort medium Permission mode workspace-write");
  });

  it("shows the extra commands under their heading, highlighted, only for the agent that has them", () => {
    const extras = parts(html, "tt-extra");
    expect(extras).toHaveLength(1);
    expect(text(extras[0])).toContain("Commands beyond its role's usual ones Bash(rm -rf:*) Bash(curl:*)");
    expect(html.match(/<h4>Commands beyond its role&#x27;s usual ones<\/h4>/g)).toHaveLength(1);
    expect(html).toContain('aria-label="Backend Agent (Mobile team 2): commands beyond its role&#x27;s usual ones"');
    const cards = parts(html, "tt-agent");
    expect(cards[0]).toContain('class="tt-extra"');
    expect(cards[1]).not.toContain("tt-extra");
    expect(cards[2]).not.toContain("tt-extra");
  });

  it("highlights the extra commands in the error banner's colours", () => {
    const css = readFileSync(new URL("../styles/app.css", import.meta.url), "utf8");
    const rule = (sel: string) => new RegExp(`^${sel.replace(".", "\\.")} \\{([^}]*)\\}`, "m").exec(css)?.[1] ?? "";
    const colours = (r: string) => ({ border: /border: 1px solid ([^;]+);/.exec(r)?.[1], color: /(?:^|;)\s*color: ([^;]+);/.exec(r)?.[1],
      background: /background: ([^;]+);/.exec(r)?.[1] });
    expect(colours(rule(".tt-extra"))).toEqual(colours(rule(".error-banner")));
    expect(colours(rule(".tt-extra"))).toEqual({ border: "var(--danger)", color: "var(--danger)", background: "var(--danger-soft)" });
  });

  it("says which MCP servers are missing, word for word", () => {
    expect(all).toContain("Needs MCP server figma: add it in Settings → MCP to switch it on.");
    expect(all).toContain("Needs MCP server linear: add it in Settings → MCP to switch it on.");
    expect(text(parts(html, "tt-agent")[1])).not.toContain("Needs MCP server");
  });

  it("shows the labels it adds and every note", () => {
    expect(all).toContain("Labels added: mobile, ios");
    const notes = [...html.matchAll(/<li>([^<]*)<\/li>/g)].map((m) => text(m[1]));
    for (const n of fromFile.notes) expect(notes).toContain(n);
  });

  it("puts the paused line, word for word, above the agents", () => {
    expect(PAUSED_LINE).toBe("Agents from a file come in paused. Read their instructions and commands, then switch them on in their form.");
    expect(all).toContain(PAUSED_LINE);
    expect(html.indexOf("tt-paused")).toBeLessThan(html.indexOf('class="tt-agents"'));
    expect(html.indexOf('class="tt-table"')).toBeLessThan(html.indexOf("tt-paused"));
  });
});

describe("the check of the built-in Software team", () => {
  const html = renderToStaticMarkup(<TemplateCheck plan={builtin} fromFile={false} />);

  it("has no paused line and no extra-commands heading", () => {
    expect(html).not.toContain(PAUSED_LINE);
    expect(html).not.toContain("Commands beyond");
    expect(html).not.toContain("Paused");
    expect(text(html)).toContain("No labels added.");
    expect(html).not.toContain('aria-label="Notes"');
  });
});

describe("Export's question", () => {
  const warnings: TemplateWarning[] = [
    { agent: "Backend Agent", field: "instructions_md", kind: "email", excerpt: "mail jef@example.com when done" },
    { agent: null, field: "description_md", kind: "path", excerpt: "see /home/jef/notes" },
    { agent: "QA Agent", field: "allowed_tools", kind: "secret", excerpt: "Bash(curl -H 'ghp_abc…':*)" },
  ];
  const html = renderToStaticMarkup(<ExportWarnings warnings={warnings} onExport={() => {}} onCancel={() => {}} />);

  it("is an alert dialog titled This may share private details", () => {
    expect(html).toContain('role="alertdialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toMatch(/aria-labelledby="tt-warn-q"/);
    expect(html).toContain('<h2 id="tt-warn-q">This may share private details</h2>');
  });

  it("lists each warning with its agent (or the team), its field and the excerpt", () => {
    const items = [...html.matchAll(/<li>(.*?)<\/li>/g)].map((m) => text(m[1]).trim());
    expect(items).toEqual([
      "Instructions of Backend Agent : an email address mail jef@example.com when done",
      "Description of the team : a local path see /home/jef/notes",
      "Allowed commands of QA Agent : what looks like a key or token Bash(curl -H 'ghp_abc…':*)",
    ]);
  });

  it("offers Cancel and Export anyway", () => {
    const buttons = [...html.matchAll(/<button[^>]*>([^<]*)<\/button>/g)].map((m) => m[1]);
    expect(buttons).toEqual(["Cancel", "Export anyway"]);
  });
});

describe("file names", () => {
  it("offers <team name>.gizai-team.json", () => {
    expect(templateFileName("Software team")).toBe("Software team.gizai-team.json");
    expect(templateFileName("Web/App: v2")).toBe("Web-App- v2.gizai-team.json");
    expect(templateFileName("   ")).toBe("Team.gizai-team.json");
  });

  it("shows a saved path's file name", () => {
    expect(baseName("/home/me/Templates/Software team.gizai-team.json")).toBe("Software team.gizai-team.json");
    expect(baseName("C:\\Users\\me\\Mobile.gizai-team.json")).toBe("Mobile.gizai-team.json");
  });

  it("names each column kind in plain words", () => {
    expect(["backlog", "ready", "in_progress", "testing", "review", "deploy", "done", "cancelled"].map(kindWord))
      .toEqual(["Backlog", "Waiting", "Work", "Testing", "Review", "Deploy", "Done", "Cancelled"]);
  });
});

describe("the app's permissions", () => {
  it("let it open the save dialog, as Export needs, and the open dialog, as A template file… needs", () => {
    const caps = JSON.parse(readFileSync(new URL("../../src-tauri/capabilities/default.json", import.meta.url), "utf8")) as { permissions: string[] };
    expect(caps.permissions).toContain("dialog:allow-save");
    expect(caps.permissions).toContain("dialog:allow-open");
  });
});
