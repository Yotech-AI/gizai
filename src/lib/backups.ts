// Settings → General → Backups (GA-217) in words: a snapshot's reason and step, its size and computer, and the last and
// next backup, from contract BK (GA-216's `backup_status` and `list_backups`). No React here.
import type { BackupEntry, BackupReason, BackupSettings, BackupStatus } from "../types";
import { formatBytes } from "./format";
import { whenLabel } from "./limits";

/** Keep's range, as the backend checks it. */
export const KEEP_MIN = 1;
export const KEEP_MAX = 100;

/** When, as the section's choices say it. */
export const EVERY: [BackupSettings["every"], string][] = [["daily", "Daily"], ["weekly", "Weekly"], ["manual", "Only when I ask"]];

/** Folder when none is set. */
export const NO_FOLDER = "Not set: Gizai makes no backups.";

/** Why a snapshot was made, in words. */
export function reasonText(r: BackupReason | string): string {
  switch (r) {
    case "daily": return "Daily";
    case "weekly": return "Weekly";
    case "manual": return "By hand";
    case "quit": return "On quit";
    case "before-restore": return "Before a restore";
  }
  return r;
}

/** The step a running snapshot is at (gizai-core's `Step`), in words. */
export function stepText(step: string): string {
  switch (step) {
    case "database": return "Copying the database…";
    case "files": return "Copying your files…";
    case "compress": return "Compressing the database…";
    case "manifest": return "Writing its manifest…";
    case "keep": return "Removing older backups…";
  }
  return `${step}…`;
}

const sameDay = (a: Date, b: Date) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

/** A moment on your clock: "today 14:02", "tomorrow 09:00", "yesterday 14:02", "Mon 13 Oct, 09:00", "13 Oct, 09:00". */
export function backupWhen(ms: number, now: number): string {
  const label = whenLabel(ms, now);
  return sameDay(new Date(ms), new Date(now)) ? `today ${label}` : label;
}

/** A snapshot's size: its database and the files it names. */
export const entrySize = (e: Pick<BackupEntry, "dbBytes" | "filesBytes">) => formatBytes(e.dbBytes + e.filesBytes);

/** "from <computer>" for another computer's snapshot, null for this computer's. A computer Gizai couldn't name (no
 *  $HOSTNAME, so "this machine") is "another computer". */
export function fromText(e: Pick<BackupEntry, "thisComputer" | "deviceName">): string | null {
  if (e.thisComputer) return null;
  const name = e.deviceName.trim();
  return !name || name === "this machine" ? "from another computer" : `from ${name}`;
}

/** Why a snapshot can't be restored, or null when it can. */
export const entryProblem = (e: Pick<BackupEntry, "restorable" | "problem">): string | null =>
  e.restorable ? null : e.problem || "It can't be restored.";

/** "Last backup: today 09:02 (1.2 MB)" (its database file, compressed), or "No backup yet". */
export function lastText(s: Pick<BackupStatus, "last">, now: number): string {
  return s.last ? `Last backup: ${backupWhen(s.last.madeAt, now)} (${formatBytes(s.last.bytes)})` : "No backup yet";
}

/** "Next: tomorrow 09:02", "Next: within the hour" (due: the schedule looks every hour), "Next: when you ask"; null without
 *  a folder. While the folder can't be used, the next one waits for it. */
export function nextText(s: Pick<BackupStatus, "settings" | "folder" | "folderProblem" | "next">, now: number): string | null {
  if (!s.folder) return null;
  if (s.settings.every === "manual") return "Next: when you ask";
  if (s.next != null) return s.next <= now ? "Next: within the hour" : `Next: ${backupWhen(s.next, now)}`;
  return s.folderProblem ? "Next: once the folder can be used again" : "Next: when you ask";
}

/** "The last backup failed (yesterday 09:00): …". */
export const lastErrorText = (e: NonNullable<BackupStatus["lastError"]>, now: number) =>
  `The last backup failed (${backupWhen(e.at, now)}): ${e.message}`;

/** Whether Back up now can start one: with a folder, and while none runs. */
export const canBackUp = (s: Pick<BackupStatus, "folder" | "running">) => !!s.folder && !s.running;

/** Keep as typed: the number to save, or why it isn't one (an empty field saves nothing: null). The backend checks the
 *  range and says why itself. */
export function keepOf(text: string): { keep: number } | { error: string } | null {
  const t = text.trim();
  if (!t) return null;
  const n = Number(t);
  return Number.isInteger(n) ? { keep: n } : { error: `Keep must be a whole number, ${KEEP_MIN} to ${KEEP_MAX}` };
}
