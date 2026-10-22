import { lazy, startTransition, Suspense, useEffect, useState } from "react";
import { TriangleAlert, X } from "lucide-react";
import { appInfo, exitApp, getDoc, getTask, getTeam, listRuns, saveDoc, updateTask, listClients, listProjects, listTasks, selftestReport, tourStill, tourSteps } from "./api";
import { href, useRoute, type Route } from "./router";
import type { AppInfo } from "./types";
import { Sidebar } from "./components/Sidebar";
import { Band } from "./components/Band";
import { CommandPalette } from "./components/CommandPalette";
import { CloseGuard } from "./components/CloseGuard";
import { TasksPage } from "./pages/TasksPage";
import { DrawerHost, type DrawerReq } from "./lib/drawers";
import { appearanceOf, setAppearance, toggleDensity, toggleTheme } from "./lib/appearance";
import { startTerminals } from "./lib/terminals";
import { watchAway } from "./lib/windowFocus";

// GA-178: Gizai opens on the board or the Inbox (TasksPage); every other page loads when it is first opened.
const Placeholder = lazy(() => import("./pages/Placeholder").then((m) => ({ default: m.Placeholder })));
const ClientsPage = lazy(() => import("./pages/ClientsPage").then((m) => ({ default: m.ClientsPage })));
const ClientPage = lazy(() => import("./pages/ClientPage").then((m) => ({ default: m.ClientPage })));
const UsersPage = lazy(() => import("./pages/UsersPage").then((m) => ({ default: m.UsersPage })));
const ProjectsPage = lazy(() => import("./pages/ProjectsPage").then((m) => ({ default: m.ProjectsPage })));
const ProjectPage = lazy(() => import("./pages/ProjectPage").then((m) => ({ default: m.ProjectPage })));
const TaskPage = lazy(() => import("./pages/TaskPage").then((m) => ({ default: m.TaskPage })));
const DocPage = lazy(() => import("./pages/DocPage").then((m) => ({ default: m.DocPage })));
const TeamPage = lazy(() => import("./pages/TeamPage").then((m) => ({ default: m.TeamPage })));
const SettingsPage = lazy(() => import("./pages/SettingsPage").then((m) => ({ default: m.SettingsPage })));
const AgentPage = lazy(() => import("./pages/AgentPage").then((m) => ({ default: m.AgentPage })));
const ChatPage = lazy(() => import("./pages/ChatPage").then((m) => ({ default: m.ChatPage })));
const UsagePage = lazy(() => import("./pages/UsagePage").then((m) => ({ default: m.UsagePage })));
const MemoryPage = lazy(() => import("./pages/MemoryPage").then((m) => ({ default: m.MemoryPage })));
const EditorPage = lazy(() => import("./pages/EditorPage").then((m) => ({ default: m.EditorPage })));
const TerminalPage = lazy(() => import("./pages/TerminalPage").then((m) => ({ default: m.TerminalPage })));
// GA-164: only a new install's first start shows it
const OnboardingPage = lazy(() => import("./pages/OnboardingPage").then((m) => ({ default: m.OnboardingPage })));

/** `onboarding`: the first start's onboarding waits (main.tsx asks before anything is drawn). It shows instead of the app
 *  until it is finished or skipped; without it Gizai opens as it always did. */
export default function App({ onboarding = false }: { onboarding?: boolean }) {
  const route = useRoute();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [palette, setPalette] = useState(false);
  const [drawer, setDrawer] = useState<DrawerReq | null>(null);
  const [setup, setSetup] = useState(onboarding);
  /** Why finishing the onboarding opened no Team Lead chat, above the page until it is closed. */
  const [note, setNote] = useState<string | null>(null);

  // the open terminals and their views live as long as the window, not a page (GA-135)
  useEffect(() => { startTerminals(); }, []);
  useEffect(() => watchAway(), []);

  useEffect(() => {
    // The theme and density are on <html> already (main.tsx); t and d switch them, like Settings → Appearance.
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return; // handled by an editor (Ctrl+K makes a link there)
      if (document.querySelector(".onb")) return; // the first start's onboarding, without the app's keys (GA-164)
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
        // GIZAI_SELFTEST_MODE=steps:<step>;<step>…: a step "#/route" goes there, any other step clicks that CSS selector;
        // runSteps has the rest (the product tour's named clicks and stills, GA-148).
        if (i.selftest_mode?.startsWith("steps:")) steps = i.selftest_mode.slice(6).split(";");
        // GA-178: the self-test's code loads only in a test run, before its first step or probe; a normal start never loads it.
        if (steps.length) {
          const { runSteps } = await import("./selftest");
          await runSteps(steps, { still: tourStill, report: tourSteps });
        }
        document.body.dataset.ready = "1";
        if (i.selftest) {
          // GA-188: GIZAI_SELFTEST_MODE=perf:<scenario> [ids] (scripts/perf.sh): the benchmark harness's probe and nothing
          // else, so the counter (lib/perf.ts) sees only what the app itself asks.
          if (i.selftest_mode?.startsWith("perf:")) {
            const { perfProbe } = await import("./selftest");
            const perf = await perfProbe(i.selftest_mode.slice(5));
            await selftestReport({ ready: true, errors: perf.errors.map((e) => `perf ${perf.scenario}: ${e}`), version: i.version, perf });
            await exitApp(0);
            return;
          }
          const { appearanceProbe, changesProbe, chatArchiveProbe, chatProbe, closeSaveProbe, docProbe, dragProbe, editorProbe, memoryEmptyProbe, memoryProbe, pasteProbe, runProbe, screenshotsProbe, teamProbe, terminalProbe, usageProbe } = await import("./selftest");
          // GA-122: GIZAI_SELFTEST_MODE=close-save:<step> on route doc/<id> or memory/<id> (scripts/close-save-test.sh): the
          // probe types in the editor and reports; the script quits Gizai from outside, so the page doesn't.
          if (i.selftest_mode?.startsWith("close-save:")) {
            const docId = decodeURIComponent((i.start_route ?? "").replace(/^(doc|memory)\//, ""));
            await closeSaveProbe(i.selftest_mode.slice(11), docId, { getDoc, report: selftestReport });
            return;
          }
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
          // GA-112: GIZAI_SELFTEST_MODE=changes-a:<ids> or changes-b:<ids> (prep_changes' cards) on a task route
          let changes: Awaited<ReturnType<typeof changesProbe>> | undefined;
          // GA-185: GIZAI_SELFTEST_MODE=screenshots:<ids> (prep_screenshots' card and runs) on route task/<card>/screenshots
          let screenshots: Awaited<ReturnType<typeof screenshotsProbe>> | undefined;
          if (i.start_route?.startsWith("task/") && /^changes-[ab]:/.test(i.selftest_mode ?? "")) {
            const [kind, list = ""] = i.selftest_mode!.split(":");
            changes = await changesProbe(kind === "changes-a" ? "a" : "b", list.trim().split(/\s+/));
            if (!changes.ok) errors.push(`changes probe: ${JSON.stringify(changes)}`);
          } else if (i.start_route?.startsWith("task/") && i.selftest_mode?.startsWith("screenshots:")) {
            screenshots = await screenshotsProbe(i.selftest_mode.slice(12).trim().split(/\s+/));
            if (!screenshots.ok) errors.push(`screenshots probe: ${JSON.stringify(screenshots)}`);
          } else if (i.start_route?.startsWith("task/") && i.selftest_mode === "run") {
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
          // GA-135: GIZAI_SELFTEST_MODE=terminal:<folder> on route terminal: a terminal in that folder (see terminalProbe)
          let terminal: Awaited<ReturnType<typeof terminalProbe>> | undefined;
          if (i.start_route === "terminal" && i.selftest_mode?.startsWith("terminal:")) {
            terminal = await terminalProbe(i.selftest_mode.slice(9));
            if (!terminal.ok) errors.push(`terminal probe: ${JSON.stringify(terminal)}`);
          }
          // GA-222 (Linux): GIZAI_SELFTEST_MODE=paste on route projects: the paste guard's step (see pasteProbe)
          let paste: Awaited<ReturnType<typeof pasteProbe>> | undefined;
          if (i.selftest_mode === "paste") {
            paste = await pasteProbe();
            if (!paste.ok) errors.push(`paste probe: ${JSON.stringify(paste)}`);
          }
          await selftestReport({ ready: true, errors, version: i.version, columns: team?.states.length, clients: clients?.length, projects: projects?.length, tasks: tasks?.length, drag, editor, doc, team: teamUi, run, chat, usage, chats, appearance, memory, changes, screenshots, terminal, paste });
          await exitApp(0);
        }
      })
      .catch((e) => setError(String(e)));
  }, []);

  // The onboarding is finished or skipped: the app opens on `to` (the chat it started, or the board), or on the page it
  // would open anyway. useRoute applies the new hash in a transition; the onboarding goes in that same one, so the page you
  // had before (the board) doesn't show for a moment first.
  const leave = (to: Route | null, why?: string) => {
    setNote(why ?? null);
    const done = () => startTransition(() => setSetup(false));
    const hash = to ? href(to) : null;
    if (hash && window.location.hash !== hash) {
      window.addEventListener("hashchange", done, { once: true });
      window.location.hash = hash;
    } else done();
  };
  if (setup) {
    return (
      <Suspense fallback={null}>
        <OnboardingPage onDone={leave} />
      </Suspense>
    );
  }

  const newTask = (stateId?: string) => setDrawer({ kind: "task", stateId });
  return (
    <DrawerHost request={drawer} setRequest={setDrawer}>
      <div className="app">
        {/* the mountains along the bottom (GA-143), while they are on: the window's lowest layer, under everything after it */}
        <Band />
        <Sidebar youId={info?.you_id ?? ""} dataLabel={info?.data_label} dataDir={info?.data_dir} route={route} onSearch={() => setPalette(true)} onNewTask={() => newTask()} />
        <main className="main">
          {error && <div className="error-banner">{error}</div>}
          {note && (
            <div className="warn-banner" role="status"><TriangleAlert className="icon" /><span>{note}</span>
              <button type="button" className="btn ghost sm icon-only" aria-label="Close" title="Close" onClick={() => setNote(null)}><X className="icon" /></button></div>
          )}
          {/* while a page's code loads the first time, the page area stays empty (no spinner); useRoute's transition keeps
              the page you came from until the next one is ready (GA-178) */}
          <Suspense fallback={null}>
            {route.page === "chat" || route.page === "chats" ? <ChatPage id={route.id} archive={route.page === "chats"} />
              : route.page === "inbox" ? <TasksPage key="inbox" inboxFor={info?.you_id ?? ""} onNewTask={newTask} />
              : route.page === "tasks" || route.page === "board" ? <TasksPage key={route.page} initialView={route.page === "board" ? "board" : undefined} onNewTask={newTask} />
              : route.page === "task" && route.id ? <TaskPage key={route.id} id={route.id} view={route.view === "changes" || route.view === "screenshots" ? route.view : "card"} run={route.run} youId={info?.you_id ?? ""} />
              : route.page === "doc" && route.id ? <DocPage key={route.id} id={route.id} />
              : route.page === "agent" && route.id ? <AgentPage key={route.id} id={route.id} />
              : route.page === "team" ? <TeamPage />
              : route.page === "settings" ? <SettingsPage tab={route.id} />
              : route.page === "clients" ? <ClientsPage />
              : route.page === "client" && route.id ? <ClientPage key={route.id} id={route.id} />
              : route.page === "users" ? <UsersPage />
              : route.page === "usage" ? <UsagePage />
              : route.page === "memory" ? <MemoryPage key={route.scope ?? "all"} route={route} youId={info?.you_id ?? ""} />
              : route.page === "editor" ? <EditorPage id={route.id} youId={info?.you_id ?? ""} />
              : route.page === "terminal" ? <TerminalPage id={route.id} youId={info?.you_id ?? ""} />
              : route.page === "projects" ? <ProjectsPage />
              : route.page === "project" && route.id ? <ProjectPage key={route.id} id={route.id} />
              : <Placeholder route={route} info={info} />}
          </Suspense>
        </main>
        <CommandPalette open={palette} onClose={() => setPalette(false)} onNewTask={() => newTask()} />
        <CloseGuard />
      </div>
    </DrawerHost>
  );
}
