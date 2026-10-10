import { useEffect, useState } from "react";
import { getClient, saveClient } from "../api";
import { go } from "../router";
import type { Client, ClientInput } from "../types";
import { modKey } from "../lib/keys";
import { isIban, isKvk, isVatNumber } from "../lib/validate";
import { Drawer } from "./Drawer";
import { Field, FormSection } from "./Form";
import { MarkdownEditor } from "./MarkdownEditor";

export function toInput(c?: Client | null): ClientInput {
  return {
    name: c?.name ?? "", legalName: c?.legalName ?? "", kind: c?.kind ?? "company", vatNumber: c?.vatNumber ?? "",
    cocNumber: c?.cocNumber ?? "", iban: c?.iban ?? "", email: c?.email ?? "", phone: c?.phone ?? "", website: c?.website ?? "",
    street: c?.street ?? "", postalCode: c?.postalCode ?? "", city: c?.city ?? "", country: c?.country ?? "NL",
    paymentTermsDays: c?.paymentTermsDays ?? 30, status: c?.status ?? "active", notesMd: c?.notesMd ?? "",
  };
}

const warn = (ok: (s: string) => boolean, v?: string | null, msg = "Doesn't look valid") => (v && v.trim() && !ok(v) ? msg : null);

/** New client (no id) or edit an existing one, in a drawer. */
export function ClientDrawer({ id, onClose }: { id?: string; onClose: () => void }) {
  const [initial, setInitial] = useState<ClientInput | null>(id ? null : toInput(null));
  const [v, setV] = useState<ClientInput | null>(initial);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => { if (id) getClient(id).then((c) => { const i = toInput(c); setInitial(i); setV(i); }).catch((e) => setErr(String(e))); }, [id]);
  if (!v || !initial) return null;
  const set = (k: keyof ClientInput) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setV({ ...v, [k]: k === "paymentTermsDays" ? (e.target.value === "" ? null : Number(e.target.value)) : e.target.value });
  const save = async () => {
    setBusy(true);
    try {
      const saved = await saveClient(id ?? null, v);
      onClose();
      if (!id) go({ page: "client", id: saved });
    } catch (e) { setErr(String(e)); setBusy(false); }
  };
  return (
    <Drawer title={id ? `Edit ${initial.name}` : "New client"} subtitle="Clients own projects. Dutch registration details are checked as you type."
      onClose={onClose} dirty={JSON.stringify(v) !== JSON.stringify(initial)} error={err} hint={`${modKey()}+Enter saves`}
      actions={<><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={busy || !v.name.trim()} onClick={save}>{id ? "Save changes" : "Create client"}</button></>}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); save(); }} onKeyDown={(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(); } }}>
        <FormSection title="Company" text="The name you use day to day, and its legal form.">
          <Field label="Name" htmlFor="c-name"><input id="c-name" className="input" autoFocus value={v.name} onChange={set("name")} placeholder="Kade Logistics B.V." /></Field>
          <Field label="Legal name" htmlFor="c-legal"><input id="c-legal" className="input" value={v.legalName ?? ""} onChange={set("legalName")} /></Field>
          <Field label="Type" htmlFor="c-kind"><select id="c-kind" className="select" value={v.kind ?? "company"} onChange={set("kind")}><option value="company">Company</option><option value="person">Person</option></select></Field>
          <Field label="Status" htmlFor="c-status"><select id="c-status" className="select" value={v.status ?? "active"} onChange={set("status")}><option value="lead">Lead</option><option value="active">Active</option><option value="inactive">Inactive</option></select></Field>
        </FormSection>
        <FormSection title="Registration" text="KvK, BTW and IBAN for invoices.">
          <Field label="KvK-nummer" htmlFor="c-kvk" warn={warn(isKvk, v.cocNumber, "KvK numbers have 8 digits")}><input id="c-kvk" className="input mono" value={v.cocNumber ?? ""} onChange={set("cocNumber")} /></Field>
          <Field label="BTW-nummer" htmlFor="c-btw" warn={warn(isVatNumber, v.vatNumber, "Expected like NL123456789B01")}><input id="c-btw" className="input mono" value={v.vatNumber ?? ""} onChange={set("vatNumber")} /></Field>
          <Field label="IBAN" htmlFor="c-iban" warn={warn(isIban, v.iban, "The IBAN checksum doesn't match")}><input id="c-iban" className="input mono" value={v.iban ?? ""} onChange={set("iban")} /></Field>
          <Field label="Payment terms (days)" htmlFor="c-terms"><input id="c-terms" className="input" type="number" min={0} value={v.paymentTermsDays ?? ""} onChange={set("paymentTermsDays")} /></Field>
        </FormSection>
        <FormSection title="Contact" text="How to reach the company itself; people go under Contacts.">
          <Field label="Email" htmlFor="c-email"><input id="c-email" className="input" type="email" value={v.email ?? ""} onChange={set("email")} /></Field>
          <Field label="Phone" htmlFor="c-phone"><input id="c-phone" className="input" value={v.phone ?? ""} onChange={set("phone")} /></Field>
          <Field label="Website" htmlFor="c-web" wide><input id="c-web" className="input" value={v.website ?? ""} onChange={set("website")} placeholder="https://" /></Field>
        </FormSection>
        <FormSection title="Address">
          <Field label="Street" htmlFor="c-street" wide><input id="c-street" className="input" value={v.street ?? ""} onChange={set("street")} /></Field>
          <Field label="Postal code" htmlFor="c-pc"><input id="c-pc" className="input" value={v.postalCode ?? ""} onChange={set("postalCode")} /></Field>
          <Field label="City" htmlFor="c-city"><input id="c-city" className="input" value={v.city ?? ""} onChange={set("city")} /></Field>
          <Field label="Country" htmlFor="c-country" hint="Two letters, like NL"><input id="c-country" className="input" maxLength={2} value={v.country ?? ""} onChange={set("country")} /></Field>
        </FormSection>
        <FormSection title="Notes" text="Anything worth remembering. Markdown.">
          <Field label="Notes" wide><MarkdownEditor value={v.notesMd ?? ""} onChange={(md) => setV((x) => (x ? { ...x, notesMd: md } : x))} ariaLabel="Notes" minHeight={120} /></Field>
        </FormSection>
      </form>
    </Drawer>
  );
}
