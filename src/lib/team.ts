import { useEffect, useState } from "react";
import type { TeamMove } from "../types";

const KEY = "gizai-team";
const EVENT = "gizai-team-changed";

function read(): string | null { try { return localStorage.getItem(KEY); } catch { return null; } }

/** The team picked on the Team page or the board, shared by them and the sidebar (null = the first team). */
export function useCurrentTeam(): [string | null, (id: string | null) => void] {
  const [id, setId] = useState<string | null>(read);
  useEffect(() => {
    const on = () => setId(read());
    window.addEventListener(EVENT, on);
    return () => window.removeEventListener(EVENT, on);
  }, []);
  const set = (next: string | null) => pickTeam(next);
  return [id, set];
}

/** Picks the team the Team page, the board and the sidebar show (null = the first team), as their Team select does. */
export function pickTeam(id: string | null) {
  try { if (id) localStorage.setItem(KEY, id); else localStorage.removeItem(KEY); } catch { /* private mode */ }
  window.dispatchEvent(new Event(EVENT));
}

/** The team to show (GA-225): the picked one while it is one of `teams`, else the first; null without teams. */
export function shownTeam(teams: { id: string }[], picked: string | null): string | null {
  return teams.find((t) => t.id === picked)?.id ?? teams[0]?.id ?? null;
}

/** The team a project's cards are on (GA-225): its own, else the first team. */
export function teamOfProject(projectId: string | null | undefined, projects: { id: string; teamId?: string | null }[], firstTeamId: string): string {
  return (projectId ? projects.find((p) => p.id === projectId)?.teamId : null) || firstTeamId;
}

/** The cards on team `teamId`'s board (GA-225): those of its projects; the first team's board also has the cards without a
 *  project (and of a project not in `projects`). */
export function cardsOfTeam<T extends { projectId?: string | null }>(tasks: T[], projects: { id: string; teamId?: string | null }[], teamId: string,
  firstTeamId: string): T[] {
  const teamOf = new Map(projects.map((p) => [p.id, p.teamId || firstTeamId]));
  return tasks.filter((t) => ((t.projectId && teamOf.get(t.projectId)) || firstTeamId) === teamId);
}

/** The project form's question before it moves a project to another team (GA-225): "Move Giz AI to Research team? Its 31
 *  cards go to the columns of the same kind, and My team's agents are taken off them." */
export function moveQuestion(m: TeamMove): string {
  const ask = `Move ${m.project} to ${m.to}?`;
  if (m.cards === 0) return `${ask} It has no cards yet.`;
  if (m.cards === 1) return `${ask} Its 1 card goes to the column of the same kind, and ${m.from}'s agents are taken off it.`;
  return `${ask} Its ${m.cards} cards go to the columns of the same kind, and ${m.from}'s agents are taken off them.`;
}
