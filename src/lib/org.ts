// The Team page's org chart: the Team Lead on top, departments below, from the agents' roles. Places the
// usual software team still lacks are shown empty, one click from being filled.
import type { Member } from "../types";

export type OrgNode = { kind: "agent"; member: Member } | { kind: "ghost"; role: string; name: string };
export type Department = { key: string; name: string; nodes: OrgNode[] };
export type Org = { leads: OrgNode[]; departments: Department[] };

const DEPARTMENTS = [
  { key: "dev", name: "Development", roles: ["frontend", "backend", "fullstack", "mobile"] },
  { key: "design", name: "Design", roles: ["design", "designer", "ux"] },
  { key: "qa", name: "Quality", roles: ["qa", "tester"] },
  { key: "ops", name: "Operations", roles: ["devops", "ops", "release"] },
];

/** The usual software team, in department order. */
export const USUAL_TEAM = [
  { role: "frontend", name: "Frontend Agent" }, { role: "backend", name: "Backend Agent" }, { role: "design", name: "Design Agent" },
  { role: "qa", name: "QA Agent" }, { role: "devops", name: "DevOps Agent" },
];

export function buildOrg(members: Member[]): Org {
  const agents = members.filter((m) => m.kind === "agent");
  const isLead = (m: Member) => m.roleKey === "lead" || m.chatEnabled;
  const leads: OrgNode[] = agents.filter(isLead).map((member) => ({ kind: "agent", member }));
  const rest = agents.filter((m) => !isLead(m));
  const departments: Department[] = DEPARTMENTS.map((d) => ({
    key: d.key, name: d.name,
    nodes: [
      ...rest.filter((m) => d.roles.includes(m.roleKey)).map((member): OrgNode => ({ kind: "agent", member })),
      ...USUAL_TEAM.filter((u) => d.roles.includes(u.role) && !rest.some((m) => m.roleKey === u.role)).map((u): OrgNode => ({ kind: "ghost", ...u })),
    ],
  }));
  const others = rest.filter((m) => !DEPARTMENTS.some((d) => d.roles.includes(m.roleKey)));
  if (others.length) departments.push({ key: "other", name: "Specialists", nodes: others.map((member) => ({ kind: "agent", member })) });
  return {
    leads: leads.length ? leads : [{ kind: "ghost", role: "lead", name: "Team Lead" }],
    departments: departments.filter((d) => d.nodes.length > 0),
  };
}
