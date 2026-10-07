# Gizai: the paid offering (research, 2026-10-06)

Status: research only, no code. Written by a research sub-agent on 2026-10-06.
Legend: **[V]** = verified today (2026-10-06) on the linked primary page, unless another date is given. **[V 10-05]** = verified on 2026-10-05 in the earlier notes (`notes/F-agents-runs.md`, `notes/E-sqlite-sync.md`, `notes/H-build-packaging.md`). **[I]** = inferred, computed or my judgement.
Limit of this pass: the session's web-search budget was used up, so every fact comes from fetching known primary pages directly. Anytype, Logseq Sync pricing, Hetzner prices and Paddle's acceptable-use policy could not be checked (pages rendered empty or 404). They are left out or marked [I].

---

## 1. Summary (10 lines)

1. Keep the app MIT and fully usable offline with no account. Sell only hosted services; no licence check lives in the open code. [I]
2. Comparable tools charge **$4–10 per user per month for personal sync** (Obsidian, Joplin, AFFiNE, Standard Notes) and **$6–16 per seat for team workspaces** (Linear, Plane, AFFiNE, Joplin Teams, AppFlowy). [V]
3. AI coding tools charge $30–50 per seat (Zed, Cursor, Warp), but that price bundles AI inference or AI governance. Gizai cannot bundle Claude usage, so it should price like a sync or project tool, not like Cursor. [V prices; I conclusion]
4. Recommended tiers: **Free** (the app); **Personal €5/month or €48/year** (encrypted backup now, sync across your devices when M4 sync ships); **Team €10/user/month yearly or €12 monthly**, minimum 2 users (shared workspaces, team logins, roles, audit log). Prices exclude VAT. SSO and SCIM only on request. [I]
5. **No hosted agents with Gizai-paid inference.** Anthropic's terms say a product that runs Claude Code may not "pay for, resell, or intermediate Claude usage" for its users; each user signs in with their own key or plan. [V]
6. If remote agents are wanted later, start with "bring your own runner": a headless Gizai on the customer's own machine, relayed by the cloud. Hosted sandboxes only with the customer's own API key, metered. [I]
7. Sell through a **merchant of record**. Paddle (5% + $0.50, all-in) or Stripe Managed Payments (3.5% on top of Stripe's 1.5% + €0.25 and 0.7% Billing; NL sellers are eligible) are the two sound choices. Lemon Squeezy is moving into Stripe Managed Payments. [V]
8. Fixed fees eat 12–17% of a €5 monthly charge but only 7–9% of a €48 yearly one. Push yearly billing. [I, computed]
9. Entitlements: sign in through the system browser (OAuth + PKCE), keep the refresh token in the OS keychain, and let the server enforce the plan on every sync or backup call. Local data is never locked. [I; patterns V from Zed, Bitwarden, Plane]
10. Biggest risk: few people pay for local-first dev tools. Vibe Kanban closed in April 2026 with "thousands" of daily users, mostly free. Hosted Huly shut down in July 2026 because hosting was "no longer funded". [V]

---

## 2. How comparable tools charge (2026)

Prices are list prices in the vendor's currency, per user per month, as shown on the page. "yr" = billed yearly.

| Product | Licence / model | Free | Paid tiers and price | What is paid | Self-hosting |
|---|---|---|---|---|---|
| **Obsidian** | Closed-source app, free for all use incl. commercial since 2025-02-20 [V] | Whole app, no account [V] | Sync Standard **$4 yr / $5 monthly**; Sync Plus **$8 yr / $10**; Publish **$8 yr / $10 per site**; Catalyst $25 once; Commercial licence $50/user/yr, optional [V] | Sync (1 vault, 1 GB, 1 month history vs 10 vaults, 10–100 GB, 12 months), web publishing, supporter badges [V] | n/a (no server to host) |
| **Joplin** | Open-source app; paid Joplin Cloud [I for licence] | App + any sync target (own WebDAV, Nextcloud, etc.) [I] | Basic **€2.40 yr / €2.99**; Pro **€4.79 / €5.99**; Pro 100 GB €7.99 / €9.99; Teams **€6.69 / €7.99 per user, min 2**; Joplin Server Business €3.33/user (2–10 users) [V] | Hosted sync, storage, sharing, team admin, consolidated billing [V] | Joplin Server; a Business tier is sold for it [V] |
| **Standard Notes** | Open source, E2E [I for licence] | E2E sync on unlimited devices [V] | Productivity **$90/yr**; Professional **$120/yr** (100 GB files) [V] | Editors, revision history, file storage, account backup [V] | Not stated on the plans page [V] |
| **Logseq** | Open source [I] | App | No public paid sync. Open Collective backers ($5/month) and up get "exclusive access to Logseq DB"; a 2026-07-13 update invites sponsors to test the DB beta and real-time collaboration [V] | Early access, as a sponsor perk | n/a |
| **AFFiNE** | Editor MIT; backend under "AFFiNE Enterprise Edition" licence [V] | 10 GB cloud, 3 members, 3 devices, 7-day history [V] | Pro **$6.75/month yr**; Team **$10/seat yr**; AI add-on $8.9/month yr; Believer $499.99 lifetime [V] | Storage, members, history, admin roles, AI [V] | Self-hosted FOSS free (10 members/workspace); Self-hosted Team $10/seat, 10+ seats [V] |
| **AppFlowy** | AppFlowy-Cloud AGPL-3.0, now archived; active server code is commercial [V] | 1 owner, 100 MB, 7-day history [V] | Pro **$16 yr / $20 per member** [V] | Members, storage, AI, 90-day history [V] | Free self-host tier: "One User Seat (per instance)"; more seats paid [V] |
| **Huly** | EPL-2.0 [V] | Pricing page still lists free $0 and $19.99–$399.99 per workspace per month, priced by storage and video traffic, unlimited users [V] | — | Storage, video traffic | **Hosted Huly shut down** (planned for July 20, 2026; README says "hosting is no longer funded"); repo frozen 2026-09-25 [V] |
| **Linear** | Closed SaaS | 250 issues, 2 teams, unlimited members [V] | Basic **$10 yr**; Business **$16 yr**; Enterprise custom, yearly only [V] | Issue limit, teams, admin roles (Basic); private teams, insights, AI (Business); **SAML, SCIM, audit log** (Enterprise only) [V] | No |
| **Plane** | Community Edition AGPL-3.0; Commercial Edition closed [V] | Cloud free up to 12 users [V] | Pro **$6 yr / $8**; Business **$13 yr / $15**; Enterprise Grid custom [V] | Wiki, time tracking, integrations, AI credits, templates; LDAP and audit logs in Enterprise [V] | Commercial Edition includes 12 free seats per workspace; keys bound to workspace + machine signature + domain; online check against prime.plane.so; air-gapped edition uses a licence file [V] |
| **Zed** | Open-source editor | Personal free: unlimited use "with your API keys or external agents like Claude Agent, Codex CLI" [V] | Pro **$10/month** ($5 tokens incl., then API price +10%); Business **$30/seat** [V] | Hosted AI models, edit predictions, org AI policies, RBAC. "SSO, SAML, and SCIM are planned but not currently available" [V] | n/a. Zed does not bill for Claude usage; users run the official `claude` CLI or bring keys [V] |
| **Cursor** | Closed | Hobby free, limited agent requests [V] | Individual **$20**; Teams **$40/user**; Enterprise custom [V] | Inference, cloud agents; Teams adds central billing, privacy mode, **SAML/OIDC SSO**; Enterprise adds SCIM, **audit logs**, pooled usage [V] | No |
| **Warp** | Closed terminal | Free with BYO inference [V] | Build **$18 yr / $20**; Max $180 / $200; Business **$45 yr / $50 per user** [V] | Inference credits; Business adds SAML SSO, admin data controls; Enterprise self-hosted cloud agents [V] | Enterprise only |
| **Paperclip** | MIT, "self-hosted, no Paperclip account required" [V] | Everything [V] | **No prices.** "Paperclip Cloud" waitlist, "rolling out gradually"; the sign-up form asks if you are an individual, small team, larger org or agency [V] | Not announced | The app itself |
| **Vibe Kanban** | Apache-2.0 | Local app | Had subscriptions (refunds issued for invoices in the last 30 days) [V] | Cloud kanban, comments, orgs | **Company closed 2026-04-10**: "couldn't find a business model that we could get excited about"; "thousands of software engineers use Vibe Kanban every day"; most were free users. Cloud services ended 30 days later [V] |
| **Bitwarden** | Clients open source; server repo has AGPL and a separate Bitwarden licence [V] | Free personal vault [V] | Premium **$19.80/yr**; Families $47.88/yr (6 users); Teams **$4/user yr**; Enterprise **$6/user yr**; "Taxes not included" [V] | Attachments, reports, Send; Teams: sharing, event logs, SCIM; Enterprise: **SSO**, policies, **self-hosting** [V] | Free to self-host; paid features need a licence file tied to an installation ID; update within 60 days of renewal or the org is disabled; optional "billing sync" token [V] |
| **Tailscale** | Clients open source; control server closed [V via Headscale README] | Personal free, up to 6 users, unlimited devices [V] | Standard **$8**; Premium **$18**; Enterprise custom [V] | Unlimited users, SCIM, ACL groups, roles; Premium: network flow logs, log streaming [V] | Headscale (BSD-3, community, one tailnet) [V] |

Agency-tool reference points: Productive.io (agency management) $10–25 per user per month (the page's monthly/yearly labels look swapped) [V]; Moneybird (Dutch bookkeeping) €3–41 per month, VAT status not stated on the page [V].

### What the pattern says [I]

- **Personal tier = sync + storage + history.** $4–10 per month. Obsidian's two-step Standard/Plus split is the clearest model.
- **Team tier = shared workspace + roles + billing.** $6–16 per seat for project and notes tools.
- **SSO, SCIM and audit logs sit in the top tier almost everywhere** (Linear Enterprise, Cursor Teams/Enterprise, Plane Enterprise, Bitwarden Enterprise). Teams of 2–10 rarely buy that tier.
- **The $30–50 seats include AI inference** (Cursor, Warp) or AI governance (Zed Business). Gizai's agents run on the customer's own plans, so it can't charge for that.
- **Free hosting kills companies.** Vibe Kanban (cloud board, mostly free users) and hosted Huly (generous free tier) both stopped. Obsidian, Joplin and Bitwarden charge from the first synced byte.
- **Open-core servers** (Plane, AFFiNE, AppFlowy, Bitwarden) keep team or self-host features under a separate licence or behind a licence key. Gizai is MIT end to end, so its only lever is hosting.

---

## 3. What solo developers and small agencies pay for [I unless marked]

| Need | Who sells it | Typical price | Fit for Gizai |
|---|---|---|---|
| Multi-device sync | Obsidian Sync, Joplin Cloud, Standard Notes, AFFiNE Pro [V] | $4–10 per month | **Core paid feature** |
| Off-site backup and restore, version history | Inside sync plans (Obsidian history 1–12 months; Standard Notes account backup) [V] | Part of sync plans | **First thing to ship (M4a)**; bundle it, don't sell it alone |
| Team seats, shared workspace, roles | Linear, Plane, Joplin Teams, AFFiNE Team, Bitwarden Teams [V] | $4–16 per seat | **Team tier** |
| Client-facing sharing (read-only pages) | Obsidian Publish ($8–10 per site) [V]; agency tools offer client access | $8–10 per site | Later add-on: publish a project page or an HTML artifact by link |
| Hosted runners / cloud agents | Cursor, Warp, Codex cloud (inference included) [V for Cursor/Warp] | Inside $20–50 plans | **Not for Gizai** (vendor terms, cost, security; §5.1) |
| SSO / SCIM | Enterprise tiers [V] | Custom, or $16+ seats | On request only |
| Audit log | Bitwarden Teams event logs; Linear/Cursor/Plane enterprise [V] | Varies | Cheap for Gizai: the `changes` table already is one. Include a basic version in Team |

**EU price points.** Joplin quotes EUR. Bitwarden says "Taxes not included". The others show USD with no VAT note [V]. EU law requires consumer prices to show "the total price, including all taxes" [V Your Europe]. B2B tools in NL usually show prices excluding btw [I]. Recommendation: show EUR prices excluding VAT, labelled "excl. btw/VAT", and let the checkout add VAT for consumers. A merchant of record does this automatically [I].

---

## 4. Recommended Gizai tiers

All prices in EUR, **excluding VAT**. For a Dutch consumer, 21% is added at checkout (€5 → €6.05).

| Tier | Price | What's in it | Why this price |
|---|---|---|---|
| **Gizai (free, MIT)** | €0, no account | The whole desktop app: clients, projects, tasks, docs, files, artifacts, teams of agents, Team Lead, MCP, worktrees, the user's own CLIs and keys. **Local snapshots** to any folder, including a synced folder (a snapshot file, never the live DB). Export. | The decision is already made. Local snapshots for free keep trust and match what MIT lets anyone build anyway. |
| **Personal** | **€5 per month or €48 per year** (−20%) | 1 person, unlimited own devices. **M4a:** end-to-end encrypted off-site backup, automatic (on change, daily, on quit); 90-day snapshot history; restore on a clean machine; 10 GB of files. **M4b:** sync between your own devices at the same price. Early subscribers get sync at no extra cost. | Between Obsidian Sync Standard ($4–5) and Plus ($8–10); above Joplin Basic (€2.40–2.99). Below €5 a month, fixed fees take 15%+. Selling backup at launch validates demand before the 10–20 agent-days of sync. |
| **Team** | **€10 per user per month yearly, €12 monthly**, minimum 2 users | Everything in Personal per member. Shared workspaces (a Gizai org) synced between members. Team logins (email magic link or passkey; GitHub/Google sign-in). Roles: owner, admin, member, guest. Seat management. Activity/audit log kept 1 year, CSV export. Remove a member (stops their sync, revokes their tokens). 25 GB pooled per seat. **Bring-your-own-runner relay** later (§4.2). | Same band as Linear Basic ($10), AFFiNE Team ($10), Plane Pro ($6–8), Joplin Teams (€6.69–7.99). No inference is included, so it can't be priced like Cursor ($40). |
| **Business** (later, on request) | from €18–20 per user per month or a quote, min. 10 users | SAML/OIDC SSO, SCIM, longer audit retention, a custom DPA, priority support, perhaps a self-hosted server licence. | Every comparable tool gates these at the top. Build them only when a customer pays for them. |

Optional, Jeffrey's call:
- **Supporter licence**, €25–50 once, like Obsidian Catalyst ($25). It funds development without promising server costs forever. Don't sell lifetime cloud plans (AFFiNE Believer $499.99): server costs are open-ended. Note that Polar's policy disallows donations [V], so present it as a perk, such as early builds or a badge.
- **Publish add-on** (later): share a project page or an HTML artifact with a client by link, hosted on Gizai. Obsidian Publish costs $8–10 per site [V]. It could also be part of Team.

### 4.1 Rough economics [I, computed]

- Storage is cheap: Cloudflare R2 $0.015 per GB-month with free egress [V 10-05]; Backblaze B2 $6.95 per TB-month [V]. A Personal user with 2 GB costs about $0.03 a month in storage. One small EU VPS for the API is about €5–20 a month [I; Hetzner prices didn't render].
- The real cost is Jeffrey's time: support, on-call for the sync server, incident handling.
- Example: 100 Personal yearly subscribers ≈ €4,800 a year gross, about €4,450 after fees. 10 teams of 5 on yearly billing ≈ €6,000 a year. This is side income unless adoption is large.

### 4.2 Hosted agents: recommendation [I]

1. **Never sell Claude (or other vendor) usage.** It is forbidden for products that run Claude Code (§5.1), and margins on resold inference are thin anyway.
2. **First step, inside Team: "bring your own runner".** The customer runs a headless Gizai core on their own server or second PC, signed in to their own `claude`/`codex`. Gizai Cloud only relays tasks, events and logs between devices. Vendor terms stay as on the desktop, and Gizai stores no code or credentials.
3. **Only later, if asked: hosted sandboxes, metered**, with the customer's own Anthropic API key (or Bedrock/Vertex credentials) passed straight into an unmodified `claude` binary. Compute reference: E2B charges $0.000014 per vCPU-second and $0.0000045 per GiB-second, so a 2 vCPU / 4 GiB sandbox is about $0.17 an hour; Pro is $150 a month plus usage [V prices; I the hourly sum]. Gizai would need about €0.30–0.50 per runner-hour, plus security work: repos, secrets and prompt injection on Gizai's servers.

---

## 5. Constraints

### 5.1 Vendor terms (agents)

**Anthropic** — Claude Code "Legal and compliance" page, fetched 2026-10-06 [V] (https://code.claude.com/docs/en/legal-and-compliance):
- Products that preinstall or run Claude Code "(e.g. in hosted sandboxes or other agent infrastructure)" must accept the **Commercial Terms**, keep the binary unmodified, and keep every built-in sign-in method.
- **"Customers may not pay for, resell, or intermediate Claude usage on their end users' behalf. Each end user must authenticate with their own Anthropic API key, Claude subscription plan credentials, or 3P inference provider credential … That usage is billed directly to the end user."**
- "Anthropic does not permit third-party developers to offer Claude.ai login into their own applications, or to route requests through Free, Pro, or Max plan credentials on behalf of their users. Moreover, developers may not collect, store, or intermediate Claude.ai credentials or session tokens — sign-in to a Claude account must complete through Anthropic's own flow."
- It "does not prevent an end user from signing in to the unmodified Claude Code binary with their own Claude subscription, including where a platform hosts Claude Code."
- "Advertised usage limits for Pro and Max plans assume ordinary, individual usage of Claude Code and the Agent SDK."
- Naming: you may say a product "runs Claude Code", but not use the name in a product or feature name.
- Commercial Terms (effective 2025-06-17): no use of the services to "resell the Services except as expressly approved by Anthropic" [V].
- Help centre: the Agent SDK credit change is **paused**. "`claude -p`, and third-party app usage still draw from your subscription's usage limits." The promised credit "isn't available" [V]. Zed's post (2026-05-14, updated 2026-06-16) confirms the postponement and that Zed doesn't bill for Claude usage [V].

What this means for Gizai [I]:
- The paid plans contain **no AI usage**. They are never sold as "Claude included".
- The desktop app keeps spawning the user's own `claude` binary under the user's own login. Gizai Cloud never receives, stores or relays OAuth tokens, `~/.claude/.credentials.json` or `setup-token` tokens.
- Hosted runners only with the customer's own API key or cloud credentials, billed by the vendor to the customer. A subscription sign-in inside a Gizai-hosted sandbox is literally allowed ("where a platform hosts Claude Code"), but the token would sit on Gizai's servers. Avoid it.
- If Gizai is sold, accept Anthropic's Commercial Terms for the hosted part and ask Anthropic sales whether the desktop app counts as "running Claude Code in your products" (this grey area is already listed in `notes/F-agents-runs.md` §2.1).
- Anthropic changed these rules about five times in 2026 [V 10-05]. Keep agent billing out of Gizai's business model so the next change can't break it.

**OpenAI (Codex)** [V]: "Use API key authentication for programmatic Codex CLI workflows, such as CI/CD jobs." "Don't expose Codex execution in untrusted or public environments." Treat `~/.codex/auth.json` "like a password". "Sign in with ChatGPT" inside third-party apps requires partner approval [I, from notes F §2.2]. The same rules as for Claude apply: own login on the user's own machine; API key for anything hosted.

### 5.2 EU VAT for digital services

- B2C electronically supplied services are "always taxed in the customer's country" [V Your Europe].
- Below **€10,000 a year** of cross-border B2C sales (EU total, current and preceding year), the home country's VAT (NL 21%) may apply instead [V EU OSS].
- **Union OSS:** register in NL and file one **quarterly** return for all EU consumer sales. If you use it, it applies to all such supplies [V].
- **B2B in another EU country:** reverse charge, no VAT charged; check the buyer's VAT number (VIES) [V Your Europe; I for VIES].
- Non-EU sales (UK, Norway, Switzerland, Australia, US states) have their own registration rules [I]. A merchant of record covers them.
- **KOR** (Dutch small-business scheme): turnover up to €20,000 a year, no VAT charged, no VAT deducted [V business.gov.nl]. It probably doesn't apply if Gizai is sold through Jeffrey's existing agency entity, because the threshold counts the whole business's turnover [I]. Ask the accountant.
- Consumer prices must show the total including taxes [V Your Europe].
- With a merchant of record, the MoR sells to the customer and Jeffrey's business invoices the MoR (Paddle: "You appoint Paddle as your non-exclusive reseller"; EU sales go through Paddle.com Market Ltd, UK [V]). The Dutch VAT treatment of that supply should be confirmed with the accountant [I].

### 5.3 Merchant of record and payment options

| Option | Fees (verified) | Effective fee on €5/month B2C (€6.05 gross) [I] | On €48/year B2C (€58.08 gross) [I] | Notes |
|---|---|---|---|---|
| **Paddle** (MoR) | **5% + $0.50** per checkout transaction, "No monthly fees"; payouts monthly by the 15th, local transfer free, international wire $15/€15 [V] | ≈ €0.75 (15% of net) | ≈ €3.35 (7.0%) | Established; MoR entity for EU sales is Paddle.com Market Ltd (UK) [V]. Can reject products "outside of Paddle's risk tolerance" [V]. Its AUP page 404'd. |
| **Stripe Managed Payments** (MoR) | **+3.5%** per successful transaction on top of Stripe fees [V]; NL fees: EEA standard cards 1.5% + €0.25, premium 2.8% + €0.25, UK 2.5%, international 3.15%, +2% currency conversion; Billing 0.7% [V] | ≈ €0.60 (12%) | ≈ €3.56 (7.4%) | **NL sellers eligible**; software and SaaS eligible; "professional services" not [V]. Customers see "LINK.COM* …" and get support from Link; Stripe may refund without your approval if you don't answer within 48 h; Checkout/Payment Links only; no custom checkout domain [V]. |
| **Polar** (MoR) | Starter **5% + $0.50**; Pro $20/month **3.8% + $0.40**; Early Member 4% + $0.40 (+0.5% on subscriptions); **+1.5% for non-US cards**; payouts $2/month + 0.25% + $0.25; disputes $15 [V] | ≈ €0.84 (17%) | ≈ €4.20 (8.8%) | Allows "Software & SaaS"; disallows donations; lists "AI Content Generation tools" under extra review [V]. Built-in licence keys, revoked on cancel [V]. EU cards always pay the +1.5%. |
| **Lemon Squeezy** (MoR) | 5% + $0.50; +1.5% international; +0.5% subscriptions; non-US payouts 1% [V] | ≈ €0.87 (17%) | ≈ €4.50 (9.4%) | 2026-01-28 update: the team is building Stripe Managed Payments and plans to help users "migrate" [V]. **Don't start here.** |
| **Stripe + Stripe Tax** (not a MoR) | 1.5% + €0.25 (EEA card) + 0.7% Billing + Tax 0.5% per transaction where registered [V]; Tax Complete (with OSS filing via Taxually) from €80/month [V] | ≈ €0.41 (8%) | ≈ €1.82 (3.8%) | Cheapest, but Jeffrey files OSS quarterly and handles non-EU tax himself. Sensible only below €10k cross-border B2C, or for EU-only B2B. |
| **Mollie** (Dutch PSP, not a MoR) | iDEAL €0.32; SEPA DD €0.35; EU consumer cards 1.8% + €0.25 [V] | ≈ €0.36 | ≈ €1.30 | No tax handling stated [V]. Same caveats as Stripe. |

Fee assumptions [I]: $0.50 ≈ €0.45; the percentage applies to the gross amount including VAT; Stripe Billing's 0.7% applies to Managed Payments subscriptions.

**Recommendation [I]:** use **Paddle or Stripe Managed Payments**. Choose whichever one Jeffrey's business already has an account with. Paddle is simpler for customers (one brand, its own support). Stripe MP is slightly cheaper on small monthly charges and keeps everything in one Stripe account, but the customer deals with "Link". Both need **annual billing pushed** to keep fees near 7%.

---

## 6. Entitlement design (no DRM in the MIT code)

Principle: **the open app never checks a licence. The server enforces every paid feature.** Anyone can remove a client-side check from MIT code, and Bitwarden, Zed and Obsidian don't rely on one either. Zed: "Signing in to Zed is not required", and only collaboration and Zed-hosted AI need an account [V]. Obsidian: "Downloading Obsidian does not require an account" [V].

1. **Account sign-in.** Settings → "Gizai Cloud" → Sign in. This opens the system browser with OAuth 2.0 Authorization Code + PKCE and a loopback or `gizai://` redirect (Zed uses a browser OAuth flow too [V]). The app never shows a password field. Server accounts use email magic link or passkey, with optional GitHub/Google sign-in. [I]
2. **Tokens.** A long-lived refresh token goes into the OS keychain, like the other secrets (the schema already keeps secrets out of SQLite and stores only keychain entry names in `settings`). The access token is short-lived (≈15–60 min) and kept in memory. The local MCP `api_tokens` table stays local and is never synced. [I]
3. **Devices.** On sign-in the app registers its existing `devices.id` (UUIDv7, already used inside the HLC string). The web account page lists devices and can revoke one; revoked devices get 401 on the next call. [I]
4. **Entitlement document.** `GET /v1/me/entitlements` returns plan, seats, quotas, features and `valid_until` (period end + grace), signed with Ed25519. The app uses it **only for display**: plan badge, quota bars, "renew" banners. The backup and sync endpoints check the plan on every request. [I]
5. **Offline.** The app needs no network at all. Unsent changes wait (`changes.pushed_at IS NULL`) and backups queue. The cached entitlement is trusted for display until `valid_until`. **Local data is never locked or deleted because of billing.** [I]
6. **Lapse.** Failed payment → the MoR retries (dunning) → plan "lapsed": uploads and sync stop; restore and download stay available read-only for 30–90 days; then deletion after an email notice (GDPR storage limitation). For comparison, Bitwarden gives self-hosted orgs 60 days to apply a renewed licence before disabling them [V]. [I]
7. **Billing link.** MoR webhooks (created, updated, cancelled, payment failed) → server `subscriptions` table → entitlements. Plan changes and invoices happen in the MoR's customer portal; the app only links to the web account page. No payment UI in the app. [I]
8. **Team removal.** Removing a member revokes their tokens and stops their sync. Data already on their disk can't be pulled back from an open-source client; say so plainly in the terms. With E2E team sync, rotate the workspace key on removal. [I]
9. **Encryption.**
   - Backups are always end-to-end encrypted: a key from a passphrase (Argon2id) or a random key plus recovery code; XChaCha20-Poly1305 or AES-256-GCM. The server sees only ciphertext, sizes and times. Obsidian's E2E default uses scrypt and AES-256-GCM, and offers an optional "standard" (managed-key) mode that allows recovery [V].
   - Personal sync can be E2E: encrypt each change payload; the server only orders by sequence.
   - Team sync is the hard case (decision 5). [I]
10. **Licence keys: not needed** while all paid features are hosted. Keep the pattern ready only for a possible self-hosted Team server: a signed licence file (Ed25519) tied to an installation ID with seats and expiry, plus an optional online "billing sync" (Bitwarden [V]). Plane binds keys to workspace + machine signature + domain and checks online, with a licence file for air-gapped installs [V]. Keygen documents the same offline-file pattern with Ed25519 and an offline TTL [V]. Polar and Lemon Squeezy offer hosted licence-key APIs that auto-expire on cancel [V]; Gizai doesn't need them.
11. **Schema fit.** Already present in `0001_init.sql`: UUIDv7 ids, `devices`, `changes` with `hlc`, `device_id`, `schema_version` and `pushed_at`, tombstones, content-addressed files. Still needed later [I]:
    - a local-only `cloud_link` row: account id, workspace id = `orgs.id`, keychain entry name, last pull cursor;
    - a mapping from `actors` (kind `person`, `email`) to cloud users for team logins.

---

## 7. Risks

1. **Willingness to pay is low for local-first dev tools.** Vibe Kanban: thousands of daily users, mostly free, closed [V]. Paperclip (MIT) still has no prices, only a cloud waitlist [V]. Gizai's buyers are mostly Jeffrey-like solo developers and small agencies. [I]
2. **MIT means anyone can build a compatible sync server or fork the app.** The moat is convenience, reliability and trust, not code. [I]
3. **Vendor-terms churn.** About five Anthropic changes in 2026 [V 10-05]. Keeping inference out of the paid plans contains this.
4. **Fixed fees on small monthly plans** (12–17% at €5). Mitigation: yearly billing, no plan below €5/month. [I]
5. **GDPR.** Gizai Cloud would hold customers' client and contact data, so Jeffrey becomes a processor. He needs a DPA (verwerkersovereenkomst), EU hosting, a sub-processor list (host, storage, MoR) and a breach process. E2E backup reduces exposure; server-readable team sync increases it. [I]
6. **Liability for data loss.** A sync bug that loses client data hurts the agency's reputation too. A BV limits personal liability (entity decision; also decision 10 in RESEARCH.md). [I]
7. **Sync correctness.** Conflicts, task-key renumbering and clock skew (`notes/E-sqlite-sync.md` §3). Ship backup first, then sync. [V 10-05 for the plan]
8. **Payment-platform churn and acceptance.** Lemon Squeezy is in transition [V]. Polar flags AI tools for review [V]. Paddle can reject by "risk tolerance" [V]. Stripe MP can refund without approval after 48 h [V].
9. **Solo-operator load.** On-call for a sync server and customer support compete with agency work. Huly's hosted service ended when funding stopped [V]. A clear shutdown and export promise (the local SQLite stays complete) limits the damage. [I]
10. **Team tier competes with tools agencies already pay for** (Linear, Plane, Productive). Gizai's edge is local agents, not issue tracking. [I]

---

## 8. Decisions for Jeffrey

| # | Decision | Recommended |
|---|---|---|
| P1 | Tiers and prices | Free / Personal €5 per month or €48 per year / Team €10 yearly or €12 monthly per user (min 2) / Business on request. EUR, excl. VAT. |
| P2 | Merchant of record | Paddle or Stripe Managed Payments; whichever your business already uses. Not Lemon Squeezy. |
| P3 | Publishing entity (eenmanszaak or BV) | Decide before selling; affects liability, VAT handling and MoR onboarding (also RESEARCH.md decision 10). Ask the accountant about KOR/OSS and invoicing the MoR. |
| P4 | Sync server source | Hosted only and closed at first, with a published sync protocol; reconsider an AGPL or licensed self-host server when a customer asks. Alternatives: open-source the server now (trust, but competitors can host it), or a Bitwarden/Plane-style licence file for self-host Team. |
| P5 | Team sync encryption | Managed server-side encryption with EU hosting for Team v1 (simpler; enables share links and audit search); E2E for backups and Personal sync. Alternative: E2E everywhere with per-workspace keys (more work, less server liability). |
| P6 | Hosted agents | No Gizai-paid inference, ever. "Bring your own runner" relay in Team; metered hosted sandboxes with the customer's own API key only if asked. |
| P7 | Data after a lapsed plan | Read-only restore for 60 days, deletion after an email notice. |
| P8 | Supporter licence (€25–50 once) and a Publish/Share add-on | Optional; supporter licence yes if you want early funding; Publish later. |

---

## 9. Sources (accessed 2026-10-06 unless noted)

Products
- Obsidian pricing https://obsidian.md/pricing ; Sync https://obsidian.md/sync ; Sync plans https://obsidian.md/help/sync/plans ; Sync security https://obsidian.md/help/sync/security ; licence (2025-02-20) https://obsidian.md/license
- Joplin plans https://joplinapp.org/plans/
- Standard Notes plans https://standardnotes.com/plans
- Logseq Open Collective https://opencollective.com/logseq (update 2026-07-13)
- AFFiNE pricing https://affine.pro/pricing
- AppFlowy pricing https://appflowy.com/pricing ; AppFlowy-Cloud README https://github.com/AppFlowy-IO/AppFlowy-Cloud
- Huly pricing https://huly.io/pricing ; README https://github.com/hcengineering/platform (commits 9aa7da386c 2026-09-17 "has shut down", e749ab9d7b 2026-09-25 "Frozen maintenance")
- Linear pricing https://linear.app/pricing
- Plane pricing https://plane.so/pricing ; editions https://developers.plane.so/self-hosting/editions-and-versions ; licensing https://developers.plane.so/self-hosting/manage/manage-licenses/overview.md ; activation https://developers.plane.so/self-hosting/manage/manage-licenses/activate-pro-and-business.md
- Zed pricing https://zed.dev/pricing ; sign-in https://zed.dev/docs/authentication ; plans and usage https://zed.dev/docs/ai/plans-and-usage ; Anthropic changes (2026-05-14, updated 2026-06-16) https://zed.dev/blog/anthropic-subscription-changes
- Cursor pricing https://cursor.com/pricing
- Warp pricing https://www.warp.dev/pricing
- Paperclip https://paperclip.ing ; waitlist https://paperclip.ing/waitlist/ ; README https://github.com/paperclipai/paperclip
- Vibe Kanban shutdown (2026-04-10) https://vibekanban.com/blog/shutdown
- Bitwarden business https://bitwarden.com/pricing/business/ ; personal https://bitwarden.com/pricing/ ; self-host licensing https://bitwarden.com/help/licensing-on-premise/ ; server repo https://github.com/bitwarden/server
- Tailscale pricing https://tailscale.com/pricing ; Headscale https://github.com/juanfont/headscale
- Productive.io https://productive.io/pricing/ ; Moneybird https://www.moneybird.nl/prijzen/
- E2B pricing https://e2b.dev/pricing ; Backblaze B2 https://www.backblaze.com/cloud-storage/pricing ; Cloudflare R2 (2026-10-05) https://developers.cloudflare.com/r2/pricing/

Vendor terms
- Claude Code legal and compliance https://code.claude.com/docs/en/legal-and-compliance
- Agent SDK with a Claude plan https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan
- Anthropic Commercial Terms (effective 2025-06-17) https://www.anthropic.com/legal/commercial-terms
- Anthropic Consumer Terms (2026-10-05) https://www.anthropic.com/legal/consumer-terms
- Codex auth https://learn.chatgpt.com/docs/auth (redirect from developers.openai.com/codex/auth)
- Earlier timeline and grey areas: `~/Herd/cordon/notes/F-agents-runs.md` §2

Tax and payments
- Your Europe, cross-border VAT https://europa.eu/youreurope/business/taxation/vat/cross-border-vat/index_en.htm
- EU One Stop Shop https://vat-one-stop-shop.ec.europa.eu/one-stop-shop_en
- Your Europe, consumer pricing https://europa.eu/youreurope/citizens/consumers/shopping/pricing-payments/index_en.htm
- Dutch KOR https://business.gov.nl/subsidy/small-businesses-scheme/
- Paddle pricing https://www.paddle.com/pricing ; terms https://www.paddle.com/legal/terms
- Stripe NL pricing https://stripe.com/en-nl/pricing ; Stripe Tax pricing https://stripe.com/en-nl/tax/pricing ; Managed Payments https://stripe.com/managed-payments , https://docs.stripe.com/payments/managed-payments , eligibility https://docs.stripe.com/payments/managed-payments/eligibility , how it works https://docs.stripe.com/payments/managed-payments/how-it-works
- Polar pricing https://polar.sh/resources/pricing ; licence keys https://polar.sh/docs/features/benefits/license-keys ; acceptable use https://polar.sh/docs/merchant-of-record/acceptable-use
- Lemon Squeezy pricing https://www.lemonsqueezy.com/pricing ; fees https://docs.lemonsqueezy.com/help/getting-started/fees ; 2026 update (2026-01-28) https://www.lemonsqueezy.com/blog/2026-update ; licence API https://docs.lemonsqueezy.com/help/licensing/license-api
- Mollie pricing https://www.mollie.com/pricing
- Keygen offline licences https://keygen.sh/docs/choosing-a-licensing-model/offline-licenses/

Local inputs (data, read 2026-10-06): `~/Herd/cordon/RESEARCH.md`, `notes/E-sqlite-sync.md`, `notes/H-build-packaging.md`, `notes/F-agents-runs.md` §2, `~/Herd/gizai/crates/gizai-core/migrations/0001_init.sql`.
