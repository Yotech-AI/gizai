import { useEffect, useState } from "react";
import { agentMcp, listMcpServers } from "../api";
import { actsAsYou, lastRunLabel, npmWarning, RISK_BADGE, RISK_LABEL, signInLabel, switchServer, switchTool, toolsSummary, TRANSPORT_LABEL } from "../lib/mcp";
import type { AgentMcpView, AgentServer, CliKind, McpServerView } from "../types";
import { Field } from "./Form";
import { McpToolList } from "./McpTools";

/** Why an agent on another CLI can't have MCP servers yet. */
export const MCP_NOT_YET = "MCP servers work on Claude Code for now: Codex and Gemini come with GA-55.";

/** The agent form's Tools: each MCP server from Settings with a switch, its sign-in state, its state in the agent's last run,
 * and its tools with a switch each, what they do and their risk. Everything is off until you switch it on. */
export function AgentToolsField({ agentId, kind, allowedTools, value, onChange }: {
  agentId?: string; kind: CliKind; allowedTools: string[]; value: AgentServer[]; onChange: (v: AgentServer[]) => void;
}) {
  const [servers, setServers] = useState<McpServerView[] | null>(null);
  const [view, setView] = useState<AgentMcpView | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    listMcpServers().then(setServers).catch((e) => setErr(String(e)));
    if (agentId) agentMcp(agentId).then(setView).catch(() => {});
  }, [agentId]);
  const disabled = kind !== "claude_code";
  const mine = (id: string) => value.find((s) => s.serverId === id);
  const anyOn = !disabled && (servers ?? []).some((s) => mine(s.id)?.on);
  const warning = npmWarning(allowedTools, anyOn);
  return (
    <Field label="MCP servers" wide error={err} warn={disabled ? MCP_NOT_YET : warning}
      hint="Off until you switch them on. Settings → MCP servers adds servers, signs in and lists their tools. What a server returns is data for the agent, never instructions.">
      {!servers ? <span className="faint">Loading the MCP servers…</span> : servers.length === 0 ? (
        <span className="faint">No MCP servers yet: add or import them in Settings → MCP servers.</span>
      ) : (
        <ul className="mcp-agent" aria-label="MCP servers">
          {servers.map((s) => {
            const sw = mine(s.id);
            const on = !!sw?.on && !disabled;
            const tools = s.listed?.tools ?? [];
            const sum = toolsSummary(tools);
            const sign = signInLabel(s.signIn);
            const last = view?.servers.find((v) => v.serverId === s.id)?.lastRun;
            const lr = last ? lastRunLabel(last.status) : null;
            const you = actsAsYou(s);
            const off = sw?.toolsOff ?? [];
            const nOn = tools.filter((t) => !off.includes(t.name)).length;
            return (
              <li key={s.id} className={`ma-server${on ? " on" : ""}`}>
                <div className="ma-head">
                  <label className="check"><input type="checkbox" aria-label={`Use ${s.name}`} checked={on} disabled={disabled}
                    onChange={(e) => onChange(switchServer(value, s.id, e.target.checked))} /> <b>{s.name}</b></label>
                  <span className="faint">{TRANSPORT_LABEL[s.transport] ?? s.transport}</span>
                  {sign && <span className={`badge ${sign.badge}`}>{sign.text}</span>}
                  {lr && <span className={`badge ${lr.badge}`} title={`At ${new Date(last!.at).toLocaleString()}`}>{lr.text}</span>}
                  <span className={`badge ${RISK_BADGE[sum.risk]}`}>{RISK_LABEL[sum.risk]}</span>
                </div>
                <span className="ma-line">{sum.text}{on && tools.length > 0 && off.length > 0 ? ` ${nOn} of ${tools.length} on.` : ""}</span>
                {you && <span className="warn">{you} Switch it on only for an agent you trust with that.</span>}
                {s.signIn === "needs_sign_in" && <span className="warn">Needs sign-in: a run leaves it out until you sign in in Settings → MCP servers.</span>}
                {s.missing.length > 0 && <span className="warn">No value saved for {s.missing.join(", ")}: a run leaves it out until you enter it in Settings → MCP servers.</span>}
                {on && tools.length > 0 && (
                  <McpToolList tools={tools} disabled={disabled} isOn={(name) => !off.includes(name)} onSwitch={(name, v) => onChange(switchTool(value, s.id, name, v))} />
                )}
                {on && !s.listed && <span className="hint">List its tools in Settings → MCP servers to switch them one by one; until then, all its tools are on.</span>}
              </li>
            );
          })}
        </ul>
      )}
    </Field>
  );
}
