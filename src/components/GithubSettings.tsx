// Settings → GitHub: whether Gizai can use GitHub (the GitHub CLI, the account it is logged in as, how pushes go),
// Check connection, and Log in with GitHub: gh's own login in the browser, whose one-time code and link show here.
// Gizai never stores a token or password: it uses gh's login and your SSH keys.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CircleCheck, CircleMinus, CircleX, Copy, ExternalLink, LogIn, PlugZap } from "lucide-react";
import { detectGh, githubCheck, githubLogin, githubLoginCancel, githubLoginWait, githubStatus } from "../api";
import { accountLine, canLogIn, ghLine, PUSH_OVER, pushOverHint, type Mark } from "../lib/github";
import type { ConnectionCheck, GithubLoginCode, GithubStatus, PushOver, Settings } from "../types";
import { Field, FormSection } from "./Form";

const MARKS = { ok: CircleCheck, failed: CircleX, skipped: CircleMinus };

function Line({ mark, children }: { mark: Mark; children: ReactNode }) {
  const Icon = MARKS[mark];
  return <div className={`gh-line ${mark}`}><Icon className="icon" /><span>{children}</span></div>;
}

/** `savedAt` changes after Settings are saved (a new gh path): the status is asked again. */
export function GithubSettings({ s, setS, save, say, savedAt }: {
  s: Settings; setS: (next: Settings) => void; save: (next: Settings) => Promise<void>; say: (ok: boolean, text: string) => void; savedAt: number;
}) {
  const [status, setStatus] = useState<GithubStatus | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [login, setLogin] = useState<GithubLoginCode | null>(null);
  const [starting, setStarting] = useState(false);
  const [loginErr, setLoginErr] = useState<string | null>(null);
  const [check, setCheck] = useState<ConnectionCheck | null>(null);
  const [checking, setChecking] = useState(false);
  const alive = useRef(true);
  const following = useRef(false);

  const refresh = async () => {
    const x = await githubStatus();
    if (alive.current) { setStatus(x); setLogin(x.login ?? null); }
    return x;
  };
  // gh waits for the code to be entered on GitHub; then the status shows the account
  const follow = async () => {
    if (following.current) return;
    following.current = true;
    try {
      const who = await githubLoginWait();
      if (alive.current) { setLoginErr(null); say(true, who ? `Logged in to GitHub as ${who}.` : "Logged in to GitHub."); }
    } catch (e) {
      if (alive.current) setLoginErr(String(e));
    } finally {
      following.current = false;
    }
    if (alive.current) { setLogin(null); setCheck(null); refresh().catch(() => {}); }
  };
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);
  useEffect(() => {
    refresh().then((x) => { if (x.login) follow(); }).catch((e) => alive.current && setLoginErr(String(e)));
  }, [savedAt]);

  const findGh = async () => {
    setDetecting(true);
    try {
      const p = await detectGh();
      if (p) { setS({ ...s, ghBin: p }); say(true, `Found the GitHub CLI at ${p}`); }
      else say(false, "The GitHub CLI (gh) was not found. Install it from cli.github.com, or type the path to the gh program.");
      refresh().catch(() => {});
    } catch (e) { say(false, String(e)); } finally { setDetecting(false); }
  };
  // saved at once, like Pause all agents (a save asks for the status again)
  const setPushOver = (p: PushOver) => {
    const next = { ...s, pushOver: p };
    setS(next);
    setCheck(null);
    save(next);
  };
  const logIn = async () => {
    setStarting(true); setLoginErr(null);
    try {
      setLogin(await githubLogin());
      follow();
    } catch (e) { setLoginErr(String(e)); } finally { setStarting(false); }
  };
  const runCheck = async () => {
    setChecking(true);
    try {
      const c = await githubCheck();
      if (alive.current) setCheck(c);
      refresh().catch(() => {});
    } catch (e) { say(false, String(e)); } finally { if (alive.current) setChecking(false); }
  };

  const gh = status ? ghLine(status) : null;
  const account = status ? accountLine(status) : null;
  const pushOver = s.pushOver ?? "ssh";
  const accountHint = status?.account ? "gh opens pull requests as this account."
    : status?.loginCommand ? <>Log in with GitHub runs gh's own login in your browser. In a terminal: <span className="mono">{status.loginCommand}</span></> : undefined;
  return (
    <FormSection title="GitHub" text="Open pull request pushes a card's branch to GitHub and opens its pull request with the GitHub CLI (gh). Every two minutes Gizai asks GitHub about the pull requests of cards in Review; a merge moves the card to Done. Gizai never stores a token or password: it uses gh's login and your SSH keys.">
      <Field label="GitHub CLI" htmlFor="s-gh" wide hint="Empty: Gizai finds gh when it needs it. Detect looks in your login shell and the usual install folders.">
        {gh ? <Line mark={gh.mark}>{gh.text}</Line> : <div className="gh-line muted">Checking…</div>}
        <div className="input-group"><input id="s-gh" className="input mono" value={s.ghBin ?? ""} onChange={(e) => setS({ ...s, ghBin: e.target.value })} placeholder="/usr/bin/gh" />
          <button className="btn" onClick={findGh} disabled={detecting}>{detecting ? "Looking…" : "Detect"}</button></div>
      </Field>
      <Field label="Account" wide hint={accountHint}>
        {account ? <Line mark={account.mark}>{account.text}</Line> : <div className="gh-line muted">Checking…</div>}
        {login ? (
          <div className="gh-login" role="status" aria-label="Log in with GitHub">
            <span>Enter this code on GitHub:</span>
            <span className="gh-code mono">{login.code}</span>
            <button className="btn sm" onClick={() => navigator.clipboard?.writeText(login.code).catch(() => {})}><Copy className="icon" />Copy</button>
            <button className="btn sm primary" onClick={() => openUrl(login.url).catch(() => {})}><ExternalLink className="icon" />Open GitHub</button>
            <button className="btn sm ghost" onClick={() => githubLoginCancel().catch(() => {})}>Cancel</button>
            <span className="muted">Waiting for the code at <span className="mono">{login.url}</span>. This updates once gh is logged in.</span>
          </div>
        ) : status && canLogIn(status) ? (
          <div><button className="btn" onClick={logIn} disabled={starting}><LogIn className="icon" />{starting ? "Starting…" : "Log in with GitHub"}</button></div>
        ) : null}
        {loginErr && <div className="gh-line failed" role="alert"><CircleX className="icon" /><span>{loginErr}</span></div>}
      </Field>
      <Field label="Push over" wide hint={pushOverHint(pushOver)}>
        <div className="radios" role="radiogroup" aria-label="Push over">
          {PUSH_OVER.map((o) => (
            <label key={o.value}><input type="radio" name="push-over" checked={pushOver === o.value} onChange={() => setPushOver(o.value)} /> {o.label}</label>
          ))}
        </div>
      </Field>
      <Field label="Connection" wide hint="Checks gh and its login, ssh to GitHub, and whether you can push to each project with a GitHub link. Nothing is pushed.">
        <div><button className="btn" onClick={runCheck} disabled={checking}><PlugZap className="icon" />{checking ? "Checking…" : "Check connection"}</button></div>
        {check && (
          <div className="gh-checks" aria-label="Connection checks">
            <div className={`gh-summary ${check.ok ? "ok" : "failed"}`}>{check.ok ? "Gizai can use GitHub." : "Something needs fixing: see what to do below."}</div>
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
            {!check.checks.some((c) => c.projectId) && <div className="gh-check skipped"><CircleMinus className="icon" /><span className="name">Projects</span><span>No project has a GitHub link yet.</span></div>}
          </div>
        )}
      </Field>
    </FormSection>
  );
}
