// Tiny hash router: #/page or #/page/id.
import { useEffect, useState } from "react";

export const PAGES_WITH_ID = ["task", "project", "client", "doc", "team", "agent", "chat"] as const;
export const PAGES = ["chat", "inbox", "tasks", "board", "task", "projects", "project", "clients", "client", "doc", "team", "agent", "users", "settings"] as const;
export type Page = (typeof PAGES)[number];
export type Route = { page: Page; id?: string };

export function parseHash(hash: string): Route {
  const parts = hash.replace(/^#\/?/, "").split("/").filter(Boolean);
  const page = parts[0] as Page;
  if (!PAGES.includes(page)) return { page: "tasks" };
  if ((PAGES_WITH_ID as readonly string[]).includes(page) && parts[1]) return { page, id: decodeURIComponent(parts[1]) };
  const LIST_OF: Record<string, Page> = { task: "tasks", project: "projects", client: "clients", doc: "projects", team: "team", agent: "team", chat: "chat" };
  if ((PAGES_WITH_ID as readonly string[]).includes(page)) return { page: LIST_OF[page] };
  return { page };
}

export function href(r: Route): string {
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
