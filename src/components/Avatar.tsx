import { Bot, Container, Crown, FlaskConical, Monitor, Palette, Server, type LucideIcon } from "lucide-react";
import { initials } from "../lib/format";

const ROLE_ICONS: Record<string, LucideIcon> = { lead: Crown, frontend: Monitor, backend: Server, design: Palette, qa: FlaskConical, devops: Container };

/** The icon for an agent's role: lead, frontend, backend, design, QA, DevOps; anything else is a bot. */
export function roleIcon(role?: string | null): LucideIcon {
  return (role && ROLE_ICONS[role]) || Bot;
}

/** People are circles with initials, agents rounded squares; an xl agent shows its role icon. */
export function Avatar({ name, kind, size, role }: { name: string; kind?: string | null; size?: "sm" | "lg" | "xl"; role?: string | null; large?: boolean }) {
  const agent = kind === "agent";
  const Role = roleIcon(role);
  return (
    <span className={`avatar${agent ? " agent" : ""}${size ? ` ${size}` : ""}`} title={name}>
      {agent && size === "xl" ? <Role className="icon" /> : initials(name)}
    </span>
  );
}
