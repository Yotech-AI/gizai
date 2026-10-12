// The sidebar (design system: Sidebar): quick actions, work areas, projects, the team's agents and their memory scroll;
// the company pages stay at the bottom, with the update notice above them when a newer release is out. A Gizai on other
// data than the usual folder says so there. Editor (GA-113), Agents and Memory fold with the caret at the end of their
// heading (GA-90).
import {
  Building2, ChartColumn, ChevronDown, ChevronUp, Crown, FlaskConical, FolderCode, FolderKanban, Inbox, Keyboard, Library, ListTodo, MessagesSquare, Network,
  Plus, Search, Settings, SquarePen, Users, type LucideIcon,
} from "lucide-react";
import { useEffect, useState } from "react";
import { editorFolders, editorGit, getTeam, listChatThreads, listProjects, listTasks, memoryNotes } from "../api";
import { go, href, type Route } from "../router";
import { leadOf, memoryHome, memoryScope, noteCount, sharersOf } from "../lib/memory";
import type { EditorFolder, FolderGit, Member, MemoryNote, Task } from "../types";
import { useData } from "../lib/useData";
import { useCurrentTeam } from "../lib/team";
import { useLiveRuns } from "../lib/useLiveRuns";
import { inboxCount } from "../lib/inbox";
import { useDrawer } from "../lib/drawers";
import { modKey } from "../lib/keys";
import { liveTotal, useFolded, type Fold } from "../lib/sidebar";
import { forgetFolder, standingTip, standingWord, useOpenedFolders } from "../lib/editor";
import { roleIcon } from "./Avatar";
import { FolderPicker, workingWho } from "./editor/FolderPicker";
import { ContextMenu, openInItems, useEditorApps, type MenuItem } from "./editor/Menus";
import { useChatLive } from "./chat/useChat";
import { UpdateNotice } from "./UpdateNotice";

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
  const notes = useData(() => memoryNotes().catch(() => []));
  const lead = leadOf(agents);
  const threads = useData(() => listChatThreads().catch(() => []));
  const inbox = inboxCount(allTasks.data ?? [], threads.data ?? [], youId);
  const [folded, fold] = useFolded();
  const is = (...pages: Route["page"][]) => pages.includes(route.page);
  const liveFor = (id: string) => live.filter((r) => r.agentId === id).length;
  const allLive = liveTotal(live, agents);
  return (
    <aside className="side" aria-label="Navigation">
      <div className="side-main">
      <button className="nav-item" onClick={onNewTask}><SquarePen className="icon" /><span>New task</span><span className="meta"><span className="kbd">N</span></span></button>
      <button className="nav-item" onClick={onSearch}><Search className="icon" /><span>Search</span><span className="meta"><span className="kbd">{`${modKey()} K`}</span></span></button>
      <Item to={{ page: "inbox" }} icon={Inbox} label="Inbox" active={is("inbox")} meta={inbox > 0 ? <span className="count alert">{inbox}</span> : undefined} />

      <div className="nav-section">
        <div className="nav-label">Work</div>
        <Item to={{ page: "chat" }} icon={MessagesSquare} label="Chat" active={is("chat", "chats")}
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

      <EditorSection route={route} tasks={allTasks.data ?? []} folded={folded("editor")} onFold={() => fold("editor")} />

      <FoldSection section="agents" name="Agents" folded={folded("agents")} onFold={() => fold("agents")} tag={allLive > 0 ? liveTag(allLive) : null}>
        {agents.length === 0 && <a className="nav-item" href={href({ page: "team" })}><span className="faint">No agents yet</span></a>}
        {agents.map((a) => {
          const Role = roleIcon(a.roleKey);
          const n = liveFor(a.actorId);
          return (
            <a key={a.actorId} className={`nav-item${route.page === "agent" && route.id === a.actorId ? " on" : ""}`} href={href({ page: "agent", id: a.actorId })}>
              <Role className="icon" /><span>{a.name}</span>
              <span className="meta">{n > 0 ? liveTag(n) : a.status !== "active" ? <span className="faint">paused</span> : null}</span>
            </a>
          );
        })}
      </FoldSection>

      <MemorySection route={route} lead={lead} agents={agents} notes={notes.data ?? []} folded={folded("memory")} onFold={() => fold("memory")}
        setUpLead={() => { go({ page: "team" }); open({ kind: "agent", teamId: team.data?.id, preset: { name: "Team Lead", role: "lead", chat: true } }); }} />

      </div>

      <div className="side-foot">
        <UpdateNotice />
        <div className="nav-section">
          <div className="nav-label">Company</div>
          <Item to={{ page: "usage" }} icon={ChartColumn} label="Usage" active={is("usage")} />
          <Item to={{ page: "team" }} icon={Network} label="Team" active={is("team")} />
          <Item to={{ page: "users" }} icon={Users} label="Users" active={is("users")} />
          <Item to={{ page: "settings" }} icon={Settings} label="Settings" active={is("settings")} />
        </div>
        {dataLabel && <div className="data-tag" title={dataDir}><FlaskConical className="icon sm" />Test data: {dataLabel}</div>}
      </div>
    </aside>
  );
}

/** An agent at work (teal means only that): on its row, and on folded Agents' heading for them all. */
const liveTag = (n: number) => <span className="live-tag"><span className="pulse" />{n} live</span>;

/** A section that folds (GA-90): a click on its heading's text or on the caret at the heading's end leaves only the
 *  heading, another shows its list again. `tag` shows on the heading while it is folded. */
function FoldSection({ section, name, label, folded, onFold, tag, add, children }: {
  section: Fold; name: string; label?: string; folded: boolean; onFold: () => void; tag?: React.ReactNode;
  /** A + on the heading, before the caret (Editor's Open a folder). */
  add?: { label: string; onClick: (e: React.MouseEvent<HTMLButtonElement>) => void };
  children: React.ReactNode;
}) {
  const does = `${folded ? "Show" : "Fold"} ${name}`;
  const Caret = folded ? ChevronUp : ChevronDown;
  return (
    <div className="nav-section" aria-label={label}>
      <div className="nav-label">
        <span className="fold" onClick={onFold}>{name}{folded && tag}</span>
        {add && <button aria-label={add.label} title={add.label} onClick={add.onClick}><Plus className="icon sm" /></button>}
        <button aria-expanded={!folded} aria-controls={`side-${section}`} aria-label={does} title={does} onClick={onFold}><Caret className="icon sm" /></button>
      </div>
      <div className="nav-list" id={`side-${section}`} hidden={folded}>{!folded && children}</div>
    </div>
  );
}

/** Memory (GA-68), under Agents: the Team Lead first (it opens every note), then each other agent (its own folder, or the
 *  folder it shares with other agents, GA-96), each with how many notes it opens. Without a Team Lead: the shared notes
 *  and a link that sets one up. */
function MemorySection({ route, lead, agents, notes, folded, onFold, setUpLead }: {
  route: Route; lead: Member | null; agents: Member[]; notes: MemoryNote[]; folded: boolean; onFold: () => void; setUpLead: () => void;
}) {
  const on = (scope?: string) => route.page === "memory" && route.scope === scope;
  const count = (scope?: string, name?: string) => <span className="count" title="Notes">{noteCount(notes, memoryScope(scope, name))}</span>;
  const LeadIcon = lead ? roleIcon(lead.roleKey) : Crown;
  return (
    <FoldSection section="memory" name="Memory" label="Memory" folded={folded} onFold={onFold}>
      {lead ? (
        <a className={`nav-item${on() ? " on" : ""}`} href={href({ page: "memory" })} title={`${lead.name}: every note (its own, the shared folders and each agent's)`}>
          <LeadIcon className="icon" /><span>{lead.name}</span><span className="meta">{count()}</span>
        </a>
      ) : (
        <>
          <a className={`nav-item${on("shared") ? " on" : ""}`} href={href({ page: "memory", scope: "shared" })} title="The shared folders">
            <Library className="icon" /><span>Shared notes</span><span className="meta">{count("shared")}</span>
          </a>
          <button className="nav-item" onClick={setUpLead}><Crown className="icon" /><span>Set up the Team Lead</span></button>
        </>
      )}
      {agents.filter((a) => a.actorId !== lead?.actorId).map((a) => {
        const Role = roleIcon(a.roleKey);
        const home = memoryHome(a, agents, notes);
        const sharers = sharersOf(a.actorId, agents);
        const title = home.shares ? `Shares ${home.name}'s folder` : `${a.name}: its own folder${sharers.length ? `, shared with ${sharers.join(", ")}` : ""}`;
        return (
          <a key={a.actorId} className={`nav-item${on(a.actorId) ? " on" : ""}`} href={href({ page: "memory", scope: a.actorId })} title={title}>
            <Role className="icon" /><span>{a.name}</span><span className="meta">{count(home.ownerId, home.name)}</span>
          </a>
        );
      })}
    </FoldSection>
  );
}

/** Editor (GA-113), between Projects and Agents: the folders you opened, newest first (at most 8), each with how it
 *  stands on the right: "read-only" with the pulse while an agent works on a card's worktree, "by hand" on a card pinned
 *  to a person, else how many files changed ("3 behind" comes with F3). + opens the folder picker; a right click has
 *  Open in, Copy path, Remove from the list and, on a card's row, Open card. */
function EditorSection({ route, tasks, folded, onFold }: { route: Route; tasks: Task[]; folded: boolean; onFold: () => void }) {
  const opened = useOpenedFolders();
  const [folders, setFolders] = useState<EditorFolder[] | null>(null);
  const [gits, setGits] = useState<Record<string, FolderGit>>({});
  const [picker, setPicker] = useState<{ x: number; y: number } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; id: string } | null>(null);
  const apps = useEditorApps();
  const key = opened.join("\n");
  useEffect(() => {
    if (!opened.length) return;
    let alive = true;
    const load = () => {
      editorFolders().then((f) => { if (alive) setFolders(f); }).catch(() => {});
      for (const id of opened) editorGit(id).then((g) => { if (alive) setGits((m) => ({ ...m, [id]: g })); }).catch(() => {});
    };
    load();
    const t = setInterval(() => { if (!document.hidden) load(); }, 30_000);
    window.addEventListener("focus", load);
    return () => { alive = false; clearInterval(t); window.removeEventListener("focus", load); };
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps
  const openPicker = (e: React.MouseEvent<HTMLElement>) => {
    const side = e.currentTarget.closest(".side")?.getBoundingClientRect();
    const r = e.currentTarget.getBoundingClientRect();
    setPicker({ x: (side?.right ?? r.right) + 6, y: r.top });
  };
  const items = (id: string): MenuItem[] => {
    const f = folders?.find((x) => x.id === id);
    return [
      ...openInItems(apps, id, null, null, () => {}),
      { sep: true },
      { label: "Copy path", run: () => { navigator.clipboard?.writeText(f?.path ?? id).catch(() => {}); } },
      { label: "Remove from the list", run: () => forgetFolder(id) },
      ...(f?.taskId ? [{ label: "Open card", run: () => go({ page: "task", id: f.taskId! }) }] : []),
    ];
  };
  return (
    <FoldSection section="editor" name="Editor" label="Editor" folded={folded} onFold={onFold} add={{ label: "Open a folder", onClick: openPicker }}>
      {opened.length === 0 && <button className="nav-item" onClick={openPicker}><FolderCode className="icon" /><span>Open a folder</span></button>}
      {opened.map((id) => {
        const f = folders?.find((x) => x.id === id);
        const task = f?.taskId ? tasks.find((t) => t.id === f.taskId) : undefined;
        const git = gits[id] ?? null;
        const name = f?.name ?? id.split(/[\\/]/).filter(Boolean).pop() ?? id;
        const stand = f?.readOnly ? <span className="stand"><span className="pulse" />read-only</span>
          : task?.byHand ? <span className="stand"><Keyboard className="icon sm" />by hand</span>
          : standingWord(git) ? <span className="stand">{standingWord(git)}</span> : null;
        const tip = f?.readOnly ? `${workingWho(f.readOnlyWhy)} on ${f.name}: read-only until its run ends`
          : task?.byHand ? `${task.identifier} ${task.title}. Pinned to you: agents leave it alone`
          : [f?.path ?? id, standingTip(git)].filter(Boolean).join("\n");
        return (
          <a key={id} className={`nav-item${route.page === "editor" && route.id === id ? " on" : ""}`} href={href({ page: "editor", id })} title={tip}
            onContextMenu={(e) => { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, id }); }}
            onKeyDown={(e) => { if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) { e.preventDefault(); const r = e.currentTarget.getBoundingClientRect(); setMenu({ x: r.left + 24, y: r.bottom, id }); } }}>
            <FolderCode className="icon" />
            {f?.kind === "card" ? <><span className="card-id">{f.name}</span>{task && <span className="faint" style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }}>{task.title}</span>}</> : <span>{name}</span>}
            {stand && <span className="meta">{stand}</span>}
          </a>
        );
      })}
      {picker && <FolderPicker at={picker} onClose={() => setPicker(null)} />}
      {menu && <ContextMenu at={menu} label="Folder" items={items(menu.id)} onClose={() => setMenu(null)} />}
    </FoldSection>
  );
}
