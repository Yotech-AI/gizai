// Screenshots (GA-185, from GA-181's design and GA-182's backend): the defaults, ranges and messages of the screenshot
// and preview settings, for the project form → Preview, the agent form's Take screenshots of what it built, Settings →
// Agents and runs → Screenshots and the card's Properties. The messages are the backend's (check_project,
// check_start_secs and check_limits in crates/gizai-core/src/screenshots.rs), so a form refuses a value before saving
// with the same why the backend would give.
import type { ProjectInput, ProjectScreenshots, Settings } from "../types";

export type ShotSize = ProjectScreenshots["sizes"][number];
/** A whole number's lowest and highest value. */
export type Range = { low: number; high: number };

/** A project has 1 to 4 sizes, each 320 to 3,840 pixels wide and 320 to 2,400 high, with a name of at most 40 characters. */
export const SIZES: Range = { low: 1, high: 4 };
export const WIDTH: Range = { low: 320, high: 3840 };
export const HEIGHT: Range = { low: 320, high: 2400 };
export const SIZE_NAME_CHARS = 40;
/** Start time: how long Gizai waits for the project's preview to answer before the agent starts anyway, in seconds. */
export const START_SECS = { low: 10, high: 600, default: 120 };

/** The sizes the project form knows, in order: every project's two (Desktop and Mobile), then Gizai's own. */
export const KNOWN_SIZES: readonly ShotSize[] = [
  { name: "Desktop", width: 1280, height: 800 }, { name: "Mobile", width: 390, height: 844 },
  { name: "Window", width: 1440, height: 900 }, { name: "Small", width: 900, height: 600 },
];

/** A project's screenshots until its form sets them: on, Desktop 1280 × 800 and Mobile 390 × 844, one theme, one
 *  window's height. */
export function defaultProjectShots(): ProjectScreenshots {
  return { on: true, sizes: KNOWN_SIZES.slice(0, 2).map((s) => ({ ...s })), bothThemes: false, fullPage: false };
}

/** The project's screenshots for its form: a copy, with the defaults for what it doesn't have. */
export function projectShotsFrom(s?: Partial<ProjectScreenshots> | null): ProjectScreenshots {
  const d = defaultProjectShots();
  return {
    on: s?.on ?? d.on,
    sizes: s?.sizes?.length ? s.sizes.map((x) => ({ name: x.name, width: x.width, height: x.height })) : d.sizes,
    bothThemes: s?.bothThemes ?? d.bothThemes,
    fullPage: s?.fullPage ?? d.fullPage,
  };
}

/** What Add a size adds: the first size the form knows whose name the project doesn't use yet. */
export function nextSize(sizes: ShotSize[]): ShotSize {
  const taken = new Set(sizes.map((s) => sizeName(s.name).toLowerCase()));
  return { ...(KNOWN_SIZES.find((k) => !taken.has(k.name.toLowerCase())) ?? KNOWN_SIZES[0]) };
}

/** A size's name as the backend keeps it: trimmed, with one space between its words. */
export const sizeName = (name: string) => name.split(/\s+/).filter(Boolean).join(" ");

const inRange = (n: number, r: Range) => Number.isInteger(n) && n >= r.low && n <= r.high;
/** 3840 → "3,840", as the backend writes the sizes' limits. */
const thousands = (n: number) => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
/** ", not 5000" after the number typed; nothing for an empty field. */
const notIt = (n: number) => (Number.isFinite(n) ? `, not ${n}` : "");

/** Where a project's screenshots can't be saved, and why: its Start time, the number of sizes, or one size's (by index)
 *  name, width or height. */
export type ShotsProblem =
  | { at: "start"; message: string }
  | { at: "sizes"; message: string }
  | { at: "name" | "width" | "height"; size: number; message: string };

/** The first problem of the project form's Preview from the top, or null: the Start time, then the sizes (1 to 4; each
 *  with a name, then its width, then its height). What is left out (null) the backend keeps, so it isn't checked. */
export function projectShotsProblem(v: Pick<ProjectInput, "screenshots" | "previewStartSecs">): ShotsProblem | null {
  const secs = v.previewStartSecs;
  if (secs != null && !inRange(secs, START_SECS)) {
    return { at: "start", message: `Start time must be from ${START_SECS.low} to ${START_SECS.high} seconds.` };
  }
  const sizes = v.screenshots?.sizes;
  if (!sizes) return null;
  if (sizes.length < SIZES.low || sizes.length > SIZES.high) {
    return { at: "sizes", message: `Screenshots need ${SIZES.low} to ${SIZES.high} sizes, not ${sizes.length}.` };
  }
  for (const [i, s] of sizes.entries()) {
    const name = sizeName(s.name);
    if (!name || [...name].length > SIZE_NAME_CHARS || /\p{Cc}/u.test(name)) {
      return { at: "name", size: i, message: `Each size needs a name of at most ${SIZE_NAME_CHARS} characters, like Desktop.` };
    }
    if (!inRange(s.width, WIDTH)) {
      return { at: "width", size: i, message: `${name} must be from ${thousands(WIDTH.low)} to ${thousands(WIDTH.high)} pixels wide${notIt(s.width)}.` };
    }
    if (!inRange(s.height, HEIGHT)) {
      return { at: "height", size: i, message: `${name} must be from ${thousands(HEIGHT.low)} to ${thousands(HEIGHT.high)} pixels high${notIt(s.height)}.` };
    }
  }
  return null;
}

/** Why the project form can't save its Preview as it is (the first problem's message), or null. */
export const checkProjectShots = (v: Pick<ProjectInput, "screenshots" | "previewStartSecs">): string | null => projectShotsProblem(v)?.message ?? null;

/** Settings → Agents and runs → Screenshots, by their names in Settings. */
export type LimitKey = "screenshotsPerRun" | "screenshotMaxMb" | "screenshotsKeepSets" | "screenshotsKeepDays" | "previewPortFrom" | "previewPortTo";
/** Each value of Settings → Agents and runs → Screenshots in the section's order: its default and range, and how a
 *  message names it (its name, and the unit after the highest number), as the backend's `Setting`s do. */
export const LIMITS: Record<LimitKey, Range & { default: number; name: string; unit: string }> = {
  screenshotsPerRun: { default: 12, low: 1, high: 30, name: "Screenshots per run", unit: "" },
  screenshotMaxMb: { default: 10, low: 1, high: 50, name: "Largest screenshot", unit: " MB" },
  screenshotsKeepSets: { default: 3, low: 1, high: 20, name: "Keep per card", unit: " sets" },
  screenshotsKeepDays: { default: 30, low: 0, high: 365, name: "Delete a finished card’s screenshots after", unit: " days" },
  previewPortFrom: { default: 47100, low: 1024, high: 65535, name: "Preview ports", unit: "" },
  previewPortTo: { default: 47999, low: 1024, high: 65535, name: "Preview ports", unit: "" },
};
/** A run's preview gets a block of ten ports: its own and the nine after it. */
export const PORTS_PER_RUN = 10;
/** Runs at once (Settings → Runs), which the backend checks before the preview ports. */
const RUNS_AT_ONCE: Range = { low: 1, high: 20 };

/** How many runs' previews fit in the ports `from` to `to`: blocks of ten. */
export const portBlocks = (from: number, to: number) => (to < from ? 0 : Math.floor((to - from + 1) / PORTS_PER_RUN));

/** Which value Save settings refuses, and why. */
export type LimitsProblem = { key: LimitKey; message: string };

/** The first value of Settings → Agents and runs → Screenshots outside its range, then whether the preview ports go up
 *  and hold ten ports for each run at once (Runs at once, Settings → Runs), or null. A value the backend didn't send
 *  isn't checked: saving keeps what it has. */
export function limitsProblem(s: Partial<Pick<Settings, LimitKey | "maxConcurrentRuns">>): LimitsProblem | null {
  for (const key of Object.keys(LIMITS) as LimitKey[]) {
    const v = s[key];
    const l = LIMITS[key];
    if (v != null && !inRange(v, l)) return { key, message: `${l.name} must be from ${l.low} to ${l.high}${l.unit}.` };
  }
  const from = s.previewPortFrom;
  const to = s.previewPortTo;
  if (from == null || to == null) return null;
  if (from > to) {
    return { key: "previewPortFrom", message: `Preview ports must go from the lower port to the higher one, not from ${from} to ${to}.` };
  }
  // Runs at once outside its range is the backend's to refuse, with its own message
  const runs = s.maxConcurrentRuns;
  if (runs == null || !inRange(runs, RUNS_AT_ONCE)) return null;
  const room = portBlocks(from, to);
  if (room < runs) {
    return { key: "previewPortTo", message: `Preview ports must hold at least ten ports for each run at once: ${from} to ${to} has room for ${room} ${room === 1 ? "run" : "runs"}, and Runs at once (Settings → Runs) is ${runs}.` };
  }
  return null;
}

/** Why Save settings can't save Settings → Agents and runs → Screenshots as they are (the first problem's message), or null. */
export const checkLimits = (s: Partial<Pick<Settings, LimitKey | "maxConcurrentRuns">>): string | null => limitsProblem(s)?.message ?? null;

/** Take screenshots of what it built, for an agent that never set it: on for the frontend, design and qa roles, off for
 *  the rest (`screenshots_default` in crates/gizai-core/src/mcp_servers.rs). */
export const screenshotsDefault = (role: string) => role === "frontend" || role === "design" || role === "qa";

/** The hint under the card's Screenshots before Review (Properties): whether the card has its own switch (`own`) or
 *  follows its project's, and what the project's switch is (`projectOn`; left out while the project loads). */
export function cardShotsHint(own: boolean | null | undefined, projectOn: boolean | null | undefined, project: string): string {
  const which = projectOn == null ? "" : `, which is ${projectOn ? "on" : "off"}`;
  if (own == null) return `Follows ${project}’s Screenshots switch${which}.`;
  return `${own ? "On" : "Off"} for this card. By default it follows ${project}’s Screenshots switch${which}.`;
}

/** A number field's value: empty for no number (NaN while you type), never React's "0" for an emptied field. */
export const shownNumber = (n: number | null | undefined): number | "" => (n == null || Number.isNaN(n) ? "" : n);
/** What a number field holds as typed: NaN while it is empty, which its check refuses. */
export const typedNumber = (text: string): number => (text.trim() === "" ? Number.NaN : Number(text));
