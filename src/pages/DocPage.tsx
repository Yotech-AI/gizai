import { Fragment, useEffect, useState } from "react";
import { docVersionBody, docVersions, getProject, renameDoc } from "../api";
import { href } from "../router";
import { useData } from "../lib/useData";
import { useDocEditor } from "../lib/useDocEditor";
import { relTime } from "../lib/format";
import { modKey } from "../lib/keys";
import { MarkdownEditor } from "../components/MarkdownEditor";
import { MarkdownView } from "../components/MarkdownView";
import { Drawer } from "../components/Drawer";

/** A project doc: always-live editor, saved on Ctrl+S / Ctrl+Enter, on leaving the editor and on leaving the page.
 * Every save is a version; a save based on an old version (an agent saved meanwhile) asks what to do (`useDocEditor`).
 * A memory note opens on the Memory page (GA-68). */
export function DocPage({ id }: { id: string }) {
  const { doc, error, text, base, status, err, setErr, adopt, save, edit, blur, replace } = useDocEditor(id);
  const { data: versions } = useData(() => docVersions(id), [id]);
  const { data: project } = useData(() => (doc?.projectId ? getProject(doc.projectId) : Promise.resolve(null)), [doc?.projectId]);
  const [title, setTitle] = useState("");
  const [viewing, setViewing] = useState<{ version: number; body: string } | null>(null);
  useEffect(() => { if (doc) setTitle((t) => t || doc.title); }, [doc]);
  // An old link to a memory note (#/doc/<id>) opens it on the Memory page, without a step back to here.
  useEffect(() => { if (doc?.kind === "memory") window.location.replace(href({ page: "memory", id })); }, [doc?.kind, id]);

  if (error) return <div className="error-banner">{error}</div>;
  if (!doc || text === null) return null;

  const openVersion = async (v: number) => { try { setViewing({ version: v, body: await docVersionBody(id, v) }); } catch (e) { setErr(String(e)); } };
  const saveTitle = () => { const t = title.trim(); if (!t) setTitle(doc.title); else if (t !== doc.title) renameDoc(id, t).catch((e) => setErr(String(e))); };
  const statusText = { saved: `Saved · version ${base}`, unsaved: `Unsaved changes · ${modKey()}+S saves`, saving: "Saving…", conflict: "Not saved" }[status];

  return (
    <>
      <div className="topbar">
        <div className="crumbs">
          {doc.kind === "memory" ? (
            // A memory note (GA-19): Memory, its folders, its title, until the Memory page (GA-68) takes it over.
            <>{["Memory", ...(doc.path ?? "").split("/").slice(0, -1)].map((f, i) => <Fragment key={i}><span>{f}</span><span className="sep">/</span></Fragment>)}</>
          ) : (
            <><a href={href({ page: "projects" })}>Projects</a><span className="sep">/</span>
              {project && <><a href={href({ page: "project", id: project.id })}>{project.name}</a><span className="sep">/</span></>}</>
          )}
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
              <MarkdownEditor value={text} ariaLabel="Doc text" minHeight={460} hint={`${modKey()}+S saves`}
                placeholder="Write in Markdown: # headings, **bold**, - [ ] checklists, tables, KADE-12 refs and @mentions."
                onChange={edit} onSave={(md) => save(md)} onBlur={blur} />
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
            <button className="btn primary" onClick={() => { const b = viewing.body; setViewing(null); replace(b, true); }}>Restore this version</button>
          </>}>
          {viewing.body.trim() ? <MarkdownView md={viewing.body} /> : <p className="faint">This version is empty.</p>}
        </Drawer>
      )}
    </>
  );
}
