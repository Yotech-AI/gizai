// What Settings → GitHub and Settings → Bitbucket share: a status line with its mark, and the result list of Check connection.
import type { ReactNode } from "react";
import { CircleCheck, CircleMinus, CircleX } from "lucide-react";
import type { Mark } from "../lib/github";
import type { ConnectionCheck } from "../types";

const MARKS = { ok: CircleCheck, failed: CircleX, skipped: CircleMinus };

export function Line({ mark, children }: { mark: Mark; children: ReactNode }) {
  const Icon = MARKS[mark];
  return <div className={`gh-line ${mark}`}><Icon className="icon" /><span>{children}</span></div>;
}

/** Check connection's result: whether Gizai can use `host` (GitHub or Bitbucket), then each check with what to do. */
export function ConnectionChecks({ check, host, label = "Connection checks" }: { check: ConnectionCheck; host: string; label?: string }) {
  return (
    <div className="gh-checks" aria-label={label}>
      <div className={`gh-summary ${check.ok ? "ok" : "failed"}`}>{check.ok ? `Gizai can use ${host}.` : "Something needs fixing: see what to do below."}</div>
      {check.checks.map((c, i) => {
        const Icon = MARKS[c.result] ?? CircleMinus;
        return (
          <div key={`${c.name}-${i}`} className={`gh-check ${c.result}`}>
            <Icon className="icon" />
            <span className="name">{c.name}{c.repo && <span className="mono muted"> {c.repo}</span>}</span>
            <span>{c.text}</span>
            {c.fix && <span className="fix">{c.fix}</span>}
          </div>
        );
      })}
      {!check.checks.some((c) => c.projectId) && <div className="gh-check skipped"><CircleMinus className="icon" /><span className="name">Projects</span><span>{`No project has a ${host} link yet.`}</span></div>}
    </div>
  );
}
