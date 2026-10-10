// GA-41: the chat with the @ picker and files. The composer has a + button (a menu that opens upward) and the @ tip (since
// GA-83 both in the row under the text, inside the box); your sent and queued messages show their links to Gizai items as chips and their files; shown Markdown keeps
// gizai: links as chips and still drops unsafe links; while the Team Lead is paused the + button is off. Rendered to HTML
// on the server with the conversation handed in.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { ChatMessage, FileRow, Member, QueuedMessage } from "../../types";

vi.mock("../../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) out[k] = typeof v !== "function" ? v : () => new Promise(() => {});
  return out;
});
const chat = vi.hoisted(() => ({ messages: [] as ChatMessage[], queue: [] as QueuedMessage[], working: false }));
const flip = vi.hoisted(() => ({ on: false }));
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => R.useState(flip.on && init === false ? true : init)) as typeof R.useState;
  return { ...R, default: { ...R, useState }, useState };
});
vi.mock("./useChat", () => ({
  useChat: () => ({ messages: chat.messages, draft: "", tool: null, working: chat.working, queue: chat.queue, error: null, setWorking: () => {} }),
}));

const { ChatThread } = await import("./ChatThread");
const { UserBubble } = await import("./ChatMessage");
const { MarkdownView, LinkedText } = await import("../MarkdownView");
const { Popover } = await import("../Popover");

const lead = (status = "active") => ({ actorId: "lead", name: "Team Lead", kind: "agent", roleKey: "lead", handle: "lead", status, isLead: false,
  allowedTools: [], chatEnabled: true }) as Member;
const file = (id: string, name: string, sizeBytes: number): FileRow => ({ id, name, sizeBytes, sha256: "x", createdAt: 0 });
const LINK = "[GA-12 - Fix the login](gizai:task/GA-12)";
const sent = (id: string, body: string, files: FileRow[] = []): ChatMessage => ({
  id, threadId: "c1", role: "user", authorId: "you", authorName: "You", bodyMd: body, runId: null, toolName: null, tool: null, createdAt: Date.now(), files,
});
const render = (status = "active") => renderToStaticMarkup(<ChatThread threadId="c1" agent={lead(status)} />);

describe("the composer", () => {
  chat.messages = [];
  chat.queue = [];
  const html = render();

  // GA-83 moved + from before the text box to the row under it, the row's first control.
  it("has a + button under the text box, first in its row, that opens a menu", () => {
    const plus = html.indexOf('aria-label="Add files or link an item"');
    expect(plus).toBeGreaterThan(-1);
    expect(html.indexOf("composer-editor")).toBeLessThan(plus);
    expect(html.indexOf('class="composer-foot"')).toBeLessThan(plus);
    expect(html.indexOf('class="composer-hint"')).toBeGreaterThan(plus);
    expect(html).toMatch(/<span aria-haspopup="menu" aria-expanded="false"><button type="button" class="btn ghost sm icon-only composer-plus"/);
  });
  it("is a Markdown editor without a toolbar", () => {
    expect(html).toContain('class="md-editor composer-editor"');
    expect(html).not.toContain("md-toolbar");
  });
  it("says what @ does next to the Enter and Shift Enter tips, in the same style", () => {
    const hint = html.slice(html.indexOf('class="composer-hint"'));
    expect(hint).toMatch(/<span class="kbd">Enter<\/span> sends<\/span>.*<span class="kbd">Shift<\/span> <span class="kbd">Enter<\/span> new line<\/span><span><span class="kbd">@<\/span> links a task, project, client or agent<\/span>/);
  });
  it("is off while the Team Lead is paused: the + button too", () => {
    const off = render("paused");
    expect(off).toContain('class="composer-box disabled"');
    expect(off).toMatch(/class="btn ghost sm icon-only composer-plus" disabled=""/);
    expect(off).toContain('class="md-editor composer-editor disabled"');
  });
});

describe("the + menu", () => {
  it("opens upward with Add files and Link an item", () => {
    // A server render can't click: every false state starts true here, the Popover's open state among them.
    chat.messages = [];
    chat.queue = [];
    flip.on = true;
    const html = render();
    flip.on = false;
    const menu = html.slice(html.indexOf('role="menu"') - 30);
    expect(menu).toMatch(/^.*<div class="pop up" role="menu" aria-label="Add to this message">/);
    expect(menu).toMatch(/<button type="button" role="menuitem" class="opt">.*Add files<\/button><button type="button" role="menuitem" class="opt">.*Link an item<\/button>/);
    expect(html).toMatch(/aria-expanded="true"><button type="button" class="btn ghost sm icon-only composer-plus on"/);
  });
  it("is the design system's Popover, which opens below without `up`", () => {
    flip.on = true;
    const down = renderToStaticMarkup(<Popover label="Menu" button={() => <button>+</button>}>{() => <span>x</span>}</Popover>);
    flip.on = false;
    expect(down).toContain('<div class="pop" role="menu" aria-label="Menu">');
  });
});

describe("your messages", () => {
  it("show a link to an item as a chip that opens it, and the files you added with their size", () => {
    chat.messages = [sent("m1", `Look at ${LINK} please`, [file("f1", "invoice 2026.pdf", 2048), file("f2", "Makefile", 10)])];
    chat.queue = [];
    const html = render();
    expect(html).toContain('<div class="bubble">Look at <a class="item-chip kind-task" href="gizai:task/GA-12" title="Open task">GA-12 - Fix the login</a> please</div>');
    expect(html).toContain('<ul class="files compact msg-files" aria-label="Files">');
    expect(html).toContain('<span class="ext">PDF</span><span class="fname"><b>invoice 2026.pdf</b><span>2.0 KB</span>');
    expect(html).toContain('<span class="ext">FILE</span><span class="fname"><b>Makefile</b><span>10 B</span>');
  });
  it("of files only show no empty bubble", () => {
    const html = renderToStaticMarkup(<UserBubble text="  " files={[file("f1", "shot.png", 5)]} />);
    expect(html).not.toContain('class="bubble"');
    expect(html).toContain("shot.png");
  });
  it("keep their files while queued", () => {
    chat.messages = [sent("m1", "first")];
    chat.queue = [{ id: "q1", threadId: "c1", bodyMd: `and ${LINK}`, createdAt: 0, updatedAt: 0, held: false, files: [file("f3", "notes.md", 100)] }];
    chat.working = true;
    const html = render();
    chat.working = false;
    const queued = html.slice(html.indexOf('class="chat-queue"'));
    expect(queued).toContain('<a class="item-chip kind-task" href="gizai:task/GA-12"');
    expect(queued).toContain("<b>notes.md</b>");
  });
});

describe("links to items where Markdown is shown", () => {
  it("become chips with the item's name; other links stay links and unsafe ones lose their target", () => {
    const html = renderToStaticMarkup(<MarkdownView md={`${LINK}, [Giz AI](gizai:project/GA), [site](https://gizai.ai) and [bad](javascript:alert(1))`} />);
    expect(html).toContain('<a class="item-chip kind-task" href="gizai:task/GA-12" title="Open task">GA-12 - Fix the login</a>');
    expect(html).toContain('<a class="item-chip kind-project" href="gizai:project/GA" title="Open project">Giz AI</a>');
    expect(html).toContain('<a href="https://gizai.ai" title="https://gizai.ai">site</a>');
    expect(html).toContain("<a>bad</a>");
  });
  it("show the escaped text of a link as written", () => {
    const html = renderToStaticMarkup(<LinkedText text={"[GA-5 - Fix \\[urgent\\]](gizai:task/GA-5)"} />);
    expect(html).toBe('<a class="item-chip kind-task" href="gizai:task/GA-5" title="Open task">GA-5 - Fix [urgent]</a>');
  });
});
