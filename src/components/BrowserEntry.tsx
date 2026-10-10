import { useEffect, useState } from "react";
import { browserEntry, listBrowserTools, saveBrowserEntry } from "../api";
import { relTime } from "../lib/format";
import { RISK_BADGE, toolsSummary } from "../lib/mcp";
import type { BrowserView } from "../types";
import { McpToolList } from "./McpTools";

/** The Browser program field's hint: a full path as this system writes one (Google Chrome's usual place on Windows,
 * `EXAMPLE_PROGRAM` in crates/gizai-agents/src/browser.rs). */
export const browserProgramHint = () => `A full path, like ${typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent || "")
  ? "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe" : "/usr/bin/chromium"}. Brave's is refused.`;

/** Settings → MCP servers, at the top: the built-in browser (Chrome DevTools MCP), like the built-in Claude Code in Coding
 * CLIs. Always hidden with a throwaway profile; only its version and browser program can change. Gizai installs nothing:
 * the form says what is missing and what to install. */
export function BrowserEntryRow() {
  const [v, setV] = useState<BrowserView | null>(null);
  const [edit, setEdit] = useState<{ version: string; program: string } | null>(null);
  const [busy, setBusy] = useState<"" | "save" | "list">("");
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { browserEntry().then(setV).catch((e) => setErr(String(e))); }, []);
  if (!v) return err ? <span className="warn" role="alert">{err}</span> : null;
  const run = async (what: "save" | "list", f: () => Promise<BrowserView>) => {
    setBusy(what);
    setErr(null);
    try { setV(await f()); if (what === "save") setEdit(null); } catch (e) { setErr(String(e)); if (what === "list") browserEntry().then(setV).catch(() => {}); }
    setBusy("");
  };
  const tools = v.listed?.tools ?? [];
  const sum = v.listed ? toolsSummary(tools) : null;
  return (
    <ul className="mcp-builtin" aria-label="Built-in browser">
      <li className="mcp-server">
        <div className="ms-head">
          <span className="cl-name">
            <span><b>{v.id}</b> <span className="faint">· Built in · Browser for agents</span>
              {sum && <> <span className={`badge ${RISK_BADGE[sum.risk]}`}>{tools.length} tool{tools.length === 1 ? "" : "s"}</span></>}</span>
            <span className="mono faint cl-cmd">{v.command}</span>
            <span className="faint">Chrome DevTools MCP: a hidden Chrome with a throwaway profile (--headless, --isolated), never on your screen and never your own browser, profile or logins. No usage statistics or page addresses go to Google.</span>
            {v.needs.missing.length === 0
              ? <span className="faint">Found: Node {v.needs.nodeVersion ?? ""} with npx, and {v.needs.browserName} ({v.needs.browser}).</span>
              : v.needs.missing.map((m) => <span key={m} className="warn">{m}</span>)}
            {v.usedBy.length > 0 && <span className="faint">On for {v.usedBy.join(", ")}</span>}
            {v.problem && <span className="warn">{v.problem}</span>}
            {busy === "list" && <span className="hint" role="status">Listing its tools (the first time npx downloads the package)…</span>}
          </span>
          <span className="cl-actions">
            <button className="btn ghost sm" disabled={!!busy} onClick={() => run("list", listBrowserTools)}>{busy === "list" ? "Listing…" : "List tools"}</button>
            <button className="btn ghost sm" disabled={!!busy} onClick={() => setEdit({ version: v.version, program: v.program })}>Edit</button>
          </span>
        </div>
        {edit && (
          <div className="cli-edit" role="group" aria-label="Edit the browser">
            <label>Version<input className="input mono" value={edit.version} onChange={(e) => setEdit({ ...edit, version: e.target.value })} placeholder="1.10.1" />
              <span className="hint">An exact version of chrome-devtools-mcp, never latest.</span></label>
            <label>Browser program<input className="input mono" value={edit.program} onChange={(e) => setEdit({ ...edit, program: e.target.value })} placeholder="Empty: Google Chrome, else Chromium" />
              <span className="hint">{browserProgramHint()}</span></label>
            <span className="cl-actions"><button className="btn ghost sm" disabled={!!busy} onClick={() => setEdit(null)}>Cancel</button>
              <button className="btn primary sm" disabled={!!busy || !edit.version.trim()} onClick={() => run("save", () => saveBrowserEntry(edit))}>{busy === "save" ? "Saving…" : "Save browser"}</button></span>
          </div>
        )}
        {v.listed && (
          <details className="ms-tools">
            <summary>{sum?.text} <span className="faint">Listed {relTime(v.listed.listedAt)}{v.listed.serverVersion ? `, version ${v.listed.serverVersion}` : ""}.</span></summary>
            <McpToolList tools={tools} />
          </details>
        )}
        {err && <span className="warn" role="alert">{err}</span>}
      </li>
    </ul>
  );
}
