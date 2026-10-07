# Gizai: paid features and how to build them

Plan, 2026-10-07. No code yet. The market research behind it, with sources and dates, is in `docs/research/paid-offering-2026-10.md`. **[V]** = verified on a primary page on 2026-10-06; **[I]** = my judgement.

## In one minute

- **The app stays MIT, free and complete.** It works without an account or network. No licence check ever goes into the open code.
- **Gizai sells hosted services only:**
  1. encrypted off-site backup;
  2. sync between your own devices;
  3. shared team workspaces with logins, roles and an audit log;
  4. later, a relay to agent runners on your own machines.
- **Proposed prices (excluding VAT):**

  | Plan | Price | For |
  |---|---|---|
  | Free | €0 | the whole app |
  | Personal | €5 a month or €48 a year | backup, then sync |
  | Team | €10 per user a month yearly, €12 monthly, minimum 2 users | teams |
  | Business | on request | SSO and SCIM |

- **Gizai never sells Claude usage.** Anthropic's terms forbid paying for, reselling or intermediating Claude usage for users [V]. Agents keep running on each user's own Claude Code login or API key.
- **The order:**
  1. a free local snapshot first;
  2. paid backup (it tests whether anyone pays before the big build);
  3. sync;
  4. team.
- **The risk:** few people pay for local-first dev tools. Vibe Kanban closed with thousands of mostly free daily users; hosted Huly stopped when hosting wasn't funded [V]. Charge from the first stored byte, push yearly billing, and keep server costs small.

## What stays free, forever

Everything in the desktop app:

- clients, projects, tasks, docs, files;
- agents, the Team Lead chat, worktrees and gates;
- local snapshots to any folder (including a Dropbox or Syncthing folder) and export.

Anyone may build their own sync server against the MIT code. Gizai's edge is that the hosted one is convenient, reliable and trustworthy.

## Plans

| | Free | Personal | Team | Business |
|---|---|---|---|---|
| Price (excl. VAT) | €0, no account | €5/month or €48/year | €10/user/month yearly, €12 monthly, min. 2 | from ~€18/user or a quote, min. 10 |
| The desktop app, agents, chat | ✓ | ✓ | ✓ | ✓ |
| Local snapshots, export | ✓ | ✓ | ✓ | ✓ |
| Encrypted off-site backup, 90-day history, restore on a new machine | | ✓ (10 GB) | ✓ (25 GB per seat, pooled) | ✓ |
| Sync between your own devices | | ✓ (when it ships, same price) | ✓ | ✓ |
| Shared workspaces, invitations, roles (owner, admin, member, guest) | | | ✓ | ✓ |
| Audit log (1 year, CSV export) | | | ✓ | longer |
| Agent runner relay (run agents on your own server) | | | ✓ (later) | ✓ |
| SAML/OIDC SSO, SCIM, a custom DPA | | | | ✓ |

Why these prices:

- **Personal** sits between Obsidian Sync Standard ($4–5) and Plus ($8–10) [V].
- **Team** matches Linear Basic ($10), AFFiNE Team ($10) and Plane ($6–8) [V].
- **The $20–50 seats of Cursor and Warp include AI inference**, which Gizai can't bundle.
- **Below €5 a month, payment fees take more than 15%.** At €48 a year they take 7–9% [I, computed].

Optional, your call: a **supporter licence** (€25–50 once, like Obsidian Catalyst) for early funding. Don't sell lifetime cloud plans: server costs never end.

## How it fits together

```
Gizai desktop (MIT)                         Gizai Cloud (hosted, EU)
├─ crates/gizai-cloud-client   ── HTTPS ──► api.gizai.app  (Rust, axum)
│   sign-in, keychain, snapshot,             ├─ accounts, devices, entitlements (signed)
│   encrypt, upload, restore,                ├─ backups: encrypted blobs → object storage (EU)
│   push/pull changes                        ├─ sync: encrypted change log, ordered per workspace
├─ Settings → Gizai Cloud                    ├─ teams: members, roles, invitations, audit
└─ (later) gizai serve: headless core        ├─ billing webhooks from the merchant of record
                                             └─ relay: websocket to your own runners
```

**The rules:**

- **No secrets in SQLite.** The app keeps the cloud refresh token in the OS keychain (on Linux: Secret Service via the `keyring` crate). The `settings` table holds only the keychain entry name; the schema already plans for this.
- **The server enforces.** The app only displays the plan (a badge, quota bars). The backup and sync endpoints check the plan on every call. Local data is never locked or deleted over billing.
- **Encryption by default.** Backups and Personal sync are end-to-end encrypted:
  - key derivation: Argon2id from a passphrase, or a random key with a recovery code;
  - cipher: XChaCha20-Poly1305;
  - the server sees ciphertext, sizes and times only.

  Team sync encryption is decision P5.
- **No inference on Gizai's servers.** No Claude or Codex credentials ever leave the user's machine.

## Phases

Effort is in agent-days, like the research's milestones. The server, payments and legal work need your time as well.

### Phase 0: local snapshots (free, in the app): 2–3 days

What it gives the user:

- Settings → Backups → *Save a snapshot to…*: a folder; weekly and on quit.
- A snapshot is `gizai.db` copied with `VACUUM INTO` (consistent while the app runs), plus the files store, in one `.gizai-snapshot` archive.
- *Restore from snapshot…* checks the archive and the schema version and swaps it in on the next start.

Why first: it is the backup format the paid service will upload. Users trust it because it works without an account. It also fixes a real gap today: there is no backup at all.

### Phase 1: Gizai Cloud account and encrypted backup (Personal): 10–15 days

**Server** (new private repository, Rust + axum + Postgres + EU object storage such as Cloudflare R2 EU or Backblaze B2 EU):

- Accounts by email magic link or passkey, with optional GitHub/Google sign-in. Sign-in from the app uses OAuth 2.0 Authorization Code + PKCE in the system browser, with a loopback redirect.
- Devices: the app's existing `devices.id`. The account page lists them and can revoke one.
- `GET /v1/me/entitlements` returns the plan, quotas and `valid_until`, signed with Ed25519. The app uses it for display only.
- Backups: chunked upload of the encrypted snapshot; list; download; 90-day retention; delete.
- Billing: webhooks from the merchant of record (subscription created, updated, cancelled, payment failed) feed a `subscriptions` table and the entitlements.
- Lapse: uploads stop; restore stays available, read-only, for 60 days. After an email notice the data is deleted.

**App:**

- New crate `crates/gizai-cloud-client` (no Tauri): sign-in, token refresh, the keychain, encrypt and decrypt, upload and download.
- A scheduler in `src-tauri`: back up on change (debounced), daily and on quit, and only when the plan allows.
- A local-only table `cloud_link` (account id, workspace id = `orgs.id`, keychain entry, last cursor). It is never synced.
- Settings → **Gizai Cloud**: sign in or out, plan badge, *Back up now*, history, *Restore on this machine*, *Manage plan* (opens the web account page; there is no payment UI in the app).

**Done when:** a backup made on one machine restores on a clean machine, and a lapsed plan stops uploads but not restores.

### Phase 2: sync between your own devices (Personal): 10–20 days

- The app already writes every change to `changes`, with a hybrid logical clock, the device and the schema version; `pushed_at` marks what was sent.
- **Push:** unsent changes, encrypted per change, go to `POST /v1/workspaces/:id/changes`. The server only orders them by sequence.
- **Pull:** changes after the last cursor are applied in hybrid-clock order.
- **Conflicts:** last writer wins per field; tombstones win over edits; task identifiers that collide are renumbered (links use UUIDs, so they survive).
- **Files:** sync by content hash; upload only what the server doesn't have.
- **Guardrails:**
  - one writer process per machine (already the rule);
  - a device on an older schema refuses to apply newer changes and asks to update;
  - runs and worktrees stay local to the machine that ran them.

**Done when:** two machines converge after offline edits on both. The research has a fault-injection test for exactly this.

### Phase 3: Team: 15–25 days

- A **shared workspace** is an `orgs` row on the server with members and roles. The app opens a second organisation in the rail (the "+" is already there, disabled).
- **Invitations by email.** Accepting one adds the workspace to the member's app.
- **Mapping:** `actors` of kind `person` are linked to cloud users by email, so assignments, comments and the activity feed show real people.
- **Audit log** = the `changes` table, kept on the server for a year, viewable and exportable as CSV.
- **Removing a member** revokes their devices' tokens and stops their sync. Data already on their disk can't be pulled back, and the terms say so.
- **Encryption** depends on decision P5: managed server-side encryption (simpler, allows search and share links) or end-to-end with per-workspace keys (more work, less liability).

### Phase 4: agent runner relay (Team): 10–15 days

Some teams want agents running on a server, not a laptop. Gizai Cloud must never run Claude on its own account, so:

- **`gizai serve`:** a headless core, without the UI, on the customer's own server, logged in to their own `claude`.
- **The relay:** it connects out to Gizai Cloud over a websocket. Cards routed to its agents start there; events and logs come back to everyone's app.
- **Vendor terms:** unchanged, because it is the customer's machine and the customer's login.
- **Hosted sandboxes:** only if customers ask, metered, and only with the customer's own Anthropic API key passed to an unmodified `claude`. That needs Anthropic's Commercial Terms and security work. It is not planned.

### Later

- **Business:** SSO (SAML/OIDC), SCIM, longer audit retention, a custom DPA. Build these only when a customer pays for them.
- **Share with a client:** a read-only project page or an HTML artifact behind a link (Obsidian Publish charges $8–10 a site [V]). It could be part of Team.

## Money, tax and law

- **Merchant of record:** Paddle (5% + $0.50) or Stripe Managed Payments (Stripe fees + 3.5%) [V]. Either one sells to the customer, handles EU VAT (OSS) and non-EU sales tax, and pays you out. Pick the one your business already uses.
  - Lemon Squeezy is moving into Stripe Managed Payments [V]: don't start there.
  - Polar charges an extra 1.5% on non-US cards [V].
- **Prices** are shown in EUR excluding VAT ("excl. btw"); consumers see VAT added at checkout. Push yearly billing.
- **Entity:** decide between eenmanszaak and BV before selling (liability for data loss, VAT, merchant-of-record onboarding). Ask the accountant about KOR and OSS and about invoicing the merchant of record.
- **GDPR:** Gizai Cloud would hold customers' client and contact data, so you become a processor. You need:
  - a DPA (verwerkersovereenkomst);
  - EU hosting;
  - a list of sub-processors (host, storage, merchant of record);
  - a breach procedure.

  End-to-end encrypted backups reduce exposure.
- **Anthropic:**
  - The app runs the user's own unmodified `claude` under their own login, which the terms allow [V].
  - Ask Anthropic whether a desktop app that runs the user's Claude Code counts as "running Claude Code in your products", which would need the Commercial Terms. The research lists this grey area.
  - Never use "Claude" in a product or feature name. "Runs Claude Code" as a description is fine [V].
- **Shutdown promise:** if Gizai Cloud ever stops, your local SQLite stays complete and you get 90 days to download backups. Say so on the pricing page.

## Rough economics [I]

- **Storage is cheap.** At Cloudflare R2's $0.015 per GB-month [V], a Personal user with 2 GB costs about $0.03 a month. The API runs on one small EU server (about €5–20 a month).
- **The real cost is your time:** support, the sync server's on-call, incidents.
- **Example revenue:**

  | Customers | Gross a year | After fees |
  |---|---|---|
  | 100 Personal, yearly | ≈ €4,800 | ≈ €4,450 |
  | 10 teams of 5, yearly | ≈ €6,000 | |

  This is side income until adoption is large. Build backup first and only build sync when backups sell.

## Decisions for Jeffrey

| # | Decision | Recommended |
|---|---|---|
| P1 | Plans and prices | Free / Personal €5 a month or €48 a year / Team €10 yearly or €12 monthly per user (min. 2) / Business on request; EUR, excl. VAT |
| P2 | Merchant of record | Paddle or Stripe Managed Payments, whichever your business already uses |
| P3 | Entity | Decide eenmanszaak or BV before selling; check KOR/OSS with the accountant |
| P4 | Sync server source | Hosted and closed at first, with a published protocol; reconsider a licensed self-host server when a customer asks |
| P5 | Team sync encryption | Managed server-side encryption with EU hosting for Team v1; end-to-end for backups and Personal |
| P6 | Hosted agents | Never Gizai-paid inference; a runner relay in Team; sandboxes only with the customer's own key, if asked |
| P7 | After a lapsed plan | Restore read-only for 60 days, then deletion after an email notice |
| P8 | Supporter licence | Optional, €25–50 once, as a perk (early builds, a badge) |
| P9 | Start Phase 0 now? | Yes: local snapshots help every user today and define the backup format |
