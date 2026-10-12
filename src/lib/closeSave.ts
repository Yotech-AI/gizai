// Closing Gizai with unsaved text in the doc page's or the Memory page's editor (GA-122). Each open editor (useDocEditor)
// is listed here; closing the window (its close button) or quitting (Quit Gizai completely, a signal) first saves their
// unsaved text as one version each, waiting at most CLOSE_SAVE_MS, and asks before closing when that fails or takes
// longer (CloseGuard). Minimising or switching windows doesn't close, so it saves nothing.

/** How long closing waits for the unsaved text to save before it asks. */
export const CLOSE_SAVE_MS = 2000;

/** An open editor, as closing sees it. */
export type OpenEdit = {
  /** The doc's or the note's title, for the question. */
  title: () => string;
  /** Whether it has text that isn't saved (also while a save is under way). */
  unsaved: () => boolean;
  /** Saves the unsaved text as one version, after a save that is under way: true when nothing is left unsaved. */
  flush: () => Promise<boolean>;
};

/** What a close comes to: everything saved, or the title of the first editor whose text didn't save (failed or too slow). */
export type CloseSave = { saved: true } | { saved: false; title: string };

const open = new Set<OpenEdit>();
let told: boolean | null = null;
let tell: ((unsaved: boolean) => void) | null = null;

/** Whether any open editor has unsaved text. */
export const anyUnsaved = (edits: Iterable<OpenEdit> = open) => [...edits].some((e) => e.unsaved());

/** Lists an editor while it is open; the returned function takes it off the list. */
export function registerEdit(edit: OpenEdit): () => void {
  open.add(edit);
  editsChanged();
  return () => { open.delete(edit); editsChanged(); };
}

/** An editor's text was saved or changed: `onUnsavedChange` hears when the answer to `anyUnsaved` changes. */
export function editsChanged() {
  const now = anyUnsaved();
  if (now === told || !tell) return;
  told = now;
  tell(now);
}

/** Who hears whether there is unsaved text (CloseGuard tells Gizai, so a close waits for the page only then): at once,
 *  then on each change. The returned function stops it. */
export function onUnsavedChange(cb: (unsaved: boolean) => void): () => void {
  tell = cb;
  told = null;
  editsChanged();
  return () => { if (tell === cb) { tell = null; told = null; } };
}

/** The editors open now. */
export const openEdits = (): OpenEdit[] => [...open];

/** Saves every editor's unsaved text before Gizai closes, waiting at most `ms` for all of them. */
export async function saveBeforeClose(edits: OpenEdit[] = openEdits(), ms = CLOSE_SAVE_MS): Promise<CloseSave> {
  const left = edits.filter((e) => e.unsaved());
  if (left.length === 0) return { saved: true };
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<"late">((ok) => { timer = setTimeout(() => ok("late"), ms); });
  const done: (boolean | undefined)[] = left.map(() => undefined);
  const all = Promise.all(left.map((e, i) => e.flush().catch(() => false).then((ok) => { done[i] = ok; })));
  try {
    await Promise.race([all, late]);
    // The first one that failed or is still saving.
    const first = done.findIndex((ok) => ok !== true);
    return first === -1 ? { saved: true } : { saved: false, title: left[first]!.title() };
  } finally {
    clearTimeout(timer);
  }
}

/** The question when the text didn't save. */
export const closeQuestion = (title: string) => `Close Gizai without saving your changes to ${title}?`;
