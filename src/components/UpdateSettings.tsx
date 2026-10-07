// Settings → Updates: this version and what the release check found, the update (its steps, why it failed, the
// restart), and Check for new releases (on or off) with Check now.
import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CircleArrowUp, CircleCheck, CircleMinus, CircleX, ExternalLink, LoaderCircle, RefreshCw, Square } from "lucide-react";
import { checkForUpdates, setUpdateAutoCheck, startUpdate, stopUpdate } from "../api";
import { canStop, checkLine, isRunning, stepText, type CheckMark } from "../lib/update";
import { useUpdate } from "../lib/useUpdate";
import type { UpdateStatus } from "../types";
import { Field, FormSection } from "./Form";
import { MarkdownView } from "./MarkdownView";
import { RestartButton } from "./UpdateNotice";

const MARKS: Record<CheckMark, typeof CircleCheck> = { ok: CircleCheck, new: CircleArrowUp, failed: CircleX, skipped: CircleMinus };

export function UpdateSettings() {
  const [s, setS] = useUpdate();
  const [err, setErr] = useState<string | null>(null);
  if (!s) return null;
  const act = (p: Promise<UpdateStatus>) => { setErr(null); p.then(setS).catch((e) => setErr(String(e))); };
  const line = checkLine(s);
  const Mark = MARKS[line.mark];
  const job = s.job;
  const release = s.available;
  const update = () => release && act(startUpdate(release.version));
  return (
    <FormSection title="Updates" text="Gizai updates from its releases on GitHub. An update builds the new version from source in the background, backs up your data, installs it and offers a restart. If a step fails, the installed version keeps working.">
      <Field label="Version" wide>
        <div className={`gh-line ${line.mark === "new" ? "ok" : line.mark}`}><Mark className="icon" /><span>Gizai {s.current}. {line.text}</span></div>
        {release?.url && (
          <div><button className="btn ghost sm" onClick={() => openUrl(release.url!).catch(() => {})}><ExternalLink className="icon" />Release {release.version} on GitHub</button></div>
        )}
        {release?.notes && (
          <details className="update-notes"><summary>What's new in {release.version}</summary><MarkdownView md={release.notes} /></details>
        )}
      </Field>
      {(job || release || s.installed) && (
        <Field label="Update" wide hint={s.installTo ? `Installs into ${s.installTo}. The source and its build stay in your data folder (update/), so the next update builds faster.` : undefined}>
          {job && isRunning(job) ? (
            <>
              <div className="gh-line"><LoaderCircle className="icon spin" /><span>{stepText(job)} {job.step === "build" ? "The first build takes a while; later ones are quicker. You can keep working." : ""}</span></div>
              {canStop(job) && <div><button className="btn sm" onClick={() => act(stopUpdate())}><Square className="icon" />Stop</button></div>}
              {job.backup && <span className="hint">Your data was backed up to <span className="mono">{job.backup}</span></span>}
            </>
          ) : job?.step === "installed" || s.installed ? (
            <>
              <div className="gh-line ok"><CircleCheck className="icon" /><span>Version {job?.step === "installed" ? job.version : s.installed} is installed. Restart Gizai to use it.</span></div>
              {job?.backup && <span className="hint">Your data was backed up to <span className="mono">{job.backup}</span></span>}
              <div><RestartButton className="btn primary" label="Restart Gizai" /></div>
            </>
          ) : (
            <>
              {job?.step === "failed" && (
                <div className="update-failed" role="alert">
                  <div className="gh-line failed"><CircleX className="icon" /><span>The update to {job.version} didn't work: {job.problem ?? "it failed"}. Gizai {s.current} is still installed and works as before.</span></div>
                  {job.output && <pre className="update-output mono">{job.output}</pre>}
                  <span className="hint">Everything it did is in <span className="mono">{job.log}</span></span>
                </div>
              )}
              {job?.step === "stopped" && (
                <div className="gh-line skipped"><CircleMinus className="icon" /><span>The update to {job.version} was stopped. Gizai {s.current} is still installed.</span></div>
              )}
              {release && (s.cannotInstall
                ? <div className="gh-line skipped"><CircleMinus className="icon" /><span>{s.cannotInstall}</span></div>
                : <div><button className="btn primary" onClick={update}><CircleArrowUp className="icon" />{job?.step === "failed" && job.version === release.version ? "Try again" : `Update to ${release.version}`}</button></div>)}
            </>
          )}
        </Field>
      )}
      <Field label="New releases" wide hint="Gizai asks GitHub for the latest release 20 seconds after it starts and every six hours. It sends nothing about you or your work.">
        <label className="check"><input type="checkbox" checked={s.autoCheck} onChange={(e) => act(setUpdateAutoCheck(e.target.checked))} />Check for new releases automatically</label>
        <div><button className="btn" onClick={() => act(checkForUpdates())} disabled={s.checking}><RefreshCw className="icon" />{s.checking ? "Checking…" : "Check now"}</button></div>
        {err && <div className="gh-line failed" role="alert"><CircleX className="icon" /><span>{err}</span></div>}
      </Field>
    </FormSection>
  );
}
