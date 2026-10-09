// The team as an org chart (design system: OrgChart): the Team Lead on top, the team's branches below, each with its
// agents and one dashed empty spot that opens the agent form with the branch's role. An agent working right now gets
// the teal ring. The agent cards are what you drag onto a column in Team → Workflow (the Team page's DndContext).
import { useEffect, useRef, useState } from "react";
import { useDraggable } from "@dnd-kit/core";
import { Plus, X } from "lucide-react";
import { addBranch, removeBranch } from "../api";
import { href } from "../router";
import { useClis } from "../lib/useClis";
import { buildOrg, type Department, type OrgNode } from "../lib/org";
import { roleLabel } from "../lib/agents";
import { cliName } from "../lib/clis";
import { useDrawer } from "../lib/drawers";
import type { Branch, CliStatus, Member } from "../types";
import { roleIcon } from "./Avatar";

/** What a dragged agent card carries (the Team page's drop on a column reads it). */
export type AgentDrag = { type: "agent"; agentId: string; name: string; role: string };

function Ghost({ node, teamId, branch }: { node: Extract<OrgNode, { kind: "ghost" }>; teamId: string; branch?: string }) {
  const open = useDrawer();
  const lead = node.role === "lead";
  const what = lead ? "Team Lead" : branch ?? roleLabel(node.role);
  return (
    <button className="org-node ghost" onClick={() => open({ kind: "agent", teamId, preset: { name: node.name, role: node.role, chat: lead } })}
      aria-label={lead ? "Add the Team Lead" : `Add an agent to ${what}`}
      title={lead ? "Add the Team Lead: the agent you chat with" : `Add a ${roleLabel(node.role)} agent to ${what}`}>
      <span className="org-icon"><Plus className="icon" /></span>
      <span className="org-text"><b>{lead ? "Team Lead" : roleLabel(node.role)}</b><span>{lead ? "Add your Team Lead" : "Add an agent"}</span></span>
    </button>
  );
}

function AgentNode({ m, working, clis }: { m: Member; working: (id: string) => boolean; clis: CliStatus[] | null }) {
  const drag: AgentDrag = { type: "agent", agentId: m.actorId, name: m.name, role: m.roleKey };
  const { listeners, setNodeRef, isDragging } = useDraggable({ id: `agent:${m.actorId}`, data: drag });
  // A drag that ends where it started would also click the link: that click is dropped.
  const dragged = useRef(false);
  useEffect(() => { if (isDragging) dragged.current = true; }, [isDragging]);
  const Icon = roleIcon(m.roleKey);
  const busy = working(m.actorId);
  const paused = m.status !== "active";
  return (
    // Only the pointer drags: the link keeps Enter, and "+ Agent" on a column does the same from the keyboard.
    <a ref={setNodeRef} className={`org-node${busy ? " live" : ""}${paused ? " paused" : ""}${isDragging ? " dragging" : ""}`} href={href({ page: "agent", id: m.actorId })}
      data-agent={m.name} draggable={false} onPointerDown={listeners?.onPointerDown as React.PointerEventHandler | undefined}
      onClick={(e) => { if (dragged.current) { e.preventDefault(); dragged.current = false; } }}
      title={`${m.name}: drag onto a column in Workflow to put it there`}>
      {busy && <span className="badge live org-badge"><span className="pulse" />Working</span>}
      <span className="org-icon"><Icon className="icon" /></span>
      <span className="org-text"><b>{m.name}</b>
        <span><i className={`org-dot${paused ? " paused" : busy ? " live" : ""}`} />{paused ? "Paused" : `${m.chatEnabled ? "Chat · " : ""}${cliName(m.adapter, clis)}`}</span></span>
    </a>
  );
}

function Node({ node, working, teamId, clis, branch }: { node: OrgNode; working: (id: string) => boolean; teamId: string; clis: CliStatus[] | null; branch?: string }) {
  return node.kind === "ghost" ? <Ghost node={node} teamId={teamId} branch={branch} /> : <AgentNode m={node.member} working={working} clis={clis} />;
}

/** A branch's name, with × to remove it while none of the team's agents has one of its roles. */
function BranchHead({ d, teamId, onError }: { d: Department; teamId: string; onError: (m: string) => void }) {
  const [busy, setBusy] = useState(false);
  if (!d.branch) return <div className="org-dept-name">{d.name}</div>;
  const key = d.branch.key;
  const why = d.agents > 0 ? `${d.name} has ${d.agents} ${d.agents === 1 ? "agent" : "agents"}: move or remove them first` : `Remove the ${d.name} branch`;
  return (
    <div className="org-dept-name">
      <span>{d.name}</span>
      <span title={why} className="org-dept-x">
        <button className="btn ghost sm icon-only" aria-label={`Remove the ${d.name} branch`} disabled={busy || d.agents > 0}
          onClick={() => { setBusy(true); removeBranch(teamId, key).catch((e) => onError(String(e))).finally(() => setBusy(false)); }}><X className="icon" /></button>
      </span>
    </div>
  );
}

/** Add branch: a name; its agents get a role made from it ("Docs" → docs). */
function AddBranch({ teamId }: { teamId: string }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const close = () => { setOpen(false); setName(""); setErr(null); };
  const add = async () => {
    if (!name.trim()) { setErr("Give the branch a name, like Docs or Security."); return; }
    setBusy(true); setErr(null);
    try { await addBranch(teamId, name.trim()); close(); } catch (e) { setErr(String(e)); }
    setBusy(false);
  };
  return (
    <div className="org-dept org-dept-add">
      {open ? (
        <div className="org-branch-form">
          <input className="input" aria-label="Branch name" autoFocus placeholder="Docs" value={name} disabled={busy}
            onChange={(e) => { setName(e.target.value); setErr(null); }}
            onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); add(); } if (e.key === "Escape") { e.preventDefault(); close(); } }} />
          <div className="org-branch-actions"><button className="btn ghost sm" onClick={close}>Cancel</button><button className="btn primary sm" disabled={busy} onClick={add}>Add branch</button></div>
          {err && <span className="error" role="alert">{err}</span>}
        </div>
      ) : (
        <button className="org-node ghost org-add-branch" onClick={() => setOpen(true)} title="Add a branch to the organisation chart, like Docs or Security">
          <span className="org-icon"><Plus className="icon" /></span>
          <span className="org-text"><b>Add branch</b><span>A new part of the team</span></span>
        </button>
      )}
    </div>
  );
}

export function OrgChart({ members, teamId, branches, working, onError = () => {} }: {
  members: Member[]; teamId: string; branches?: Branch[]; working: (id: string) => boolean; onError?: (m: string) => void;
}) {
  const org = buildOrg(members, branches);
  const clis = useClis();
  return (
    <div className="org-scroll">
      <div className="org">
        <div className="org-top">{org.leads.map((n, i) => <Node key={n.kind === "agent" ? n.member.actorId : `lead-${i}`} node={n} working={working} teamId={teamId} clis={clis} />)}</div>
        <div className="org-stem" />
        <div className="org-depts">
          {org.departments.map((d) => (
            <div key={d.key} className="org-dept" data-branch={d.name}>
              <BranchHead d={d} teamId={teamId} onError={onError} />
              <div className="org-dept-nodes">
                {d.nodes.map((n, i) => <Node key={n.kind === "agent" ? n.member.actorId : `${d.key}-spot-${i}`} node={n} working={working} teamId={teamId} clis={clis} branch={d.name} />)}
              </div>
            </div>
          ))}
          <AddBranch teamId={teamId} />
        </div>
      </div>
    </div>
  );
}
