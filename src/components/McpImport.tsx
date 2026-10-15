// Import from Claude Code's rows: the servers in each Claude Code's config file, each with a tick, the names of its lines
// only (never their values), Already in the list, and another name for a ticked one whose name is taken. Settings → MCP
// servers → Import from Claude Code (McpSettings) and step 3 of the first start's onboarding (GA-164) show the same rows
// with the same rules; each keeps the ticks itself.
import { nameProblem, serverWhere, TRANSPORT_LABEL } from "../lib/mcp";
import type { McpScan } from "../types";

/** Per server (its key): whether it is ticked, and the name it is imported under. */
export type ImportPicks = Record<string, { on: boolean; name: string }>;

/** None ticked, each under its own name. */
export const importPicks = (scan: McpScan): ImportPicks => Object.fromEntries(scan.servers.map((c) => [c.key, { on: false, name: c.name }]));

/** The ticked servers, in the scan's order. */
export const tickedServers = (scan: McpScan, picks: ImportPicks) => scan.servers.filter((c) => picks[c.key]?.on);

/** Why ticked server `key` can't be imported under its name (taken by a server in the list, `existing`, or by another
 *  ticked one); null when it can, or isn't ticked. */
export function importProblem(scan: McpScan, picks: ImportPicks, existing: string[], key: string): string | null {
  const p = picks[key];
  const others = tickedServers(scan, picks).filter((c) => c.key !== key).map((c) => picks[c.key].name.trim());
  return p?.on ? nameProblem(p.name, [...existing, ...others]) : null;
}

/** The rows, one per server in `scan`; `onPicks` gets the ticks and names after a change. */
export function ImportRows({ scan, existing, picks, onPicks }: {
  scan: McpScan; existing: string[]; picks: ImportPicks; onPicks: (picks: ImportPicks) => void;
}) {
  return (
    <ul>
      {scan.servers.map((c) => {
        const p = picks[c.key];
        const why = importProblem(scan, picks, existing, c.key);
        return (
          <li key={c.key}>
            <label className="check"><input type="checkbox" aria-label={`Import ${c.name} (${c.account}, ${c.scope} scope)`} checked={p.on}
              onChange={(e) => onPicks({ ...picks, [c.key]: { ...p, on: e.target.checked } })} /></label>
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
                  <input className="input mono" aria-label={`New name for ${c.name}`} value={p.name} onChange={(e) => onPicks({ ...picks, [c.key]: { ...p, name: e.target.value } })} />
                </span>
              )}
              {why && <span className="warn">{why}</span>}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
