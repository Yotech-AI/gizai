// Reads that screens share (GA-190; research docs/research/performance.md F1, P2). A screen's data hook (useData) runs its
// fetch through runFetch: the commands the fetch calls are noted, so the hook learns which tables it shows (lib/tables.ts),
// and a read already on its way with the same arguments, asked by another hook, is shared: the sidebar and the page that
// both show the cards, the team or the projects ask the backend once. Only hooks share, never a call made elsewhere (a
// read right after a write wants that write's result), and a notice about a table drops what was on its way for it.
import { READS } from "./tables";

/** The commands the hook's fetch that runs now has called; null outside one. */
let calls: string[] | null = null;
/** Hooks' reads on their way, by command and arguments. */
const flying = new Map<string, { answer: Promise<unknown>; tables: readonly string[] }>();

/** Runs a data hook's fetch, and says which commands it called before it returned (its first reads). */
export function runFetch<T>(fetch: () => Promise<T>): { answer: Promise<T>; commands: string[] } {
  const outer = calls;
  const commands: string[] = [];
  calls = commands;
  try {
    return { answer: fetch(), commands };
  } catch (e) {
    return { answer: Promise.reject(e), commands };
  } finally {
    calls = outer;
  }
}

/** Every backend call goes through here (src/api.ts). In a hook's fetch, a read the table map knows joins the same read
 *  another hook already has on its way, else it starts one the next hook can join. */
export function shared<T>(command: string, args: unknown, call: () => Promise<T>): Promise<T> {
  if (!calls) return call();
  calls.push(command);
  const tables = READS[command];
  if (!tables) return call();
  const key = `${command}\n${JSON.stringify(args ?? null)}`;
  const on = flying.get(key);
  if (on) return on.answer as Promise<T>;
  const answer = call();
  flying.set(key, { answer, tables });
  const done = () => { if (flying.get(key)?.answer === answer) flying.delete(key); };
  answer.then(done, done);
  return answer;
}

/** A write changed `table`: a read of it that is on its way may have read before the write, so no hook joins it now. */
export function forgetReads(table: string) {
  for (const [key, on] of flying) if (on.tables.includes(table)) flying.delete(key);
}
