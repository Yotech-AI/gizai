// The @ picker: the Gizai items it lists, how typing after @ filters them, and the gizai: links it writes
// ([GA-12 - Fix the login](gizai:task/GA-12)). Pure, so it is tested.
import { fold } from "./palette";
import type { Client, Doc, Member, Person, Project, Task } from "../types";

export const ITEM_KINDS = ["task", "project", "client", "agent", "person", "doc"] as const;
export type ItemKind = (typeof ITEM_KINDS)[number];
export const KIND_NAME: Record<ItemKind, string> = { task: "Task", project: "Project", client: "Client", agent: "Agent", person: "Person", doc: "Doc" };
export const KIND_GROUP: Record<ItemKind, string> = { task: "Tasks", project: "Projects", client: "Clients", agent: "Agents", person: "People", doc: "Docs" };

/** One item the picker lists. `key`: what its link names (a task's identifier, a project's key, else its id). `label`: the
 *  row's text; `text`: the link's text; `hint`: more words it is found by (shown faint). */
export type PickItem = { kind: ItemKind; key: string; label: string; text: string; hint?: string };

export const taskItem = (t: Task): PickItem =>
  ({ kind: "task", key: t.identifier, label: `${t.identifier} - ${t.title}`, text: `${t.identifier} - ${t.title}`, hint: t.projectName ?? undefined });
export const projectItem = (p: Project): PickItem =>
  ({ kind: "project", key: p.key, label: `${p.key} - ${p.name}`, text: p.name, hint: p.clientName ?? undefined });
export const clientItem = (c: Client): PickItem => ({ kind: "client", key: c.id, label: c.name, text: c.name, hint: c.legalName ?? undefined });
export const agentItem = (m: Member): PickItem => ({ kind: "agent", key: m.actorId, label: m.name, text: m.name, hint: m.title ?? undefined });
export const personItem = (p: Person): PickItem => ({ kind: "person", key: p.id, label: p.name, text: p.name, hint: p.handle ? `@${p.handle}` : undefined });
export const docItem = (d: Doc, project?: Project): PickItem =>
  ({ kind: "doc", key: d.id, label: d.title, text: d.title, hint: project ? `${project.key} - ${project.name}` : undefined });

/** The `@…` right before the cursor: where its @ is (`from`, plus `offset`) and what was typed after it. Null when the
 *  cursor isn't in one: the @ must start the text or follow a space or bracket (not an email address), and what follows
 *  it has no @ or line break. Spaces only belong to a search in one kind (`@task.fix login`), one at a time. */
export type Trigger = { from: number; query: string };
const MAX_QUERY = 60;

export function findTrigger(before: string, offset = 0): Trigger | null {
  const at = before.lastIndexOf("@");
  if (at < 0) return null;
  if (at > 0 && !/[\s([]/.test(before[at - 1] ?? "")) return null;
  const query = before.slice(at + 1);
  if (query.length > MAX_QUERY || /[@\n]/.test(query)) return null;
  if (/\s/.test(query)) {
    const { kind, text } = parseQuery(query);
    if (!kind || /^\s|\s\s/.test(text)) return null;
  }
  return { from: offset + at, query };
}

/** What was typed after the @: a kind (`task.` and so on, case ignored) and the search after its dot, or no kind and the
 *  search in all of them. */
export function parseQuery(query: string): { kind: ItemKind | null; text: string } {
  const m = /^([a-z]+)\.(.*)$/is.exec(query);
  const kind = m ? (ITEM_KINDS as readonly string[]).find((k) => k === m[1]?.toLowerCase()) as ItemKind | undefined : undefined;
  return kind && m ? { kind, text: m[2] ?? "" } : { kind: null, text: query };
}

/** A row in the picker: a kind (with only @ typed), or an item. */
export type PickRow = { type: "kind"; kind: ItemKind } | { type: "item"; item: PickItem };

/** Rows per kind when the search runs in all kinds, and in one kind. */
export const PER_KIND = 5;
export const IN_KIND = 50;

/** The picker's rows for what was typed after the @: only @, the kinds; a kind and its dot, that kind's items (all of
 *  them, or those the search finds); else the items of every kind the search finds, grouped by kind in ITEM_KINDS'
 *  order. An item is found when every word appears in its label or hint (ID or key, and name); one whose ID, key or
 *  label starts with the first word comes first. */
export function pickRows(query: string, items: readonly PickItem[]): PickRow[] {
  if (query === "") return ITEM_KINDS.map((kind) => ({ type: "kind", kind }));
  const { kind, text } = parseQuery(query);
  const words = fold(text).split(/\s+/).filter(Boolean);
  const scored: { item: PickItem; score: number; i: number }[] = [];
  items.forEach((item, i) => {
    if (kind && item.kind !== kind) return;
    const label = fold(item.label);
    const key = fold(item.key);
    const hay = `${label} ${fold(item.hint ?? "")} ${item.kind === "task" || item.kind === "project" ? key : ""}`;
    if (!words.every((w) => hay.includes(w))) return;
    const first = words[0] ?? "";
    const score = words.length === 0 ? 0 : key.startsWith(first) || label.startsWith(first) ? 0 : words.every((w) => label.includes(w)) ? 1 : 2;
    scored.push({ item, score, i });
  });
  scored.sort((a, b) => a.score - b.score || a.i - b.i);
  const limit = kind ? IN_KIND : PER_KIND;
  const rows: PickRow[] = [];
  for (const k of ITEM_KINDS) {
    if (kind && k !== kind) continue;
    for (const s of scored.filter((x) => x.item.kind === k).slice(0, limit)) rows.push({ type: "item", item: s.item });
  }
  return rows;
}

/** A link's text, safe inside [ ]: one line, with its backslashes and brackets escaped. */
export function escapeLinkText(text: string): string {
  return text.replace(/\s*\n\s*/g, " ").replace(/[\\[\]]/g, (c) => `\\${c}`);
}

/** Markdown's backslash escapes undone: `\[` is `[`. */
export function unescapeLinkText(text: string): string {
  return text.replace(/\\([!-/:-@[-`{-~])/g, "$1");
}

/** The Markdown link the picker puts in the text: the row's text (a project's name), and a target that names the kind
 *  and the item: `[GA-12 - Fix the login](gizai:task/GA-12)`, `[Giz AI](gizai:project/GA)`, `[Kade](gizai:client/<id>)`. */
export function itemLink(item: PickItem): string {
  return `[${escapeLinkText(item.text)}](gizai:${item.kind}/${encodeURIComponent(item.key)})`;
}

/** The kind and item a gizai: link names; null for any other link. */
export function parseItemUrl(url: string | null | undefined): { kind: ItemKind; key: string } | null {
  const m = /^gizai:([a-z]+)\/([^/?#\s]+)$/.exec((url ?? "").trim());
  if (!m || !(ITEM_KINDS as readonly string[]).includes(m[1] ?? "")) return null;
  let key = m[2] ?? "";
  try { key = decodeURIComponent(key); } catch { /* keep it as written */ }
  return key ? { kind: m[1] as ItemKind, key } : null;
}

/** A gizai: link anywhere in Markdown text: its text (escaped), kind and target. */
export const ITEM_LINK_SOURCE = String.raw`\[((?:\\.|[^\\\]\n])+)\]\(gizai:(task|project|client|agent|person|doc)\/([^()\s]+)\)`;

/** Plain text cut into its gizai: links and the text between them (a sent chat message, which isn't Markdown). */
export type TextPart = { text: string } | { link: { kind: ItemKind; key: string; label: string } };
export function splitItemLinks(text: string): TextPart[] {
  const out: TextPart[] = [];
  const re = new RegExp(ITEM_LINK_SOURCE, "g");
  let last = 0;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    const target = parseItemUrl(`gizai:${m[2]}/${m[3]}`);
    if (!target) continue;
    if (m.index > last) out.push({ text: text.slice(last, m.index) });
    out.push({ link: { ...target, label: unescapeLinkText(m[1] ?? "") } });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push({ text: text.slice(last) });
  return out;
}
