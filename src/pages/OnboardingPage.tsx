// The first start's onboarding (GA-164; GA-168's design: design/onboarding/README.md, with every size and text). While
// onboarding_pending() is true, App shows this instead of the app: the whole window, without the sidebar. Four steps: you
// and your team, coding CLIs, MCP servers (optional) and ready. Next saves a step and goes on (step 2 adds the ticked CLIs to
// Settings → Coding CLIs, step 3 imports the ticked MCP servers), Back keeps what you filled in, Change and a done step in
// the indicator open an earlier step whose Next comes back, and Skip setup opens the app as today. Step 4's button saves the
// names and the agents' CLI and starts the Team Lead's welcome chat (finish_onboarding); App then opens that chat, or the
// board with why there is none. Enter goes to Next; an error from a command shows under its step, and the step stays.
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from "react";
import { Check, CircleAlert, ExternalLink, Info, ListTodo, MessagesSquare, Plug, RefreshCw, TriangleAlert } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { appInfo, findClis, finishOnboarding, getTeam, importMcpServers, listClis, listMcpServers, listUsers, saveClis, scanClaudeCodeMcp, skipOnboarding } from "../api";
import { Avatar } from "../components/Avatar";
import { Band } from "../components/Band";
import { ImportRows, importPicks, importProblem, tickedServers, type ImportPicks } from "../components/McpImport";
import { CLAUDE_CODE } from "../lib/clis";
import { afterSave, CLAUDE_CODE_DOCS, claudeCodeFound, cliRows, clisToSave, defaultAgentsCli, isTicked, noChatNote, notFoundLine, STEPS, tickedLine,
  WELCOME, type CliRow } from "../lib/onboarding";
import type { Route } from "../router";
import type { Cli, CliStatus, McpScan } from "../types";
import logo from "../assets/gizai-logo.svg?raw";
import "../styles/onboarding.css";

const TITLES = ["Welcome to GizAi", "Coding CLIs", "MCP servers", "You're all set"];
const LEADS = [
  "Four short steps, then your Team Lead says hello. First, you and your team.",
  "Your agents work through the coding CLIs on this computer, with your own logins.",
  "Bring the MCP servers your Claude Code has. Each stays off until you switch it on for an agent.",
  "Here's what you set up. Your Team Lead is ready to say hello.",
];
const NO_CLAUDE_CODE = "Claude Code isn't installed yet. The Team Lead's chat needs it.";

/** What a button started and waits for: the other buttons wait too. */
type Busy = "next" | "look" | "skip" | "finish";

/** `onDone`: the onboarding is finished or skipped; App opens `to` (null: the page it would open anyway) with `note` above it. */
export function OnboardingPage({ onDone }: { onDone: (to: Route | null, note?: string) => void }) {
  const [step, setStep] = useState(0);
  /** The step Next goes back to, once Change or a done step in the indicator has opened an earlier one. */
  const [back, setBack] = useState<number | null>(null);
  const [busy, setBusy] = useState<Busy | null>(null);
  /** An error from a command, under the step; the step stays. */
  const [err, setErr] = useState<string | null>(null);
  // 1 · you and your team (the names are saved when you finish), and the team's agents
  const [you, setYou] = useState("");
  const [team, setTeam] = useState("");
  const [nameErr, setNameErr] = useState<{ you?: string; team?: string }>({});
  const [agents, setAgents] = useState<string[]>([]);
  // 2 · coding CLIs: listed in Settings and found; the ids listed before the onboarding; the rows you unticked; your pick
  const [clis, setClis] = useState<{ listed: CliStatus[]; found: Cli[] } | null>(null);
  const [clisErr, setClisErr] = useState<string | null>(null);
  const [before, setBefore] = useState<string[] | null>(null);
  const [unticked, setUnticked] = useState<string[]>([]);
  const [pick, setPick] = useState<string | null>(null);
  // 3 · MCP servers: your Claude Code's, the names Settings → MCP servers has, your ticks, and the names imported so far
  const [mcp, setMcp] = useState<{ scan: McpScan; existing: string[] } | null>(null);
  const [mcpErr, setMcpErr] = useState<string | null>(null);
  const [picks, setPicks] = useState<ImportPicks>({});
  const [imported, setImported] = useState<string[]>([]);

  const heading = useRef<HTMLHeadingElement>(null);
  const youField = useRef<HTMLInputElement>(null);
  const teamField = useRef<HTMLInputElement>(null);
  const body = useRef<HTMLElement>(null);
  const shown = useRef(step);
  const readingMcp = useRef(false);

  /** Step 2's CLIs: the ones found first (find_clis reads your login shell's PATH again, so a CLI installed a moment ago
   *  is found), then the list, with whether Claude Code is found now. */
  const loadClis = async () => {
    const found = await findClis();
    const listed = await listClis();
    setBefore((b) => b ?? listed.filter((c) => c.id !== CLAUDE_CODE).map((c) => c.id));
    setClis({ listed, found });
    setClisErr(null);
  };

  useEffect(() => {
    // your person's name, as the seed made it (your login name)
    appInfo().then(async (info) => {
      const me = (await listUsers()).find((p) => p.id === info.you_id);
      if (me) setYou((y) => y || me.name);
    }).catch((e) => setErr(String(e)));
    getTeam().then((t) => setAgents(t.members.filter((m) => m.kind === "agent").map((m) => m.name))).catch(() => {});
    loadClis().catch((e) => setClisErr(String(e)));
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Between steps nothing slides: the focus goes to the new step's heading. At the start the name is there, so the cursor
  // waits in Team name.
  useEffect(() => {
    if (shown.current === step) { teamField.current?.focus(); return; }
    shown.current = step;
    heading.current?.focus();
  }, [step]);

  // Step 3 reads Claude Code's MCP servers when it opens, and again after an import (those are in the list then).
  useEffect(() => {
    if (step !== 2 || mcp || readingMcp.current) return;
    readingMcp.current = true;
    Promise.all([scanClaudeCodeMcp(), listMcpServers()])
      .then(([scan, list]) => { setMcp({ scan, existing: list.map((s) => s.name) }); setPicks(importPicks(scan)); setMcpErr(null); })
      .catch((e) => setMcpErr(String(e)))
      .finally(() => { readingMcp.current = false; });
  }, [step, mcp]);

  const claude = clis ? claudeCodeFound(clis.listed) : true;
  const rows = clis ? cliRows(clis.listed, clis.found, before ?? []) : [];
  const ticked = rows.filter((r) => isTicked(r, unticked));
  const agentsKey = pick && ticked.some((r) => r.key === pick) ? pick : defaultAgentsCli(rows, unticked);
  // What finishing gives the agents: the pick once Next has added it to Settings → Coding CLIs (it has an id then)
  const saved = ticked.filter((r) => !!r.cli.id);
  const agentsRow = saved.find((r) => r.key === agentsKey) ?? saved.find((r) => r.builtIn) ?? saved[0] ?? null;

  const go = (to: number) => { setErr(null); setStep(to); };
  /** On from step `s`, once it is saved: to the step Change came from, else the next one. */
  const onFrom = (s: number) => { const to = back !== null && back > s ? back : s + 1; setBack(null); go(to); };
  /** Change, or a done step in the indicator: opens an earlier step, whose Next comes back here. */
  const open = (to: number) => { setBack((b) => Math.max(b ?? step, step)); go(to); };
  const goBack = () => { setBack(null); go(step - 1); };

  /** Runs `f` as `what`, the buttons waiting; its error shows under the step. Resolves with whether it worked. */
  const run = async (what: Busy, f: () => Promise<unknown>): Promise<boolean> => {
    setBusy(what);
    setErr(null);
    try { await f(); return true; } catch (e) { setErr(String(e)); return false; } finally { setBusy(null); }
  };

  const nextFromNames = () => {
    const e = { you: you.trim() ? undefined : "Type your name to go on.", team: team.trim() ? undefined : "Type a name for your team to go on." };
    setNameErr(e);
    if (e.you) youField.current?.focus();
    else if (e.team) teamField.current?.focus();
    else onFrom(0);
  };
  const nextFromClis = async () => {
    if (!clis) { await run("look", loadClis); return; } // nothing found yet: look again, and the step stays
    const save = clisToSave(clis.listed, clis.found, before ?? [], unticked);
    if (save && !(await run("next", async () => { const listed = await saveClis(save); setClis((c) => c && afterSave(c, listed)); }))) return;
    onFrom(1);
  };
  const nextFromMcp = async () => {
    if (mcp) {
      const chosen = tickedServers(mcp.scan, picks);
      const clash = chosen.find((c) => importProblem(mcp.scan, picks, mcp.existing, c.key));
      if (clash) { // its row asks for another name: the cursor goes there
        [...(body.current?.querySelectorAll("input") ?? [])].find((i) => i.getAttribute("aria-label") === `New name for ${clash.name}`)?.focus();
        return;
      }
      if (chosen.length > 0) {
        const ok = await run("next", async () => {
          const added = await importMcpServers(chosen.map((c) => ({ key: c.key, name: picks[c.key].name.trim() })));
          setImported((i) => [...i, ...added.map((s) => s.name)]);
          setMcp(null);
        });
        if (!ok) { // some may have come in before it failed: their names are taken now
          listMcpServers().then((l) => setMcp((m) => m && { ...m, existing: l.map((s) => s.name) })).catch(() => {});
          return;
        }
      }
    }
    onFrom(2);
  };
  const finish = async () => {
    setBusy("finish");
    setErr(null);
    try {
      const chat = await finishOnboarding({ youName: you.trim(), teamName: team.trim(), agentsCli: agentsRow?.cli.id ?? null });
      onDone(chat ? { page: "chat", id: chat } : { page: "board" }, chat ? undefined : noChatNote(claude));
    } catch (e) { setErr(String(e)); setBusy(null); }
  };
  const skip = async () => {
    setBusy("skip");
    setErr(null);
    try { await skipOnboarding(); onDone(null); } catch (e) { setErr(String(e)); setBusy(null); }
  };
  const next = () => {
    if (busy) return;
    if (step === 0) nextFromNames();
    else if (step === 1) void nextFromClis();
    else if (step === 2) void nextFromMcp();
    else void finish();
  };
  // Enter goes to Next, but on what does its own thing with it (a button, a link, a select)
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Enter" || e.repeat || e.shiftKey || e.ctrlKey || e.metaKey || e.altKey || e.nativeEvent.isComposing) return;
    if ((e.target as HTMLElement).closest("button, a, select, textarea")) return;
    e.preventDefault();
    next();
  };

  const stepErr = err ?? (step === 1 && !clis ? clisErr : step === 2 && !mcp ? mcpErr : null);
  return (
    <div className="onb" onKeyDown={onKey}>
      <Band />
      <header className="onb-top">
        <span className="onb-logo" dangerouslySetInnerHTML={{ __html: logo }} />
        <nav aria-label="Setup steps">
          <ol className="onb-steps">
            {STEPS.map((name, i) => (
              <li key={name}>
                {i < step ? (
                  <button type="button" className="onb-step done" aria-label={`Back to step ${i + 1}: ${name}`} disabled={!!busy} onClick={() => open(i)}>
                    <span className="num"><Check className="icon" /></span><span className="nm">{name}</span>
                  </button>
                ) : (
                  <span className={`onb-step${i === step ? " on" : ""}`} aria-current={i === step ? "step" : undefined}>
                    <span className="num">{i + 1}</span><span className="nm">{name}</span>
                  </span>
                )}
              </li>
            ))}
          </ol>
        </nav>
        <button type="button" className="btn ghost onb-skip" title="Open the board. You can set things up later in Settings." disabled={!!busy} onClick={() => void skip()}>
          {busy === "skip" ? "Skipping…" : "Skip setup"}
        </button>
      </header>
      <div className="onb-scroll">
        <main className="onb-body" ref={body} aria-labelledby="onb-heading">
          <p className="onb-eyebrow" id="onb-eyebrow">Step {step + 1} of 4</p>
          <h1 id="onb-heading" ref={heading} tabIndex={-1} aria-describedby="onb-eyebrow">
            {TITLES[step]}{step === 2 && <span className="badge outline">Optional</span>}
          </h1>
          <p className="onb-lead">{step === 3 && !claude ? "Here's what you set up." : LEADS[step]}</p>
          <div className={`onb-parts${step === 0 ? " first" : ""}`}>
            {step === 0 ? (
              <YouAndTeam you={you} team={team} errors={nameErr} agents={agents} youRef={youField} teamRef={teamField}
                onYou={(v) => { setYou(v); if (v.trim()) setNameErr((x) => ({ ...x, you: undefined })); }}
                onTeam={(v) => { setTeam(v); if (v.trim()) setNameErr((x) => ({ ...x, team: undefined })); }} />
            ) : step === 1 ? (
              <CodingClis loaded={!!clis} failed={!!clisErr} rows={rows} unticked={unticked} claude={claude} agentsKey={agentsKey} looking={busy === "look"}
                onTick={(key, on) => setUnticked((u) => (on ? u.filter((k) => k !== key) : [...u, key]))} onPick={setPick}
                onLook={() => void run("look", loadClis)} disabled={!!busy} />
            ) : step === 2 ? (
              <McpServers mcp={mcp} failed={!!mcpErr} picks={picks} onPicks={setPicks} />
            ) : (
              <Ready you={you.trim()} team={team.trim()} agentsCli={agentsRow?.cli.name ?? (clis ? "None found" : "")} imported={imported} claude={claude}
                disabled={!!busy} onChange={open} />
            )}
          </div>
        </main>
      </div>
      {stepErr && <div className="onb-error"><Notice kind="bad">{stepErr}</Notice></div>}
      <div className="onb-actions">
        <button type="button" className="btn" disabled={step === 0 || !!busy} onClick={goBack}>Back</button>
        {step < 3 ? (
          <button type="button" className="btn primary" disabled={!!busy} onClick={next}>
            {busy === "next" ? (step === 1 ? "Saving…" : "Importing…") : "Next"}
          </button>
        ) : (
          <button type="button" className="btn primary" disabled={!!busy} onClick={next}>
            {claude ? <MessagesSquare className="icon" /> : <ListTodo className="icon" />}
            {busy === "finish" ? "Opening…" : claude ? "Open the Team Lead chat" : "Open the board"}
          </button>
        )}
      </div>
    </div>
  );
}

/** A warning (Claude Code isn't installed) or an error from a command; its words are `text`, as GA-168's notes ask. */
function Notice({ kind, children, actions }: { kind: "warn" | "bad"; children: ReactNode; actions?: ReactNode }) {
  const Icon = kind === "bad" ? CircleAlert : TriangleAlert;
  return (
    <div className={`onb-notice ${kind}`} role={kind === "bad" ? "alert" : undefined}>
      <span className="ni"><Icon className="icon" /></span>
      <div className="nt"><p>{children}</p>{actions && <div className="na">{actions}</div>}</div>
    </div>
  );
}

/** Step 1: your name (the seeded one, filled in) and the team's, and the five agents the team starts with. */
function YouAndTeam({ you, team, errors, agents, youRef, teamRef, onYou, onTeam }: {
  you: string; team: string; errors: { you?: string; team?: string }; agents: string[];
  youRef: RefObject<HTMLInputElement | null>; teamRef: RefObject<HTMLInputElement | null>; onYou: (v: string) => void; onTeam: (v: string) => void;
}) {
  return (
    <>
      <div className="fields">
        <div className="field">
          <label htmlFor="onb-you">Your name</label>
          <input id="onb-you" ref={youRef} className="input" value={you} spellCheck={false} autoComplete="off" aria-invalid={errors.you ? true : undefined}
            aria-describedby={errors.you ? "onb-you-error" : "onb-you-hint"} onChange={(e) => onYou(e.target.value)} />
          {errors.you ? <span className="error" id="onb-you-error">{errors.you}</span>
            : <span className="hint" id="onb-you-hint">From your login name. The Team Lead calls you by it.</span>}
        </div>
        <div className="field">
          <label htmlFor="onb-team">Team name</label>
          <input id="onb-team" ref={teamRef} className="input" value={team} placeholder="My team" spellCheck={false} autoComplete="off"
            aria-invalid={errors.team ? true : undefined} aria-describedby={errors.team ? "onb-team-error" : "onb-team-hint"} onChange={(e) => onTeam(e.target.value)} />
          {errors.team ? <span className="error" id="onb-team-error">{errors.team}</span>
            : <span className="hint" id="onb-team-hint">Your agents work in this team.</span>}
        </div>
      </div>
      {agents.length > 0 && (
        <section className="onb-part" aria-labelledby="onb-agents-head">
          <h2 className="onb-head" id="onb-agents-head">{agents.length === 5 ? "Your team starts with five agents" : "Your team's agents"}</h2>
          <ul className="onb-chips">
            {agents.map((name) => <li key={name} className="agent-chip"><Avatar name={name} kind="agent" size="sm" /><span className="nm">{name}</span></li>)}
          </ul>
        </section>
      )}
    </>
  );
}

/** Step 2: the CLIs found on this computer, ticked to add to Settings → Coding CLIs, and the one the agents run on. Without
 *  Claude Code, a warning says the Team Lead's chat needs it, with how to install it and Look again. */
function CodingClis({ loaded, failed, rows, unticked, claude, agentsKey, looking, disabled, onTick, onPick, onLook }: {
  loaded: boolean; failed: boolean; rows: CliRow[]; unticked: string[]; claude: boolean; agentsKey: string | null; looking: boolean; disabled: boolean;
  onTick: (key: string, on: boolean) => void; onPick: (key: string) => void; onLook: () => void;
}) {
  const lookAgain = (cls: string) => (
    <button type="button" className={cls} disabled={disabled} onClick={onLook}><RefreshCw className="icon" />{looking ? "Looking…" : "Look again"}</button>
  );
  if (!loaded) {
    return failed ? <div className="onb-head"><span>Found on this computer</span>{lookAgain("btn ghost sm")}</div>
      : <p className="faint" role="status">Looking for coding CLIs on this computer…</p>;
  }
  const ticked = rows.filter((r) => isTicked(r, unticked));
  return (
    <>
      {!claude && (
        <Notice kind="warn" actions={<>
          <a className="link-btn onb-link" href={CLAUDE_CODE_DOCS} onClick={(e) => { e.preventDefault(); openUrl(CLAUDE_CODE_DOCS).catch(() => {}); }}>
            How to install Claude Code<ExternalLink className="icon" /></a>
          {lookAgain("btn sm")}
        </>}>{NO_CLAUDE_CODE}</Notice>
      )}
      <section className="onb-part" aria-labelledby="onb-found">
        <div className="onb-head"><h2 id="onb-found">Found on this computer</h2>{claude && lookAgain("btn ghost sm")}</div>
        {rows.length === 0 ? <div className="empty">No coding CLIs found on this computer.</div> : (
          <ul className="panel onb-rows onb-clis">
            {rows.map((r) => (
              <li key={r.key}>
                <label className="check">
                  <input type="checkbox" checked={isTicked(r, unticked)} disabled={r.builtIn || r.kept} onChange={(e) => onTick(r.key, e.target.checked)} />
                  <span className="cli-name"><span className="nm">{r.cli.name}</span>{(r.builtIn || r.kept) && <span className="faint">· {r.builtIn ? "Built in" : "Already added"}</span>}</span>
                </label>
                <span className="prog mono" title={r.program}>{r.program}</span>
                {r.problem && <span className="warn">{r.problem}</span>}
              </li>
            ))}
          </ul>
        )}
        <p className="onb-caption">{notFoundLine(rows)}</p>
      </section>
      {ticked.length > 0 && (
        <div className="fields">
          <div className="field">
            <label htmlFor="onb-agents-cli">Your agents build and test with</label>
            <select id="onb-agents-cli" className="select" value={agentsKey ?? ""} aria-describedby="onb-agents-cli-hint" onChange={(e) => onPick(e.target.value)}>
              {ticked.map((r) => <option key={r.key} value={r.key}>{r.cli.name}</option>)}
            </select>
            <span className="hint" id="onb-agents-cli-hint">The Backend, Frontend, QA and DevOps agents run on it. Each can change it in its settings.</span>
          </div>
        </div>
      )}
      {claude && <p className="onb-info"><Info className="icon" />The Team Lead's chat needs Claude Code, whichever CLI your agents use.</p>}
    </>
  );
}

/** Step 3: your Claude Code's MCP servers, with Settings → MCP servers → Import from Claude Code's rows and rules; none ticked. */
function McpServers({ mcp, failed, picks, onPicks }: {
  mcp: { scan: McpScan; existing: string[] } | null; failed: boolean; picks: ImportPicks; onPicks: (p: ImportPicks) => void;
}) {
  if (!mcp) return failed ? null : <p className="faint" role="status">Looking for the MCP servers in your Claude Code…</p>;
  const { scan, existing } = mcp;
  const problems = scan.problems.map((p) => <p key={p} className="onb-caption warn">{p}</p>);
  if (scan.servers.length === 0) {
    return (
      <>
        <div className="empty"><Plug className="icon" /><span><b>Your Claude Code has no MCP servers yet.</b> You can add them later in Settings → MCP servers.</span></div>
        {problems}
      </>
    );
  }
  return (
    <section className="onb-part onb-mcp" aria-labelledby="onb-mcp-head">
      <div className="onb-head"><h2 id="onb-mcp-head">In your Claude Code</h2><span className="right">{tickedLine(tickedServers(scan, picks).length)}</span></div>
      <div className="panel onb-rows onb-servers"><ImportRows scan={scan} existing={existing} picks={picks} onPicks={onPicks} /></div>
      <p className="onb-caption">Read only: GizAi doesn't change Claude Code's files or read its sign-ins. An import is a copy: later changes in Claude Code don't follow.</p>
      {problems}
    </section>
  );
}

/** Step 4: what you set up, each with Change, and the message the chat opens with (none without Claude Code). */
function Ready({ you, team, agentsCli, imported, claude, disabled, onChange }: {
  you: string; team: string; agentsCli: string; imported: string[]; claude: boolean; disabled: boolean; onChange: (step: number) => void;
}) {
  const row = (key: string, value: string, step: number, label: string) => (
    <div>
      <dt>{key}</dt>
      <dd>{value}</dd>
      <dd><button type="button" className="btn ghost sm" aria-label={label} disabled={disabled} onClick={() => onChange(step)}>Change</button></dd>
    </div>
  );
  return (
    <>
      <dl className="panel onb-summary">
        {row("Your name", you, 0, "Change your name")}
        {row("Team", team, 0, "Change the team's name")}
        {row("Your agents build and test with", agentsCli, 1, "Change the agents' coding CLI")}
        {row("MCP servers imported", imported.length ? imported.join(", ") : "None", 2, "Change the MCP servers")}
      </dl>
      {claude ? (
        <section className="onb-part" aria-labelledby="onb-message">
          <h2 className="onb-head" id="onb-message">The chat opens with this message from you:</h2>
          <blockquote className="onb-quote">{WELCOME}</blockquote>
        </section>
      ) : <Notice kind="warn">{NO_CLAUDE_CODE}</Notice>}
    </>
  );
}
