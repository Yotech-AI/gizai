// Settings → Quit (GA-21): closing the window only hides Gizai, so this is where you quit it completely, with what
// quitting stops. It quits like the tray's Quit Gizai completely: agents at work are stopped first.
import { useEffect, useState } from "react";
import { Power } from "lucide-react";
import { exitApp } from "../api";
import { quitWarning } from "../lib/settings";
import { useLiveRuns } from "../lib/useLiveRuns";
import { useChatLive } from "./chat/useChat";
import { Field, FormSection } from "./Form";

/** Quit Gizai completely. The warning says what stops, and how many runs and chat answers when some are at work; then
 *  the button asks once more first. */
export function QuitSettings() {
  const runs = useLiveRuns();
  const chats = useChatLive();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const working = runs.length + chats.length;
  useEffect(() => {
    if (!asking) return;
    const t = setTimeout(() => setAsking(false), 8000);
    return () => clearTimeout(t);
  }, [asking]);
  const quit = () => {
    if (working > 0 && !asking) { setAsking(true); return; }
    setBusy(true); setErr(null);
    exitApp(0).catch((e) => { setErr(String(e)); setBusy(false); });
  };
  const label = busy ? (working > 0 ? "Stopping agents…" : "Quitting…") : asking ? "Stop them and quit?" : "Quit Gizai completely";
  return (
    <FormSection title="Quit" text="Closing the window only hides Gizai: it keeps running in the tray, and its agents keep working. Open Gizai in the tray, or starting Gizai again, brings the window back.">
      <Field label="Quit Gizai" wide error={err} warn={quitWarning(runs.length, chats.length)}>
        <div><button className="btn danger" onClick={quit} disabled={busy}><Power className="icon" /><span>{label}</span></button></div>
      </Field>
    </FormSection>
  );
}
