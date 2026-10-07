import { describe, expect, it } from "vitest";
import { applyEdit, format, activeFormats } from "./mdFormat";

// Run a format command on `doc` where [ and ] mark the selection (or | the cursor); return the result marked the same way.
function run(marked: string, cmd: Parameters<typeof format>[3]): string {
  const from = marked.includes("|") ? marked.indexOf("|") : marked.indexOf("[");
  const doc = marked.replace(/[|[\]]/g, "");
  const to = marked.includes("|") ? from : marked.indexOf("]") - 1;
  const edit = format(doc, from, to, cmd);
  const out = applyEdit(doc, edit);
  const [a, b] = [Math.min(edit.anchor, edit.head), Math.max(edit.anchor, edit.head)];
  return a === b ? out.slice(0, a) + "|" + out.slice(a) : out.slice(0, a) + "[" + out.slice(a, b) + "]" + out.slice(b);
}

describe("inline formats", () => {
  it("wraps the selection and keeps it selected", () => {
    expect(run("make [this] bold", "bold")).toBe("make **[this]** bold");
    expect(run("make [this] italic", "italic")).toBe("make _[this]_ italic");
    expect(run("run [npm test]", "code")).toBe("run `[npm test]`");
  });
  it("unwraps when the selection is already wrapped", () => {
    expect(run("make **[this]** bold", "bold")).toBe("make [this] bold");
  });
  it("inserts a pair of marks around the cursor when nothing is selected", () => {
    expect(run("say |", "bold")).toBe("say **|**");
  });
  it("makes a link, selecting the URL to type over", () => {
    expect(run("see [the docs]", "link")).toBe("see [the docs]([https://])");
    expect(run("see |", "link")).toBe("see [|](https://)");
  });
});

describe("line formats", () => {
  it("prefixes every selected line and toggles back", () => {
    expect(run("[one\ntwo]", "bullet")).toBe("[- one\n- two]");
    expect(run("[- one\n- two]", "bullet")).toBe("[one\ntwo]");
    expect(run("[a\nb\nc]", "ordered")).toBe("[1. a\n2. b\n3. c]");
    expect(run("[- a]", "check")).toBe("[- [ ] a]");
    expect(run("[say it]", "quote")).toBe("[> say it]");
  });
  it("cycles headings on the cursor's line", () => {
    expect(run("Title|", "heading")).toBe("## Title|");
    expect(run("## Title|", "heading")).toBe("### Title|");
    expect(run("### Title|", "heading")).toBe("Title|");
  });
  it("inserts a rule and a table on lines of their own", () => {
    expect(run("text|", "rule")).toBe("text\n\n---\n|");
    expect(run("|", "table")).toBe("| Column | Column |\n| --- | --- |\n| | |\n|");
  });
});

describe("activeFormats", () => {
  it("knows the line's block format and the inline marks around the cursor", () => {
    const doc = "## Head\n- [ ] task with **bold** text";
    expect(activeFormats(doc, 3)).toEqual(new Set(["heading"]));
    const inBold = doc.indexOf("bold") + 2;
    expect(activeFormats(doc, inBold)).toEqual(new Set(["check", "bold"]));
  });
});
