// "Run this for me" (GA-31): the commands an agent asks you to run because it may not run them itself (sudo, an install,
// a command its list refuses), each exactly as it wrote it with a copy button, and `action`: Done, continue, which
// continues its run. On the card's Run panel and in the Inbox.
import { useState, type ReactNode } from "react";
import { Check, Copy } from "lucide-react";

export function RunForMe({ commands, agent, action }: { commands: string[]; agent?: string | null; action?: ReactNode }) {
  const [copied, setCopied] = useState<number | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const copy = async (i: number) => {
    try {
      await navigator.clipboard.writeText(commands[i]);
      setErr(null);
      setCopied(i);
      setTimeout(() => setCopied((c) => (c === i ? null : c)), 2000);
    } catch { setErr("Couldn't copy to the clipboard"); }
  };
  const one = commands.length === 1;
  const who = agent || "The agent";
  return (
    <div className="run-for-me" role="group" aria-label="Run this for me">
      <div className="rfm-head">{who} asks you to run {one ? "this command" : "these commands"}:</div>
      <ul>
        {commands.map((c, i) => (
          <li key={i}>
            <code className="mono">{c}</code>
            <button className="btn sm ghost" aria-label={`Copy ${c}`} title="Copy" onClick={() => copy(i)}>
              {copied === i ? <><Check className="icon" />Copied</> : <><Copy className="icon" />Copy</>}</button>
          </li>
        ))}
      </ul>
      <div className="rfm-foot">
        <span className="grow">Run {one ? "it" : "them"} in a terminal, then press Done, continue: {agent || "the agent"} checks that {one ? "it" : "they"} worked
          and carries on.</span>
        {action}
      </div>
      {err && <div role="alert" style={{ color: "var(--danger)", marginTop: 6 }}>{err}</div>}
    </div>
  );
}
