// Team templates on the Team page (GA-227, the backend is GA-226 and docs/team-template-format.md): the check the New team
// drawer shows before it makes a team from a template, and Export's question when the team has text that looks private.
import { useEffect, useRef, type KeyboardEvent } from "react";
import type { TemplatePlan, TemplateWarning } from "../types";
import { Avatar } from "./Avatar";
import { FormSection } from "./Form";

/** The paused line above a file's agents, word for word. */
export const PAUSED_LINE = "Agents from a file come in paused. Read their instructions and commands, then switch them on in their form.";

/** A column's kind in the words of Add column. */
const KIND_WORDS: Record<string, string> = {
  backlog: "Backlog", ready: "Waiting", in_progress: "Work", testing: "Testing", review: "Review", deploy: "Deploy", done: "Done", cancelled: "Cancelled",
};
export const kindWord = (kind: string) => KIND_WORDS[kind] ?? kind;

/** The file name Export offers: `<team name>.gizai-team.json`, with characters a file name can't have as "-". */
export const templateFileName = (team: string) => `${team.replace(/[<>:"/\\|?*]/g, "-").trim() || "Team"}.gizai-team.json`;

/** A path's last part, with / or \ between folders. */
export const baseName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

/** What the new team gets: its columns, its agents (with what to look at before switching them on), labels and notes. */
export function TemplateCheck({ plan, fromFile }: { plan: TemplatePlan; fromFile: boolean }) {
  return (
    <>
      <FormSection title="Columns" text="In board order, as the new team gets them.">
        <div className="tt-wide tt-scroll">
          <table className="tt-table" aria-label="Columns">
            <thead><tr><th>Column</th><th>Kind</th><th>Auto or Manual</th><th>Next column</th><th>Agents</th></tr></thead>
            <tbody>
              {plan.columns.map((c) => (
                <tr key={c.name}>
                  <td><b>{c.name}</b></td><td>{kindWord(c.kind)}</td><td>{c.auto ? "Auto" : "Manual"}</td>
                  <td>{c.next ?? <span className="faint">None</span>}</td>
                  <td>{c.agents.length ? c.agents.join(", ") : <span className="faint">None</span>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </FormSection>
      <FormSection title="Agents" text={plan.agents.length === 1 ? "1 agent." : `${plan.agents.length} agents.`}>
        <div className="tt-wide">
          {fromFile && <p className="tt-paused">{PAUSED_LINE}</p>}
          {plan.agents.length === 0 ? <span className="faint">No agents.</span> : (
            <ul className="tt-agents">
              {plan.agents.map((a) => (
                <li key={a.name} className="tt-agent">
                  <div className="h">
                    <Avatar name={a.name} kind="agent" size="sm" role={a.role} />
                    <b>{a.name}</b>
                    {a.fileName !== a.name && <span className="faint">{a.fileName} in the file</span>}
                    {!a.active && <span className="badge">Paused</span>}
                  </div>
                  <dl className="tt-props">
                    <dt>Role</dt><dd>{a.role}</dd>
                    <dt>CLI</dt><dd>{a.cliName}</dd>
                    <dt>Model</dt><dd>{a.model ?? <span className="faint">The CLI's default</span>}</dd>
                    <dt>Effort</dt><dd>{a.effort ?? <span className="faint">The CLI's default</span>}</dd>
                    <dt>Permission mode</dt><dd>{a.permissionMode || <span className="faint">None</span>}</dd>
                  </dl>
                  {a.extraTools.length > 0 && (
                    <div className="tt-extra" role="group" aria-label={`${a.name}: commands beyond its role's usual ones`}>
                      <h4>Commands beyond its role's usual ones</h4>
                      <ul>{a.extraTools.map((t) => <li key={t} className="mono">{t}</li>)}</ul>
                    </div>
                  )}
                  {a.missingServers.map((s) => <p key={s} className="tt-missing">Needs MCP server {s}: add it in Settings → MCP to switch it on.</p>)}
                </li>
              ))}
            </ul>
          )}
        </div>
      </FormSection>
      <FormSection title="Labels and notes" text="Labels belong to every team: the new ones are added for all of them.">
        <div className="tt-wide">
          <p className="tt-labels">{plan.labelsAdded.length ? <>Labels added: {plan.labelsAdded.join(", ")}</> : <span className="faint">No labels added.</span>}</p>
          {plan.notes.length > 0 && (
            <ul className="tt-notes" aria-label="Notes">{plan.notes.map((n, i) => <li key={i}>{n}</li>)}</ul>
          )}
        </div>
      </FormSection>
    </>
  );
}

/** A warning's field in plain words. */
const FIELD_WORDS: Record<string, string> = {
  name: "Name", description_md: "Description", branches: "Branches", labels: "Labels", columns: "Column names", title: "Title", model: "Model",
  instructions_md: "Instructions", allowed_tools: "Allowed commands", fetch_domains: "Fetch domains", builtin_tools: "Built-in tools",
  mcp_servers: "MCP servers",
};
const KIND_FOUND: Record<TemplateWarning["kind"], string> = {
  email: "an email address", path: "a local path", url: "a URL with a query or a password", secret: "what looks like a key or token",
};

/** Export's question when the template has text that looks private: each place, then Export anyway or Cancel. Escape and a
 *  click beside it cancel; focus starts on Cancel and stays in the dialog. */
export function ExportWarnings({ warnings, onExport, onCancel }: { warnings: TemplateWarning[]; onExport: () => void; onCancel: () => void }) {
  const box = useRef<HTMLDivElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => { cancel.current?.focus(); }, []);
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onCancel(); return; }
    if (e.key !== "Tab" || !box.current) return;
    const all = [...box.current.querySelectorAll<HTMLElement>("button, [tabindex='0']")];
    const first = all[0], last = all[all.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus(); }
  };
  return (
    <div className="ask-scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) onCancel(); }}>
      <div ref={box} className="ask tt-warn" role="alertdialog" aria-modal="true" aria-labelledby="tt-warn-q" aria-describedby="tt-warn-why" onKeyDown={onKey}>
        <h2 id="tt-warn-q">This may share private details</h2>
        <p id="tt-warn-why">The template keeps this text as it is. Change it in the team first, or export anyway.</p>
        <ul className="tt-warn-list" tabIndex={0} aria-label="What looks private">
          {warnings.map((w, i) => (
            <li key={i}>
              <span>{FIELD_WORDS[w.field] ?? w.field} of <b>{w.agent ?? "the team"}</b>: {KIND_FOUND[w.kind] ?? w.kind}</span>
              <code className="mono">{w.excerpt}</code>
            </li>
          ))}
        </ul>
        <div className="ask-actions">
          <button ref={cancel} className="btn ghost" onClick={onCancel}>Cancel</button>
          <button className="btn primary" onClick={onExport}>Export anyway</button>
        </div>
      </div>
    </div>
  );
}
