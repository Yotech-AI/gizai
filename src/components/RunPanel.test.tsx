// GA-48: the Run panel lists what a run was refused ("Refused in this run"), and its output marks each refusal where it
// happened. Rendered to HTML on the server, so no data loads.
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Refusal, SeqEvent } from "../types";
import { Refused, Stream } from "./RunPanel";

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&lt;/g, "<").replace(/&gt;/g, ">")
  .replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();

const heredoc: Refusal = { tool: "Bash", input: "cat <<EOF\nhello\nEOF", reason: "Heredoc with unquoted delimiter undergoes shell expansion" };
const write: Refusal = { tool: "Write", input: "/tmp/ga48-check-write.txt", reason: "" };

describe("Refused", () => {
  it("lists each refused call with its tool, what it asked for and why, under Refused in this run", () => {
    const html = renderToStaticMarkup(<Refused list={[heredoc, write]} />);
    expect(html).toContain('aria-label="Refused in this run"');
    const t = text(html);
    expect(t).toContain("Refused in this run (2)");
    expect(t).toContain("Bash cat <<EOF hello EOF Heredoc with unquoted delimiter undergoes shell expansion");
    expect(t).toContain("Write /tmp/ga48-check-write.txt");
    expect(html.match(/<li>/g)).toHaveLength(2);
    expect(t.indexOf("cat <<EOF")).toBeLessThan(t.indexOf("/tmp/ga48-check-write.txt"));
  });
  it("shows a command as text, never as markup, and leaves out an empty reason", () => {
    const html = renderToStaticMarkup(<Refused list={[{ tool: "Bash", input: "echo <b>x</b> > /tmp/a" }]} />);
    expect(html).toContain("echo &lt;b&gt;x&lt;/b&gt; &gt; /tmp/a");
    expect(html).not.toContain('class="muted"');
  });
});

describe("Stream", () => {
  it("marks a refused call where it happened, with the reason when there is one", () => {
    const events: SeqEvent[] = [
      { seq: 1, event: { kind: "tool_use", name: "Bash", summary: "cat <<EOF" } },
      { seq: 2, event: { kind: "refused", tool: "Bash", input: "cat <<EOF", reason: "Heredoc with unquoted delimiter undergoes shell expansion" } },
      { seq: 3, event: { kind: "refused", tool: "Write", input: "/tmp/x.txt", reason: "" } },
    ];
    const t = text(renderToStaticMarkup(<Stream events={events} />));
    expect(t).toContain("Refused: Bash cat <<EOF (Heredoc with unquoted delimiter undergoes shell expansion)");
    expect(t).toContain("Refused: Write /tmp/x.txt");
    expect(t).not.toContain("/tmp/x.txt (");
  });
});
