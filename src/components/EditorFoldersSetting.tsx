// Settings → General → Editor folders (GA-113): the folders your projects are in. Besides projects' linked folders and
// card worktrees, the Editor opens every folder directly inside these. A row each, like an agent's Folders, plus Browse…
// for the system's folder picker; saved as soon as you add, change or remove one. The backend refuses a folder the
// Editor may never open (gizai-core's `editor::clean_roots`): that list isn't saved, and the reason shows under its row.
import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Plus, X } from "lucide-react";
import { checkAgentFolders, editorFolders, getSettings, saveSettings } from "../api";
import { isSaved, keptRows, MISSING, missingOf, refusalOf, rootSummary, rootsToSave, savedAs, type Refusal } from "../lib/editorRoots";
import type { AgentFolder, EditorFolder, Settings } from "../types";
import { Field, FormSection } from "./Form";

/** How long a row you type in waits before it is saved. */
const TYPING_MS = 600;

/** `s` and `setS` are Settings' own: `s.editorRoots` stays the list as saved, so Save settings never writes back an older
 *  list, nor a refused one. */
export function EditorFoldersSetting({ s, setS }: { s: Settings; setS: Dispatch<SetStateAction<Settings | null>> }) {
  const [rows, setRows] = useState<string[]>(s.editorRoots);
  // The last list saved: as it was sent, and as the backend kept it (in the same order).
  const [saved, setSaved] = useState({ sent: s.editorRoots, kept: s.editorRoots });
  const [refused, setRefused] = useState<Refusal | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [found, setFound] = useState<EditorFolder[] | null>(null);
  const [missing, setMissing] = useState<string[]>([]);
  const [added, setAdded] = useState<number | null>(null);
  const alive = useRef(true);
  const typing = useRef<number | null>(null);
  const sentKey = useRef(JSON.stringify(s.editorRoots));
  const waiting = useRef<string[] | null>(null);
  const last = useRef(0);
  const queue = useRef(Promise.resolve());
  const list = rootsToSave(rows);
  const key = JSON.stringify(list);

  /** Saves the folders, one save after the other. Only them: the other settings go as they are saved now, so a field
   *  still being typed in on another tab isn't saved with them. Then the rows show the folders as the backend kept them.
   *  A refused list isn't saved: the last good one stays, in `s` too. */
  const store = (next: string[]) => {
    sentKey.current = JSON.stringify(next);
    waiting.current = null;
    const mine = ++last.current;
    queue.current = queue.current.then(async () => {
      try {
        await saveSettings({ ...(await getSettings()), editorRoots: next });
        const kept = (await getSettings()).editorRoots;
        setS((x) => x && { ...x, editorRoots: kept });
        const f = await editorFolders().catch(() => null);
        if (!alive.current) return;
        setSaved({ sent: next, kept });
        if (f) setFound(f);
        // only the latest save's answer changes the rows; an older one only says what is saved
        if (mine === last.current) { setRefused(null); setErr(null); setRows((rs) => keptRows(rs, next, kept, typing.current)); }
      } catch (e) {
        if (!alive.current || mine !== last.current) return;
        const r = refusalOf(String(e));
        // under its row when one was sent as the refusal names it, else under the section
        setRefused(r && next.includes(r.path) ? r : null);
        setErr(r && next.includes(r.path) ? null : String(e));
      }
    });
  };
  /** Adding a folder with Browse… or removing one saves at once. */
  const saveNow = (next: string[]) => {
    setRows(next);
    setAdded(null);
    const l = rootsToSave(next);
    if (JSON.stringify(l) !== sentKey.current && !isSaved(l, saved.sent, saved.kept)) store(l);
  };

  useEffect(() => {
    alive.current = true;
    editorFolders().then((f) => { if (alive.current) setFound(f); }).catch(() => {});
    // leaving Settings within a moment of typing still saves it
    return () => { alive.current = false; if (waiting.current) store(waiting.current); };
  }, []);
  // Typing saves once you pause.
  useEffect(() => {
    const next: string[] = JSON.parse(key);
    if (key === sentKey.current || isSaved(next, saved.sent, saved.kept)) { waiting.current = null; return; }
    waiting.current = next;
    const t = setTimeout(() => store(next), TYPING_MS);
    return () => clearTimeout(t);
  }, [key, saved]);
  // A folder that isn't there gets a warning as you type. The agent folders' check says so; its refusals are an agent's,
  // so a refusal shows when the save says it.
  useEffect(() => {
    const paths: string[] = JSON.parse(key);
    if (paths.length === 0) { setMissing([]); return; }
    let live = true;
    const t = setTimeout(() => {
      checkAgentFolders(paths.map((path): AgentFolder => ({ path, access: "read" }))).then((c) => { if (live) setMissing(missingOf(paths, c)); }).catch(() => {});
    }, 300);
    return () => { live = false; clearTimeout(t); };
  }, [key]);

  const put = (i: number, v: string) => setRows((rs) => rs.map((r, j) => (j === i ? v : r)));
  // Leaving a row shows it as it was saved: ~/Herd for /home/you/Herd/.
  const leave = (i: number) => {
    typing.current = null;
    setRows((rs) => rs.map((r, j) => (j === i ? savedAs(r, saved.sent, saved.kept) ?? r : r)));
  };
  const add = () => { setAdded(rows.length); setRows([...rows, ""]); };
  const remove = (i: number) => { typing.current = null; saveNow(rows.filter((_, j) => j !== i)); };
  const browse = async () => {
    try {
      const dir = await open({ directory: true, multiple: false, title: "Choose the folder your projects are in" });
      if (typeof dir !== "string") return;
      // into the empty row at the end, when there is one
      saveNow(rows.length > 0 && !rows[rows.length - 1].trim() ? [...rows.slice(0, -1), dir] : [...rows, dir]);
    } catch (e) { setErr(String(e)); }
  };

  return (
    <FormSection title="Editor folders" text="Where your projects are. Besides projects' linked folders and card worktrees, the Editor opens every folder directly inside these.">
      <Field label="Folders" wide error={err}
        hint="Never /, your home folder, Gizai's data folder or folders with keys (~/.ssh, ~/.gnupg, ~/.config, …). Saved as soon as you add, change or remove one.">
        <div className="folder-list">
          {rows.length === 0 && <span className="hint">No folders: the Editor opens projects' linked folders and card worktrees only.</span>}
          {rows.map((r, i) => {
            const t = r.trim();
            const root = savedAs(r, saved.sent, saved.kept);
            return (
              <div key={i} className="folder-row">
                <div className="input-group">
                  <input className="input mono" aria-label={`Folder ${i + 1}`} value={r} placeholder="~/Herd" autoFocus={added === i}
                    onFocus={() => { typing.current = i; }} onBlur={() => leave(i)} onChange={(e) => put(i, e.target.value)} />
                  <button type="button" className="btn ghost sm icon-only" aria-label={`Remove folder ${i + 1}`} title="Remove" onClick={() => remove(i)}><X className="icon" /></button>
                </div>
                {refused && t === refused.path ? <span className="error">{refused.why}</span>
                  : t && missing.includes(t) ? <span className="warn">{MISSING}</span>
                  : root && found ? <span className="hint">{rootSummary(root, found)}</span> : null}
              </div>
            );
          })}
          <div style={{ display: "flex", gap: 8 }}>
            <button type="button" className="btn sm" onClick={add}><Plus className="icon" />Add folder</button>
            <button type="button" className="btn sm ghost" onClick={browse}><FolderOpen className="icon" />Browse…</button>
          </div>
        </div>
      </Field>
    </FormSection>
  );
}
