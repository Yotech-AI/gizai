import { useState } from "react";
import { addUser } from "../api";
import { Drawer } from "./Drawer";
import { Field, FormSection } from "./Form";

export function PersonDrawer({ onClose }: { onClose: () => void }) {
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const save = async () => {
    try { await addUser(name, email.trim() || null); onClose(); } catch (e) { setErr(String(e)); }
  };
  return (
    <Drawer title="Add person" subtitle="People can be assigned tasks and mentioned. Agents are set up on the Team page." onClose={onClose}
      dirty={!!(name || email)} error={err} hint="Enter adds"
      actions={<><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={!name.trim()} onClick={save}>Add person</button></>}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); save(); }}>
        <FormSection title="Person" text="Their name becomes their @handle.">
          <Field label="Name" htmlFor="u-name"><input id="u-name" className="input" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Sanne Bakker" /></Field>
          <Field label="Email" htmlFor="u-email"><input id="u-email" className="input" type="email" value={email} onChange={(e) => setEmail(e.target.value)} /></Field>
        </FormSection>
        <button type="submit" hidden />
      </form>
    </Drawer>
  );
}
