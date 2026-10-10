import { useEffect, useState } from "react";
import { memoryEnabled, setMemoryEnabled } from "../api";
import { Field } from "./Form";

/** Settings → Runs → Memory (GA-19): memory for every agent's runs and the Team Lead's chat and board checks, on unless
 * switched off. Saved at once; each agent's form has its own switch too. */
export function MemorySetting({ say }: { say: (ok: boolean, text: string) => void }) {
  const [on, setOn] = useState<boolean | null>(null);
  useEffect(() => { memoryEnabled().then(setOn).catch(() => setOn(true)); }, []);
  const flip = (next: boolean) => {
    setOn(next);
    setMemoryEnabled(next).catch((e) => { setOn(!next); say(false, String(e)); });
  };
  return (
    <Field label="Memory" wide hint={on === false
      ? "Off for every agent: runs, board checks and chat get no notes, and what runs learn is not kept. The Team Lead's memory tools still work."
      : "Runs get their agent's notes and the team's notes on the card's project, client and role, and the Team Lead's chat gets its notes. An agent's form can turn it off for that agent."}>
      <label className="check"><input type="checkbox" checked={on !== false} disabled={on === null} onChange={(e) => flip(e.target.checked)} />Use memory</label>
    </Field>
  );
}
