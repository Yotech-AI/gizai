// Editing a doc or a memory note (the doc page and the Memory page): the text in the editor and the version it is
// based on, saved on Ctrl+S / Ctrl+Enter, on leaving the editor and on leaving the page. Every save is a version; a save
// based on an old version (an agent saved meanwhile) is a conflict, and the page asks what to do.
import { useEffect, useRef, useState } from "react";
import { getDoc, saveDoc } from "../api";
import { useData } from "./useData";

export type SaveStatus = "saved" | "unsaved" | "saving" | "conflict";

export function useDocEditor(id: string) {
  const { data: doc, error } = useData(() => getDoc(id), [id]);
  const [text, setText] = useState<string | null>(null);
  const [base, setBase] = useState<number | null>(null);
  const [status, setStatus] = useState<SaveStatus>("saved");
  const [err, setErr] = useState<string | null>(null);
  const live = useRef({ text, base, status });
  live.current = { text, base, status };
  const saved = useRef(""); // the text of version `base`
  const inflight = useRef(false); // blur and the Save button can both fire: one save at a time

  const adopt = (body: string, version: number) => { saved.current = body; setText(body); setBase(version); setStatus("saved"); };
  // First load, and later versions saved by someone else (an agent) while we have nothing unsaved.
  useEffect(() => {
    if (!doc) return;
    const { base: b, status: s } = live.current;
    if (b === null || (doc.currentVersion !== b && s === "saved")) adopt(doc.bodyMd, doc.currentVersion);
  }, [doc]); // eslint-disable-line react-hooks/exhaustive-deps

  const save = async (md: string, force = false) => {
    const { base: b } = live.current;
    if (b === null || !doc || inflight.current) return;
    if (!force && md === saved.current) { setStatus("saved"); return; }
    inflight.current = true;
    setStatus("saving");
    try {
      const v = await saveDoc(id, md, force ? (await getDoc(id)).currentVersion : b);
      saved.current = md;
      setBase(v); setErr(null);
      setStatus(live.current.text === md ? "saved" : "unsaved");
    } catch (e) {
      if (String(e).includes("changed since")) setStatus("conflict"); else { setStatus("unsaved"); setErr(String(e)); }
    } finally { inflight.current = false; }
  };
  const saveRef = useRef(save);
  saveRef.current = save;
  // Leaving the page saves unsaved text.
  useEffect(() => () => { const { text: t, status: s } = live.current; if (t !== null && s === "unsaved") saveRef.current(t); }, []);

  /** The editor's text changed. */
  const edit = (md: string) => { setText(md); setStatus((s) => (s === "conflict" || s === "saving" ? s : md === saved.current ? "saved" : "unsaved")); };
  /** The editor lost the focus: unsaved text is saved. */
  const blur = (md: string) => { if (live.current.status === "unsaved") save(md); };
  /** Sets the text from outside the editor (a property, a restored version) and saves it. */
  const replace = (md: string, force = false) => { setText(md); save(md, force); };

  return { doc, error, text, base, status, err, setErr, adopt, save, edit, blur, replace };
}
