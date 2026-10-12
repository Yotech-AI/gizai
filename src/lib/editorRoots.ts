// Settings → General → Editor folders (GA-113): the folders to save from the rows on screen, what each row was saved as,
// the backend's refusal of a folder as a sentence under its row, and the line under a saved folder: how many folders the
// Editor opens in it, with the first names.
import type { EditorFolder, FolderCheck } from "../types";

/** The backend's refusal of one Editor folder (gizai-core's `editor::clean_roots`): the folder as it was typed, and why. */
export type Refusal = { path: string; why: string };

/** The warning under a folder that isn't there (it may be a disk that isn't mounted): it is saved all the same. */
export const MISSING = "It isn't there now: the picker lists its folders once it is.";

/** The Editor folders to save from the rows on screen: each trimmed, without the empty ones. */
export function rootsToSave(rows: string[]): string[] {
  return rows.map((r) => r.trim()).filter((r) => r !== "");
}

/** What a row was saved as. The backend writes each folder the way Settings shows it (`~/Herd` for `/home/you/Herd/`):
 *  `sent` is the list the last save sent, `kept` what the backend kept of it, in the same order. Null when the row isn't
 *  saved (yet). */
export function savedAs(row: string, sent: string[], kept: string[]): string | null {
  const t = row.trim();
  const i = sent.indexOf(t);
  if (i >= 0 && i < kept.length) return kept[i];
  return t !== "" && kept.includes(t) ? t : null;
}

/** Whether these folders are saved already: each one as typed or as the backend kept it, in the same order. */
export function isSaved(list: string[], sent: string[], kept: string[]): boolean {
  return list.length === kept.length && list.every((r, i) => savedAs(r, sent, kept) === kept[i]);
}

/** The rows after a save: each saved one written as the backend kept it, apart from the row you are typing in. */
export function keptRows(rows: string[], sent: string[], kept: string[], typing: number | null): string[] {
  return rows.map((r, i) => (i === typing ? r : savedAs(r, sent, kept) ?? r));
}

/** A sentence: a capital first and a full stop at the end. */
export function sentence(text: string): string {
  const t = text.trim();
  if (!t) return t;
  const s = t[0].toUpperCase() + t.slice(1);
  return /[.!?…]$/.test(s) ? s : `${s}.`;
}

/** The refusal in a save's error ("Editor folders: ~ can't be used: that's your home folder: pick …"): the folder as typed
 *  ("~") and why, as a sentence ("That's your home folder: pick …."). Null for any other error. */
export function refusalOf(error: string): Refusal | null {
  const m = /Editor folders: ([\s\S]*?) can't be used: ([\s\S]+)$/.exec(error.trim());
  return m ? { path: m[1].trim(), why: sentence(m[2]) } : null;
}

/** The folders of `paths` that aren't there now, from the agent folders' check (`checkAgentFolders`, in the same order):
 *  a warning and no refusal. Its refusals are an agent's, so they are left to the save. */
export function missingOf(paths: string[], checks: FolderCheck[]): string[] {
  return paths.filter((_, i) => !!checks[i]?.warning && !checks[i]?.error);
}

/** A path as shown without the slashes at its end: `~/Herd/` is `~/Herd` (on Windows `~\Herd\` is `~\Herd`). */
const bare = (p: string) => p.trim().replace(/[\\/]+$/, "");

/** Whether `path` is directly inside `root`, both as shown: `~/Herd/kade-portal` is in `~/Herd`. */
export function inRoot(root: string, path: string): boolean {
  const r = bare(root);
  const p = bare(path);
  const cut = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return r !== "" && cut > 0 && p.slice(0, cut) === r;
}

/** "a", "a and b", "a, b and c". */
const andList = (names: string[]) => names.length < 2 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;

/** The line under a saved Editor folder: how many folders the Editor opens directly inside it (from `editorFolders()`,
 *  those of kind "root"), and the first names. "14 folders: gizai-site, invoice-api, kade-portal and 11 more." */
export function rootSummary(root: string, folders: EditorFolder[]): string {
  const names = folders.filter((f) => f.kind === "root" && inRoot(root, f.path)).map((f) => f.name);
  if (names.length === 0) return "No folders in it yet.";
  const count = `${names.length} ${names.length === 1 ? "folder" : "folders"}`;
  return names.length <= 3 ? `${count}: ${andList(names)}.` : `${count}: ${names.slice(0, 3).join(", ")} and ${names.length - 3} more.`;
}
