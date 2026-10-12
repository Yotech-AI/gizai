// GA-102: the Team Lead's calls that wait for your approval, as the chat shows them: "Waiting for your OK", why (the outside
// tool in words), each call's plain line and tool, its preview (what it is about and what changes, a warning, the text or what
// changes against the current version), Approve and Discard, and Approve all when several wait. In the chat they sit under
// the answer that tried them. Rendered to HTML on the server with the conversation handed in.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { ChatMessage, Member, PendingAction, PendingPreview } from "../../types";

vi.mock("../../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) out[k] = typeof v !== "function" ? v : () => new Promise(() => {});
  return out;
});
const chat = vi.hoisted(() => ({ messages: [] as ChatMessage[], pending: [] as PendingAction[] }));
vi.mock("./useChat", () => ({
  useChat: () => ({ messages: chat.messages, draft: "", tool: null, working: false, queue: [], pending: chat.pending, error: null, setWorking: () => {} }),
}));

const { PendingActions } = await import("./PendingActions");
const { ChatThread } = await import("./ChatThread");

const preview = (o: Partial<PendingPreview>): PendingPreview => ({ line: "Do it", fields: [], ...o });
const action = (id: string, o: Partial<PendingAction> = {}): PendingAction => ({
  id, threadId: "c1", runId: "r1", actorId: "lead", tool: "memory_write", args: {}, outsideTool: "mcp__otus__search", status: "pending", createdAt: 0, ...o,
});
const render = (actions: PendingAction[]) => renderToStaticMarkup(<PendingActions actions={actions} onError={() => {}} />);
const count = (html: string, s: string) => html.split(s).length - 1;

describe("a call that waits for your OK", () => {
  const write = action("a1", { preview: preview({
    line: "Save a new version of the note Lessons/Otus",
    fields: [{ label: "Note", value: "Lessons/Otus" }, { label: "Version", value: "4", from: "3" }],
    text: "Fridays.\n- Since October.",
    diff: [{ kind: "same", text: "# Otus" }, { kind: "del", text: "Thursdays." }, { kind: "add", text: "Fridays." }, { kind: "add", text: "- Since October." }],
  }) });
  const html = render([write]);

  it("says it waits and why, in words", () => {
    expect(html).toContain('aria-label="Waiting for your OK"');
    expect(html).toContain("Waiting for your OK</span>");
    expect(html).toContain("This answer used search on the otus server, a tool from outside Gizai, so Gizai didn&#x27;t run this.");
    expect(html).toContain('title="mcp__otus__search"');
  });

  it("shows its plain line, the tool, and what changes from what to what", () => {
    expect(html).toContain("<b>Save a new version of the note Lessons/Otus</b>");
    expect(html).toContain('<span class="pa-tool">memory_write</span>');
    expect(html).toMatch(/<dt>Note<\/dt><dd>Lessons\/Otus<\/dd>/);
    expect(html).toMatch(/<dt>Version<\/dt><dd><span class="pa-from">3<\/span><svg[^]*?aria-label="becomes"[^]*?<\/svg>4<\/dd>/);
  });

  it("shows what changes against the current version line by line, instead of the whole text", () => {
    expect(html).toContain('aria-label="What changes"');
    expect(html).toMatch(/<div class="pa-dl del"><span class="pa-sign" aria-label="removed">−<\/span>Thursdays.<\/div>/);
    expect(html).toMatch(/<div class="pa-dl add"><span class="pa-sign" aria-label="added">\+<\/span>Fridays.<\/div>/);
    expect(html).toMatch(/<div class="pa-dl same"><span class="pa-sign">.*?<\/span># Otus<\/div>/);
    expect(html).not.toContain('class="pa-text"');
  });

  it("has Approve and Discard, and no Approve all for one call", () => {
    expect(count(html, ">Approve</button>")).toBe(1);
    expect(count(html, ">Discard</button>")).toBe(1);
    expect(html).toContain('title="Runs exactly this call, once"');
    expect(html).toContain('title="Runs nothing"');
    expect(html).not.toContain("Approve all");
  });

  it("shows the text exactly when there is nothing to compare with, and a warning when approving would fail", () => {
    const h = render([action("a2", { tool: "memory_append", preview: preview({ line: "Add to the note Team Lead/Notes",
      fields: [{ label: "Where", value: "At the end of the section Working agreements" }], text: "- Fridays.\n- <b>not bold</b>",
      warning: "Team Lead/Notes changed since the Team Lead read it (version 1; it is at version 2 now), so approving fails and nothing is saved." }) })]);
    expect(h).toContain('<pre class="pa-text" aria-label="Text">- Fridays.\n- &lt;b&gt;not bold&lt;/b&gt;</pre>');
    expect(h).toMatch(/<div class="pa-warn"><svg[^]*?<\/svg><span>Team Lead\/Notes changed since the Team Lead read it/);
    expect(h).not.toContain('aria-label="What changes"');
  });

  it("folds long unchanged runs of a diff", () => {
    const same = Array.from({ length: 12 }, (_, i) => ({ kind: "same" as const, text: `line ${i}` }));
    const h = render([action("a3", { preview: preview({ diff: [...same, { kind: "add", text: "new" }] }) })]);
    expect(h).toContain('<button type="button" class="pa-fold">10 unchanged lines</button>');
    expect(h).toContain("line 10") ;
    expect(h).not.toContain("line 9<");
  });

  it("shows the tool when it has no preview, and a fallback line never breaks the list", () => {
    const h = render([action("a4", { tool: "set_column", preview: undefined })]);
    expect(h).toContain("<b>set_column</b>");
    expect(h).toContain(">Approve</button>");
  });
});

describe("several calls under one answer", () => {
  it("adds Approve all, which runs the ones still waiting", () => {
    const h = render([action("a1", { preview: preview({ line: "Pause Backend Agent" }) }), action("a2", { preview: preview({ line: "Add the column Otus after Review" }) })]);
    expect(count(h, ">Approve all</button>")).toBe(1);
    expect(h).toContain('title="Runs each of these, in this order"');
    expect(count(h, ">Approve</button>")).toBe(2);
    expect(count(h, ">Discard</button>")).toBe(2);
    expect(h).toContain("so Gizai didn&#x27;t run these.");
    expect(h.indexOf("Pause Backend Agent")).toBeLessThan(h.indexOf("Add the column Otus after Review"));
  });

  it("shows one you approved as running, without buttons, and no Approve all for a single one left", () => {
    const h = render([action("a1", { status: "running", preview: preview({ line: "Pause Backend Agent" }) }),
      action("a2", { preview: preview({ line: "Add the column Otus after Review" }) })]);
    expect(h).toContain("Approved: running…");
    expect(count(h, ">Approve</button>")).toBe(1);
    expect(h).not.toContain("Approve all");
  });

  it("names a web tool in words", () => {
    expect(render([action("a1", { outsideTool: "WebFetch" })])).toContain("This answer used fetching a web page, a tool from outside Gizai");
  });
});

// ---- in the chat ----

const lead = { actorId: "lead", name: "Team Lead", kind: "agent", roleKey: "lead", handle: "lead", status: "active", isLead: false, allowedTools: [],
  chatEnabled: true } as Member;
const at = Date.now() - 3_600_000;
const msg = (id: string, role: string, body: string, runId: string | null, minute: number): ChatMessage => ({
  id, threadId: "c1", role, authorId: role === "user" ? "you" : "lead", authorName: role === "user" ? "You" : "Team Lead", bodyMd: body, runId,
  toolName: null, tool: null, createdAt: at + minute * 60_000,
});

describe("in the chat", () => {
  chat.messages = [
    msg("u1", "user", "QUESTION-ONE", null, 0), msg("a1", "agent", "ANSWER-ONE", "r1", 1),
    msg("u2", "user", "QUESTION-TWO", null, 10), msg("a2", "agent", "ANSWER-TWO", "r2", 11),
    msg("n1", "system", "Approved: save the note Lessons/Otus. Saved as version 1.", null, 12),
  ];
  chat.pending = [
    action("p1", { runId: "r1", preview: preview({ line: "CALL-OF-ONE" }) }),
    action("p2", { runId: "r2", preview: preview({ line: "CALL-OF-TWO" }) }),
    action("p3", { runId: "r-gone", preview: preview({ line: "CALL-WITHOUT-ANSWER" }) }),
    action("p4", { threadId: "other", runId: "r1", preview: preview({ line: "CALL-OF-ANOTHER-CHAT" }) }),
  ];
  const html = renderToStaticMarkup(<ChatThread threadId="c1" agent={lead} />);
  const i = (s: string) => { const n = html.indexOf(s); expect(n, s).toBeGreaterThan(-1); return n; };

  it("shows each call under the answer that tried it, before your next message", () => {
    expect(i("ANSWER-ONE")).toBeLessThan(i("CALL-OF-ONE"));
    expect(i("CALL-OF-ONE")).toBeLessThan(i("QUESTION-TWO"));
    expect(i("ANSWER-TWO")).toBeLessThan(i("CALL-OF-TWO"));
  });

  it("shows a call whose answer isn't in the chat at the end, and never another chat's", () => {
    expect(i("Approved: save the note Lessons/Otus")).toBeLessThan(i("CALL-WITHOUT-ANSWER"));
    expect(html).not.toContain("CALL-OF-ANOTHER-CHAT");
    expect(count(html, 'aria-label="Waiting for your OK"')).toBe(3);
  });
});
