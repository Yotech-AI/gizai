// One place opens every create/edit drawer, so any page (or the sidebar, or a key) can ask for one.
import { createContext, useContext, useState, type ReactNode } from "react";
import { NewTaskDrawer } from "../components/NewTaskDrawer";
import { ProjectDrawer } from "../components/ProjectForm";
import { ClientDrawer } from "../components/ClientForm";
import { PersonDrawer } from "../components/PersonDrawer";
import { AgentDrawer, type AgentPreset } from "../components/AgentForm";
import { DocDrawer } from "../components/DocDrawer";

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
  return (
    <Ctx.Provider value={set}>
      {children}
      {req?.kind === "task" && <NewTaskDrawer onClose={close} stateId={req.stateId} projectId={req.projectId} assigneeId={req.assigneeId} />}
      {req?.kind === "project" && <ProjectDrawer id={req.id} onClose={close} />}
      {req?.kind === "client" && <ClientDrawer id={req.id} onClose={close} />}
      {req?.kind === "person" && <PersonDrawer onClose={close} />}
      {req?.kind === "agent" && <AgentDrawer teamId={req.teamId} agentId={req.id} preset={req.preset} onClose={close} />}
      {req?.kind === "doc" && <DocDrawer projectId={req.projectId} onClose={close} />}
    </Ctx.Provider>
  );
}
