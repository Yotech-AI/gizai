// Settings → Bitbucket: the account your API token belongs to (or what's wrong and what to do), your Atlassian email and API
// token with Save and Remove, and Check connection. The token goes to your keychain and is never shown again; pushes go over
// SSH with your own keys.
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CircleX, ExternalLink, PlugZap } from "lucide-react";
import { bitbucketCheck, bitbucketRemoveLogin, bitbucketSaveLogin, bitbucketStatus } from "../api";
import { accountLine, canRemove, saveBlocked, SSH_LINE, TOKEN_HOW_TO, TOKEN_PAGE, tokenPlaceholder } from "../lib/bitbucket";
import type { StatusLine } from "../lib/github";
import type { BitbucketStatus, ConnectionCheck } from "../types";
import { ConnectionChecks, Line } from "./ConnectionChecks";
import { Field, FormSection } from "./Form";

export function BitbucketSettings({ say }: { say: (ok: boolean, text: string) => void }) {
  const [status, setStatus] = useState<BitbucketStatus | null>(null);
  const [statusErr, setStatusErr] = useState<string | null>(null);
  const [email, setEmail] = useState("");
  const [token, setToken] = useState("");
  const [doing, setDoing] = useState<"save" | "remove" | null>(null);
  const [removing, setRemoving] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [check, setCheck] = useState<ConnectionCheck | null>(null);
  const [checking, setChecking] = useState(false);
  const [checkErr, setCheckErr] = useState<string | null>(null);
  const alive = useRef(true);
  // The email as you type it stays until it is saved; until then the saved one fills the field.
  const typed = useRef(false);

  const refresh = async () => {
    try {
      const x = await bitbucketStatus();
      if (alive.current) { setStatus(x); setStatusErr(null); if (!typed.current) setEmail(x.email ?? ""); }
      return x;
    } catch (e) {
      if (alive.current) setStatusErr(String(e));
      return null;
    }
  };
  useEffect(() => {
    alive.current = true;
    refresh();
    return () => { alive.current = false; };
  }, []);

  const blocked = saveBlocked(email, token);
  const save = async () => {
    if (blocked || doing) return;
    setDoing("save"); setErr(null); setRemoving(false);
    try {
      await bitbucketSaveLogin(email.trim(), token.trim());
      if (!alive.current) return;
      typed.current = false;
      setToken(""); setCheck(null);
      const x = await refresh();
      say(true, x?.account ? `Logged in to Bitbucket as ${x.account}.` : "Saved your Bitbucket email and API token.");
    } catch (e) { if (alive.current) setErr(String(e)); } finally { if (alive.current) setDoing(null); }
  };
  const remove = async () => {
    setDoing("remove"); setErr(null);
    try {
      await bitbucketRemoveLogin();
      if (!alive.current) return;
      typed.current = false;
      setRemoving(false); setToken(""); setCheck(null);
      await refresh();
      say(true, "Removed your Bitbucket email and API token.");
    } catch (e) { if (alive.current) setErr(String(e)); } finally { if (alive.current) setDoing(null); }
  };
  const runCheck = async () => {
    setChecking(true); setCheckErr(null);
    try {
      const c = await bitbucketCheck();
      if (alive.current) setCheck(c);
      refresh();
    } catch (e) { if (alive.current) setCheckErr(String(e)); } finally { if (alive.current) setChecking(false); }
  };
  const onEnter = (e: KeyboardEvent) => { if (e.key === "Enter") { e.preventDefault(); save(); } };

  const account: StatusLine | null = statusErr ? { mark: "failed", text: statusErr } : status ? accountLine(status) : null;
  return (
    <FormSection title="Bitbucket" text="Gizai pushes a card's branch to Bitbucket over SSH after every agent run, and Open pull request pushes it and opens its pull request through Bitbucket's API, with your Atlassian email and an API token. Every two minutes Gizai asks Bitbucket about the pull requests of cards in Review; a merge moves the card to Done. Your email and token stay in your keychain.">
      <Field label="Account" wide hint={status?.account && !statusErr ? "Gizai opens pull requests on Bitbucket as this account." : undefined}>
        {account ? <Line mark={account.mark}>{account.text}</Line> : <div className="gh-line muted">Checking…</div>}
      </Field>
      <Field label="Atlassian email" htmlFor="s-bb-email" hint="The email you log in to Atlassian with">
        <input id="s-bb-email" className="input" type="email" autoComplete="off" spellCheck={false} value={email} placeholder="you@example.com"
          onChange={(e) => { typed.current = true; setEmail(e.target.value); }} onKeyDown={onEnter} />
      </Field>
      <Field label="API token" htmlFor="s-bb-token" hint={status?.hasToken ? "A saved token is never shown again" : "Goes to your keychain, never to Gizai's database"}>
        <input id="s-bb-token" className="input mono" type="password" autoComplete="off" value={token} placeholder={tokenPlaceholder(status)}
          onChange={(e) => setToken(e.target.value)} onKeyDown={onEnter} />
      </Field>
      <div className="field wide">
        <div className="bb-actions">
          <button className="btn" onClick={save} disabled={!!blocked || !!doing} title={blocked ?? undefined}>{doing === "save" ? "Saving…" : "Save"}</button>
          {canRemove(status) && !removing && <button className="btn ghost" onClick={() => setRemoving(true)} disabled={!!doing}>Remove</button>}
        </div>
        {removing && (
          <div className="bb-confirm" role="alert">
            <span>Remove your Bitbucket email and API token? Gizai can't open or follow pull requests on Bitbucket until you save them again.</span>
            <span className="bb-actions"><button className="btn ghost sm" onClick={() => setRemoving(false)}>Cancel</button>
              <button className="btn sm" disabled={!!doing} onClick={remove}>{doing === "remove" ? "Removing…" : "Remove"}</button></span>
          </div>
        )}
        {err && <div className="gh-line failed" role="alert"><CircleX className="icon" /><span>{err}</span></div>}
        <span className="hint">{TOKEN_HOW_TO}{" "}
          <a href={TOKEN_PAGE} onClick={(e) => { e.preventDefault(); openUrl(TOKEN_PAGE).catch(() => {}); }}>Open API tokens<ExternalLink className="icon" style={{ width: 12, height: 12, marginLeft: 3 }} /></a></span>
        <span className="hint">{SSH_LINE}</span>
      </div>
      <Field label="Connection" wide hint="Checks your email and token, ssh to Bitbucket, and each project with a Bitbucket link. Nothing is pushed.">
        <div><button className="btn" onClick={runCheck} disabled={checking}><PlugZap className="icon" />{checking ? "Checking…" : "Check connection"}</button></div>
        {checkErr && <div className="gh-line failed" role="alert"><CircleX className="icon" /><span>{checkErr}</span></div>}
        {check && <ConnectionChecks check={check} host="Bitbucket" label="Bitbucket connection checks" />}
      </Field>
    </FormSection>
  );
}
