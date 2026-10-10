// The items the @ picker lists: open tasks, projects that aren't archived (and their docs), clients, agents and people.
// Asked for when a picker opens and kept for a short while, so the next @ shows them at once.
import { getTeam, listClients, listDocs, listProjects, listTasks, listTeams, listUsers } from "../api";
import { agentItem, clientItem, docItem, personItem, projectItem, taskItem, type PickItem } from "./itemLinks";
import type { Member } from "../types";

const FRESH_MS = 20_000;
let cache: { at: number; items: Promise<PickItem[]> } | null = null;

export function loadPickItems(): Promise<PickItem[]> {
  if (cache && Date.now() - cache.at < FRESH_MS) return cache.items;
  const items = fetchItems();
  const mine = { at: Date.now(), items };
  cache = mine;
  items.catch(() => { if (cache === mine) cache = null; });
  return items;
}

async function fetchItems(): Promise<PickItem[]> {
  const [tasks, projects, clients, people, teams] = await Promise.all([
    listTasks({ openOnly: true }), listProjects(), listClients(), listUsers(), listTeams(),
  ]);
  const live = projects.filter((p) => p.status !== "archived");
  const [members, docs] = await Promise.all([
    Promise.all(teams.map((t) => getTeam(t.id).then((team) => team.members).catch(() => [] as Member[]))),
    Promise.all(live.map((p) => listDocs(p.id).then((list) => list.map((d) => docItem(d, p))).catch(() => [] as PickItem[]))),
  ]);
  // An agent works in one team, but list each once anyway.
  const agents = new Map<string, Member>();
  for (const m of members.flat()) if (m.kind === "agent" && m.status !== "archived" && !agents.has(m.actorId)) agents.set(m.actorId, m);
  return [
    ...tasks.map(taskItem),
    ...live.map(projectItem),
    ...clients.filter((c) => c.status !== "archived").map(clientItem),
    ...[...agents.values()].map(agentItem),
    ...people.map(personItem),
    ...docs.flat(),
  ];
}
