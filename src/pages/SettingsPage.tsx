import { useEffect, useState } from "react";
import { detectClaude, detectGh, getSettings, saveSettings } from "../api";
import type { Settings } from "../types";
import { Field, FormSection } from "../components/Form";
import { OldWorktrees } from "../components/OldWorktrees";

export function SettingsPage() {
  const [s, setS] = useState<Settings | null>(null);
  const [budget, setBudget] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [detectingGh, setDetectingGh] = useState(false);
  useEffect(() => { getSettings().then((x) => { setS(x); setBudget(x.maxRunUsd != null ? String(x.maxRunUsd) : ""); }).catch((e) => setMsg({ ok: false, text: String(e) })); }, []);
  if (!s) return msg ? <div className="error-banner">{msg.text}</div> : null;
  const detect = async () => {
    setDetecting(true);
    try {
      const p = await detectClaude();
      if (p) { setS({ ...s, claudeBin: p }); setMsg({ ok: true, text: `Found Claude Code at ${p}` }); }
      else setMsg({ ok: false, text: "Claude Code was not found. Install it, or type the path to the claude program." });
    } finally { setDetecting(false); }
  };
  const findGh = async () => {
    setDetectingGh(true);
    try {
      const p = await detectGh();
      if (p) { setS({ ...s, ghBin: p }); setMsg({ ok: true, text: `Found the GitHub CLI at ${p}` }); }
      else setMsg({ ok: false, text: "The GitHub CLI (gh) was not found. Install it from cli.github.com, or type the path to the gh program." });
    } catch (e) { setMsg({ ok: false, text: String(e) }); } finally { setDetectingGh(false); }
  };
  const save = async (next: Settings = s) => {
    const usd = budget.trim() === "" ? null : Number(budget.replace(",", "."));
    if (usd !== null && !(usd > 0)) { setMsg({ ok: false, text: "The spending limit must be a positive amount, or empty for no limit." }); return; }
    try { await saveSettings({ ...next, maxRunUsd: usd }); setMsg({ ok: true, text: "Settings saved." }); } catch (e) { setMsg({ ok: false, text: String(e) }); }
  };
  return (
    <>
      <div className="topbar"><div className="crumbs"><b>Settings</b></div>
        <div className="actions"><button className="btn primary" onClick={() => save()}>Save settings</button></div></div>
      <div className="content"><div className="page" style={{ maxWidth: 1100 }}>
        {msg && <div className={msg.ok ? "ok-banner" : "error-banner"} role="status" style={{ margin: 0 }}>{msg.text}</div>}
        <div className="form">
          <FormSection title="Claude Code" text="Agents run this program headless (claude -p) with your Claude login.">
            <Field label="Program" htmlFor="s-bin" wide hint="Detect looks in your login shell and the usual install folders.">
              <div className="input-group"><input id="s-bin" className="input mono" value={s.claudeBin ?? ""} onChange={(e) => setS({ ...s, claudeBin: e.target.value })} placeholder="/home/you/.local/bin/claude" />
                <button className="btn" onClick={detect} disabled={detecting}>{detecting ? "Looking…" : "Detect"}</button></div></Field>
          </FormSection>
          <FormSection title="GitHub" text="Open pull request pushes a card's branch with your own git login and opens its pull request with the GitHub CLI, logged in as you (gh auth login). Every two minutes Gizai asks GitHub about the pull requests of cards in Review; a merge moves the card to Done.">
            <Field label="GitHub CLI" htmlFor="s-gh" wide hint="Empty: Gizai finds gh when it needs it. Detect looks in your login shell and the usual install folders.">
              <div className="input-group"><input id="s-gh" className="input mono" value={s.ghBin ?? ""} onChange={(e) => setS({ ...s, ghBin: e.target.value })} placeholder="/usr/bin/gh" />
                <button className="btn" onClick={findGh} disabled={detectingGh}>{detectingGh ? "Looking…" : "Detect"}</button></div></Field>
          </FormSection>
          <FormSection title="Runs" text="Each run is a Claude Code process in its own git worktree. Gizai stops a run at the first limit it reaches, and tells the agent these limits so it can commit its work in time.">
            <Field label="Runs at once" htmlFor="s-max" hint="All agents together, 1 to 20; each agent also has its own cards at once"><input id="s-max" className="input" type="number" min={1} max={20} value={s.maxConcurrentRuns} onChange={(e) => setS({ ...s, maxConcurrentRuns: Number(e.target.value) })} /></Field>
            <Field label="Spend per run ($)" htmlFor="s-usd" hint="Claude Code stops a run that reaches this amount"><input id="s-usd" className="input" inputMode="decimal" value={budget} onChange={(e) => setBudget(e.target.value)} placeholder="No limit" /></Field>
            <Field label="Minutes per run" htmlFor="s-min" hint="5 to 480"><input id="s-min" className="input" type="number" min={5} max={480} value={s.maxRunMinutes} onChange={(e) => setS({ ...s, maxRunMinutes: Number(e.target.value) })} /></Field>
            <Field label="Tool calls per run" htmlFor="s-calls" hint="20 to 2000. Every file read, edit and command is one."><input id="s-calls" className="input" type="number" min={20} max={2000} value={s.maxRunToolCalls} onChange={(e) => setS({ ...s, maxRunToolCalls: Number(e.target.value) })} /></Field>
            <Field label="Pause all agents" wide hint={s.agentsPaused ? "Paused: no heartbeats and no automatic starts. Run still works by hand." : "Agents wake up on their own (heartbeats, assignments)."}>
              <label className="check"><input type="checkbox" checked={s.agentsPaused} onChange={(e) => { const next = { ...s, agentsPaused: e.target.checked }; setS(next); save(next); }} />Pause all agents</label></Field>
          </FormSection>
          <FormSection title="Data" text="Everything Gizai stores lives in this folder on this computer.">
            <Field label="Data folder" wide><span className="mono">{s.dataDir}</span></Field>
            <Field label="Worktrees" wide><span className="mono">{s.dataDir}/worktrees</span></Field>
            <Field label="Run logs" wide><span className="mono">{s.dataDir}/runs</span></Field>
            <Field label="Worktrees of finished cards" wide hint="Done and Cancelled cards keep their worktree, so a new card of the same project can take it over with a warm build. Remove the ones you no longer need.">
              <OldWorktrees /></Field>
          </FormSection>
        </div>
      </div></div>
    </>
  );
}
