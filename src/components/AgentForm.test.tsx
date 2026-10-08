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
