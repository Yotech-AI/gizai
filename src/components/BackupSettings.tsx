// Settings → General → Backups (GA-217, on GA-216's contract BK): the folder (Browse… or Turn off), When, Keep, Back up now
// with the step of the snapshot that runs, the last and next backup, and the snapshots in the folder. Each change saves at
// once, as the Editor's folders do; a refusal shows its reason under its field and nothing else is saved. `backups-changed`
// keeps it up to date while a snapshot runs, ends or fails. Restoring comes with GA-219.
import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { CircleX, DatabaseBackup, FolderOpen, LoaderCircle } from "lucide-react";
import { backUpNow, backupStatus, listBackups, onBackupsChanged, saveBackupSettings } from "../api";
import { backupWhen, canBackUp, entryProblem, entrySize, EVERY, fromText, keepOf, KEEP_MAX, KEEP_MIN, lastErrorText, lastText, nextText,
  NO_FOLDER, reasonText, stepText } from "../lib/backups";
import { fullWhen } from "../lib/limits";
import type { BackupEntry, BackupSettings as Settings, BackupStatus } from "../types";
import { Field, FormSection } from "./Form";

/** How long Keep waits after you type before it is saved. */
const TYPING_MS = 600;

/** The fields a refusal shows under: Folder, When (also On quit), Keep, and Back up now's. */
type Errs = Partial<Record<"folder" | "when" | "keep" | "now", string | null>>;

export function BackupSettings() {
  const [s, setS] = useState<BackupStatus | null>(null);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [list, setList] = useState<BackupEntry[] | null>(null);
  const [listErr, setListErr] = useState<string | null>(null);
  const [errs, setErrs] = useState<Errs>({});
  // Keep as typed, until it is saved (null: the saved value shows)
  const [keepText, setKeepText] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const alive = useRef(true);
  // The status as last seen, for the next save to build on, and how many `backups-changed` came: an answer that comes after
  // one of them may be older than it (Back up now's snapshot can end before its answer arrives), so the event's wins.
  const cur = useRef<BackupStatus | null>(null);
  const events = useRef(0);
  const queue = useRef(Promise.resolve());
  const keepSent = useRef<string | null>(null);

  const take = (x: BackupStatus | null | undefined, eventsBefore: number) => {
    if (!alive.current || !x || events.current !== eventsBefore) return;
    cur.current = x;
    setS(x);
  };
  const setErr = (k: keyof Errs, v: string | null) => setErrs((e) => (e[k] === v ? e : { ...e, [k]: v }));

  useEffect(() => {
    alive.current = true;
    let un: (() => void) | undefined;
    onBackupsChanged((x) => {
      events.current++;
      if (!alive.current || !x) return;
      cur.current = x;
      setS(x);
    }).then((f) => (alive.current ? (un = f) : f())).catch(() => {});
    const before = events.current;
    backupStatus().then((x) => { take(x, before); setLoadErr(null); }).catch((e) => { if (alive.current) setLoadErr(String(e)); });
    return () => { alive.current = false; un?.(); };
  }, []);

  // The snapshots, read again when the folder changes, a snapshot starts or ends, or one fails.
  const listKey = s ? JSON.stringify([s.folder, !!s.running, s.last?.name ?? null, s.lastError?.at ?? null]) : null;
  useEffect(() => {
    if (listKey === null) return;
    let live = true;
    listBackups(null).then((l) => { if (live) { setList(l ?? []); setListErr(null); } }).catch((e) => { if (live) setListErr(String(e)); });
    return () => { live = false; };
  }, [listKey]);

  /** Saves the settings as they are now with `change`, one save after the other. A refusal shows under `field`, and the
   *  settings stay as they were. */
  const save = (field: keyof Errs, change: Partial<Settings>, then?: (ok: boolean) => void) => {
    queue.current = queue.current.then(async () => {
      const base = cur.current?.settings;
      if (!base || !alive.current) return;
      const before = events.current;
      try {
        take(await saveBackupSettings({ ...base, ...change }), before);
        if (alive.current) { setErr(field, null); then?.(true); }
      } catch (e) {
        if (alive.current) { setErr(field, String(e)); then?.(false); }
      }
    });
  };

  /** Saves Keep as typed, unless it was sent already: an empty field saves nothing. */
  const saveKeep = (text: string | null, leaving = false) => {
    if (text === null) return;
    const k = keepOf(text);
    if (k === null) {
      if (leaving) { setKeepText(null); setErr("keep", null); keepSent.current = null; }
      return;
    }
    if ("error" in k) { setErr("keep", k.error); return; }
    if (text === keepSent.current) return;
    // back to the saved value: nothing to save
    if (k.keep === cur.current?.settings.keep) { setKeepText(null); setErr("keep", null); keepSent.current = null; return; }
    keepSent.current = text;
    save("keep", { keep: k.keep }, (ok) => {
      if (ok) setKeepText((t) => (t === text ? null : t));
      keepSent.current = ok ? null : text;
    });
  };
  // Typing saves once you pause.
  useEffect(() => {
    if (keepText === null) return;
    const t = setTimeout(() => saveKeep(keepText), TYPING_MS);
    return () => clearTimeout(t);
  }, [keepText]);

  if (!s) {
    return loadErr ? (
      <FormSection title="Backups"><Field label="Backups" wide error={loadErr}><span className="faint">The backups can't be shown now.</span></Field></FormSection>
    ) : null;
  }

  const now = Date.now();
  const browse = async () => {
    try {
      const dir = await open({ directory: true, multiple: false, title: "Choose a folder for backups", defaultPath: s.settings.folder ?? undefined });
      if (typeof dir === "string") save("folder", { folder: dir });
    } catch (e) { setErr("folder", String(e)); }
  };
  const backUp = () => {
    setErr("now", null);
    setStarting(true);
    const before = events.current;
    backUpNow().then((x) => take(x, before)).catch((e) => { if (alive.current) setErr("now", String(e)); })
      .finally(() => { if (alive.current) setStarting(false); });
  };
  const next = nextText(s, now);

  return (
    <FormSection title="Backups" text="Copies of your data in a folder you choose: a USB disk, or a folder that Dropbox or Syncthing keeps. A backup holds the database and your files, never keys or passwords: those stay in your system's keychain.">
      <Field label="Folder" wide error={errs.folder} warn={s.folderProblem}>
        {s.folder ? <span className="mono bk-folder">{s.folder}</span> : <span className="faint">{NO_FOLDER}</span>}
        <div className="bk-actions">
          <button type="button" className="btn sm" onClick={browse}><FolderOpen className="icon" />Browse…</button>
          <button type="button" className="btn sm ghost" disabled={!s.settings.folder} onClick={() => save("folder", { folder: null })}>Turn off</button>
        </div>
      </Field>
      <Field label="When" error={errs.when} hint="Quitting then waits for the backup, usually a few seconds.">
        <div className="radios" role="radiogroup" aria-label="When">
          {EVERY.map(([value, label]) => (
            <label key={value}><input type="radio" name="backup-every" checked={s.settings.every === value}
              onChange={() => save("when", { every: value })} />{label}</label>
          ))}
        </div>
        <label className="check"><input type="checkbox" checked={s.settings.onQuit} onChange={(e) => save("when", { onQuit: e.target.checked })} />Also when I quit Gizai completely</label>
      </Field>
      <Field label="Keep" htmlFor="bk-keep" error={errs.keep} hint="Your newest backups from this computer. Older ones are removed. A backup made before a restore is never removed.">
        <div className="input-group">
          <input id="bk-keep" className="input" type="number" min={KEEP_MIN} max={KEEP_MAX} step={1} value={keepText ?? String(s.settings.keep)}
            onChange={(e) => setKeepText(e.target.value)} onBlur={() => saveKeep(keepText, true)}
            onKeyDown={(e) => { if (e.key === "Enter") saveKeep(keepText); }} />
          <span className="faint">backups</span>
        </div>
      </Field>
      <Field label="Last and next" wide error={errs.now}>
        <span>{lastText(s, now)}</span>
        {next && <span className="muted">{next}</span>}
        {s.lastError && <div className="gh-line failed" role="alert"><CircleX className="icon" /><span className="bk-error">{lastErrorText(s.lastError, now)}</span></div>}
        <div className="bk-actions">
          <button type="button" className="btn" onClick={backUp} disabled={!canBackUp(s) || starting}><DatabaseBackup className="icon" />Back up now</button>
          {s.running && <span className="gh-line bk-step" role="status"><LoaderCircle className="icon spin" /><span>{stepText(s.running.step)}</span></span>}
        </div>
      </Field>
      <Field label="Backups" wide hint={list && list.length > 0 ? "Newest first, from every computer that backs up to this folder." : undefined}>
        {!s.settings.folder ? <span className="faint">Choose a folder to see its backups.</span>
          // a folder that can't be found can't be listed either: its warning under Folder says why
          : listErr ? (s.folderProblem ? <span className="faint">They show once the folder can be used again.</span> : <span className="warn">{listErr}</span>)
          : !list ? <span className="faint">Reading the folder…</span>
          : list.length === 0 ? <span className="faint">No backups in this folder yet.</span>
          : (
            <ul className="backup-list" aria-label="Backups in this folder">
              {list.map((e) => {
                const from = fromText(e);
                const problem = entryProblem(e);
                return (
                  <li key={e.name}>
                    <div className="bk-row">
                      <span className="bk-when" title={fullWhen(e.madeAt)}>{backupWhen(e.madeAt, now)}</span>
                      <span className="bk-what">{reasonText(e.reason)}{from && <span className="faint"> · {from}</span>}</span>
                      <span className="bk-size mono">{entrySize(e)}</span>
                    </div>
                    {problem && <span className="warn">{problem}</span>}
                  </li>
                );
              })}
            </ul>
          )}
      </Field>
    </FormSection>
  );
}
