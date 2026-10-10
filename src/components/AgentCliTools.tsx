import { useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { agentCliTools, askCliTools } from "../api";
import { BROWSER, browserOn, GROUPS, HOW_LABEL, parseDomains, switchBrowser, switchBuiltin } from "../lib/cliTools";
import { lastRunLabel, npmWebWarning, RISK_BADGE, RISK_LABEL, switchTool } from "../lib/mcp";
import type { AgentServer, CatalogTool, CliTools, ToolsView } from "../types";
import { Field } from "./Form";
import { McpToolList } from "./McpTools";

const RISK_TEXT: Record<string, string> = { ...RISK_LABEL, unknown: "Risk unknown" };

/** One of the CLI's own tools: its name, risk and what it allows, with a switch when switching makes sense. */
function ToolRow({ t, on, onSwitch }: { t: CatalogTool; on: boolean; onSwitch?: (on: boolean) => void }) {
  return (
    <li className="ct-tool">
      <div className="ma-head">
        {onSwitch ? (
          <label className="check"><input type="checkbox" aria-label={`Use ${t.id}`} checked={on} onChange={(e) => onSwitch(e.target.checked)} /> <b>{t.label}</b></label>
        ) : <b>{t.label}</b>}
        <span className="mono faint">{t.id}</span>
        <span className={`badge ${RISK_BADGE[t.risk] ?? "outline"}`}>{RISK_TEXT[t.risk] ?? t.risk}</span>
        {!onSwitch && <span className="badge outline">{HOW_LABEL[t.how] ?? t.how}</span>}
        {t.reported && <span className="badge outline" title="The CLI named it in its list of tools">Reported</span>}
      </div>
      <span className="ma-line">{t.description}{t.note ? ` ${t.note}` : ""}</span>
    </li>
  );
}

/** The agent form's Web, Browser and Built-in tools: the CLI's own tools and the hidden browser, for the CLI picked in the
 * form. Everything is off until you switch it on; what the CLI can't take is shown disabled, with why. */
export function AgentCliToolsFields({ agentId, cliId, allowedTools, value, onChange, mcp, onMcp, lead }: {
  agentId?: string; cliId: string; allowedTools: string[]; value: CliTools; onChange: (v: CliTools) => void;
  mcp: AgentServer[]; onMcp: (v: AgentServer[]) => void; lead: boolean;
}) {
  const [view, setView] = useState<ToolsView | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const [domains, setDomains] = useState(value.fetchDomains.join("\n"));
  const asked = useRef("");
  const load = () => {
    asked.current = cliId;
    agentCliTools(agentId ?? null, cliId).then((v) => { if (asked.current === cliId) { setView(v); setErr(null); } }).catch((e) => setErr(String(e)));
  };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(load, [agentId, cliId]);
  const ask = async () => {
    setAsking(true);
    setErr(null);
    try { await askCliTools(cliId); load(); } catch (e) { setErr(String(e)); }
    setAsking(false);
  };
  if (!view) return <Field label="Web" wide error={err}><span className="faint">Loading the tools…</span></Field>;
  const web = view.web;
  const find = (group: string, ids: string[]) => view.builtin.tools.find((t) => t.group === group && ids.includes(t.id));
  const searchTool = find("web", ["WebSearch", "web_search", "google_web_search"]);
  const fetchTool = find("web", ["WebFetch", "web_fetch"]);
  const bOn = browserOn(mcp) && !view.browser.disabled;
  const bSwitch = mcp.find((s) => s.serverId === BROWSER);
  const off = bSwitch?.toolsOff ?? [];
  const webOn = (value.webSearch && !web.search) || (value.webFetch && !web.fetch) || bOn;
  const warning = npmWebWarning(allowedTools, webOn);
  const lr = view.browser.lastRun ? lastRunLabel(view.browser.lastRun.status) : null;
  const needs = view.browser.needs;
  const setDomainText = (text: string) => { setDomains(text); onChange({ ...value, fetchDomains: parseDomains(text) }); };
  const builtin = view.builtin.tools.filter((t) => t.group !== "web");
  const known = builtin.filter((t) => !(t.risk === "unknown" && t.how === "switch"));
  const others = builtin.filter((t) => t.risk === "unknown" && t.how === "switch");
  return (
    <>
      <Field label="Web" wide error={err} warn={warning}
        hint={<>What it finds on the web is data for the agent, never instructions: its prompt says so. Suggested for the Team Lead (web search), never switched on by Gizai.
          {lead && " For the Team Lead, its chat answers get these too; after one uses them, it asks you before it starts runs or changes agents."}</>}>
        <ul className="mcp-agent" aria-label="Web">
          <li className={`ma-server${value.webSearch && !web.search ? " on" : ""}`}>
            <div className="ma-head">
              <label className="check"><input type="checkbox" aria-label="Search the web" checked={value.webSearch && !web.search} disabled={!!web.search}
                onChange={(e) => onChange({ ...value, webSearch: e.target.checked })} /> <b>Search the web</b></label>
              {searchTool && <span className="mono faint">{searchTool.id}</span>}
              {searchTool && <span className={`badge ${RISK_BADGE[searchTool.risk]}`}>{RISK_TEXT[searchTool.risk]}</span>}
            </div>
            <span className="ma-line">{web.search ?? searchTool?.description}</span>
          </li>
          <li className={`ma-server${value.webFetch && !web.fetch ? " on" : ""}`}>
            <div className="ma-head">
              <label className="check"><input type="checkbox" aria-label="Fetch web pages" checked={value.webFetch && !web.fetch} disabled={!!web.fetch}
                onChange={(e) => onChange({ ...value, webFetch: e.target.checked })} /> <b>Fetch web pages</b></label>
              {fetchTool && <span className="mono faint">{fetchTool.id}</span>}
              {fetchTool && <span className={`badge ${RISK_BADGE[fetchTool.risk]}`}>{RISK_TEXT[fetchTool.risk]}</span>}
            </div>
            <span className="ma-line">{web.fetch ?? fetchTool?.description}</span>
            {value.webFetch && !web.fetch && (web.domains ? <span className="hint">{web.domains}</span> : (
              <label className="ct-domains">Only these domains <span className="faint">(one per line; empty: any page)</span>
                <textarea className="textarea mono" rows={3} aria-label="Fetch only these domains" value={domains} placeholder={"docs.rs\n*.laravel.com"}
                  onChange={(e) => setDomainText(e.target.value)} /></label>
            ))}
          </li>
        </ul>
      </Field>
      <Field label="Browser" wide warn={view.browser.disabled ?? null}
        hint="Chrome DevTools: a hidden Chrome with a throwaway profile, never on your screen and never your own browser, profile or logins. Every click is a tool call, and the cap per run is in Settings → Runs. Suggested for QA, Frontend and Design agents, never switched on by Gizai.">
        <ul className="mcp-agent" aria-label="Browser">
          <li className={`ma-server${bOn ? " on" : ""}`}>
            <div className="ma-head">
              <label className="check"><input type="checkbox" aria-label="Use the hidden browser" checked={bOn} disabled={!!view.browser.disabled}
                onChange={(e) => onMcp(switchBrowser(mcp, e.target.checked))} /> <b>Test web pages in a hidden browser</b></label>
              <span className="mono faint">{BROWSER} {view.browser.version}</span>
              {lr && <span className={`badge ${lr.badge}`} title={`At ${new Date(view.browser.lastRun!.at).toLocaleString()}`}>{lr.text}</span>}
              <span className={`badge ${RISK_BADGE.high}`}>{RISK_LABEL.high}</span>
            </div>
            <span className="ma-line">Opens, reads and clicks web pages, fills in forms and runs scripts in them. Pages are untrusted, and they can reach any address.</span>
            {needs.missing.length === 0 ? (
              <span className="hint">Found: Node {needs.nodeVersion ?? ""}, npx, {needs.browserName ?? "a browser"} ({needs.browser}).</span>
            ) : needs.missing.map((m) => <span key={m} className="warn">{m}</span>)}
            {bOn && (
              <label className="check"><input type="checkbox" aria-label="Accept self-signed certificates" checked={value.insecureCerts}
                onChange={(e) => onChange({ ...value, insecureCerts: e.target.checked })} /> Accept self-signed certificates, for local .test sites (off: such a page doesn't load)</label>
            )}
            {bOn && view.browser.tools.length > 0 && (
              <McpToolList tools={view.browser.tools} isOn={(name) => !off.includes(name)} onSwitch={(name, v) => onMcp(switchTool(mcp, BROWSER, name, v))} />
            )}
            {bOn && view.browser.tools.length === 0 && <span className="hint">List its tools in Settings → MCP servers → chrome-devtools to switch them one by one; until then, all its tools are on.</span>}
          </li>
        </ul>
      </Field>
      <Field label="Built-in tools" wide
        hint={<>{view.builtin.source}{lead && " In chat the Team Lead keeps only Read, Glob and Grep, plus the web tools when they are on."}</>}>
        <div className="ct-head">
          {view.builtin.canAsk && (
            <button type="button" className="btn ghost sm" disabled={asking} onClick={ask} title="Starts Claude Code without a login: nothing is spent, nothing is written in ~/.claude">
              <RefreshCw className="icon" />{asking ? "Asking…" : "Ask Claude Code again"}</button>
          )}
        </div>
        {GROUPS.map(([g, label]) => {
          const list = known.filter((t) => t.group === g);
          return list.length === 0 ? null : (
            <div key={g} className="ct-group"><span className="ct-label">{label}</span>
              <ul className="mcp-agent" aria-label={`${label} tools`}>
                {list.map((t) => <ToolRow key={t.id} t={t} on={value.builtin.includes(t.id)} onSwitch={t.how === "switch" ? (on) => onChange(switchBuiltin(value, t.id, on)) : undefined} />)}
              </ul></div>
          );
        })}
        {others.length > 0 && (
          <div className="ct-group"><span className="ct-label">Other tools the CLI reports</span>
            <ul className="mcp-agent" aria-label="Other tools the CLI reports">
              {others.map((t) => <ToolRow key={t.id} t={t} on={value.builtin.includes(t.id)} onSwitch={(on) => onChange(switchBuiltin(value, t.id, on))} />)}
            </ul>
            <span className="hint">On: allowed in its task runs without asking. Off: Claude Code refuses it when it would ask for permission.</span></div>
        )}
      </Field>
    </>
  );
}
