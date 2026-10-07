// Files attached to a client, project or task. Drops arrive through Tauri's native drag-and-drop event
// (HTML5 drop never fires in the webview; see the spike); "Add files" uses the system file picker.
import { useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { Upload, X } from "lucide-react";
import { addFiles, listFiles, openFile, removeFile } from "../api";
import { useData } from "../lib/useData";
import { formatBytes, relTime } from "../lib/format";
import { dropHits } from "../lib/drop";
import type { FileOwner } from "../types";

// Every mounted drop zone. A drop that hits no zone goes to the only zone on screen, if there is one.
const zones = new Set<symbol>();

export function FileDrop({ ownerType, ownerId, emptyText }: { ownerType: FileOwner; ownerId: string; emptyText?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState(false);
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
  const addRef = useRef(add);
  addRef.current = add;

  useEffect(() => {
    const me = Symbol("filedrop");
    zones.add(me);
    let un: (() => void) | undefined;
    let alive = true;
    getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload;
      const el = ref.current;
      if (!el) return;
      if (p.type === "leave") { setHover(false); return; }
      const hit = dropHits(p.position, window.devicePixelRatio, el.getBoundingClientRect()) || zones.size === 1;
      if (p.type === "drop") { setHover(false); if (hit) addRef.current(p.paths); }
      else setHover(hit);
    }).then((f) => (alive ? (un = f) : f())).catch(() => {});
    return () => { alive = false; un?.(); zones.delete(me); };
  }, []);

  const choose = async () => {
    const sel = await open({ multiple: true, directory: false, title: "Add files" });
    if (Array.isArray(sel)) add(sel); else if (typeof sel === "string") add([sel]);
  };

  return (
    <div ref={ref} className={`filedrop${hover ? " hover" : ""}`}>
      {(files ?? []).length > 0 && (
        <ul className="files">
          {(files ?? []).map((f) => (
            <li key={f.id}>
              <button className="file" title={`Open ${f.name}`} onClick={() => openFile(f.id).catch((e) => setMsg(String(e)))}>
                <span className="ext">{(f.name.includes(".") ? f.name.split(".").pop()! : "file").slice(0, 4).toUpperCase()}</span>
                <span className="fname"><b>{f.name}</b><span>{formatBytes(f.sizeBytes)} · {relTime(f.createdAt)}</span></span>
              </button>
              {confirm === f.id
                ? <button className="btn sm danger" onClick={() => { setConfirm(null); removeFile(f.id).catch((e) => setMsg(String(e))); }}>Remove?</button>
                : <button className="btn ghost sm icon-only" aria-label={`Remove ${f.name}`} title="Remove" onClick={() => setConfirm(f.id)}><X className="icon" /></button>}
            </li>
          ))}
        </ul>
      )}
      <div className="filedrop-bar">
        <span className="faint">{busy ? "Adding…" : hover ? "Drop to add" : (files ?? []).length ? "Drop more files here, or" : emptyText ?? "Drop files here, or"}</span>
        <button className="btn sm" onClick={choose} disabled={busy}><Upload className="icon" />Add files</button>
      </div>
      {msg && <div style={{ color: "var(--warning)", fontSize: "var(--fs-sm)" }}>{msg}</div>}
    </div>
  );
}
