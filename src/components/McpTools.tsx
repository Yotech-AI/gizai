import { RISK_BADGE, RISK_LABEL } from "../lib/mcp";
import type { McpToolView } from "../types";

/** One MCP tool: its name and title, one line on what it may do with its risk, and (opened) its description, parameters
 * and what the server said about it. With `onSwitch`, a switch for the agent. */
export function McpTool({ t, on, onSwitch, disabled }: { t: McpToolView; on?: boolean; onSwitch?: (on: boolean) => void; disabled?: boolean }) {
  return (
    <li className="mcp-tool">
      <div className="mt-line">
        {onSwitch && (
          <input type="checkbox" className="mt-switch" aria-label={`Use ${t.name}`} checked={!!on} disabled={disabled} onChange={(e) => onSwitch(e.target.checked)} />
        )}
        <span className="mt-name"><span className="mono">{t.name}</span>{t.title && <span className="faint"> · {t.title}</span>}</span>
        <span className={`badge ${RISK_BADGE[t.risk] ?? "outline"}`}>{RISK_LABEL[t.risk] ?? t.risk}</span>
      </div>
      <span className="mt-summary">{t.summary}</span>
      <details className="mt-more">
        <summary>What it does</summary>
        {t.description ? <p className="mt-desc">{t.description}</p> : <p className="faint">The server gives no description.</p>}
        {t.params.length > 0 ? (
          <ul className="mt-params" aria-label={`Parameters of ${t.name}`}>
            {t.params.map((p) => (
              <li key={p.name}><span className="mono">{p.name}</span> <span className="faint">({p.ty}, {p.required ? "required" : "optional"})</span>
                {p.description && <> · {p.description}</>}</li>
            ))}
          </ul>
        ) : <p className="faint">No parameters.</p>}
        {t.notes.map((n) => <p key={n} className="faint">{n}</p>)}
      </details>
    </li>
  );
}

/** A server's tools, each with what it does and its risk; with `isOn`/`onSwitch`, a switch per tool. */
export function McpToolList({ tools, isOn, onSwitch, disabled }: {
  tools: McpToolView[]; isOn?: (name: string) => boolean; onSwitch?: (name: string, on: boolean) => void; disabled?: boolean;
}) {
  if (tools.length === 0) return <span className="faint">This server lists no tools.</span>;
  return (
    <ul className="mcp-tools">
      {tools.map((t) => (
        <McpTool key={t.name} t={t} on={isOn?.(t.name)} onSwitch={onSwitch ? (on) => onSwitch(t.name, on) : undefined} disabled={disabled} />
      ))}
    </ul>
  );
}
