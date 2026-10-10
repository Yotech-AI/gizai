import { useEffect, useState } from "react";
import { Plus, X } from "lucide-react";
import { importMcpServers, listMcpServers, listMcpTools, mcpSignIn, mcpSignOut, removeMcpServer, saveMcpServer, scanClaudeCodeMcp } from "../api";
import { linesOf, nameProblem, parseArgs, renameLine, RISK_BADGE, secretLines, serverWhere, signInLabel, toolsSummary, TRANSPORT_LABEL, type LineDraft } from "../lib/mcp";
import { relTime } from "../lib/format";
import type { McpScan, McpServerInput, McpServerView } from "../types";
import { BrowserEntryRow } from "./BrowserEntry";
import { McpToolList } from "./McpTools";

type Draft = { id: string; name: string; transport: string; command: string; args: string; env: LineDraft[]; url: string; headers: LineDraft[];
  clientId: string; source: string };
const blank: Draft = { id: "", name: "", transport: "stdio", command: "", args: "", env: [], url: "", headers: [], clientId: "", source: "" };
const draftOf = (s: McpServerView): Draft => ({ id: s.id, name: s.name, transport: s.transport, command: s.command, args: s.args.join("\n"),
  env: linesOf(s.envNames, s.missing), url: s.url, headers: linesOf(s.headerNames, s.missing), clientId: s.clientId, source: s.source });
const inputOf = (d: Draft): McpServerInput => {
  const stdio = d.transport === "stdio";
  return {
    server: { id: d.id, name: d.name.trim(), transport: d.transport, command: stdio ? d.command.trim() : "", args: stdio ? parseArgs(d.args) : [], envNames: [],
      url: stdio ? "" : d.url.trim(), headerNames: [], clientId: stdio ? "" : d.clientId.trim(), source: d.source },
    env: stdio ? secretLines(d.env) : [],
    headers: stdio ? [] : secretLines(d.headers),
  };
};

/** What a line's value box says while it is empty. */
const valueHint = (l: LineDraft) =>
  l.saved ? "Saved in your keychain: type to replace" : l.savedAs ? `Renamed from ${l.savedAs}: type its value again` : "Value";

/** Environment or header lines: a name and a value each. A saved value is never shown: typing replaces it. */
function SecretLines({ what, lines, onChange, namePlaceholder }: { what: string; lines: LineDraft[]; onChange: (l: LineDraft[]) => void; namePlaceholder: string }) {
  const put = (i: number, l: LineDraft) => onChange(lines.map((x, j) => (j === i ? l : x)));
  return (
    <div className="folder-list">
      {lines.map((l, i) => (
        <div key={i} className="input-group">
          <input className="input mono" style={{ flex: "0 0 38%" }} aria-label={`${what} ${i + 1} name`} value={l.name} placeholder={namePlaceholder}
            onChange={(e) => put(i, renameLine(l, e.target.value))} />
          <input className="input mono" type="password" autoComplete="off" aria-label={`${what} ${i + 1} value`} value={l.value}
            placeholder={valueHint(l)} onChange={(e) => put(i, { ...l, value: e.target.value })} />
          <button type="button" className="btn ghost sm icon-only" aria-label={`Remove ${what.toLowerCase()} ${i + 1}`} title="Remove"
            onClick={() => onChange(lines.filter((_, j) => j !== i))}><X className="icon" /></button>
        </div>
      ))}
      <div><button type="button" className="btn sm" onClick={() => onChange([...lines, { name: "", value: "", saved: false }])}><Plus className="icon" />Add {what.toLowerCase()}</button></div>
    </div>
  );
}

/** Import from Claude Code: the servers in each Claude Code's config file, ticked to import, a clashing name changed first. */
function ImportPanel({ scan, existing, busy, onImport, onCancel }: {
  scan: McpScan; existing: string[]; busy: boolean; onImport: (picks: { key: string; name: string }[]) => void; onCancel: () => void;
}) {
  const [picks, setPicks] = useState<Record<string, { on: boolean; name: string }>>(
    () => Object.fromEntries(scan.servers.map((c) => [c.key, { on: false, name: c.name }])));
  const ticked = scan.servers.filter((c) => picks[c.key]?.on);
  const problem = (key: string) => {
    const p = picks[key];
    const others = ticked.filter((c) => c.key !== key).map((c) => picks[c.key].name.trim());
    return p?.on ? nameProblem(p.name, [...existing, ...others]) : null;
  };
  const blocked = ticked.length === 0 || ticked.some((c) => problem(c.key));
  return (
    <div className="mcp-import" role="group" aria-label="Import from Claude Code">
      {scan.servers.length === 0 ? <span className="faint">No MCP servers in Claude Code's config files.</span> : (
        <ul>
          {scan.servers.map((c) => {
            const p = picks[c.key];
            const why = problem(c.key);
            return (
              <li key={c.key}>
                <label className="check"><input type="checkbox" aria-label={`Import ${c.name} (${c.account}, ${c.scope} scope)`} checked={p.on}
                  onChange={(e) => setPicks({ ...picks, [c.key]: { ...p, on: e.target.checked } })} /></label>
                <span className="cl-name">
                  <span><b>{c.name}</b> <span className="faint">· {c.account} · {c.scope} scope{c.folder ? `: ${c.folder}` : ""} · {TRANSPORT_LABEL[c.transport] ?? c.transport}</span>
                    {c.already && <> <span className="badge outline">Already in the list</span></>}</span>
                  <span className="mono faint cl-cmd">{serverWhere(c)}</span>
                  {(c.envNames.length > 0 || c.headerNames.length > 0) && (
                    <span className="faint">{c.envNames.length > 0 && <>Environment: <span className="mono">{c.envNames.join(", ")}</span></>}
                      {c.headerNames.length > 0 && <>Headers: <span className="mono">{c.headerNames.join(", ")}</span></>} (values go to your keychain)</span>
                  )}
                  {p.on && (c.clash || why) && (
                    <span className="input-group" style={{ maxWidth: 420 }}>
                      <input className="input mono" aria-label={`New name for ${c.name}`} value={p.name} onChange={(e) => setPicks({ ...picks, [c.key]: { ...p, name: e.target.value } })} />
                    </span>
                  )}
                  {why && <span className="warn">{why}</span>}
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {scan.problems.map((p) => <span key={p} className="warn">{p}</span>)}
      <span className="hint">Read only: Gizai doesn't change Claude Code's files, doesn't read its sign-ins and starts no server to find these. An import is a copy: later changes in Claude Code don't follow.
        Connectors from your claude.ai account and servers from plugins aren't in these files: add those by address.</span>
      <span className="cl-actions"><button className="btn ghost sm" disabled={busy} onClick={onCancel}>Cancel</button>
        <button className="btn primary sm" disabled={busy || blocked}
          onClick={() => onImport(ticked.map((c) => ({ key: c.key, name: picks[c.key].name.trim() })))}>{busy ? "Importing…" : `Import ${ticked.length || ""}`.trim()}</button></span>
    </div>
  );
}

/** Settings → MCP servers: one list for all agents. Add by hand or import from Claude Code, List tools, sign in. */
export function McpSettings() {
  const [list, setList] = useState<McpServerView[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [edit, setEdit] = useState<Draft | null>(null);
  const [busy, setBusy] = useState<Record<string, string>>({});
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [removing, setRemoving] = useState<string | null>(null);
  const [scan, setScan] = useState<McpScan | null>(null);
  const [importing, setImporting] = useState(false);
  const load = () => listMcpServers().then(setList).catch((e) => setErr(String(e)));
  useEffect(() => { load(); }, []);
  if (err && !list) return <span className="warn">{err}</span>;
  if (!list) return <span className="faint">Loading the MCP servers…</span>;
  const put = (v: McpServerView) => setList((l) => (l ?? []).some((x) => x.id === v.id) ? (l ?? []).map((x) => (x.id === v.id ? v : x)) : [...(l ?? []), v]);
  /** Runs `f` for server `id` with its button showing `doing`; `f` gives the message to show. */
  const act = async (id: string, doing: string, f: () => Promise<string | null>) => {
    setBusy((b) => ({ ...b, [id]: doing }));
    setErr(null);
    setMsg(null);
    try { setMsg(await f()); } catch (e) { setErr(String(e)); load(); }
    setBusy((b) => { const { [id]: _, ...rest } = b; return rest; });
  };
  const save = () => edit && act(edit.id || "new", "save", async () => {
    const v = await saveMcpServer(inputOf(edit));
    put(v);
    setEdit(null);
    return `Saved ${v.name}.${v.listed ? "" : " List tools shows what it offers before you switch it on for an agent."}`;
  });
  const listTools = (s: McpServerView) => act(s.id, "list", async () => {
    const v = await listMcpTools(s.id);
    put(v);
    setOpen((o) => ({ ...o, [s.id]: true }));
    if (v.problem) return null;
    return `${v.name} lists ${v.listed?.tools.length ?? 0} tool${v.listed?.tools.length === 1 ? "" : "s"}.`;
  });
  const signIn = (s: McpServerView) => act(s.id, "signin", async () => { const v = await mcpSignIn(s.id); put(v); return `Signed in to ${s.name}. List tools to see what it offers.`; });
  const signOut = (s: McpServerView) => act(s.id, "signout", async () => { const v = await mcpSignOut(s.id); put(v); return `Signed out of ${s.name}.`; });
  const remove = (s: McpServerView) => act(s.id, "remove", async () => {
    await removeMcpServer(s.id);
    setList((l) => (l ?? []).filter((x) => x.id !== s.id));
    setRemoving(null);
    return `Removed ${s.name}.`;
  });
  const startImport = async () => {
    setImporting(true);
    setErr(null);
    setMsg(null);
    try { setScan(await scanClaudeCodeMcp()); } catch (e) { setErr(String(e)); }
    setImporting(false);
  };
  const doImport = async (picks: { key: string; name: string }[]) => {
    setImporting(true);
    try {
      const added = await importMcpServers(picks);
      setScan(null);
      await load();
      setMsg(`Imported ${added.map((v) => v.name).join(", ")}. List tools shows what each offers.`);
    } catch (e) { setErr(String(e)); await load(); }
    setImporting(false);
  };
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => setEdit((x) => (x ? { ...x, [k]: v } : x));
  const editProblem = edit ? nameProblem(edit.name, list.filter((s) => s.id !== edit.id).map((s) => s.name)) : null;
  const anyBusy = Object.keys(busy).length > 0;
  return (
    <div className="cli-list mcp-list">
      <BrowserEntryRow />
      {list.length === 0 ? <span className="faint">No MCP servers yet.</span> : (
        <ul>
          {list.map((s) => {
            const doing = busy[s.id];
            const sign = signInLabel(s.signIn);
            const tools = s.listed?.tools ?? [];
            const sum = s.listed ? toolsSummary(tools) : null;
            return (
              <li key={s.id} className="mcp-server">
                <div className="ms-head">
                  <span className="cl-name">
                    <span><b>{s.name}</b> <span className="faint">· {TRANSPORT_LABEL[s.transport] ?? s.transport}</span>
                      {sign && <> <span className={`badge ${sign.badge}`}>{sign.text}</span></>}
                      {sum && <> <span className={`badge ${RISK_BADGE[sum.risk]}`}>{tools.length} tool{tools.length === 1 ? "" : "s"}</span></>}</span>
                    <span className="mono faint cl-cmd">{serverWhere(s)}</span>
                    {s.source && <span className="faint">Imported from {s.source}</span>}
                    {s.usedBy.length > 0 && <span className="faint">On for {s.usedBy.join(", ")}</span>}
                    {s.signIn === "signed_in" && <span className="faint">Its tools act as you in {s.name}.</span>}
                    {s.missing.length > 0 && <span className="warn">No value saved for {s.missing.join(", ")}: Edit to enter it.</span>}
                    {s.problem && <span className="warn">{s.problem}</span>}
                    {doing === "signin" && <span className="hint" role="status">Sign in in your browser: Gizai waits up to 10 minutes for it.</span>}
                    {doing === "list" && <span className="hint" role="status">Listing its tools (a server started with npx may first download its package)…</span>}
                  </span>
                  <span className="cl-actions">
                    <button className="btn ghost sm" disabled={!!doing} onClick={() => listTools(s)}>{doing === "list" ? "Listing…" : "List tools"}</button>
                    {s.transport !== "stdio" && s.signIn === "signed_in" && (
                      <button className="btn ghost sm" disabled={!!doing} onClick={() => signOut(s)}>{doing === "signout" ? "Signing out…" : "Sign out"}</button>)}
                    {s.transport !== "stdio" && s.signIn === "needs_sign_in" && (
                      <button className="btn primary sm" disabled={!!doing} onClick={() => signIn(s)}>{doing === "signin" ? "Waiting…" : "Sign in"}</button>)}
                    <button className="btn ghost sm" disabled={!!doing} onClick={() => { setEdit(draftOf(s)); setMsg(null); }}>Edit</button>
                    <button className="btn ghost sm" disabled={!!doing} onClick={() => setRemoving(s.id)}>Remove</button>
                  </span>
                </div>
                {removing === s.id && (
                  <div className="ms-confirm" role="alert">
                    <span>Remove {s.name}? {s.signIn === "signed_in" ? "Gizai signs out of it, " : ""}its values leave your keychain{s.usedBy.length > 0 ? `, and ${s.usedBy.join(", ")} no longer get it` : ""}.</span>
                    <span className="cl-actions"><button className="btn ghost sm" onClick={() => setRemoving(null)}>Cancel</button>
                      <button className="btn sm" disabled={!!doing} onClick={() => remove(s)}>{doing === "remove" ? "Removing…" : `Remove ${s.name}`}</button></span>
                  </div>
                )}
                {s.listed && (
                  <details className="ms-tools" open={!!open[s.id]} onToggle={(e) => { const o = (e.target as HTMLDetailsElement).open; setOpen((x) => ({ ...x, [s.id]: o })); }}>
                    <summary>{sum?.text} <span className="faint">Listed {relTime(s.listed.listedAt)}{s.listed.serverVersion ? `, version ${s.listed.serverVersion}` : ""}.</span></summary>
                    <McpToolList tools={tools} />
                  </details>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {scan && <ImportPanel scan={scan} existing={list.map((s) => s.name)} busy={importing} onImport={doImport} onCancel={() => setScan(null)} />}
      {edit ? (
        <div className="cli-edit" role="group" aria-label={edit.id ? `Edit ${edit.name}` : "Add an MCP server"}>
          <label>Name<input className="input mono" value={edit.name} onChange={(e) => set("name", e.target.value)} placeholder="otus" autoFocus /></label>
          <label>Kind<select className="select" value={edit.transport} onChange={(e) => set("transport", e.target.value)}>
            <option value="stdio">A command (stdio)</option>
            <option value="http">An address (HTTP)</option>
            <option value="sse">An address (SSE, older servers)</option>
          </select></label>
          {edit.transport === "stdio" ? (
            <label>Command<input className="input mono" value={edit.command} onChange={(e) => set("command", e.target.value)} placeholder="npx" /></label>
          ) : (
            <label>Address<input className="input mono" value={edit.url} onChange={(e) => set("url", e.target.value)} placeholder="https://os.example.com/api/mcp" /></label>
          )}
          {editProblem && edit.name.trim() && <span className="warn wide">{editProblem}</span>}
          {edit.transport === "stdio" ? (
            <>
              <label className="wide">Arguments<textarea className="textarea mono" rows={3} value={edit.args} onChange={(e) => set("args", e.target.value)} placeholder={"-y\n@acme/mcp-server"} />
                <span className="hint">One per line. Gizai starts exactly this: it installs nothing itself.</span></label>
              <div className="wide field-like"><span>Environment</span>
                <SecretLines what="Environment line" lines={edit.env} onChange={(l) => set("env", l)} namePlaceholder="ACME_TOKEN" />
                <span className="hint">Values go to your keychain, never to Gizai's database, and aren't shown again once saved.</span></div>
            </>
          ) : (
            <>
              <div className="wide field-like"><span>Headers</span>
                <SecretLines what="Header" lines={edit.headers} onChange={(l) => set("headers", l)} namePlaceholder="X-Api-Key" />
                <span className="hint">Values go to your keychain, never to Gizai's database, and aren't shown again once saved. A server that asks you to sign in doesn't need an Authorization header: Sign in does that.</span></div>
              <label className="wide">Client id for sign-in<input className="input mono" value={edit.clientId} onChange={(e) => set("clientId", e.target.value)} placeholder="Only when the service doesn't let Gizai register itself" />
                <span className="hint">Empty: Gizai registers itself with the service when you sign in.</span></label>
            </>
          )}
          <span className="cl-actions"><button className="btn ghost sm" disabled={anyBusy} onClick={() => setEdit(null)}>Cancel</button>
            <button className="btn primary sm" disabled={anyBusy || !!editProblem || !(edit.transport === "stdio" ? edit.command.trim() : edit.url.trim())} onClick={save}>
              {busy[edit.id || "new"] ? "Saving…" : edit.id ? "Save server" : "Add server"}</button></span>
        </div>
      ) : !scan && (
        <div className="cl-actions">
          <button className="btn sm" disabled={anyBusy} onClick={() => { setEdit({ ...blank }); setMsg(null); }}>Add MCP server</button>
          <button className="btn ghost sm" disabled={anyBusy || importing} onClick={startImport}>{importing ? "Reading…" : "Import from Claude Code"}</button>
        </div>
      )}
      {err && <span className="warn" role="alert">{err}</span>}
      {msg && !err && <span className="hint" role="status">{msg}</span>}
    </div>
  );
}
