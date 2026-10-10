// Tiny hash router: #/page or #/page/id.
import { useEffect, useState } from "react";

export const PAGES_WITH_ID = ["task", "project", "client", "doc", "team", "agent", "chat"] as const;
// chats: Chat → Archive, every chat with a search (chat/<id> is one chat).
export const PAGES = ["chat", "chats", "inbox", "tasks", "board", "task", "projects", "project", "clients", "client", "doc", "team", "agent", "users", "settings", "usage", "memory"] as const;
export type Page = (typeof PAGES)[number];
/** `scope`: on the Memory page only, which notes it shows (`memoryScope`); `view` "graph": its graph (GA-69). */
export type Route = { page: Page; id?: string; scope?: string; view?: "graph" };
// Settings' tabs (GA-42): #/settings/<tab> opens one; #/settings, or a tab it doesn't know, opens General.
export const SETTINGS_TABS = ["general", "appearance", "notifications", "agents", "mcp", "github"] as const;
export type SettingsTab = (typeof SETTINGS_TABS)[number];

// The Memory page (GA-68): #/memory is everything (the Team Lead's view), #/memory/shared the shared folders (no Team
// Lead yet) and #/memory/agent/<agent id> one agent's own folder. A note's id after any of them opens that note there:
// #/memory/<note>, #/memory/shared/<note>, #/memory/agent/<agent id>/<note>. The scope is "shared" or the agent's id.
// GA-69: "graph" in the note's place shows the graph of those notes: #/memory/graph, #/memory/shared/graph,
// #/memory/agent/<agent id>/graph (a note's id is never "graph").
function memoryRoute(rest: string[]): Route {
  const [a, b, c] = rest.map((p) => decodeURIComponent(p));
  const at = (scope: string | undefined, id: string | undefined): Route =>
    id === "graph" ? { page: "memory", ...(scope ? { scope } : {}), view: "graph" } : { page: "memory", ...(scope ? { scope } : {}), ...(id ? { id } : {}) };
  if (a === "shared") return at("shared", b);
  if (a === "agent") return !b ? { page: "memory" } : at(b, c);
  return at(undefined, a);
}

export function parseHash(hash: string): Route {
  const parts = hash.replace(/^#\/?/, "").split("/").filter(Boolean);
  const page = parts[0] as Page;
  if (!PAGES.includes(page)) return { page: "tasks" };
  if (page === "settings") return (SETTINGS_TABS as readonly string[]).includes(parts[1] ?? "") ? { page, id: parts[1] } : { page };
  if (page === "memory") return memoryRoute(parts.slice(1));
  if ((PAGES_WITH_ID as readonly string[]).includes(page) && parts[1]) return { page, id: decodeURIComponent(parts[1]) };
  const LIST_OF: Record<string, Page> = { task: "tasks", project: "projects", client: "clients", doc: "projects", team: "team", agent: "team", chat: "chat" };
  if ((PAGES_WITH_ID as readonly string[]).includes(page)) return { page: LIST_OF[page] };
  return { page };
}

export function href(r: Route): string {
  if (r.page === "memory") {
    const scope = r.scope === "shared" ? "/shared" : r.scope ? `/agent/${encodeURIComponent(r.scope)}` : "";
    return `#/memory${scope}${r.view === "graph" ? "/graph" : r.id ? "/" + encodeURIComponent(r.id) : ""}`;
  }
  return `#/${r.page}${r.id ? "/" + encodeURIComponent(r.id) : ""}`;
}

export function go(r: Route) {
  window.location.hash = href(r);
}

export function useRoute(): Route {
  const [route, setRoute] = useState(() => parseHash(window.location.hash));
  useEffect(() => {
    const on = () => setRoute(parseHash(window.location.hash));
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  return route;
}
