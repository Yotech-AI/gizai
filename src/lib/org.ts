// The Team page's org chart: the Team Lead on top, the team's branches below (Design, Development, Quality, Operations
// and the ones you add), each with its agents and one empty spot that opens the agent form with the branch's role.
// Agents whose role no branch has go under Specialists.
import type { Branch, Member } from "../types";
import { branchPreset } from "./columns";

export type OrgNode = { kind: "agent"; member: Member } | { kind: "ghost"; role: string; name: string };
/** `branch`: the branch it shows (Specialists has none); `agents`: how many of the team's agents have one of its roles,
 * the Team Lead included (the backend refuses to remove a branch with any). */
export type Department = { key: string; name: string; nodes: OrgNode[]; branch?: Branch; agents: number };
export type Org = { leads: OrgNode[]; departments: Department[] };

/** The branches a team starts with, Design first (gizai-core `team::default_branches`). */
export const DEFAULT_BRANCHES: Branch[] = [
  { key: "design", name: "Design", roles: ["design", "designer", "ux"] },
  { key: "dev", name: "Development", roles: ["frontend", "backend", "fullstack", "mobile"] },
  { key: "qa", name: "Quality", roles: ["qa", "tester"] },
  { key: "ops", name: "Operations", roles: ["devops", "ops", "release"] },
];

export function buildOrg(members: Member[], branches: Branch[] = DEFAULT_BRANCHES): Org {
  const agents = members.filter((m) => m.kind === "agent");
  const isLead = (m: Member) => m.roleKey === "lead" || m.chatEnabled;
  const leads: OrgNode[] = agents.filter(isLead).map((member) => ({ kind: "agent", member }));
  const rest = agents.filter((m) => !isLead(m));
  const departments: Department[] = branches.map((b) => ({
    key: b.key, name: b.name, branch: b,
    agents: agents.filter((m) => b.roles.includes(m.roleKey)).length,
    nodes: [
      ...rest.filter((m) => b.roles.includes(m.roleKey)).map((member): OrgNode => ({ kind: "agent", member })),
      { kind: "ghost", ...branchPreset(b, members) },
    ],
  }));
  const others = rest.filter((m) => !branches.some((b) => b.roles.includes(m.roleKey)));
  if (others.length) departments.push({ key: "other", name: "Specialists", agents: others.length, nodes: others.map((member) => ({ kind: "agent", member })) });
  return { leads: leads.length ? leads : [{ kind: "ghost", role: "lead", name: "Team Lead" }], departments };
}
