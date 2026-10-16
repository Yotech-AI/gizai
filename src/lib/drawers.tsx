// One place opens every create/edit drawer, so any page (or the sidebar, or a key) can ask for one. Each drawer's code
// loads when it is first opened (GA-190; research docs/research/performance.md F5, P6): five of them hold the Markdown
// editor, and with it CodeMirror, which so stay out of the files a start loads.
import { createContext, lazy, Suspense, useContext, useState, type ReactNode } from "react";
import type { AgentPreset } from "../components/AgentForm";

const NewTaskDrawer = lazy(() => import("../components/NewTaskDrawer").then((m) => ({ default: m.NewTaskDrawer })));
const ProjectDrawer = lazy(() => import("../components/ProjectForm").then((m) => ({ default: m.ProjectDrawer })));
const ClientDrawer = lazy(() => import("../components/ClientForm").then((m) => ({ default: m.ClientDrawer })));
const PersonDrawer = lazy(() => import("../components/PersonDrawer").then((m) => ({ default: m.PersonDrawer })));
const AgentDrawer = lazy(() => import("../components/AgentForm").then((m) => ({ default: m.AgentDrawer })));
const DocDrawer = lazy(() => import("../components/DocDrawer").then((m) => ({ default: m.DocDrawer })));

export type DrawerReq =
  | { kind: "task"; stateId?: string | null; projectId?: string | null; assigneeId?: string | null }
  | { kind: "project"; id?: string }
  | { kind: "client"; id?: string }
  | { kind: "person" }
  | { kind: "agent"; teamId?: string | null; id?: string; preset?: AgentPreset }
  | { kind: "doc"; projectId: string };

const Ctx = createContext<(r: DrawerReq) => void>(() => {});
export const useDrawer = () => useContext(Ctx);

export function DrawerHost({ children, request, setRequest }: { children: ReactNode; request?: DrawerReq | null; setRequest?: (r: DrawerReq | null) => void }) {
  const [own, setOwn] = useState<DrawerReq | null>(null);
  const req = request !== undefined ? request : own;
  const set = setRequest ?? setOwn;
  const close = () => set(null);
  // the page stays as it is while a drawer's code loads, the first time it opens
  return (
    <Ctx.Provider value={set}>
      {children}
      <Suspense fallback={null}>
        {req?.kind === "task" && <NewTaskDrawer onClose={close} stateId={req.stateId} projectId={req.projectId} assigneeId={req.assigneeId} />}
        {req?.kind === "project" && <ProjectDrawer id={req.id} onClose={close} />}
        {req?.kind === "client" && <ClientDrawer id={req.id} onClose={close} />}
        {req?.kind === "person" && <PersonDrawer onClose={close} />}
        {req?.kind === "agent" && <AgentDrawer teamId={req.teamId} agentId={req.id} preset={req.preset} onClose={close} />}
        {req?.kind === "doc" && <DocDrawer projectId={req.projectId} onClose={close} />}
      </Suspense>
    </Ctx.Provider>
  );
}
