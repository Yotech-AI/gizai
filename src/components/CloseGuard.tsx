// Closing Gizai with unsaved text in the doc page's or the Memory page's editor (GA-122). Gizai hears from here whether
// there is some; only then does closing the window (its close button) or quitting (Quit Gizai completely, a signal)
// wait for the page. The text saves as one version (at most CLOSE_SAVE_MS), then Gizai closes; when that fails or takes
// longer, this asks first. Stay keeps the window open, with the text and its "Unsaved changes" status.
import { useEffect, useRef, useState } from "react";
import { closeAnswer, onCloseRequested, onQuitRequested, setUnsavedEdits, type CloseAnswer } from "../api";
import { closeQuestion, onUnsavedChange, saveBeforeClose } from "../lib/closeSave";

export function CloseGuard() {
  const [asking, setAsking] = useState<string | null>(null);
  const busy = useRef(false); // saving or asking already: a second request waits for that answer
  const stay = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const offUnsaved = onUnsavedChange((unsaved) => { setUnsavedEdits(unsaved).catch(() => {}); });
    const closing = async () => {
      if (busy.current) return;
      busy.current = true;
      const r = await saveBeforeClose();
      if (r.saved) { busy.current = false; closeAnswer("close").catch(() => {}); return; }
      setAsking(r.title);
      closeAnswer("asking").catch(() => {});
    };
    const off = [onCloseRequested(() => { void closing(); }), onQuitRequested(() => { void closing(); })];
    return () => { offUnsaved(); for (const p of off) p.then((f) => f()).catch(() => {}); };
  }, []);
  useEffect(() => { if (asking !== null) stay.current?.focus(); }, [asking]);

  if (asking === null) return null;
  const answer = (a: CloseAnswer) => { setAsking(null); busy.current = false; closeAnswer(a).catch(() => {}); };
  return (
    <div className="ask-scrim">
      <div className="ask" role="alertdialog" aria-modal="true" aria-labelledby="close-ask-q" aria-describedby="close-ask-why"
        onKeyDown={(e) => { if (e.key === "Escape") { e.preventDefault(); answer("stay"); } }}>
        <h2 id="close-ask-q">{closeQuestion(asking)}</h2>
        <p id="close-ask-why">Saving them failed or took too long. Stay keeps them in the editor, unsaved.</p>
        <div className="ask-actions">
          <button className="btn danger" onClick={() => answer("close")}>Close</button>
          <button ref={stay} className="btn primary" onClick={() => answer("stay")}>Stay</button>
        </div>
      </div>
    </div>
  );
}
