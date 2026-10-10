// GA-88 in the @ picker's list: `@task.` shows a small heading (.pop-label) above each column's first task, in the board's
// order. The headings aren't options: the editor's ↑, ↓ and Enter work on the rows by index, so they only ever land on a
// task, and the selected task scrolls into view with its column's heading. A search across all kinds and the other kinds
// look as before. Rendered to HTML on the server (createPortal stood in for); for the clicks and the scroll the component
// is called as a function, with an effect that runs at once and a stand-in for the list's element.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { DependencyList, EffectCallback, ReactElement } from "react";
import { ItemPicker } from "./ItemPicker";
import { clientItem, docItem, itemLink, pickRows, taskItem, type PickItem, type PickRow } from "../lib/itemLinks";
import type { Client, Doc, Task } from "../types";

const fake = vi.hoisted(() => ({ on: false, list: null as unknown }));
vi.mock("react-dom", async (orig) => ({ ...(await orig<typeof import("react-dom")>()), createPortal: (children: unknown) => children }));
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useEffect = ((effect: EffectCallback, deps?: DependencyList) => (fake.on ? void effect() : R.useEffect(effect, deps))) as typeof R.useEffect;
  const useRef = ((init: unknown) => (fake.on ? { current: fake.list } : R.useRef(init))) as typeof R.useRef;
  return { ...R, useEffect, useRef, default: { ...R, useEffect, useRef } };
});

const BOARD = ["Backlog", "To do", "In progress", "Testing", "Review", "Deploy"];
const task = (identifier: string, title: string, stateName: string) => ({ identifier, title, stateName, projectName: "Giz AI" }) as Task;
const ITEMS: PickItem[] = [
  taskItem(task("GA-1", "Ship the release", "Deploy"), BOARD),
  taskItem(task("GA-2", "Fix the login", "In progress"), BOARD),
  taskItem(task("GA-3", "Write the docs", "Backlog"), BOARD),
  taskItem(task("GA-4", "Login page", "To do"), BOARD),
  taskItem(task("GA-5", "Test the picker", "Testing"), BOARD),
  taskItem(task("GA-6", "Review the login", "Review"), BOARD),
  taskItem(task("GA-7", "Plan the sprint", "Backlog"), BOARD),
  clientItem({ id: "c1", name: "Login Company", legalName: null } as Client),
  docItem({ id: "d1", title: "Login flow" } as Doc),
];

const AT = { left: 100, top: 100, bottom: 120 };
const props = (query: string, rows: PickRow[], selected = 0) =>
  ({ at: AT, query, rows, loading: false, error: null, selected, onSelect: vi.fn(), onPick: vi.fn() });
const html = (query: string, selected = 0) => renderToStaticMarkup(<ItemPicker {...props(query, pickRows(query, ITEMS), selected)} />);

/** The list top to bottom: `# Heading` for a heading, the row's first text for a row, `> ` before the selected one. */
function shown(markup: string): string[] {
  return [...markup.matchAll(/<div class="pop-label">([^<]*)<\/div>|<button[^>]*class="opt( active)?"[^>]*>(?:<svg[\s\S]*?<\/svg>)?<span[^>]*>([^<]*)<\/span>/g)]
    .map((m) => (m[1] !== undefined ? `# ${m[1]}` : `${m[2] ? "> " : ""}${m[3]}`));
}

/** The elements of a rendered tree, top to bottom. */
type El = { type: unknown; props: { children?: unknown; [key: string]: unknown } };
function elements(node: unknown, out: El[] = []): El[] {
  if (Array.isArray(node)) for (const n of node) elements(n, out);
  else if (node && typeof node === "object" && "props" in node) { out.push(node as El); elements((node as El).props.children, out); }
  return out;
}

beforeEach(() => {
  vi.stubGlobal("window", { innerHeight: 800, innerWidth: 1200 });
  vi.stubGlobal("document", { body: {} });
});
afterEach(() => {
  fake.on = false;
  fake.list = null;
  vi.unstubAllGlobals();
});

describe("ItemPicker: @task. by column", () => {
  it("shows a small heading above each column's first task, the columns in the board's order", () => {
    expect(shown(html("task."))).toEqual([
      "# Backlog", "> GA-3 - Write the docs", "GA-7 - Plan the sprint",
      "# To do", "GA-4 - Login page",
      "# In progress", "GA-2 - Fix the login",
      "# Testing", "GA-5 - Test the picker",
      "# Review", "GA-6 - Review the login",
      "# Deploy", "GA-1 - Ship the release",
    ]);
  });
  it("heads only the columns with matching tasks when searching, the best match first in its column", () => {
    const items = [...ITEMS, taskItem(task("LOGIN-9", "Something", "In progress"), BOARD)];
    const rows = pickRows("task.login", items);
    expect(shown(renderToStaticMarkup(<ItemPicker {...props("task.login", rows)} />))).toEqual([
      "# To do", "> GA-4 - Login page",
      "# In progress", "LOGIN-9 - Something", "GA-2 - Fix the login",
      "# Review", "GA-6 - Review the login",
    ]);
  });
  it("makes only the tasks options: a heading is never selected, the selected task is the row ↑ and ↓ stepped to", () => {
    const rows = pickRows("task.", ITEMS);
    expect(rows.every((r) => r.type === "item" && r.item.kind === "task")).toBe(true);
    for (let i = 0; i < rows.length; i++) {
      const markup = html("task.", i);
      expect(markup.match(/role="option"/g)).toHaveLength(rows.length);
      expect(markup.match(/aria-selected="true"/g)).toHaveLength(1);
      const active = shown(markup).filter((s) => s.startsWith("> "));
      expect(active).toEqual([`> ${(rows[i] as Extract<PickRow, { type: "item" }>).item.label}`]);
    }
    // ↓ from Backlog's last task goes to To do's first: the heading between them is passed over.
    expect(shown(html("task.", 2)).slice(2, 5)).toEqual(["GA-7 - Plan the sprint", "# To do", "> GA-4 - Login page"]);
    expect(html("task.")).not.toMatch(/class="pop-label"[^>]*role=/);
  });
  it("picks the task a row stands for with its old link, and selects the row under the mouse", () => {
    const rows = pickRows("task.", ITEMS);
    const p = props("task.", rows);
    fake.on = true;
    const options = elements(ItemPicker(p)).filter((e) => e.props.role === "option");
    expect(options).toHaveLength(rows.length);
    (options[2].props.onClick as () => void)();
    expect(p.onPick).toHaveBeenCalledWith(2);
    const picked = rows[2] as Extract<PickRow, { type: "item" }>;
    expect(picked.item.key).toBe("GA-4");
    expect(itemLink(picked.item)).toBe("[GA-4 - Login page](gizai:task/GA-4)");
    (options[3].props.onMouseMove as () => void)();
    expect(p.onSelect).toHaveBeenCalledWith(3);
  });
});

describe("ItemPicker: the selected row in view", () => {
  type Box = { classList: { contains: (c: string) => boolean }; previousElementSibling: Box | null; scrollIntoView: (o: unknown) => void };
  const scrolled: string[] = [];
  const box = (name: string, cls: string, prev: Box | null = null): Box =>
    ({ classList: { contains: (c) => cls.split(" ").includes(c) }, previousElementSibling: prev, scrollIntoView: (o) => { scrolled.push(`${name} ${JSON.stringify(o)}`); } });
  const run = (active: Box | null) => {
    scrolled.length = 0;
    fake.on = true;
    fake.list = { querySelector: (sel: string) => (sel === ".opt.active" ? active : null) };
    ItemPicker(props("task.", pickRows("task.", ITEMS))) as ReactElement;
    return [...scrolled];
  };

  it("scrolls the column's heading into view with the column's first task, the heading first", () => {
    expect(run(box("row", "opt active", box("heading", "pop-label")))).toEqual(['heading {"block":"nearest"}', 'row {"block":"nearest"}']);
  });
  it("scrolls only the task when another task is above it, or nothing is", () => {
    expect(run(box("row", "opt active", box("other", "opt")))).toEqual(['row {"block":"nearest"}']);
    expect(run(box("row", "opt active"))).toEqual(['row {"block":"nearest"}']);
    expect(run(null)).toEqual([]);
  });
});

describe("ItemPicker: the rest looks as before", () => {
  it("heads a search across all kinds by kind, without columns, the tasks in their old order", () => {
    expect(shown(html("login"))).toEqual([
      "# Tasks", "> GA-2 - Fix the login", "GA-4 - Login page", "GA-6 - Review the login",
      "# Clients", "Login Company",
      "# Docs", "Login flow",
    ]);
  });
  it("shows no headings for one of the other kinds, and the kinds under Link a for only an @", () => {
    expect(shown(html("client."))).toEqual(["> Login Company"]);
    expect(shown(html("doc."))).toEqual(["> Login flow"]);
    expect(shown(html(""))).toEqual(["# Link a", "> Task", "Project", "Client", "Agent", "Person", "Doc"]);
  });
});
