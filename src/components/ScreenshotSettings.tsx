// Settings → Agents and runs → Screenshots (GA-185, from GA-181's design): how many screenshots a run keeps, the largest
// one, how long a card keeps them and the ports of the projects' previews. Saved with Save settings, which refuses a value
// outside its range first, with why (lib/screenshotSettings.ts), as Settings → Runs does. The sizes and themes are each
// project's, in its form → Preview.
import { LIMITS, shownNumber, typedNumber, type LimitKey, type LimitsProblem } from "../lib/screenshotSettings";
import type { Settings } from "../types";
import { Field, FormSection } from "./Form";

/** The input of each value. */
const INPUT: Record<LimitKey, string> = {
  screenshotsPerRun: "s-shots-run", screenshotMaxMb: "s-shots-mb", screenshotsKeepSets: "s-shots-keep", screenshotsKeepDays: "s-shots-days",
  previewPortFrom: "s-ports-from", previewPortTo: "s-ports-to",
};

/** Puts the focus on the input of a value Save settings refused, when its tab is open. */
export function focusLimit(key: LimitKey) {
  if (typeof document !== "undefined") document.getElementById(INPUT[key])?.focus();
}

/** `s` and `setS` are Settings' own: a value typed here is saved with the rest by Save settings. `problem`: the value Save
 *  settings refused last, whose field says why in place of its hint. */
export function ScreenshotSettings({ s, setS, problem }: { s: Settings; setS: (s: Settings) => void; problem?: LimitsProblem | null }) {
  const error = (...keys: LimitKey[]) => (problem && keys.includes(problem.key) ? problem.message : null);
  const input = (key: LimitKey, label?: string) => (
    <input id={INPUT[key]} className="input" type="number" min={LIMITS[key].low} max={LIMITS[key].high} step={1} aria-label={label}
      value={shownNumber(s[key])} onChange={(e) => setS({ ...s, [key]: typedNumber(e.target.value) })} />
  );
  return (
    <FormSection id="settings-screenshots" title="Screenshots"
      text="What Gizai keeps of the screenshots agents take at the end of their runs, and the ports of the projects’ previews. Each project sets its sizes and themes in its form → Preview.">
      <Field label="Screenshots per run" htmlFor={INPUT.screenshotsPerRun} error={error("screenshotsPerRun")} hint="1 to 30; 12 by default. Gizai keeps the first ones by file name and says in the run what it left out.">
        {input("screenshotsPerRun")}</Field>
      <Field label="Largest screenshot (MB)" htmlFor={INPUT.screenshotMaxMb} error={error("screenshotMaxMb")} hint="1 to 50; 10 by default.">
        {input("screenshotMaxMb")}</Field>
      <Field label="Keep per card (sets)" htmlFor={INPUT.screenshotsKeepSets} error={error("screenshotsKeepSets")} hint="1 to 20; 3 by default. A newer run’s set removes the oldest beyond these.">
        {input("screenshotsKeepSets")}</Field>
      <Field label="Delete a finished card’s screenshots after (days)" htmlFor={INPUT.screenshotsKeepDays} error={error("screenshotsKeepDays")} hint="0 to 365; 30 by default, counted from Done or Cancelled. 0 keeps them.">
        {input("screenshotsKeepDays")}</Field>
      <Field label="Preview ports" htmlFor={INPUT.previewPortFrom} wide error={error("previewPortFrom", "previewPortTo")}
        hint="From 1024 to 65535; 47100 to 47999 by default. At least ten ports for each run at once: a run’s preview gets its own port and the nine after it.">
        <div className="input-group shot-ports">{input("previewPortFrom", "Preview ports from")}<span className="faint">to</span>{input("previewPortTo", "Preview ports to")}</div>
      </Field>
    </FormSection>
  );
}
