import { useEffect, useRef, useState } from "react";
import { docVersionBody, docVersions, getDoc, getProject, renameDoc, saveDoc } from "../api";
import { href } from "../router";
import { useData } from "../lib/useData";
import { relTime } from "../lib/format";
import { MarkdownEditor } from "../components/MarkdownEditor";
import { MarkdownView } from "../components/MarkdownView";
import { Drawer } from "../components/Drawer";

type Status = "saved" | "unsaved" | "saving" | "conflict";

/** A project doc: always-live editor, saved on Ctrl+S / Ctrl+Enter, on leaving the editor and on leaving the page.
 * Every save is a version; a save based on an old version (an agent saved meanwhile) asks what to do. */
export function DocPage({ id }: { id: string }) {
  const { data: doc, error } = useData(() => getDoc(id), [id]);
  const { data: versions } = useData(() => docVersions(id), [id]);
  const { data: project } = useData(() => (doc?.projectId ? getProject(doc.projectId) : Promise.resolve(null)), [doc?.projectId]);
  const [text, setText] = useState<string | null>(null);
  const [base, setBase] = useState<number | null>(null);
  const [status, setStatus] = useState<Status>("saved");
  const [title, setTitle] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [viewing, setViewing] = useState<{ version: number; body: string } | null>(null);
  const live = useRef({ text, base, status });
  live.current = { text, base, status };
  const saved = useRef(""); // the text of version `base`
  const inflight = useRef(false); // blur and the Save button can both fire: one save at a time

  const adopt = (body: string, version: number) => { saved.current = body; setText(body); setBase(version); setStatus("saved"); };
  // First load, and later versions saved by someone else (an agent) while we have nothing unsaved.
  useEffect(() => {
    if (!doc) return;
    setTitle((t) => t || doc.title);
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

  if (error) return <div className="error-banner">{error}</div>;
  if (!doc || text === null) return null;

  const openVersion = async (v: number) => { try { setViewing({ version: v, body: await docVersionBody(id, v) }); } catch (e) { setErr(String(e)); } };
  const saveTitle = () => { const t = title.trim(); if (!t) setTitle(doc.title); else if (t !== doc.title) renameDoc(id, t).catch((e) => setErr(String(e))); };
  const statusText = { saved: `Saved · version ${base}`, unsaved: "Unsaved changes · Ctrl+S saves", saving: "Saving…", conflict: "Not saved" }[status];

  return (
    <>
      <div className="topbar">
        <div className="crumbs">
          <a href={href({ page: "projects" })}>Projects</a><span className="sep">/</span>
          {project && <><a href={href({ page: "project", id: project.id })}>{project.name}</a><span className="sep">/</span></>}
          <b>{doc.title}</b>
        </div>
        <div className="actions"><span className={status === "saved" ? "faint" : "muted"} role="status">{statusText}</span>
          <button className="btn primary" disabled={status === "saved" || status === "saving"} onClick={() => save(text)}>Save</button></div>
      </div>
      {err && <div className="error-banner" role="alert">{err}</div>}
      {status === "conflict" && (
        <div className="error-banner" role="alert">
          This doc was saved by someone else after you opened it (version {doc.currentVersion}). Your text is still in the editor.
          <button className="btn" onClick={() => save(text, true)}>Save mine as a new version</button>
          <button className="btn ghost" onClick={() => adopt(doc.bodyMd, doc.currentVersion)}>Discard mine, load theirs</button>
        </div>
      )}
      <div className="split">
        <div className="doc">
          <div className="doc-in" style={{ maxWidth: 980 }}>
            <input className="title-input" aria-label="Doc title" value={title} onChange={(e) => setTitle(e.target.value)} onBlur={saveTitle}
              onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }} />
            <div className="doc-editor">
              <MarkdownEditor value={text} ariaLabel="Doc text" minHeight={460} hint="Ctrl+S saves"
                placeholder="Write in Markdown: # headings, **bold**, - [ ] checklists, tables, KADE-12 refs and @mentions."
                onChange={(md) => { setText(md); setStatus((s) => (s === "conflict" || s === "saving" ? s : md === saved.current ? "saved" : "unsaved")); }}
                onSave={(md) => save(md)} onBlur={(md) => { if (live.current.status === "unsaved") save(md); }} />
            </div>
          </div>
        </div>
        <aside className="props" aria-label="History">
          <div className="props-head">History</div>
          <div className="props-body">
          {(versions ?? []).map((v) => (
            <button key={v.version} className={`version${v.version === base ? " on" : ""}`} onClick={() => openVersion(v.version)}>
              <span className="mono">v{v.version}</span><span>{v.authorName ?? "Someone"}</span><span className="faint">{relTime(v.createdAt)}</span>
            </button>
          ))}
          </div>
        </aside>
      </div>
      {viewing && (
        <Drawer wide title={`${doc.title}, version ${viewing.version}`} subtitle="Restoring saves this text as a new version; nothing is lost." onClose={() => setViewing(null)}
          actions={<>
            <button className="btn ghost" onClick={() => setViewing(null)}>Close</button>
            <button className="btn primary" onClick={() => { const b = viewing.body; setViewing(null); setText(b); save(b, true); }}>Restore this version</button>
          </>}>
          {viewing.body.trim() ? <MarkdownView md={viewing.body} /> : <p className="faint">This version is empty.</p>}
        </Drawer>
      )}
    </>
  );
}
