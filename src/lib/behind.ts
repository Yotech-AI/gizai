// Behind main in the Editor (GA-114, F3 of the Code Editor; research §3.2 Q7, design design/code-editor/editor.html §5):
// how a folder stands against main in the Editor's top bar and on its sidebar row, and which bar the page shows under
// the top bar: behind with Update, a refusal, Update running, how it ended, a fetch that failed. Worked out from contract
// C3's answers (FolderMain, EditorUpdated) and kept apart from the screen so it can be tested; the words are D2's.
import type { EditorUpdated, FolderGit, FolderMain } from "../types";
import { relTime } from "./format";
import { standingTip, standingWord } from "./editor";

/** The Editor fetches main when it opens a folder and then every 10 minutes while it shows it. */
export const FETCH_EVERY = 10 * 60_000;

const n = (x: number) => x.toLocaleString("en-US");
const plural = (x: number, one: string, many: string) => `${n(x)} ${x === 1 ? one : many}`;
/** The sentence with a full stop at its end. */
export const fullStop = (s: string) => { const t = s.trim(); return /[.!?…]$/.test(t) ? t : `${t}.`; };
const noStop = (s: string) => s.trim().replace(/\.$/, "");
const capital = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
/** When main was last fetched, for a sentence: "2m ago", "just now". */
const fetched = (m: FolderMain, now: number) => (m.fetchedAt === null ? null : relTime(m.fetchedAt, now));

/** The folder isn't a git repository: no branch, no git marks, Fetch or Update. */
export const notGit = (m: FolderMain | null) => !!m && !m.mainLabel && /isn't a git repository/.test(m.updateSays);
/** The folder has a main to compare with and fetch (not outside git, not without an origin remote or a project's link). */
export const hasMain = (m: FolderMain | null): m is FolderMain => !!m && m.mainLabel !== "";

/** The folder's git status for how many files changed: the Editor's when it has it (it follows saves at once), else
 *  main's count. */
const gitOf = (m: FolderMain | null, git: FolderGit | null): FolderGit | null => git ?? (m ? { branch: m.branch, files: {}, uncommitted: m.uncommitted } : null);

/** How the folder stands, in sentences for a tooltip: "main is 3 commits behind origin/main, fetched 2m ago",
 *  "main has 1 commit that origin/main doesn't", "4 files changed, not committed"; while Update runs, what it does. */
export function standingSentences(m: FolderMain | null, git: FolderGit | null, now = Date.now()): string[] {
  const files = standingTip(gitOf(m, git)) || null;
  if (!m) return files ? [files] : [];
  if (!m.mainLabel) return [m.updateSays, ...(files && !notGit(m) ? [files] : [])].filter(Boolean);
  if (m.updating) return [m.updateSays ? `Updating: ${noStop(m.updateSays)}` : "Updating"];
  const branch = m.branch ?? "It";
  const ago = fetched(m, now);
  const out: string[] = m.branch ? [] : ["No branch checked out"];
  if (m.behind > 0) out.push(`${branch} is ${plural(m.behind, "commit", "commits")} behind ${m.mainLabel}${ago ? `, fetched ${ago}` : ""}`);
  else if (ago === null) out.push(m.branch ? `${m.branch}, ${m.mainLabel} not fetched yet` : `${m.mainLabel} not fetched yet`);
  else out.push(m.branch ? `${m.branch}, up to date with ${m.mainLabel}` : `Up to date with ${m.mainLabel}`);
  if (m.ahead > 0) out.push(`${branch} has ${plural(m.ahead, "commit", "commits")} that ${m.mainLabel} doesn't`);
  if (files) out.push(files);
  return out;
}

/** The top bar's standing: the branch (mono, null for none to show) and the words after it, each after a "·":
 *  "main · 3 behind origin/main · 1 ahead · 2 changed", "main · up to date with origin/main", "main · not fetched yet",
 *  "main · no remote", "Not a git repository". `tip` says it in sentences. */
export function standing(m: FolderMain | null, git: FolderGit | null, now = Date.now()): { branch: string | null; words: string[]; tip: string } {
  const files = standingWord(gitOf(m, git)) || null;
  const tip = standingSentences(m, git, now).join(" · ");
  if (!m) return { branch: git?.branch ?? null, words: files ? [files] : [], tip };
  if (notGit(m)) return { branch: null, words: ["Not a git repository"], tip: m.updateSays };
  const branch = m.branch ?? "no branch";
  if (!m.mainLabel) return { branch, words: ["no remote", ...(files ? [files] : [])], tip };
  const words = [m.behind > 0 ? `${n(m.behind)} behind ${m.mainLabel}` : m.fetchedAt === null ? "not fetched yet" : `up to date with ${m.mainLabel}`];
  if (m.ahead > 0) words.push(`${n(m.ahead)} ahead`);
  if (files) words.push(files);
  return { branch, words, tip };
}

/** One word on how the folder stands, on its sidebar row: "updating" while Update runs, else behind first ("3 behind"),
 *  else changed ("4 changed"), else ahead ("1 ahead"), else "". A card's worktree is on the card's own branch, which
 *  Update doesn't move: its row says only how many files changed. */
export function rowWord(m: FolderMain | null, git: FolderGit | null, card = false): string {
  if (m?.updating) return "updating";
  const changed = standingWord(gitOf(m, git));
  if (!card && hasMain(m) && m.behind > 0) return `${n(m.behind)} behind`;
  if (changed) return changed;
  if (!card && hasMain(m) && m.ahead > 0) return `${n(m.ahead)} ahead`;
  return "";
}

/** The sidebar row's tooltip: the folder's path, then how it stands. */
export function rowTip(path: string, m: FolderMain | null, git: FolderGit | null, now = Date.now()): string {
  return [path, ...standingSentences(m, git, now)].join(" · ");
}

/** The folded Editor heading's tag and its tooltip: "2 behind", "2 folders are behind origin/main"; null when none is.
 *  Cards' worktrees don't count. */
export function foldedBehind(list: { main: FolderMain | null; card: boolean }[]): { word: string; tip: string } | null {
  const behind = list.filter((f) => !f.card && hasMain(f.main) && f.main.behind > 0).map((f) => f.main!);
  if (!behind.length) return null;
  const labels = [...new Set(behind.map((m) => m.mainLabel))];
  const k = behind.length;
  return { word: `${n(k)} behind`, tip: `${plural(k, "folder", "folders")} ${k === 1 ? "is" : "are"} behind ${labels.length === 1 ? labels[0] : "main"}` };
}

/** The top bar's Fetch: its tooltip. */
export function fetchTitle(m: FolderMain, now = Date.now()): string {
  const ago = fetched(m, now);
  return `Fetch ${m.mainLabel} now (${ago ? `last fetched ${ago}` : "not fetched yet"})`;
}

/** Whether the Editor should fetch main again: 10 minutes after it last tried, while the window shows and no update runs. */
export const fetchDue = (lastTried: number, now: number, m: FolderMain | null) => now - lastTried >= FETCH_EVERY && !m?.updating;

/** The bar under the top bar about main, or null for none. One at a time, the first that applies:
 *  - `updating`: Update runs (here or from the Team Lead's chat): what it does, with a turning loader.
 *  - `done`, `stopped`, `didnt-start`: how the update ended (`editor-updated`), until Dismiss; with its output to show.
 *  - `refused` with Update on: Update was refused when clicked, though the folder's checks now let it run.
 *  - `fetch-failed`: the last fetch failed, with Try again.
 *  - `behind`: behind main, Update can run: what it will do. `refused`: behind, and why Update can't run (Update off).
 *  `lead` is the bold start ("3 commits behind origin/main"), `rest` what follows it; `tip` is Update's tooltip. */
export type MainBar =
  | { kind: "updating"; text: string }
  | { kind: "done" | "stopped" | "didnt-start"; text: string; output: string }
  | { kind: "busy"; text: string }
  | { kind: "fetch-failed"; text: string }
  | { kind: "behind" | "refused"; lead: string | null; rest: string; tip: string; update: "on" | "off" };

export function mainBar(m: FolderMain | null, o: { name: string; ended: EditorUpdated | null; refusal: string | null; starting: boolean; now?: number }): MainBar | null {
  const now = o.now ?? Date.now();
  if (o.starting || m?.updating) {
    const says = m?.updating ? m.updateSays : m?.canUpdate ? m.updateSays : "";
    return { kind: "updating", text: says ? `Updating ${o.name}: ${noStop(says)}…` : `Updating ${o.name}…` };
  }
  if (o.ended) {
    const e = o.ended;
    if (e.ok) return { kind: "done", text: `Updated ${o.name}: ${e.said.replace(/^(Moved|Left) /, (w) => w.toLowerCase())}`, output: e.output };
    return { kind: / didn't start: /.test(e.said) ? "didnt-start" : "stopped", text: e.said, output: e.output };
  }
  if (o.refusal && /^An update of .* is already running/.test(o.refusal)) return { kind: "busy", text: fullStop(o.refusal) };
  if (!hasMain(m)) return o.refusal ? { kind: "refused", lead: null, rest: `Update can't run: ${fullStop(o.refusal)}`, tip: `Update can't run: ${fullStop(o.refusal)}`, update: "off" } : null;
  const ago = fetched(m, now);
  const lead = m.behind > 0 ? `${plural(m.behind, "commit", "commits")} behind ${m.mainLabel}` : null;
  const after = lead ? (ago ? `, fetched ${ago}.` : ".") : "";
  if (o.refusal && m.canUpdate) {
    const why = `Update can't run: ${fullStop(o.refusal)}`;
    return { kind: "refused", lead, rest: lead ? `${after} ${why}` : why, tip: why, update: "on" };
  }
  if (m.fetchError) {
    return { kind: "fetch-failed", text: `${fullStop(m.fetchError)} ${ago ? `The counts are from the last fetch, ${ago}.` : `Gizai can't tell yet whether ${o.name} is behind.`}` };
  }
  if (!lead) return null;
  if (m.canUpdate) return { kind: "behind", lead, rest: `${after} Update will ${fullStop(m.updateSays)}`, tip: capital(noStop(m.updateSays)), update: "on" };
  const why = `Update can't run: ${fullStop(m.updateSays)}`;
  return { kind: "refused", lead, rest: `${after} ${why}`, tip: why, update: "off" };
}

// ---- one answer for the sidebar and the page (GA-190) ----

const asking = new Map<string, Promise<unknown>>();
/** One answer for everyone who asks `key` while it is on its way. */
function once<T>(key: string, ask: () => Promise<T>): Promise<T> {
  const on = asking.get(key);
  if (on) return on as Promise<T>;
  const answer = ask();
  asking.set(key, answer);
  const done = () => { if (asking.get(key) === answer) asking.delete(key); };
  answer.then(done, done);
  return answer;
}
/** How the folder stands against main (`ask` is editor_main, some 15 to 30 git processes), asked once for the sidebar
 *  and the Editor page together: while an answer is on its way, asking again gets that one (a window focus asks both). */
export const askMain = (folder: string, ask: (folder: string) => Promise<FolderMain>) => once(`main\n${folder}`, () => ask(folder));
/** The folder's uncommitted marks (`ask` is editor_git), shared the same way. */
export const askGit = (folder: string, ask: (folder: string) => Promise<FolderGit>) => once(`git\n${folder}`, () => ask(folder));

// ---- the sidebar follows the page: each FolderMain the page gets is sent on the window ----

const MAIN_EVENT = "gizai-editor-main";
/** Tells the sidebar how the folder `id` stands now (Update started, a fetch came back), so its row follows at once. */
export function tellMain(id: string, main: FolderMain) {
  try { window.dispatchEvent(new CustomEvent(MAIN_EVENT, { detail: { id, main } })); } catch { /* no window in tests */ }
}
/** Calls `on` with each folder's FolderMain the page tells; returns how to stop. */
export function onTold(on: (id: string, main: FolderMain) => void): () => void {
  const f = (e: Event) => { const d = (e as CustomEvent<{ id: string; main: FolderMain }>).detail; if (d) on(d.id, d.main); };
  try { window.addEventListener(MAIN_EVENT, f); } catch { return () => {}; }
  return () => { try { window.removeEventListener(MAIN_EVENT, f); } catch { /* no window */ } };
}
