// Updates in plain words: the notice above Company in the sidebar, and Settings → Updates.
import type { UpdateJob, UpdateStatus } from "../types";
import { relTime } from "./format";

/** The notice above Company: an update to offer, its progress, a restart, or an update that failed. */
export type Notice =
  | { kind: "offer"; version: string; text: string; canInstall: boolean }
  | { kind: "working"; version: string; text: string }
  | { kind: "restart"; version: string; text: string }
  | { kind: "failed"; version: string; text: string };

export function isRunning(job: UpdateJob): boolean {
  return job.step === "source" || job.step === "build" || job.step === "backup" || job.step === "install";
}

/** Whether Stop can end the update now: while it gets the source or builds (the backup and install take seconds). */
export function canStop(job: UpdateJob): boolean {
  return job.step === "source" || job.step === "build";
}

/** The notice above Company, or null when there is nothing newer. */
export function notice(s: UpdateStatus): Notice | null {
  const job = s.job;
  if (job && isRunning(job)) return { kind: "working", version: job.version, text: stepText(job) };
  const installed = job?.step === "installed" ? job.version : s.installed;
  if (installed) return { kind: "restart", version: installed, text: `Restart to use ${installed}` };
  if (!s.available) return null;
  const v = s.available.version;
  if (job?.step === "failed" && job.version === v) return { kind: "failed", version: v, text: `Update to ${v} failed` };
  return { kind: "offer", version: v, text: `Update to ${v}`, canInstall: !s.cannotInstall };
}

/** Where an update is, in a few words: "Building 0.1.6…". */
export function stepText(job: UpdateJob): string {
  switch (job.step) {
    case "source": return `Getting ${job.version}…`;
    case "build": return `Building ${job.version}…`;
    case "backup": return "Backing up your data…";
    case "install": return `Installing ${job.version}…`;
    case "installed": return `Installed ${job.version}`;
    case "failed": return `Update to ${job.version} failed`;
    case "stopped": return `Update to ${job.version} stopped`;
  }
}

/** ok: this is the latest version; new: a newer one is out or installed; failed: the check didn't work; skipped: no
 * check yet, or no release yet. */
export type CheckMark = "ok" | "new" | "failed" | "skipped";

/** What the last release check found, in one line. */
export function checkLine(s: UpdateStatus, now = Date.now()): { mark: CheckMark; text: string } {
  const when = s.checkedAt ? ` (checked ${relTime(s.checkedAt, now)})` : "";
  if (s.checking) return { mark: "skipped", text: "Asking GitHub for the latest release…" };
  if (s.problem) return { mark: "failed", text: `The last check didn't work${when}: ${s.problem.fix ? `${s.problem.what}. ${s.problem.fix}` : s.problem.what}` };
  if (s.installed) return { mark: "new", text: `Version ${s.installed} is installed. Restart Gizai to use it.` };
  if (s.available) return { mark: "new", text: `Version ${s.available.version} is out${when}.` };
  if (!s.checkedAt) return { mark: "skipped", text: "Not checked yet." };
  if (!s.latest) return { mark: "skipped", text: `There is no release on GitHub yet${when}.` };
  return { mark: "ok", text: `This is the latest version${when}.` };
}
