import { useEffect, useState } from "react";
import { appInfo, exitApp, getDoc, getTask, getTeam, listRuns, saveDoc, updateTask, listClients, listProjects, listTasks, selftestReport } from "./api";
import { useRoute } from "./router";
import type { AppInfo } from "./types";
import { Sidebar } from "./components/Sidebar";
import { CommandPalette } from "./components/CommandPalette";
import { Placeholder } from "./pages/Placeholder";
import { ClientsPage } from "./pages/ClientsPage";
import { ClientPage } from "./pages/ClientPage";
import { UsersPage } from "./pages/UsersPage";
import { ProjectsPage } from "./pages/ProjectsPage";
import { ProjectPage } from "./pages/ProjectPage";
import { TasksPage } from "./pages/TasksPage";
import { TaskPage } from "./pages/TaskPage";
import { DocPage } from "./pages/DocPage";
import { TeamPage } from "./pages/TeamPage";
import { SettingsPage } from "./pages/SettingsPage";
import { AgentPage } from "./pages/AgentPage";
import { ChatPage } from "./pages/ChatPage";
import { UsagePage } from "./pages/UsagePage";
import { MemoryPage } from "./pages/MemoryPage";
import { DrawerHost, type DrawerReq } from "./lib/drawers";
import { appearanceProbe, chatArchiveProbe, chatProbe, docProbe, dragProbe, editorProbe, memoryEmptyProbe, memoryProbe, runProbe, teamProbe, usageProbe } from "./selftest";
import { appearanceOf, setAppearance, toggleDensity, toggleTheme } from "./lib/appearance";

export default function App() {
  const route = useRoute();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [palette, setPalette] = useState(false);
  const [drawer, setDrawer] = useState<DrawerReq | null>(null);

  useEffect(() => {
    // The theme and density are on <html> already (main.tsx); t and d switch them, like Settings → Appearance.
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return; // handled by an editor (Ctrl+K makes a link there)
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") { e.preventDefault(); setPalette((p) => !p); return; }
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, select, [contenteditable], .cm-editor") || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === "n" && !document.querySelector("[role=dialog]")) { e.preventDefault(); setDrawer({ kind: "task" }); }
      if (e.key === "t") toggleTheme(); // dark is the default
      if (e.key === "d") toggleDensity();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    appInfo()
      .then(async (i) => {
        setInfo(i);
        if (i.start_route) window.location.hash = `#/${i.start_route}`;
        // Dev hook for headless screenshots: GIZAI_SELFTEST_MODE=open:<drawer kind> opens that drawer.
        const kind = i.selftest_mode?.startsWith("open:") ? i.selftest_mode.slice(5) : null;
        if (kind === "task" || kind === "project" || kind === "client" || kind === "person") setDrawer({ kind });
        if (kind === "agent-lead") setDrawer({ kind: "agent", preset: { name: "Team Lead", role: "lead", chat: true } });
        if (i.selftest_mode === "theme:light") document.documentElement.dataset.theme = "light";
        // GIZAI_SELFTEST_MODE=appearance:chat=20,ui=16.5,docs=20,font=inter,theme=light[;<step>…]: Settings → Appearance for
        // this start only (not kept), then the steps, as below.
        let steps: string[] = [];
        if (i.selftest_mode?.startsWith("appearance:")) {
          const [spec = "", ...then] = i.selftest_mode.slice(11).split(";");
          setAppearance(appearanceOf(spec), false);
          steps = then;
        }
        // GIZAI_SELFTEST_MODE=steps:<step>;<step>…: a step "#/route" goes there, any other step clicks that CSS selector.
        if (i.selftest_mode?.startsWith("steps:")) steps = i.selftest_mode.slice(6).split(";");
        if (steps.length) {
          for (const step of steps) {
            await new Promise((r) => setTimeout(r, 1200));
            if (step.startsWith("#/")) window.location.hash = step;
            else { const el = document.querySelector(step) as HTMLElement | null; el?.scrollIntoView({ block: "start" }); el?.click(); }
          }
        }
        document.body.dataset.ready = "1";
        if (i.selftest) {
          const errors: string[] = [];
          const probe = async <X,>(name: string, f: () => Promise<X>) => { try { return await f(); } catch (err) { errors.push(`${name}: ${err}`); return null; } };
          const team = await probe("getTeam", () => getTeam());
          const clients = await probe("listClients", () => listClients());
          const projects = await probe("listProjects", () => listProjects());
          const tasks = await probe("listTasks", () => listTasks());
          await new Promise((r) => setTimeout(r, 300));
          if (!document.querySelector(".side .nav-item")) errors.push("sidebar did not render");
          let drag: Awaited<ReturnType<typeof dragProbe>> | undefined;
          if (i.start_route === "board") {
            drag = await dragProbe();
            if (!drag.moved) errors.push(`drag probe: ${JSON.stringify(drag)}`);
          }
          let editor: Awaited<ReturnType<typeof editorProbe>> | undefined;
          let run: Awaited<ReturnType<typeof runProbe>> | undefined;
          if (i.start_route?.startsWith("task/") && i.selftest_mode === "run") {
            run = await runProbe(i.start_route.slice(5), {
              getTask,
              clearHang: async (id) => { const t = await getTask(id); await updateTask(id, { descriptionMd: t.descriptionMd.replace("FAKE_HANG", "").trim() }); },
              lastRunStatus: async (id) => (await listRuns(id))[0]?.status,
            });
            if (!run.ok) errors.push(`run probe: ${JSON.stringify(run)}`);
          } else if (i.start_route?.startsWith("task/")) {
            editor = await editorProbe(i.start_route.slice(5), async (id) => (await getTask(id)).descriptionMd);
            if (!editor.saved || !editor.editor_closed) errors.push(`editor probe: ${JSON.stringify(editor)}`);
          }
          let doc: Awaited<ReturnType<typeof docProbe>> | undefined;
          if (i.start_route?.startsWith("doc/")) {
            doc = await docProbe(i.start_route.slice(4), { getDoc, saveDoc });
            if (!doc.ok) errors.push(`doc probe: ${JSON.stringify(doc)}`);
          }
          let chat: Awaited<ReturnType<typeof chatProbe>> | undefined;
          if (i.start_route === "chat" && i.selftest_mode === "chat") {
            chat = await chatProbe(async () => (await listTasks()).map((t) => t.title));
            if (!chat.ok) errors.push(`chat probe: ${JSON.stringify(chat)}`);
          }
          let teamUi: Awaited<ReturnType<typeof teamProbe>> | undefined;
          if (i.start_route === "team") {
            teamUi = await teamProbe(() => getTeam());
            if (!teamUi.ok) errors.push(`team probe: ${JSON.stringify(teamUi)}`);
          }
          let usage: Awaited<ReturnType<typeof usageProbe>> | undefined;
          if (i.start_route === "usage") {
            usage = await usageProbe();
            if (!usage.ok) errors.push(`usage probe: ${JSON.stringify(usage)}`);
          }
          let chats: Awaited<ReturnType<typeof chatArchiveProbe>> | undefined;
          if (i.start_route === "chat" && i.selftest_mode === "archive") {
            chats = await chatArchiveProbe();
            if (!chats.ok) errors.push(`chat archive probe: ${JSON.stringify(chats)}`);
          }
          let appearance: Awaited<ReturnType<typeof appearanceProbe>> | undefined;
          if (i.start_route === "settings/appearance" && (i.selftest_mode === "appearance-set" || i.selftest_mode === "appearance-kept")) {
            appearance = await appearanceProbe(i.selftest_mode === "appearance-kept" ? "kept" : "set");
            if (!appearance.ok) errors.push(`appearance probe: ${JSON.stringify(appearance)}`);
          }
          // GA-68: GIZAI_SELFTEST_MODE=memory:<agent id> on route memory/<note id> (prep_memory's notes); memory-empty on
          // route memory/shared (the demo data: no agents, no notes).
          let memory: Awaited<ReturnType<typeof memoryProbe>> | Awaited<ReturnType<typeof memoryEmptyProbe>> | undefined;
          if (i.start_route?.startsWith("memory/") && i.selftest_mode?.startsWith("memory:")) {
            memory = await memoryProbe(decodeURIComponent(i.start_route.slice(7)), i.selftest_mode.slice(7));
            if (!memory.ok) errors.push(`memory probe: ${JSON.stringify(memory)}`);
          }
          if (i.start_route === "memory/shared" && i.selftest_mode === "memory-empty") {
            memory = await memoryEmptyProbe();
            if (!memory.ok) errors.push(`memory empty probe: ${JSON.stringify(memory)}`);
          }
          await selftestReport({ ready: true, errors, version: i.version, columns: team?.states.length, clients: clients?.length, projects: projects?.length, tasks: tasks?.length, drag, editor, doc, team: teamUi, run, chat, usage, chats, appearance, memory });
          await exitApp(0);
        }
      })
      .catch((e) => setError(String(e)));
  }, []);

  const newTask = (stateId?: string) => setDrawer({ kind: "task", stateId });
  return (
    <DrawerHost request={drawer} setRequest={setDrawer}>
      <div className="app">
        <Sidebar youId={info?.you_id ?? ""} dataLabel={info?.data_label} dataDir={info?.data_dir} route={route} onSearch={() => setPalette(true)} onNewTask={() => newTask()} />
        <main className="main">
          {error && <div className="error-banner">{error}</div>}
          {route.page === "chat" || route.page === "chats" ? <ChatPage id={route.id} archive={route.page === "chats"} />
            : route.page === "inbox" ? <TasksPage key="inbox" inboxFor={info?.you_id ?? ""} onNewTask={newTask} />
            : route.page === "tasks" || route.page === "board" ? <TasksPage key={route.page} initialView={route.page === "board" ? "board" : undefined} onNewTask={newTask} />
            : route.page === "task" && route.id ? <TaskPage key={route.id} id={route.id} />
            : route.page === "doc" && route.id ? <DocPage key={route.id} id={route.id} />
            : route.page === "agent" && route.id ? <AgentPage key={route.id} id={route.id} />
            : route.page === "team" ? <TeamPage />
            : route.page === "settings" ? <SettingsPage tab={route.id} />
            : route.page === "clients" ? <ClientsPage />
            : route.page === "client" && route.id ? <ClientPage key={route.id} id={route.id} />
            : route.page === "users" ? <UsersPage />
            : route.page === "usage" ? <UsagePage />
            : route.page === "memory" ? <MemoryPage key={route.scope ?? "all"} route={route} youId={info?.you_id ?? ""} />
            : route.page === "projects" ? <ProjectsPage />
            : route.page === "project" && route.id ? <ProjectPage key={route.id} id={route.id} />
            : <Placeholder route={route} info={info} />}
        </main>
        <CommandPalette open={palette} onClose={() => setPalette(false)} onNewTask={() => newTask()} />
      </div>
    </DrawerHost>
  );
}
