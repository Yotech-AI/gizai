import { useEffect, useState } from "react";
import { findClis, listClis, saveClis } from "../api";
import { CLAUDE_CODE, cliSummary, KIND_LABEL, KINDS, parseEnv } from "../lib/clis";
import type { Cli, CliKind, CliStatus } from "../types";

/** The program each kind usually has, filled in when you pick the kind of a new CLI. */
const PROGRAM: Record<CliKind, string> = { claude_code: "claude", codex: "codex", gemini: "gemini", other: "" };
const KEEP_NOTE = " Gizai keeps these lines as plain text: point to a folder, don't paste a key.";
const ENV_HINT: Record<CliKind, string> = {
  claude_code: "For a second account: CLAUDE_CONFIG_DIR=~/.claude-2 (the folder that account logged in with)." + KEEP_NOTE,
  codex: "For a second account: CODEX_HOME=~/.codex-2 (the folder that account logged in with)." + KEEP_NOTE,
  gemini: "Optional, one NAME=value per line." + KEEP_NOTE,
  other: "Optional, one NAME=value per line." + KEEP_NOTE,
};

type Draft = { id: string; name: string; kind: CliKind; command: string; env: string; args: string };
const blank: Draft = { id: "", name: "", kind: "codex", command: "codex", env: "", args: "" };
const draftOf = (c: Cli): Draft => ({ id: c.id, name: c.name, kind: c.kind, command: c.command, env: c.env.join("\n"), args: c.args });
const cliOf = (d: Draft): Cli => ({ id: d.id, name: d.name.trim(), kind: d.kind, command: d.command.trim(), env: parseEnv(d.env), args: d.kind === "other" ? d.args.trim() : "" });
const bare = (c: CliStatus): Cli => ({ id: c.id, name: c.name, kind: c.kind, command: c.command, env: c.env, args: c.args });

/** Settings → Coding CLIs: the CLIs added next to Claude Code (Codex, Gemini, other programs, more accounts), each saved at once. */
export function CliSettings() {
  const [list, setList] = useState<CliStatus[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [edit, setEdit] = useState<Draft | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => { listClis().then(setList).catch((e) => setErr(String(e))); }, []);
  if (err && !list) return <span className="warn">{err}</span>;
  if (!list) return <span className="faint">Looking for your CLIs…</span>;
  const added = list.filter((c) => c.id !== CLAUDE_CODE);
  const store = async (next: Cli[], done: string) => {
    setBusy(true);
    try { setList(await saveClis(next)); setMsg(done); setErr(null); setEdit(null); } catch (e) { setErr(String(e)); }
    setBusy(false);
  };
  const saveEdit = () => {
    if (!edit) return;
    const c = cliOf(edit);
    const rest = added.map(bare);
    store(edit.id ? rest.map((o) => (o.id === edit.id ? c : o)) : [...rest, c], `Saved ${c.name}.`);
  };
  const find = async () => {
    setBusy(true);
    try {
      const found = await findClis();
      setBusy(false);
      if (found.length === 0) { setMsg("No other coding CLIs found on your PATH. Add one by hand."); return; }
      await store([...added.map(bare), ...found], `Added ${found.map((c) => c.name).join(", ")}.`);
    } catch (e) { setErr(String(e)); setBusy(false); }
  };
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => setEdit((x) => (x ? { ...x, [k]: v } : x));
  return (
    <div className="cli-list">
      {added.length === 0 ? <span className="faint">Only Claude Code so far.</span> : (
        <ul>
          {added.map((c) => (
            <li key={c.id}>
              <span className="cl-name"><b>{c.name}</b> <span className="faint">· {KIND_LABEL[c.kind]}</span>
                <span className="mono faint cl-cmd">{cliSummary(c)}</span>
                {c.problem && <span className="warn">{c.problem}</span>}</span>
              <button className="btn ghost sm" disabled={busy} onClick={() => { setEdit(draftOf(c)); setMsg(null); }}>Edit</button>
              <button className="btn ghost sm" disabled={busy} onClick={() => store(added.filter((o) => o.id !== c.id).map(bare), `Removed ${c.name}.`)}>Remove</button>
            </li>
          ))}
        </ul>
      )}
      {edit ? (
        <div className="cli-edit" role="group" aria-label={edit.id ? `Edit ${edit.name}` : "Add a CLI"}>
          <label>Name<input className="input" value={edit.name} onChange={(e) => set("name", e.target.value)} placeholder="Claude Code (2nd account)" autoFocus /></label>
          <label>Kind<select className="select" value={edit.kind} onChange={(e) => {
            const k = e.target.value as CliKind;
            setEdit((x) => x && { ...x, kind: k, command: !x.command || x.command === PROGRAM[x.kind] ? PROGRAM[k] : x.command });
          }}>{KINDS.map((k) => <option key={k} value={k}>{KIND_LABEL[k]}</option>)}</select></label>
          <label>Program<input className="input mono" value={edit.command} onChange={(e) => set("command", e.target.value)} placeholder="codex or /usr/local/bin/codex" /></label>
          <label className="wide">Environment<textarea className="textarea mono" rows={2} value={edit.env} onChange={(e) => set("env", e.target.value)} placeholder="NAME=value" />
            <span className="hint">{ENV_HINT[edit.kind]}</span></label>
          {edit.kind === "other" && (
            <label className="wide">Arguments<input className="input mono" value={edit.args} onChange={(e) => set("args", e.target.value)} placeholder="run -m {model} {prompt}" />
              <span className="hint">{"{prompt} is the task's prompt; without it the prompt goes in on stdin. {model} is the agent's model. Gizai reads its output as plain text and looks for the GIZAI_RESULT line at the end."}</span></label>
          )}
          <span className="cl-actions"><button className="btn ghost sm" disabled={busy} onClick={() => setEdit(null)}>Cancel</button>
            <button className="btn primary sm" disabled={busy || !edit.name.trim() || !edit.command.trim()} onClick={saveEdit}>{busy ? "Saving…" : edit.id ? "Save CLI" : "Add CLI"}</button></span>
        </div>
      ) : (
        <div className="cl-actions">
          <button className="btn sm" disabled={busy} onClick={() => { setEdit({ ...blank }); setMsg(null); }}>Add CLI</button>
          <button className="btn ghost sm" disabled={busy} onClick={find}>{busy ? "Looking…" : "Find installed CLIs"}</button>
        </div>
      )}
      {err && <span className="warn" role="alert">{err}</span>}
      {msg && !err && <span className="hint" role="status">{msg}</span>}
    </div>
  );
}
