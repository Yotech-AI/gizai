// Editing a doc or a memory note (the doc page and the Memory page): the text in the editor and the version it is
// based on. Every save is a version, so the text saves on Ctrl+S / Ctrl+Enter and the Save button, after AUTOSAVE_MS
// without typing, on leaving the page and when Gizai closes (GA-122: `closeSave`); leaving the editor (a click on the
// title, the history or another window) does not save (GA-118). A save based on an old version (an agent saved meanwhile)
// is a conflict, and the page asks what to do.
import { useEffect, useRef, useState } from "react";
import { getDoc, saveDoc } from "../api";
import { editsChanged, registerEdit } from "./closeSave";
import { titleOf } from "./memory";
import { useData } from "./useData";

export type SaveStatus = "saved" | "unsaved" | "saving" | "conflict";

/** Unsaved changes save on their own after this long without typing: one version, each change restarts the wait. */
export const AUTOSAVE_MS = 5 * 60 * 1000;

/** What the automatic save does when its wait is over: save unsaved text, wait again while a save is under way, and
 *  nothing when all is saved or the conflict bar shows (the person decides there). */
export function autosaveStep(status: SaveStatus): "save" | "wait" | "none" {
  return status === "unsaved" ? "save" : status === "saving" ? "wait" : "none";
}

/** A wait that each `restart` begins again: `fire` runs once, `ms` after the last restart, unless `cancel`led first. */
export function autosaveTimer(fire: () => void, ms = AUTOSAVE_MS) {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const cancel = () => { if (timer !== null) { clearTimeout(timer); timer = null; } };
  const restart = () => { cancel(); timer = setTimeout(() => { timer = null; fire(); }, ms); };
  return { restart, cancel, pending: () => timer !== null };
}

export function useDocEditor(id: string) {
  const { data: doc, error } = useData(() => getDoc(id), [id]);
  const [text, setText] = useState<string | null>(null);
  const [base, setBase] = useState<number | null>(null);
  const [status, setStatus] = useState<SaveStatus>("saved");
  const [err, setErr] = useState<string | null>(null);
  const live = useRef({ text, base, status, doc });
  live.current = { text, base, status, doc };
  const saved = useRef(""); // the text of version `base`
  // The save under way, if any: the Save button, a shortcut and the automatic save can meet, so one save at a time.
  const inflight = useRef<Promise<void> | null>(null);
  const auto = useRef<ReturnType<typeof autosaveTimer> | null>(null);

  const adopt = (body: string, version: number) => { saved.current = body; setText(body); setBase(version); setStatus("saved"); };
  // First load, and later versions saved by someone else (an agent) while we have nothing unsaved.
  useEffect(() => {
    if (!doc) return;
    const { base: b, status: s } = live.current;
    if (b === null || (doc.currentVersion !== b && s === "saved")) adopt(doc.bodyMd, doc.currentVersion);
  }, [doc]); // eslint-disable-line react-hooks/exhaustive-deps

  /** Saves `md` as a version: true when it saved (or was saved already), false when it didn't (or another save was
   *  under way). */
  const save = async (md: string, force = false): Promise<boolean> => {
    const { base: b } = live.current;
    if (b === null || !doc || inflight.current) return false;
    if (!force && md === saved.current) { setStatus("saved"); return true; }
    let done = () => {};
    inflight.current = new Promise<void>((ok) => { done = ok; });
    auto.current?.cancel();
    setStatus("saving");
    try {
      const v = await saveDoc(id, md, force ? (await getDoc(id)).currentVersion : b);
      saved.current = md;
      live.current.base = v; // before the next render, for a save right after this one (closing Gizai)
      setBase(v); setErr(null);
      setStatus(live.current.text === md ? "saved" : "unsaved");
      return true;
    } catch (e) {
      if (String(e).includes("changed since")) setStatus("conflict"); else { setStatus("unsaved"); setErr(String(e)); }
      return false;
    } finally { inflight.current = null; done(); }
  };
  const saveRef = useRef(save);
  saveRef.current = save;
  // The automatic save, AUTOSAVE_MS after the last change. Not a forced save: an agent's newer version shows the conflict bar.
  if (!auto.current) {
    auto.current = autosaveTimer(() => {
      const { text: t, status: s } = live.current;
      const step = autosaveStep(s);
      if (step === "wait") auto.current?.restart();
      else if (step === "save" && t !== null) saveRef.current(t);
    });
  }
  // Leaving the page saves unsaved text.
  useEffect(() => () => {
    auto.current?.cancel();
    const { text: t, status: s } = live.current;
    if (t !== null && s === "unsaved") saveRef.current(t);
  }, []);
  // Closing Gizai saves unsaved text first (GA-122): the editor is listed while it is open, and until the save on leaving
  // the page has ended. Unsaved is also a save under way and the conflict bar's text.
  useEffect(() => {
    const off = registerEdit({
      title: () => { const d = live.current.doc; return d ? (d.kind === "memory" ? titleOf(d.path ?? d.title) : d.title) : "this doc"; },
      unsaved: () => inflight.current !== null || (live.current.text !== null && live.current.text !== saved.current),
      flush: async () => {
        while (inflight.current) await inflight.current;
        const t = live.current.text;
        return t === null || t === saved.current || saveRef.current(t);
      },
    });
    return () => { if (inflight.current) inflight.current.then(off); else off(); };
  }, []);
  useEffect(() => { editsChanged(); }, [status]);

  /** The editor's text changed: unsaved text waits AUTOSAVE_MS again before it saves on its own. */
  const edit = (md: string) => {
    setText(md);
    setStatus((s) => (s === "conflict" || s === "saving" ? s : md === saved.current ? "saved" : "unsaved"));
    if (md === saved.current) auto.current?.cancel(); else auto.current?.restart();
  };
  /** Sets the text from outside the editor (a property, a restored version) and saves it. */
  const replace = (md: string, force = false) => { setText(md); save(md, force); };

  return { doc, error, text, base, status, err, setErr, adopt, save, edit, replace };
}
