// GA-185 QA: Settings → Agents and runs → Screenshots, against GA-181's frame 175:4718 (design/screenshots/README.md):
// its intro and each field's label and hint (default and range) word for word, the values as saved with their ranges on
// the inputs, a typed value handed to Settings' own state (an emptied field as NaN, which Save settings refuses), and a
// refused value's why under its own field in place of the hint. Rendered to HTML on the server; the inputs' onChange is
// called on the element tree.
import { describe, expect, it } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { Settings } from "../types";
import { ScreenshotSettings } from "./ScreenshotSettings";
import { limitsProblem } from "../lib/screenshotSettings";

const SETTINGS = {
  claudeBin: "/usr/bin/claude", dataDir: "/home/you/.local/share/gizai", maxConcurrentRuns: 3, agentsPaused: false, maxRunUsd: null,
  maxRunMinutes: 90, maxRunToolCalls: 200, maxChatMinutes: 15, maxChatToolCalls: 60, pushOver: "ssh",
  notifications: { hold: true, waiting: true, leadAsks: true, leadAnswered: true }, editorRoots: ["~/Herd"],
  screenshotsPerRun: 12, screenshotMaxMb: 10, screenshotsKeepSets: 3, screenshotsKeepDays: 30, previewPortFrom: 47100, previewPortTo: 47999,
} as Settings;

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
/** Each field as its label, then its hint or error. */
const fields = (html: string) => [...html.matchAll(/<div class="field[^"]*"[^>]*><label[^>]*>([^<]*)<\/label>[\s\S]*?<span class="(hint|error)">([^<]*)<\/span><\/div>/g)]
  .map((m) => [m[1], m[2], m[3]]);
/** The element tree with its function components called. */
function expand(node: ReactNode): ReactNode {
  if (Array.isArray(node)) return node.map(expand);
  if (node && typeof node === "object" && "props" in node) {
    const el = node as ReactElement<Record<string, unknown>>;
    if (typeof el.type === "function") return expand((el.type as (p: unknown) => ReactNode)(el.props));
    return { ...el, props: { ...el.props, children: expand(el.props.children as ReactNode) } } as ReactNode;
  }
  return node;
}
function findAll(node: ReactNode, test: (p: Record<string, unknown>) => boolean, out: Record<string, unknown>[] = []) {
  if (Array.isArray(node)) node.forEach((n) => findAll(n, test, out));
  else if (node && typeof node === "object" && "props" in node) {
    const p = (node as ReactElement<Record<string, unknown>>).props;
    if (test(p)) out.push(p);
    findAll(p.children as ReactNode, test, out);
  }
  return out;
}

describe("Settings → Agents and runs → Screenshots", () => {
  it("has the design's intro, and each field with its default and range, word for word", () => {
    const html = renderToStaticMarkup(<ScreenshotSettings s={SETTINGS} setS={() => {}} />);
    expect(html).toContain('id="settings-screenshots"');
    expect(text(html)).toMatch(/^Screenshots What Gizai keeps of the screenshots agents take at the end of their runs, and the ports of the projects’ previews\. Each project sets its sizes and themes in its form → Preview\./);
    expect(fields(html)).toEqual([
      ["Screenshots per run", "hint", "1 to 30; 12 by default. Gizai keeps the first ones by file name and says in the run what it left out."],
      ["Largest screenshot (MB)", "hint", "1 to 50; 10 by default."],
      ["Keep per card (sets)", "hint", "1 to 20; 3 by default. A newer run’s set removes the oldest beyond these."],
      ["Delete a finished card’s screenshots after (days)", "hint", "0 to 365; 30 by default, counted from Done or Cancelled. 0 keeps them."],
      ["Preview ports", "hint", "From 1024 to 65535; 47100 to 47999 by default. At least ten ports for each run at once: a run’s preview gets its own port and the nine after it."],
    ]);
  });

  it("shows the saved values, each input with its range, and an emptied one empty", () => {
    const inputs = (s: Settings) => [...renderToStaticMarkup(<ScreenshotSettings s={s} setS={() => {}} />).matchAll(/<input ([^>]*)>/g)].map((m) => {
      const a = (k: string) => new RegExp(`${k}="([^"]*)"`).exec(m[1])?.[1];
      return [a("id"), a("value"), a("min"), a("max")];
    });
    expect(inputs(SETTINGS)).toEqual([
      ["s-shots-run", "12", "1", "30"], ["s-shots-mb", "10", "1", "50"], ["s-shots-keep", "3", "1", "20"], ["s-shots-days", "30", "0", "365"],
      ["s-ports-from", "47100", "1024", "65535"], ["s-ports-to", "47999", "1024", "65535"],
    ]);
    expect(inputs({ ...SETTINGS, screenshotsPerRun: Number.NaN })[0]).toEqual(["s-shots-run", "", "1", "30"]);
  });

  it("hands a typed value to Settings' own state, which Save settings saves or refuses", () => {
    const got: Settings[] = [];
    const t = expand(ScreenshotSettings({ s: SETTINGS, setS: (s) => got.push(s) }));
    const input = (id: string) => findAll(t, (p) => p.id === id)[0] as { onChange: (e: unknown) => void };
    input("s-shots-mb").onChange({ target: { value: "25" } });
    input("s-ports-to").onChange({ target: { value: "48099" } });
    input("s-shots-run").onChange({ target: { value: "" } });
    expect(got[0]).toEqual({ ...SETTINGS, screenshotMaxMb: 25 });
    expect(got[1]).toEqual({ ...SETTINGS, previewPortTo: 48099 });
    expect(Number.isNaN(got[2].screenshotsPerRun)).toBe(true);
    expect(limitsProblem(got[2])).toEqual({ key: "screenshotsPerRun", message: "Screenshots per run must be from 1 to 30." });
  });

  it("says why under the field Save settings refused, in place of its hint; the other fields keep theirs", () => {
    const bad = { ...SETTINGS, screenshotsKeepSets: 21 };
    const html = renderToStaticMarkup(<ScreenshotSettings s={bad} setS={() => {}} problem={limitsProblem(bad)} />);
    const f = fields(html);
    expect(f[2]).toEqual(["Keep per card (sets)", "error", "Keep per card must be from 1 to 20 sets."]);
    expect(f.filter((x) => x[1] === "error")).toHaveLength(1);
    // both port inputs share one field: a refused To says why there too
    const ports = { ...SETTINGS, previewPortFrom: 47100, previewPortTo: 47110 };
    expect(fields(renderToStaticMarkup(<ScreenshotSettings s={ports} setS={() => {}} problem={limitsProblem(ports)} />))[4]).toEqual(["Preview ports", "error",
      "Preview ports must hold at least ten ports for each run at once: 47100 to 47110 has room for 1 run, and Runs at once (Settings → Runs) is 3."]);
  });
});
