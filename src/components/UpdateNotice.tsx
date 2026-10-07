// The notice above Company in the sidebar, when a newer release is out. Update to <version> starts the update: it
// builds in the background while Gizai stays usable, backs up your data and installs. While it runs the notice says
// where it is; afterwards it offers the restart, or says the update failed (Settings → Updates says why).
import { useEffect, useState, type ReactNode } from "react";
import { CircleArrowUp, LoaderCircle, RotateCw, TriangleAlert } from "lucide-react";
import { restartGizai, startUpdate } from "../api";
import { href } from "../router";
import { notice } from "../lib/update";
import { useUpdate } from "../lib/useUpdate";
import { useLiveRuns } from "../lib/useLiveRuns";
import { useChatLive } from "./chat/useChat";

/** Restart Gizai to use the version an update installed. With agents at work it asks once more first: quitting stops
 * them, as it always does. */
export function RestartButton({ className, label }: { className: string; label: ReactNode }) {
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
  const restart = () => {
    if (working > 0 && !asking) { setAsking(true); return; }
    setBusy(true); setErr(null);
    restartGizai().catch((e) => { setErr(String(e)); setBusy(false); });
  };
  const ask = `${working === 1 ? "An agent is" : `${working} agents are`} at work: restarting stops ${working === 1 ? "it" : "them"}. Restart anyway?`;
  return (
    <>
      <button className={className} onClick={restart} disabled={busy}>
        <RotateCw className="icon" /><span>{busy ? "Restarting…" : asking ? ask : label}</span>
      </button>
      {err && <div className="update-error" role="alert">{err}</div>}
    </>
  );
}

export function UpdateNotice() {
  const [status, setStatus] = useUpdate();
  const [err, setErr] = useState<string | null>(null);
  const n = status ? notice(status) : null;
  if (!status || !n) return null;
  const settings = href({ page: "settings" });
  const start = () => {
    setErr(null);
    startUpdate(n.version).then(setStatus).catch((e) => setErr(String(e)));
  };
  let body: ReactNode;
  switch (n.kind) {
    case "offer":
      body = n.canInstall ? (
        <button className="update-notice" onClick={start} title="Builds it in the background, backs up your data, installs it and offers a restart">
          <CircleArrowUp className="icon" /><span>{n.text}</span></button>
      ) : (
        <a className="update-notice" href={settings} title={status.cannotInstall ?? undefined}><CircleArrowUp className="icon" /><span>{n.text}</span></a>
      );
      break;
    case "working":
      body = <a className="update-notice working" href={settings} title="Gizai stays usable meanwhile. Settings shows more."><LoaderCircle className="icon spin" /><span>{n.text}</span></a>;
      break;
    case "restart":
      body = <RestartButton className="update-notice ready" label={n.text} />;
      break;
    case "failed":
      body = <a className="update-notice failed" href={settings} title={status.job?.problem ?? undefined}><TriangleAlert className="icon" /><span>{n.text}</span></a>;
      break;
  }
  return (
    <div className="update-box" aria-label="Update">
      {body}
      {err && <div className="update-error" role="alert">{err}</div>}
    </div>
  );
}
