import { useState } from "react";
import { Archive, Pencil, Plus } from "lucide-react";
import { archiveClient, getClient, listContacts, listProjects, saveContact } from "../api";
import { go, href } from "../router";
import { useData } from "../lib/useData";
import { companyInitials } from "../lib/format";
import { useDrawer } from "../lib/drawers";
import type { Contact } from "../types";
import { Avatar } from "../components/Avatar";
import { Drawer } from "../components/Drawer";
import { Field, FormSection } from "../components/Form";
import { MarkdownView } from "../components/MarkdownView";

/** Add a contact (no `initial`) or edit one. */
function ContactDrawer({ clientId, first, initial, onClose }: { clientId: string; first: boolean; initial?: Contact; onClose: () => void }) {
  const start = initial ?? { id: "", clientId, name: "", isPrimary: first };
  const [c, setC] = useState<Contact>(start);
  const [err, setErr] = useState<string | null>(null);
  const save = async () => { try { await saveContact(c); onClose(); } catch (e) { setErr(String(e)); } };
  return (
    <Drawer title={initial ? `Edit ${initial.name}` : "Add contact"} subtitle="A person at this client. One contact is the primary one." onClose={onClose}
      dirty={JSON.stringify(c) !== JSON.stringify(start)} error={err}
      actions={<><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={!c.name.trim()} onClick={save}>{initial ? "Save changes" : "Add contact"}</button></>}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); save(); }}>
        <FormSection title="Contact">
          <Field label="Name" htmlFor="k-name"><input id="k-name" className="input" autoFocus value={c.name} onChange={(e) => setC({ ...c, name: e.target.value })} /></Field>
          <Field label="Role" htmlFor="k-role"><input id="k-role" className="input" value={c.role ?? ""} onChange={(e) => setC({ ...c, role: e.target.value })} placeholder="Head of finance" /></Field>
          <Field label="Email" htmlFor="k-email"><input id="k-email" className="input" type="email" value={c.email ?? ""} onChange={(e) => setC({ ...c, email: e.target.value })} /></Field>
          <Field label="Phone" htmlFor="k-phone"><input id="k-phone" className="input" value={c.phone ?? ""} onChange={(e) => setC({ ...c, phone: e.target.value })} /></Field>
          <Field label="Primary" wide><label className="check"><input type="checkbox" checked={c.isPrimary} onChange={(e) => setC({ ...c, isPrimary: e.target.checked })} />The main contact for this client</label></Field>
        </FormSection>
        <button type="submit" hidden />
      </form>
    </Drawer>
  );
}

export function ClientPage({ id }: { id: string }) {
  const client = useData(() => getClient(id), [id]);
  const contacts = useData(() => listContacts(id), [id]);
  const projects = useData(() => listProjects(), []);
  const open = useDrawer();
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<Contact | null>(null);
  const [confirmArchive, setConfirmArchive] = useState(false);
  if (client.error) return <div className="error-banner">{client.error}</div>;
  if (!client.data) return null;
  const c = client.data;
  const mine = (projects.data ?? []).filter((p) => p.clientId === id);
  const kv = (k: string, v?: string | number | null, mono = false) => <><span className="k">{k}</span><span className={mono ? "mono" : ""}>{v ?? <span className="faint">—</span>}</span></>;
  return (
    <>
      <div className="topbar"><div className="crumbs"><a href={href({ page: "clients" })}>Clients</a><span className="sep">/</span><b>{c.name}</b></div></div>
      <div className="content">
        <div className="page">
          <div className="entity-head">
            <span className="avatar xl" style={{ borderRadius: "var(--radius-l)" }}>{companyInitials(c.name)}</span>
            <div className="names"><h1>{c.name}</h1><p>{[c.city, c.country].filter(Boolean).join(", ")} · {c.status} · {c.openTasks} open tasks</p></div>
            <div className="actions">
              {confirmArchive
                ? <><span className="muted">Archive {c.name}?</span><button className="btn danger" onClick={async () => { await archiveClient(id); go({ page: "clients" }); }}>Archive</button><button className="btn ghost" onClick={() => setConfirmArchive(false)}>Keep</button></>
                : <button className="btn ghost" onClick={() => setConfirmArchive(true)}><Archive className="icon" />Archive</button>}
              <button className="btn" onClick={() => open({ kind: "client", id })}><Pencil className="icon" />Edit</button>
            </div>
          </div>
          <div style={{ display: "grid", gridTemplateColumns: "minmax(0, 1.2fr) minmax(320px, 1fr)", gap: 20, alignItems: "start" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
              <section><div className="section-head"><h3>Details</h3></div>
                <div className="panel kv">{kv("Legal name", c.legalName)}{kv("KvK-nummer", c.cocNumber, true)}{kv("BTW-nummer", c.vatNumber, true)}{kv("IBAN", c.iban, true)}
                  {kv("Email", c.email)}{kv("Phone", c.phone)}{kv("Website", c.website)}{kv("Address", [c.street, [c.postalCode, c.city].filter(Boolean).join(" ")].filter(Boolean).join(", ") || null)}
                  {kv("Payment terms", c.paymentTermsDays != null ? `${c.paymentTermsDays} days` : null)}</div></section>
              {c.notesMd?.trim() && <section><div className="section-head"><h3>Notes</h3></div><div className="panel" style={{ padding: "14px 16px" }}><MarkdownView md={c.notesMd} /></div></section>}
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
              <section><div className="section-head"><h3>Contacts</h3><button className="link" onClick={() => setAdding(true)}><Plus className="icon sm" />Add contact</button></div>
                <div className="panel">
                  {(contacts.data ?? []).length === 0 && <div className="panel-row faint">No contacts yet.</div>}
                  {(contacts.data ?? []).map((k) => (
                    <button key={k.id} type="button" className="panel-row contact-row" aria-label={`Edit ${k.name}`} onClick={() => setEditing(k)}>
                      <Avatar name={k.name} /><span className="grow">{k.name}{k.isPrimary && <span className="badge" style={{ marginLeft: 8 }}>primary</span>} <span className="faint">{k.role ?? ""}</span></span>
                      <span className="faint">{[k.email, k.phone].filter(Boolean).join(" · ")}</span><Pencil className="icon sm edit-hint" /></button>
                  ))}
                </div></section>
              <section><div className="section-head"><h3>Projects</h3><button className="link" onClick={() => open({ kind: "project" })}><Plus className="icon sm" />New project</button></div>
                <div className="panel">
                  {mine.length === 0 && <div className="panel-row faint">No projects for this client yet.</div>}
                  {mine.map((p) => (
                    <a key={p.id} className="panel-row" href={href({ page: "project", id: p.id })}>
                      <span className="dot" style={{ width: 9, height: 9, borderRadius: "50%", background: p.color ?? "var(--text-3)" }} /><span className="id">{p.number}</span><span className="grow">{p.name}</span>
                      <span className="faint">{p.openTasks} open · {p.doneTasks} done</span>
                    </a>
                  ))}
                </div></section>
            </div>
          </div>
        </div>
      </div>
      {adding && <ContactDrawer clientId={id} first={(contacts.data ?? []).length === 0} onClose={() => setAdding(false)} />}
      {editing && <ContactDrawer key={editing.id} clientId={id} first={false} initial={editing} onClose={() => setEditing(null)} />}
    </>
  );
}
