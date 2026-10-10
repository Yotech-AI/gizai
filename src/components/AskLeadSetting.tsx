import { useEffect, useState } from "react";
import { askLeadEnabled, setAskLeadEnabled } from "../api";
import { Field } from "./Form";

/** Settings → Runs → Ask the Team Lead first (GA-70): an agent's question goes to the Team Lead before you, on unless
 * switched off. Saved at once. */
export function AskLeadSetting({ say }: { say: (ok: boolean, text: string) => void }) {
  const [on, setOn] = useState<boolean | null>(null);
  useEffect(() => { askLeadEnabled().then(setOn).catch(() => setOn(true)); }, []);
  const flip = (next: boolean) => {
    setOn(next);
    setAskLeadEnabled(next).catch((e) => { setOn(!next); say(false, String(e)); });
  };
  return (
    <Field label="Questions" wide hint={on === false
      ? "Off: an agent that needs a decision puts its card in your Inbox at once."
      : "An agent that needs a decision asks the Team Lead first: it answers from memory and the card, and the agent carries on, or it asks you in the Inbox with the options and its advice. Money, scope, deadlines, client messages, security and deleting always come to you."}>
      <label className="check"><input type="checkbox" checked={on !== false} disabled={on === null} onChange={(e) => flip(e.target.checked)} />Ask the Team Lead first</label>
    </Field>
  );
}
