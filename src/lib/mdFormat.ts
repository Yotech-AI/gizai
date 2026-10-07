// Markdown formatting commands for the editor toolbar, as pure functions over the text and the selection.
// The editor turns an Edit into a CodeMirror transaction.

export type FormatCmd = "heading" | "bold" | "italic" | "quote" | "code" | "link" | "ordered" | "bullet" | "check" | "rule" | "table";
export type Edit = { changes: { from: number; to: number; insert: string }[]; anchor: number; head: number };

const MARK: Partial<Record<FormatCmd, string>> = { bold: "**", italic: "_", code: "`" };
const LIST = /^(\s*)([-*+] \[[ xX]\] |[-*+] |\d+[.)] |> )/;

export function applyEdit(doc: string, e: Edit): string {
  let out = doc;
  for (const c of [...e.changes].sort((a, b) => b.from - a.from)) out = out.slice(0, c.from) + c.insert + out.slice(c.to);
  return out;
}

function lineStart(doc: string, pos: number) { return doc.lastIndexOf("\n", pos - 1) + 1; }
function lineEnd(doc: string, pos: number) { const i = doc.indexOf("\n", pos); return i === -1 ? doc.length : i; }

function inline(doc: string, from: number, to: number, mark: string): Edit {
  const n = mark.length;
  if (doc.slice(from - n, from) === mark && doc.slice(to, to + n) === mark) {
    return { changes: [{ from: to, to: to + n, insert: "" }, { from: from - n, to: from, insert: "" }], anchor: from - n, head: to - n };
  }
  return { changes: [{ from, to, insert: mark + doc.slice(from, to) + mark }], anchor: from + n, head: to + n };
}

function link(doc: string, from: number, to: number): Edit {
  const text = doc.slice(from, to);
  if (!text) return { changes: [{ from, to, insert: "[](https://)" }], anchor: from + 1, head: from + 1 };
  const urlAt = from + text.length + 3;
  return { changes: [{ from, to, insert: `[${text}](https://)` }], anchor: urlAt, head: urlAt + "https://".length };
}

function blockPrefix(cmd: FormatCmd, i: number): string {
  return cmd === "bullet" ? "- " : cmd === "check" ? "- [ ] " : cmd === "ordered" ? `${i + 1}. ` : "> ";
}
function kindOf(prefix: string): FormatCmd {
  return /\[[ xX]\]/.test(prefix) ? "check" : /^\d/.test(prefix) ? "ordered" : prefix.startsWith(">") ? "quote" : "bullet";
}

function lines(doc: string, from: number, to: number, cmd: FormatCmd): Edit {
  const start = lineStart(doc, from);
  const end = lineEnd(doc, to);
  const block = doc.slice(start, end).split("\n");
  const all = block.every((l) => { const m = LIST.exec(l); return m && kindOf(m[2]) === cmd; });
  const out = block.map((l, i) => {
    const m = LIST.exec(l);
    const body = m ? l.slice(m[0].length) : l;
    const indent = m ? m[1] : "";
    return all ? indent + body : indent + blockPrefix(cmd, i) + body;
  }).join("\n");
  if (from === to) {
    const delta = out.split("\n")[0].length - block[0].length;
    return { changes: [{ from: start, to: end, insert: out }], anchor: from + delta, head: from + delta };
  }
  return { changes: [{ from: start, to: end, insert: out }], anchor: start, head: start + out.length };
}

function heading(doc: string, from: number): Edit {
  const start = lineStart(doc, from);
  const m = /^(#{1,6}) /.exec(doc.slice(start, lineEnd(doc, from)));
  const level = m ? m[1].length : 0;
  const next = level === 0 || level === 1 ? "## " : level === 2 ? "### " : "";
  const cut = m ? m[0].length : 0;
  const delta = next.length - cut;
  return { changes: [{ from: start, to: start + cut, insert: next }], anchor: Math.max(start, from + delta), head: Math.max(start, from + delta) };
}

function insertBlock(doc: string, from: number, block: string): Edit {
  const end = lineEnd(doc, from);
  const lineEmpty = doc.slice(lineStart(doc, from), end).trim() === "";
  const insert = (lineEmpty ? "" : "\n\n") + block;
  return { changes: [{ from: end, to: end, insert }], anchor: end + insert.length, head: end + insert.length };
}

/** The edit a toolbar command makes on `doc` with the selection from..to. */
export function format(doc: string, from: number, to: number, cmd: FormatCmd): Edit {
  if (MARK[cmd]) return inline(doc, from, to, MARK[cmd]!);
  switch (cmd) {
    case "link": return link(doc, from, to);
    case "heading": return heading(doc, from);
    case "rule": return insertBlock(doc, to, "---\n");
    case "table": return insertBlock(doc, to, "| Column | Column |\n| --- | --- |\n| | |\n");
    default: return lines(doc, from, to, cmd);
  }
}

/** Which toolbar buttons show pressed with the cursor at `pos`. */
export function activeFormats(doc: string, pos: number): Set<FormatCmd> {
  const start = lineStart(doc, pos);
  const line = doc.slice(start, lineEnd(doc, pos));
  const out = new Set<FormatCmd>();
  if (/^#{1,6} /.test(line)) out.add("heading");
  const m = LIST.exec(line);
  if (m) out.add(kindOf(m[2]));
  const before = line.slice(0, pos - start);
  const count = (re: RegExp) => (before.match(re) ?? []).length;
  if (count(/\*\*/g) % 2 === 1) out.add("bold");
  if (count(/(?<![_\w])_(?!_)|(?<=\w)_(?![_\w])/g) % 2 === 1) out.add("italic");
  if (count(/`/g) % 2 === 1) out.add("code");
  return out;
}
