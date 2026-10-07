// The sidebar (design system: Sidebar): quick actions, work areas, projects and the team's agents scroll; the
// company pages stay at the bottom. A Gizai on other data than the usual folder says so there.
import { Building2, FlaskConical, FolderKanban, Inbox, ListTodo, MessagesSquare, Network, Plus, Search, Settings, SquarePen, Users, type LucideIcon } from "lucide-react";
import { getTeam, listProjects, listTasks } from "../api";
import { href, type Route } from "../router";
import { useData } from "../lib/useData";
import { useCurrentTeam } from "../lib/team";
import { useLiveRuns } from "../lib/useLiveRuns";
import { needsYou } from "../lib/inbox";
import { useDrawer } from "../lib/drawers";
import { roleIcon } from "./Avatar";
import { useChatLive } from "./chat/useChat";

type Props = { route: Route; youId: string; onSearch: () => void; onNewTask: () => void; dataLabel?: string | null; dataDir?: string };

function Item({ to, icon: I, label, active, meta }: { to: Route; icon: LucideIcon; label: string; active: boolean; meta?: React.ReactNode }) {
  return (
    <a className={`nav-item${active ? " on" : ""}`} aria-current={active ? "page" : undefined} href={href(to)}>
      <I className="icon" /><span>{label}</span>{meta && <span className="meta">{meta}</span>}
    </a>
  );
}

export function Sidebar({ route, youId, onSearch, onNewTask, dataLabel, dataDir }: Props) {
  const projects = useData(() => listProjects());
  const allTasks = useData(() => listTasks());
  const [teamId] = useCurrentTeam();
  const team = useData(() => getTeam(teamId), [teamId]);
  const open = useDrawer();
  const live = useLiveRuns();
  const chatLive = useChatLive();
  const agents = (team.data?.members ?? []).filter((m) => m.kind === "agent");
  const inbox = (allTasks.data ?? []).filter((t) => needsYou(t, youId)).length;
  const is = (...pages: Route["page"][]) => pages.includes(route.page);
  const liveFor = (id: string) => live.filter((r) => r.agentId === id).length;
  return (
    <aside className="side" aria-label="Navigation">
      <div className="side-main">
      <button className="nav-item" onClick={onNewTask}><SquarePen className="icon" /><span>New task</span><span className="meta"><span className="kbd">N</span></span></button>
      <button className="nav-item" onClick={onSearch}><Search className="icon" /><span>Search</span><span className="meta"><span className="kbd">Ctrl K</span></span></button>
      <Item to={{ page: "inbox" }} icon={Inbox} label="Inbox" active={is("inbox")} meta={inbox > 0 ? <span className="count alert">{inbox}</span> : undefined} />

      <div className="nav-section">
        <div className="nav-label">Work</div>
        <Item to={{ page: "chat" }} icon={MessagesSquare} label="Chat" active={is("chat")}
          meta={chatLive.length > 0 ? <span className="live-tag"><span className="pulse" />working</span> : undefined} />
        <Item to={{ page: "tasks" }} icon={ListTodo} label="Tasks" active={is("tasks", "board", "task")} />
        <Item to={{ page: "clients" }} icon={Building2} label="Clients" active={is("clients", "client")} />
        <Item to={{ page: "projects" }} icon={FolderKanban} label="Projects" active={is("projects")} />
      </div>

      <div className="nav-section">
        <div className="nav-label">Projects<button aria-label="New project" title="New project" onClick={() => open({ kind: "project" })}><Plus className="icon sm" /></button></div>
        {(projects.data ?? []).filter((p) => p.status === "active").slice(0, 12).map((p) => (
          <a key={p.id} className={`nav-item${route.page === "project" && route.id === p.id ? " on" : ""}`} href={href({ page: "project", id: p.id })}>
            <span className="dot" style={{ background: p.color ?? "var(--text-3)" }} /><span>{p.name}</span>
          </a>
        ))}
      </div>

      <div className="nav-section">
        <div className="nav-label">Agents<button aria-label="Add agent" title="Add agent" onClick={() => open({ kind: "agent", teamId: team.data?.id })}><Plus className="icon sm" /></button></div>
        {agents.length === 0 && <a className="nav-item" href={href({ page: "team" })}><span className="faint">No agents yet</span></a>}
        {agents.map((a) => {
          const Role = roleIcon(a.roleKey);
          const n = liveFor(a.actorId);
          return (
            <a key={a.actorId} className={`nav-item${route.page === "agent" && route.id === a.actorId ? " on" : ""}`} href={href({ page: "agent", id: a.actorId })}>
              <Role className="icon" /><span>{a.name}</span>
              <span className="meta">{n > 0 ? <span className="live-tag"><span className="pulse" />{n} live</span> : a.status !== "active" ? <span className="faint">paused</span> : null}</span>
            </a>
          );
        })}
      </div>

      </div>

      <div className="side-foot">
        <div className="nav-section">
          <div className="nav-label">Company</div>
          <Item to={{ page: "team" }} icon={Network} label="Team" active={is("team")} />
          <Item to={{ page: "users" }} icon={Users} label="Users" active={is("users")} />
          <Item to={{ page: "settings" }} icon={Settings} label="Settings" active={is("settings")} />
        </div>
        {dataLabel && <div className="data-tag" title={dataDir}><FlaskConical className="icon sm" />Test data: {dataLabel}</div>}
      </div>
    </aside>
  );
}
