// GA-14: the Runs tab shows the commits a run made: how many, and their subjects, oldest first. Rendered to HTML on
// the server, so no data loads: the list is given, as the Runs tab gets it from Gizai.
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Commit, Run } from "../types";
import { RunCommits, RunHistory } from "./RunHistory";

const first: Commit = { sha: "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", subject: "First change" };
const second: Commit = { sha: "2222222bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", subject: "Second change" };
const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();

describe("RunCommits", () => {
  it("shows a run that committed twice as 2 commits with their subjects, oldest first", () => {
    const html = renderToStaticMarkup(<RunCommits commits={[first, second]} />);
    expect(html).toContain("<h4>2 commits</h4>");
    const t = text(html);
    expect(t).toContain("1111111aa First change");
    expect(t).toContain("2222222bb Second change");
    expect(t.indexOf("First change")).toBeLessThan(t.indexOf("Second change"));
    expect(html.match(/<li>/g)).toHaveLength(2);
  });
  it("shows the short id, with the full one on hover", () => {
    const html = renderToStaticMarkup(<RunCommits commits={[first]} />);
    expect(html).toContain("<h4>1 commit</h4>");
    expect(html).toContain(`title="${first.sha}">1111111aa<`);
  });
  it("says No commits, without an empty list, for a run that didn't commit", () => {
    const html = renderToStaticMarkup(<RunCommits commits={[]} />);
    expect(html).toBe("<h4>No commits</h4>");
  });
  it("shows a subject as text, never as markup", () => {
    const html = renderToStaticMarkup(<RunCommits commits={[{ sha: first.sha, subject: "Fix <b>bold</b> & co" }]} />);
    expect(html).toContain("Fix &lt;b&gt;bold&lt;/b&gt; &amp; co");
  });
});

describe("RunHistory", () => {
  const run = (more: Partial<Run> = {}): Run => ({
    id: "01HZRUN0000000000000000001", agentId: "a1", agentName: "Backend Agent", taskId: "t1", roleKey: "backend", trigger: "manual",
    status: "succeeded", outcome: "ready_for_testing", createdAt: 1, costUsdMicros: 420_000, inputTokens: 0, outputTokens: 0,
    logPath: "/runs/r.jsonl", baseSha: first.sha, headSha: second.sha, ...more,
  } as Run);
  it("lists a run that saved where it ended, closed, before its commits load", () => {
    const html = renderToStaticMarkup(<RunHistory runs={[run(), run({ id: "01HZRUN0000000000000000002", headSha: null })]} />);
    expect(html.match(/class="run-row"/g)).toHaveLength(2);
    expect(html).not.toContain("commit");
  });
});
