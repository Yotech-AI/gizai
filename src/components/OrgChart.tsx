// The team as an org chart (design system: OrgChart): the Team Lead on top, departments below. An agent
// working right now gets the teal ring; empty places of the usual team are dashed and open the agent form.
import { Plus } from "lucide-react";
import { href } from "../router";
import { buildOrg, type OrgNode } from "../lib/org";
import { roleLabel } from "../lib/agents";
import { useDrawer } from "../lib/drawers";
import type { Member } from "../types";
import { roleIcon } from "./Avatar";

function Node({ node, working, teamId }: { node: OrgNode; working: (id: string) => boolean; teamId: string }) {
  const open = useDrawer();
  if (node.kind === "ghost") {
    const lead = node.role === "lead";
    return (
      <button className="org-node ghost" onClick={() => open({ kind: "agent", teamId, preset: { name: node.name, role: node.role, chat: lead } })}
        title={lead ? "Add the Team Lead: the agent you chat with" : `Add a ${roleLabel(node.role)} agent`}>
        <span className="org-icon"><Plus className="icon" /></span>
        <span className="org-text"><b>{lead ? "Team Lead" : roleLabel(node.role)}</b><span>{lead ? "Add your Team Lead" : "Add an agent"}</span></span>
      </button>
    );
  }
  const m = node.member;
  const Icon = roleIcon(m.roleKey);
  const busy = working(m.actorId);
  const paused = m.status !== "active";
  return (
    <a className={`org-node${busy ? " live" : ""}${paused ? " paused" : ""}`} href={href({ page: "agent", id: m.actorId })}>
      {busy && <span className="badge live org-badge"><span className="pulse" />Working</span>}
      <span className="org-icon"><Icon className="icon" /></span>
      <span className="org-text"><b>{m.name}</b>
        <span><i className={`org-dot${paused ? " paused" : busy ? " live" : ""}`} />{paused ? "Paused" : m.chatEnabled ? "Chat · Claude Code" : "Claude Code"}</span></span>
    </a>
  );
}

export function OrgChart({ members, teamId, working }: { members: Member[]; teamId: string; working: (id: string) => boolean }) {
  const org = buildOrg(members);
  return (
    <div className="org-scroll">
      <div className="org">
        <div className="org-top">{org.leads.map((n, i) => <Node key={n.kind === "agent" ? n.member.actorId : `lead-${i}`} node={n} working={working} teamId={teamId} />)}</div>
        {org.departments.length > 0 && <div className="org-stem" />}
        <div className="org-depts">
          {org.departments.map((d) => (
            <div key={d.key} className="org-dept">
              <div className="org-dept-name">{d.name}</div>
              <div className="org-dept-nodes">
                {d.nodes.map((n, i) => <Node key={n.kind === "agent" ? n.member.actorId : `${d.key}-${n.role}-${i}`} node={n} working={working} teamId={teamId} />)}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
