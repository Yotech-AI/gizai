// Settings: the warning under "Runs at once" (was GA-27), and the warning by Quit Gizai completely (GA-21).
import type { Team } from "../types";

/** An agent that takes cards, with its cards at once. */
export type CardAgent = { name: string; cardsAtOnce: number };

/** The agents "Runs at once" is shared by: the active agents on at least one column (Team → Workflow), each once. The
 *  columns decide when an agent works, so one on no column, like the Team Lead who answers in chat, takes no cards. */
export function cardAgents(teams: Team[]): CardAgent[] {
  const seen = new Set<string>();
  const out: CardAgent[] = [];
  for (const team of teams) {
    const onColumns = new Set(team.states.flatMap((s) => s.agentIds ?? []));
    for (const m of team.members) {
      if (m.kind !== "agent" || m.status !== "active" || !onColumns.has(m.actorId) || seen.has(m.actorId)) continue;
      seen.add(m.actorId);
      out.push({ name: m.name, cardsAtOnce: Math.max(1, m.maxRuns ?? 1) });
    }
  }
  return out;
}

/** The warning under "Runs at once" when it is lower than the agents' cards at once added up: some agents then can't
 *  use all their slots. Null when the numbers fit, or while the field holds no number from 1 up. */
export function runsAtOnceWarning(runsAtOnce: number, agents: CardAgent[]): string | null {
  const total = agents.reduce((n, a) => n + a.cardsAtOnce, 0);
  if (!Number.isInteger(runsAtOnce) || runsAtOnce < 1 || runsAtOnce >= total) return null;
  const each = agents.map((a) => `${a.cardsAtOnce} for ${a.name}`).join(", ");
  const fix = total <= 20
    ? `Set Runs at once to ${total}, or lower an agent's Cards at once on the Team page.`
    : "Runs at once goes up to 20, so lower some agents' Cards at once on the Team page.";
  return `Your active agents can work on ${total} cards at once together (${each}), but Runs at once is ${runsAtOnce}, so some of them can't use all their slots. ${fix}`;
}

/** The warning by Quit Gizai completely: what stops, and how many runs and chat answers it stops when some are at work. */
export function quitWarning(runs: number, answers: number): string {
  const text = "Quitting stops all agents (running cards are stopped), heartbeats and notifications until you start Gizai again. Closing the window only hides Gizai.";
  const live = [runs > 0 && `${runs} ${runs === 1 ? "run" : "runs"}`, answers > 0 && `${answers} chat ${answers === 1 ? "answer" : "answers"}`].filter(Boolean);
  if (live.length === 0) return text;
  return `${text} Now ${live.join(" and ")} will be stopped.`;
}
