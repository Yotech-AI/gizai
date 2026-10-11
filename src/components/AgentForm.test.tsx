// GA-45: the agent form's Folders list (Permissions). Rendered to HTML on the server, so no data loads: this checks
// what a new agent's form shows, and what the form saves and loads again.
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { AgentDrawer } from "./AgentForm";
import { draftFrom, foldersFrom, inputFrom } from "../lib/agents";
import { FOLDERS_NOTE } from "../lib/clis";
import type { Member } from "../types";

describe("Folders in the agent form", () => {
  it("shows the list with an Add folder button and says it limits the file tools, not the commands", () => {
    const html = renderToStaticMarkup(<AgentDrawer teamId="t1" onClose={() => {}} />);
    const permissions = html.slice(html.indexOf("Permissions"));
    expect(permissions).toContain(">Folders<");
    expect(permissions).toContain("Add folder");
    expect(html).toContain("They limit the file tools, not the commands it may run: an allowed command can still reach any folder.");
    expect(html).toContain("Never /, your home folder, Gizai&#x27;s data folder or folders with keys (~/.ssh, ~/.gnupg, ~/.config, …).");
    expect(html).toContain(FOLDERS_NOTE.claude_code.replace(/'/g, "&#x27;"));
    expect(html).not.toContain("update_checkout");
  });

  it("tells the Team Lead that it only reads them in chat", () => {
    const html = renderToStaticMarkup(<AgentDrawer teamId="t1" preset={{ name: "Team Lead", role: "lead", chat: true }} onClose={() => {}} />);
    expect(html).toContain("In chat the Team Lead only reads them; read and change also lets it update that folder (update_checkout) after you say yes in the chat.");
  });

  it("says for each CLI what it does with the folders, and that an Other CLI can't limit them", () => {
    expect(Object.keys(FOLDERS_NOTE).sort()).toEqual(["claude_code", "codex", "gemini", "other"]);
    expect(FOLDERS_NOTE.claude_code).toContain("can't edit or write files in a read folder");
    expect(FOLDERS_NOTE.gemini).toContain("only the read and change folders");
    expect(FOLDERS_NOTE.other).toContain("can't limit folders");
  });
});

describe("saving and loading the folders", () => {
  it("starts a new agent with none", () => {
    expect(draftFrom(null).folders).toEqual([]);
    expect(inputFrom(draftFrom(null)).folders).toEqual([]);
  });

  it("sends the rows trimmed, without the empty ones, each with its access", () => {
    const d = { ...draftFrom(null), folders: [{ path: " ~/Herd/shared ", access: "read" as const }, { path: "   ", access: "change" as const },
      { path: "/srv/out", access: "change" as const }] };
    expect(inputFrom(d).folders).toEqual([{ path: "~/Herd/shared", access: "read" }, { path: "/srv/out", access: "change" }]);
    expect(foldersFrom([])).toEqual([]);
  });

  it("loads a saved agent's folders back into the form, as saved", () => {
    const m = { actorId: "a1", name: "Backend Agent", roleKey: "backend", kind: "agent", isLead: false, allowedTools: [],
      folders: [{ path: "/home/u/Herd/shared", access: "read" }, { path: "/srv/out", access: "change" }] } as unknown as Member;
    const d = draftFrom(m);
    expect(d.folders).toEqual([{ path: "/home/u/Herd/shared", access: "read" }, { path: "/srv/out", access: "change" }]);
    expect(inputFrom(d).folders).toEqual(d.folders);
    // an agent saved before folders existed has none
    expect(draftFrom({ ...m, folders: undefined }).folders).toEqual([]);
  });

  it("keeps the agent's folders when another field changes", () => {
    const m = { actorId: "a1", name: "Backend Agent", roleKey: "backend", kind: "agent", isLead: false, allowedTools: [],
      folders: [{ path: "/srv/out", access: "change" }] } as unknown as Member;
    const input = inputFrom({ ...draftFrom(m), model: "sonnet" });
    expect([input.model, input.folders]).toEqual(["sonnet", [{ path: "/srv/out", access: "change" }]]);
  });
});

// GA-53 merged with GA-39: the agent form without Wake-up or heartbeat (the columns decide) next to the Tools section.
describe("the agent form with GA-39's Tools and GA-53's columns", () => {
  const sections = (html: string) => [...html.matchAll(/<h3>([^<]+)<\/h3>/g)].map((m) => m[1]);
  const section = (html: string, title: string) => {
    const at = html.indexOf(`<h3>${title}</h3>`);
    const next = html.indexOf("<h3>", at + 1);
    return html.slice(at, next < 0 ? undefined : next);
  };

  it("has Agent, Chat, Memory (GA-19), Work, Permissions, Tools and Instructions, and no Wake-up or heartbeat, opened from an empty spot", () => {
    const html = renderToStaticMarkup(<AgentDrawer teamId="t1" preset={{ role: "backend" }} onClose={() => {}} />);
    expect(sections(html)).toEqual(["Agent", "Chat", "Memory", "Work", "Permissions", "Tools", "Instructions"]);
    // Use memory, on for a new agent
    expect(section(html, "Memory")).toContain('<input id="a-memory" type="checkbox" checked=""/> Use memory');
    expect(html).not.toContain("Wakes up");
    expect(html).not.toContain("Wake-up");
    expect(html).not.toMatch(/heartbeat/i);
    // Work holds only Cards at once
    const work = section(html, "Work");
    expect([...work.matchAll(/<label for="[^"]*">([^<]+)<\/label>/g)].map((m) => m[1])).toEqual(["Cards at once"]);
    expect(section(html, "Tools")).toContain(">MCP servers<");
    expect(html).toMatch(/<option value="backend" selected="">/);
  });

  it("keeps the Team Lead's board check in Chat, with Tools further down", () => {
    const html = renderToStaticMarkup(<AgentDrawer teamId="t1" preset={{ name: "Team Lead", role: "lead", chat: true }} onClose={() => {}} />);
    expect(sections(html)).toEqual(["Agent", "Chat", "Memory", "Work", "Permissions", "Tools", "Instructions"]);
    expect(section(html, "Chat")).toContain('aria-label="Board check minutes"');
    expect(html.indexOf("Board check")).toBeLessThan(html.indexOf("<h3>Tools</h3>"));
    expect(html).not.toMatch(/heartbeat/i);
  });

  it("loads an agent's MCP switches next to no wake-up, and saves the rest without them or an old heartbeat", () => {
    const m = { actorId: "a1", name: "Backend Agent 2", roleKey: "backend", kind: "agent", isLead: false, allowedTools: [], chatEnabled: false,
      wakeup: "heartbeat", heartbeatMinutes: 20, boardCheckMinutes: null,
      tools: { mcp: [{ serverId: "otus", on: true, toolsOff: ["delete_doc"] }] } } as unknown as Member;
    const d = draftFrom(m);
    expect(d.mcp).toEqual([{ serverId: "otus", on: true, toolsOff: ["delete_doc"] }]);
    expect(d).not.toHaveProperty("wakeup");
    const input = inputFrom({ ...d, maxRuns: "2" });
    expect(input.maxRuns).toBe(2);
    // the switches go on their own (saveAgentMcp); the old wake-up and heartbeat go
    for (const k of ["mcp", "tools", "wakeup", "heartbeatMinutes"]) expect(input, k).not.toHaveProperty(k);
  });
});

// GA-96: Shares memory with, in the Memory section under Use memory. Rendered on the server, so no agents load: the
// choices themselves are checked in src/lib/memoryGroups.test.ts.
describe("Shares memory with in the agent form (GA-96)", () => {
  const memory = (html: string) => html.slice(html.indexOf("<h3>Memory</h3>"), html.indexOf("<h3>", html.indexOf("<h3>Memory</h3>") + 1));
  const select = (html: string) => memory(html).match(/<select id="a-shares"[^>]*>.*?<\/select>/)?.[0] ?? "";

  it("sits under Use memory, on Its own folder for a new agent, and says what sharing does", () => {
    const html = memory(renderToStaticMarkup(<AgentDrawer teamId="t1" preset={{ role: "backend" }} onClose={() => {}} />));
    expect(html).toContain("Each agent has its own folder in Memory, or shares another agent&#x27;s.");
    expect(html.indexOf("Use memory")).toBeGreaterThan(-1);
    expect(html.indexOf('<label for="a-shares">Shares memory with</label>')).toBeGreaterThan(html.indexOf("Use memory"));
    expect(select(html)).toMatch(/^<select id="a-shares" class="select">/);
    expect(select(html)).toContain('<option value="" selected="">Its own folder</option>');
    expect(html).toContain("Backend Agent 2 with Backend Agent");
    expect(html).toContain("Joining moves its notes into that folder; back to its own folder, it starts a fresh Notes.");
  });

  it("names the new agent's own folder by its name", () => {
    const html = memory(renderToStaticMarkup(<AgentDrawer teamId="t1" preset={{ name: "Ops: 2/3", role: "devops" }} onClose={() => {}} />));
    expect(html).toContain("Its own folder (Agents/Ops- 2-3/)");
  });

  it("is off on the Team Lead's form, which keeps its own notes", () => {
    for (const preset of [{ name: "Team Lead", role: "lead", chat: true }, { name: "Team Lead", role: "lead" }, { name: "Chat Agent", role: "backend", chat: true }]) {
      const html = memory(renderToStaticMarkup(<AgentDrawer teamId="t1" preset={preset} onClose={() => {}} />));
      expect(select(html), JSON.stringify(preset)).toMatch(/^<select id="a-shares" class="select" disabled="">/);
      expect(html).toContain("The Team Lead keeps its own notes, in Team Lead/.");
    }
  });
});
