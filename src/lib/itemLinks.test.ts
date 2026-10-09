// GA-41: the @ picker. When `@…` before the cursor opens it, which rows it lists for what was typed after the @, and the
// gizai: links it writes (and reads back for chips).
import { describe, expect, it } from "vitest";
import {
  agentItem, clientItem, docItem, escapeLinkText, findTrigger, IN_KIND, ITEM_KINDS, itemLink, parseItemUrl, parseQuery, PER_KIND, personItem,
  pickRows, projectItem, splitItemLinks, taskItem, unescapeLinkText, type PickItem, type PickRow,
} from "./itemLinks";
import type { Client, Doc, Member, Person, Project, Task } from "../types";

const task = (identifier: string, title: string, projectName = "Giz AI") => ({ identifier, title, projectName }) as Task;
const project = (key: string, name: string, clientName: string | null = null) => ({ id: `p-${key}`, key, name, clientName }) as Project;
const client = (id: string, name: string, legalName: string | null = null) => ({ id, name, legalName }) as Client;
const agent = (actorId: string, name: string, title: string | null = null) => ({ actorId, name, title }) as Member;
const person = (id: string, name: string, handle = "") => ({ id, name, handle }) as Person;
const doc = (id: string, title: string) => ({ id, title }) as Doc;

const ITEMS: PickItem[] = [
  taskItem(task("GA-12", "Fix the login")),
  taskItem(task("GA-3", "Picker in the chat")),
  taskItem(task("KADE-7", "Login page for Kade", "Kade portal")),
  projectItem(project("GA", "Giz AI", "YoTech")),
  projectItem(project("KADE", "Kade portal", "Spoorwegmuseum")),
  clientItem(client("c1", "Spoorwegmuseum", "Stichting Het Spoorwegmuseum")),
  clientItem(client("c2", "Café Login")),
  agentItem(agent("a1", "Backend Agent", "Backend developer")),
  agentItem(agent("a2", "Team Lead")),
  personItem(person("u1", "Sanne de Vries", "sanne")),
  docItem(doc("d1", "Login flow"), project("GA", "Giz AI")),
];

const labels = (rows: PickRow[]) => rows.map((r) => (r.type === "kind" ? `kind:${r.kind}` : `${r.item.kind}:${r.item.label}`));

describe("findTrigger: the @… before the cursor", () => {
  it("is a lone @ at the start, after a space or after a bracket", () => {
    expect(findTrigger("@")).toEqual({ from: 0, query: "" });
    expect(findTrigger("see @")).toEqual({ from: 4, query: "" });
    expect(findTrigger("see (@ga")).toEqual({ from: 5, query: "ga" });
    expect(findTrigger("[@task.GA")).toEqual({ from: 1, query: "task.GA" });
  });
  it("counts from the line's start when given its offset", () => {
    expect(findTrigger("hi @gi", 100)).toEqual({ from: 103, query: "gi" });
  });
  it("is nothing without an @, or for an email address", () => {
    expect(findTrigger("no at sign here")).toBeNull();
    expect(findTrigger("mail jef@example")).toBeNull();
    expect(findTrigger("@a@b")).toBeNull();
  });
  it("ends at a space unless the search is in one kind, one space at a time", () => {
    expect(findTrigger("@sanne said")).toBeNull();
    expect(findTrigger("@task.fix login")).toEqual({ from: 0, query: "task.fix login" });
    expect(findTrigger("@task.fix  login")).toBeNull();
    expect(findTrigger("@task. fix")).toBeNull();
  });
  it("ends at a line break and after 60 characters", () => {
    expect(findTrigger("@ga\nmore")).toBeNull();
    expect(findTrigger(`@${"a".repeat(60)}`)).toEqual({ from: 0, query: "a".repeat(60) });
    expect(findTrigger(`@${"a".repeat(61)}`)).toBeNull();
  });
});

describe("parseQuery", () => {
  it("reads a kind and its dot, case ignored, and the search after it", () => {
    for (const k of ITEM_KINDS) expect(parseQuery(`${k}.`)).toEqual({ kind: k, text: "" });
    expect(parseQuery("TASK.Fix")).toEqual({ kind: "task", text: "Fix" });
    expect(parseQuery("person.san")).toEqual({ kind: "person", text: "san" });
  });
  it("searches every kind for anything else", () => {
    expect(parseQuery("task")).toEqual({ kind: null, text: "task" });
    expect(parseQuery("tasks.x")).toEqual({ kind: null, text: "tasks.x" });
    expect(parseQuery("ga")).toEqual({ kind: null, text: "ga" });
  });
});

describe("pickRows", () => {
  it("lists the six kinds for only an @", () => {
    expect(labels(pickRows("", ITEMS))).toEqual(["kind:task", "kind:project", "kind:client", "kind:agent", "kind:person", "kind:doc"]);
  });
  it("searches every kind at once, grouped by kind in the kinds' order", () => {
    const shuffled = [...ITEMS].reverse();
    expect(labels(pickRows("login", shuffled))).toEqual([
      "task:KADE-7 - Login page for Kade", "task:GA-12 - Fix the login", "client:Café Login", "doc:Login flow",
    ]);
  });
  it("lists only one kind after @task. and so on, all of it with nothing after the dot", () => {
    expect(labels(pickRows("task.", ITEMS))).toEqual(["task:GA-12 - Fix the login", "task:GA-3 - Picker in the chat", "task:KADE-7 - Login page for Kade"]);
    expect(labels(pickRows("project.", ITEMS))).toEqual(["project:GA - Giz AI", "project:KADE - Kade portal"]);
    expect(labels(pickRows("client.", ITEMS))).toEqual(["client:Spoorwegmuseum", "client:Café Login"]);
    expect(labels(pickRows("agent.", ITEMS))).toEqual(["agent:Backend Agent", "agent:Team Lead"]);
    expect(labels(pickRows("person.", ITEMS))).toEqual(["person:Sanne de Vries"]);
    expect(labels(pickRows("doc.", ITEMS))).toEqual(["doc:Login flow"]);
  });
  it("filters a kind on its ID or key and on its name", () => {
    expect(labels(pickRows("task.ga-12", ITEMS))).toEqual(["task:GA-12 - Fix the login"]);
    expect(labels(pickRows("task.kade", ITEMS))).toEqual(["task:KADE-7 - Login page for Kade"]);
    expect(labels(pickRows("task.login", ITEMS))).toEqual(["task:GA-12 - Fix the login", "task:KADE-7 - Login page for Kade"]);
    expect(labels(pickRows("task.fix login", ITEMS))).toEqual(["task:GA-12 - Fix the login"]);
    expect(labels(pickRows("project.kade", ITEMS))).toEqual(["project:KADE - Kade portal"]);
    expect(labels(pickRows("project.giz", ITEMS))).toEqual(["project:GA - Giz AI"]);
    expect(labels(pickRows("agent.lead", ITEMS))).toEqual(["agent:Team Lead"]);
    expect(labels(pickRows("Task.GA-3", ITEMS))).toEqual(["task:GA-3 - Picker in the chat"]);
  });
  it("finds a person by handle, a project by its client, and ignores accents", () => {
    expect(labels(pickRows("person.sanne", ITEMS))).toEqual(["person:Sanne de Vries"]);
    expect(labels(pickRows("spoorweg", ITEMS))).toEqual(["project:KADE - Kade portal", "client:Spoorwegmuseum"]);
    expect(labels(pickRows("cafe", ITEMS))).toEqual(["client:Café Login"]);
  });
  it("puts a match at the start of the ID, key or name first", () => {
    expect(labels(pickRows("ga", ITEMS)).slice(0, 2)).toEqual(["task:GA-12 - Fix the login", "task:GA-3 - Picker in the chat"]);
    const tasks = [taskItem(task("GA-1", "Old login")), taskItem(task("LOGIN-2", "Something"))];
    expect(labels(pickRows("task.login", tasks))).toEqual(["task:LOGIN-2 - Something", "task:GA-1 - Old login"]);
    const clients = [clientItem(client("c1", "Big Login")), clientItem(client("c2", "Login Co"))];
    expect(labels(pickRows("client.login", clients))).toEqual(["client:Login Co", "client:Big Login"]);
  });
  it("finds nothing for a search that matches no item (the picker closes, @name stays a mention)", () => {
    expect(pickRows("zzz", ITEMS)).toEqual([]);
    expect(pickRows("task.zzz", ITEMS)).toEqual([]);
  });
  it("shows 5 per kind when searching every kind and 50 in one kind", () => {
    const many = Array.from({ length: 60 }, (_, i) => taskItem(task(`GA-${i + 1}`, `Item ${i + 1}`)));
    expect(PER_KIND).toBe(5);
    expect(IN_KIND).toBe(50);
    expect(pickRows("item", many)).toHaveLength(5);
    expect(pickRows("task.item", many)).toHaveLength(50);
    expect(pickRows("task.", many)).toHaveLength(50);
  });
});

describe("the items' rows and link texts", () => {
  it("shows a task as `GA-12 - Task title` and a project as `GA - Giz AI`, the others by name", () => {
    expect(taskItem(task("GA-12", "Task title"))).toMatchObject({ kind: "task", key: "GA-12", label: "GA-12 - Task title", text: "GA-12 - Task title" });
    expect(projectItem(project("GA", "Giz AI"))).toMatchObject({ kind: "project", key: "GA", label: "GA - Giz AI", text: "Giz AI" });
    expect(clientItem(client("c1", "Spoorwegmuseum"))).toMatchObject({ kind: "client", key: "c1", label: "Spoorwegmuseum", text: "Spoorwegmuseum" });
    expect(agentItem(agent("a1", "Backend Agent"))).toMatchObject({ kind: "agent", key: "a1", label: "Backend Agent" });
    expect(personItem(person("u1", "Sanne", "sanne"))).toMatchObject({ kind: "person", key: "u1", label: "Sanne", hint: "@sanne" });
    expect(docItem(doc("d1", "Spec"), project("GA", "Giz AI"))).toMatchObject({ kind: "doc", key: "d1", label: "Spec", hint: "GA - Giz AI" });
  });
});

describe("itemLink", () => {
  it("writes a Markdown link whose target names the kind and the item", () => {
    expect(itemLink(taskItem(task("GA-12", "Task title")))).toBe("[GA-12 - Task title](gizai:task/GA-12)");
    expect(itemLink(projectItem(project("GA", "Giz AI")))).toBe("[Giz AI](gizai:project/GA)");
    expect(itemLink(clientItem(client("01J9ZC", "Spoorwegmuseum")))).toBe("[Spoorwegmuseum](gizai:client/01J9ZC)");
    expect(itemLink(agentItem(agent("a1", "Backend Agent")))).toBe("[Backend Agent](gizai:agent/a1)");
    expect(itemLink(personItem(person("u1", "Sanne")))).toBe("[Sanne](gizai:person/u1)");
    expect(itemLink(docItem(doc("d1", "Login flow")))).toBe("[Login flow](gizai:doc/d1)");
  });
  it("escapes brackets and backslashes in the text and keeps it on one line", () => {
    expect(itemLink(taskItem(task("GA-5", "Fix [urgent] a\\b")))).toBe("[GA-5 - Fix \\[urgent\\] a\\\\b](gizai:task/GA-5)");
    expect(escapeLinkText("two\n  lines")).toBe("two lines");
    expect(unescapeLinkText("Fix \\[urgent\\] a\\\\b")).toBe("Fix [urgent] a\\b");
  });
  it("encodes a key that isn't safe in a URL", () => {
    expect(itemLink({ kind: "doc", key: "a b/c", label: "X", text: "X" })).toBe("[X](gizai:doc/a%20b%2Fc)");
  });
});

describe("parseItemUrl", () => {
  it("reads the kind and the item of a gizai: link", () => {
    expect(parseItemUrl("gizai:task/GA-12")).toEqual({ kind: "task", key: "GA-12" });
    expect(parseItemUrl(" gizai:project/GA ")).toEqual({ kind: "project", key: "GA" });
    expect(parseItemUrl("gizai:doc/a%20b%2Fc")).toEqual({ kind: "doc", key: "a b/c" });
  });
  it("is null for any other link", () => {
    for (const url of ["https://gizai.ai", "gizai:board/x", "gizai:task/", "gizai:task/GA-1/x", "gizai:task/GA-1?x", "javascript:alert(1)", "", null, undefined]) {
      expect(parseItemUrl(url)).toBeNull();
    }
  });
});

describe("splitItemLinks: a sent chat message with chips", () => {
  it("cuts the text into its gizai: links and the text between them", () => {
    expect(splitItemLinks("See [GA-12 - Fix the login](gizai:task/GA-12) and [Giz AI](gizai:project/GA).")).toEqual([
      { text: "See " }, { link: { kind: "task", key: "GA-12", label: "GA-12 - Fix the login" } }, { text: " and " },
      { link: { kind: "project", key: "GA", label: "Giz AI" } }, { text: "." },
    ]);
  });
  it("reads back what itemLink wrote, brackets included", () => {
    const item = taskItem(task("GA-5", "Fix [urgent] bug"));
    expect(splitItemLinks(itemLink(item))).toEqual([{ link: { kind: "task", key: "GA-5", label: "GA-5 - Fix [urgent] bug" } }]);
  });
  it("leaves other links, unknown kinds and plain text as text", () => {
    expect(splitItemLinks("[site](https://gizai.ai) [x](gizai:board/1) GA-12 @sanne")).toEqual([{ text: "[site](https://gizai.ai) [x](gizai:board/1) GA-12 @sanne" }]);
    expect(splitItemLinks("")).toEqual([]);
  });
});
