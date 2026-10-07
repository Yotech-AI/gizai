import { useState } from "react";
import { createDoc, saveDoc } from "../api";
import { go } from "../router";
import { Drawer } from "./Drawer";
import { Field, FormSection } from "./Form";
import { MarkdownEditor } from "./MarkdownEditor";

/** A new project doc: its title and, optionally, a first version of its text. */
export function DocDrawer({ projectId, onClose }: { projectId: string; onClose: () => void }) {
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const create = async () => {
    try {
      const id = await createDoc(projectId, title);
      if (body.trim()) await saveDoc(id, body, 1);
      onClose();
      go({ page: "doc", id });
    } catch (e) { setErr(String(e)); }
  };
  return (
    <Drawer title="New doc" subtitle="Requirements, meeting notes, decisions. Every save keeps a version; agents can read them." onClose={onClose}
      dirty={!!(title || body)} error={err} hint="Ctrl+Enter creates"
      actions={<><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={!title.trim()} onClick={create}>Create doc</button></>}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); create(); }} onKeyDown={(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); create(); } }}>
        <FormSection title="Doc">
          <Field label="Title" htmlFor="d-title" wide><input id="d-title" className="input" autoFocus value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Requirements" /></Field>
        </FormSection>
        <FormSection title="Text" text="Optional: start writing now, or open the doc and write there.">
          <Field label="Text" wide><MarkdownEditor value={body} onChange={setBody} ariaLabel="Doc text" minHeight={220} /></Field>
        </FormSection>
      </form>
    </Drawer>
  );
}
