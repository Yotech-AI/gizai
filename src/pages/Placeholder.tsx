import type { Route } from "../router";
import type { AppInfo } from "../types";

const TITLES: Record<Route["page"], string> = {
  chat: "Chat", inbox: "Inbox", tasks: "Tasks", board: "Board", task: "Task", projects: "Projects", project: "Project", clients: "Clients",
  client: "Client", doc: "Doc", team: "Team", agent: "Agent", users: "Users", settings: "Settings", usage: "Usage",
};

/** Temporary page used until each screen is built (Tasks 7–9). */
export function Placeholder({ route, info }: { route: Route; info: AppInfo | null }) {
  return (
    <>
      <div className="topbar"><div className="crumbs"><b>{TITLES[route.page]}</b></div></div>
      <div className="placeholder">
        This screen is being built. {info && <span className="faint">Gizai {info.version} · data in {info.data_dir}</span>}
      </div>
    </>
  );
}
