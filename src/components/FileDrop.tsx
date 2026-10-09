// Files attached to a client, project or task, and files picked for a task that doesn't exist yet. Drops arrive through
// lib/useDropZone (Tauri's native drag-and-drop event), which gives each drop to one zone; "Add files" uses the system file picker.
import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Upload, X } from "lucide-react";
import { addFiles, listFiles, openFile, removeFile } from "../api";
import { useData } from "../lib/useData";
import { useDropZone } from "../lib/useDropZone";
import { fileExt as ext, fileFolder, fileName } from "../lib/files";
import { formatBytes, relTime } from "../lib/format";
import type { FileOwner } from "../types";

/** The system file picker, several files at once; [] when nothing was picked. */
export async function pickFiles(): Promise<string[]> {
  const sel = await open({ multiple: true, directory: false, title: "Add files" });
  return Array.isArray(sel) ? sel : typeof sel === "string" ? [sel] : [];
}

/** `readOnly` (an archived card): the files open, but none are added or removed, and drops go elsewhere. */
export function FileDrop({ ownerType, ownerId, emptyText, readOnly }: { ownerType: FileOwner; ownerId: string; emptyText?: string; readOnly?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const { data: files } = useData(() => listFiles(ownerType, ownerId), [ownerType, ownerId]);

  const add = async (paths: string[]) => {
    if (paths.length === 0) return;
    setBusy(true); setMsg(null);
    try {
      const r = await addFiles(ownerType, ownerId, paths);
      if (r.failed.length) setMsg(`Not added: ${r.failed.join("; ")}`);
    } catch (e) { setMsg(String(e)); }
    finally { setBusy(false); }
  };
  const hover = useDropZone(ref, add, { on: !readOnly });
  const choose = async () => add(await pickFiles());

  return (
    <div ref={ref} className={`filedrop${hover ? " hover" : ""}${readOnly ? " read-only" : ""}`}>
      {(files ?? []).length > 0 && (
        <ul className="files">
          {(files ?? []).map((f) => (
            <li key={f.id}>
              <button className="file" title={`Open ${f.name}`} onClick={() => openFile(f.id).catch((e) => setMsg(String(e)))}>
                <span className="ext">{ext(f.name)}</span>
                <span className="fname"><b>{f.name}</b><span>{formatBytes(f.sizeBytes)} · {relTime(f.createdAt)}</span></span>
              </button>
              {readOnly ? null : confirm === f.id
                ? <button className="btn sm danger" onClick={() => { setConfirm(null); removeFile(f.id).catch((e) => setMsg(String(e))); }}>Remove?</button>
                : <button className="btn ghost sm icon-only" aria-label={`Remove ${f.name}`} title="Remove" onClick={() => setConfirm(f.id)}><X className="icon" /></button>}
            </li>
          ))}
        </ul>
      )}
      {readOnly ? (files && files.length === 0 && <span className="faint">No attachments.</span>) : (
        <div className="filedrop-bar">
          <span className="faint">{busy ? "Adding…" : hover ? "Drop to add" : (files ?? []).length ? "Drop more files here, or" : emptyText ?? "Drop files here, or"}</span>
          <button className="btn sm" onClick={choose} disabled={busy}><Upload className="icon" />Add files</button>
        </div>
      )}
      {msg && <div style={{ color: "var(--warning)", fontSize: "var(--fs-sm)" }}>{msg}</div>}
    </div>
  );
}

/** Picked paths, each with its type badge, name, folder and an X that takes it off the list (the New task drawer's Files,
 *  and the chips above the chat's text box with `compact`). Nothing when there are none. */
export function PendingFileList({ paths, onRemove, disabled, compact }: {
  paths: string[]; onRemove: (path: string) => void; disabled?: boolean; compact?: boolean;
}) {
  if (paths.length === 0) return null;
  return (
    <ul className={`files${compact ? " compact" : ""}`} aria-label="Files to add">
      {paths.map((p) => {
        const name = fileName(p);
        return (
          <li key={p}>
            <span className="file pending" title={p}>
              <span className="ext">{ext(name)}</span>
              <span className="fname"><b>{name}</b><span>{fileFolder(p)}</span></span>
            </span>
            <button type="button" className="btn ghost sm icon-only" aria-label={`Remove ${name}`} title="Remove" disabled={disabled} onClick={() => onRemove(p)}>
              <X className="icon" /></button>
          </li>
        );
      })}
    </ul>
  );
}

/** Files picked for a task that doesn't exist yet (the New task drawer): only their paths, nothing is copied until the task
 *  is created. It sits in a drawer, so every drop goes here while it is open. `disabled` (the task is being created, or
 *  exists): drops still come here, so none reach the page behind, but they change nothing. `adding`: the files are being added. */
export function PendingFiles({ paths, onAdd, onRemove, adding, disabled }: {
  paths: string[]; onAdd: (paths: string[]) => void; onRemove: (path: string) => void; adding?: boolean; disabled?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const over = useDropZone(ref, (dropped) => { if (!disabled) onAdd(dropped); }, { modal: true });
  const hover = over && !disabled;
  // Files dropped anywhere land here: bring the list into view, so the drop shows.
  useEffect(() => { if (hover) ref.current?.scrollIntoView({ block: "nearest" }); }, [hover]);
  const choose = async () => {
    const picked = await pickFiles();
    if (picked.length) onAdd(picked);
  };

  return (
    <div ref={ref} className={`filedrop${hover ? " hover" : ""}`}>
      <PendingFileList paths={paths} onRemove={onRemove} disabled={disabled} />
      <div className="filedrop-bar">
        <span className="faint">{adding ? "Adding…" : hover ? "Drop to add" : paths.length ? "Drop more files here, or" : "Drop files here, or"}</span>
        <button type="button" className="btn sm" onClick={choose} disabled={disabled}><Upload className="icon" />Add files</button>
      </div>
    </div>
  );
}
