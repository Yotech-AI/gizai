# Counting GizAi installs without telemetry in the app (research, 2026-10-13)

Card GA-166. Research only: nothing here is built, and nothing changes in the app, `get.sh`, `get.ps1`, the website or any setting. Jefsev approves before anything is built. Each claim is marked **[V]** when I checked it in the code or a primary source (named next to it), or **[I]** when it is my judgement. Sources are in §8, all read on 2026-10-13.

## 1. Summary

1. **gizai.app is live on Cloudflare.** Its DNS is at Cloudflare. It answers through Cloudflare's network, and the Pages project `gizai-website` serves the same site. Both `/install.sh` and `/install.ps1` answer with a 302 to `get.sh` and `get.ps1` on `production`. [V]
2. **Count with Cloudflare's own totals** (Option A). On every plan, Cloudflare's GraphQL Analytics API can filter requests by path and user agent, and it keeps them for at least 31 days, at no cost. A daily workflow saves one total per day, per script and per kind of client (curl, PowerShell, browser, bot). It saves no IP address and no user agent. Nothing changes on the website or in the scripts. [V docs; the free plan's exact limits get confirmed in card S1]
3. **The in-app updater never asks gizai.app** (`crates/gizai-agents/src/update.rs:24`, `:128-130`, `:319-335`). [V] So gizai.app's count holds the one-line runs: first installs, updates from a terminal, re-runs after installing a missing tool, `--check` and `--uninstall`. It also holds people reading the script in a browser, and bots. Nothing in a request says whether it is a first install, and that is how it should stay.
4. **GitHub's traffic needs Administration read**, which `GITHUB_TOKEN` can't be given, so the workflow needs a fine-grained token in a secret. [V] GitHub counts "full clones (not fetches)". [V] `get.sh` and the updater never run `git clone`, and GA's own CI adds clones many times a day, so clones are a weak install count. [I]
5. **The numbers live in a new private repository, `Yotech-AI/gizai-stats`**, with the workflow. Its weekly `README.md` is the one place to read them. Nothing changes in GA, and no CI runs on `main`. Whether the numbers should be public is open question 1.
6. **The AUR has votes and popularity** (votes that lose 2% of their weight a day): they measure goodwill, not installs. [V] pkgstats, which Arch users opt into, counts AUR packages too. [V] **Homebrew's public analytics include public third-party taps**, so `yotech-ai/tap/gizai` gets install counts for 30, 90 and 365 days. [V]
7. **Cards:** S1, the stats workflow; S2, the package rows once GA-165's packages exist; G1, the "No telemetry" sentence in GA; W1, the website's README. Jefsev first changes 9 settings by hand (§5.5), and there are 10 open questions (§7).

## 2. The project now

### 2.1 What GizAi is, and how it's built

- **For its users:** GizAi is a desktop app where a team of AI agents builds your software through your board, while you review. Each card gets its own git worktree, and everything runs on your own computer (README.md:25). [V]
- **How it's built:** Rust (a Cargo workspace: `crates/gizai-core`, `crates/gizai-agents`, `crates/gizai-mcp` and `src-tauri`, Cargo.toml:3) with Tauri 2, and a React 19 front end in TypeScript, built with Vite 8 (package.json:38, :51, :63). [V] Data lives in one folder on your computer (docs/install.md:143-145). [V]
- **How it's installed:** from source only, with no downloads or binaries (docs/install.md:5; docs/RELEASING.md:100). [V] The one line, `curl -fsSL https://gizai.app/install.sh | sh` or `irm https://gizai.app/install.ps1 | iex` (docs/install.md:14, :20), gets the latest release's source and runs its `install.sh` or `install.ps1`. "Run it again to update" (docs/install.md:23). [V]
- **How it's released:** a GitHub Release with a tag `vX.Y.Z` on `production`. `main` changes only by pull requests, and `production` is protected (docs/RELEASING.md:5, :9-12). [V] There are five releases from v0.7.1 to v0.10.0, the newest on 13 Oct, none with assets (GitHub's releases API). [V]
- **How it's tested:** CI (`.github/workflows/ci.yml`) runs on every pull request and every push to `main` (ci.yml:6-9). It builds and tests on Linux, macOS and Windows, and runs the checks of `get.sh` and `get.ps1` (ci.yml:112-169). [V] The Install workflow runs on pull requests that change an installer, and by hand (install.yml:8-15). [V]

### 2.2 What goes where on the network

What the feature touches is who asks what of whom when GizAi is installed, updated or packaged:

| Who | Asks | For | Where (code) |
|---|---|---|---|
| The one line (`curl … \| sh`, `irm … \| iex`) | `gizai.app/install.sh` or `/install.ps1` | the script, through a 302 to raw.githubusercontent.com | docs/install.md:14, :20, :25 [V] |
| `get.sh` / `get.ps1` | `api.github.com/repos/Yotech-AI/gizai/releases/latest` | the latest tag | scripts/get.sh:104-116; scripts/get.ps1:96-117 [V] |
| `get.sh` / `get.ps1` | `github.com/Yotech-AI/gizai.git` | a shallow fetch of that tag into `~/.local/share/gizai-src` (`%LOCALAPPDATA%\gizai-src`): `git init` the first time, then `git fetch --depth 1` | scripts/get.sh:129-140; scripts/get.ps1:134-151 [V] |
| The app's release check | `api.github.com/…/releases/latest`, with the user agent `gizai/<version>` | the latest release | crates/gizai-agents/src/update.rs:128-141; src-tauri/src/update.rs:57-66, :268 [V] |
| The app's update | `github.com/Yotech-AI/gizai.git` | a shallow fetch of the tag into `<data folder>/update/source`, a folder that is kept between updates | crates/gizai-agents/src/update.rs:24, :319-335; src-tauri/src/update.rs:377-391; docs/RELEASING.md:112 [V] |
| GA's CI | github.com (`actions/checkout`, and `get.sh`/`get.ps1` checks of `production`) | the repository | ci.yml:37, :118, :135-139, :158-162 [V] |
| GA-165's AUR package and tap (not built yet) | `github.com/Yotech-AI/gizai/archive/refs/tags/vX.Y.Z.tar.gz` | GitHub's tarball of the tag | GA-165's `docs/research/packaging.md` on its branch (4f8579c), lines 135, 298, 450 [V] |

Nothing in the app asks gizai.app. A search of the code (`rg gizai\.app`, leaving out the docs) finds only the app's identifier `ai.gizai.app` (src-tauri/tauri.conf.json:6), the lines `get.sh` and `get.ps1` *show* (scripts/get.sh:17-20; scripts/get.ps1:36-40), and tests. One of those tests checks that what `get.sh` downloads never comes from gizai.app (scripts/get.test.mjs:372-401). [V] The app's window can't fetch anything from the web either: its content security policy allows only `connect-src ipc: http://ipc.localhost` (src-tauri/tauri.conf.json:18). [V]

### 2.3 The website: where gizai.app is hosted (what I found)

- **DNS:** the zone's nameservers are `carter.ns.cloudflare.com` and `lorna.ns.cloudflare.com`. `gizai.app` resolves to `188.114.96.2` and `188.114.97.2`, Cloudflare addresses, with a TTL of 300 seconds, so the record is proxied. [V: dns.google] `www.gizai.app` has no record. [V]
- **Served by Cloudflare:** `https://gizai.app/cdn-cgi/trace`, which only Cloudflare answers, gives `h=gizai.app`. [V]
- **Pages:** `https://gizai-website.pages.dev/` serves the same site, and its `/install.sh` gives the same 302. So a Cloudflare Pages project named `gizai-website` holds the site and applies its `_redirects`. [V] The live site has GW-2's feature pages (`https://gizai.app/features/board/`). [V]
- **The redirects:** `https://gizai.app/install.sh` answers `302 Found` with `Location: https://raw.githubusercontent.com/Yotech-AI/gizai/production/scripts/get.sh`, and `/install.ps1` the same for `get.ps1`. [V] They come from GW's `public/_redirects`, added in GW-2 (Yotech-AI/gizai-website pull request #2, merged 13 Oct):
  ```
  /install.sh  https://raw.githubusercontent.com/Yotech-AI/gizai/production/scripts/get.sh  302
  /install.ps1 https://raw.githubusercontent.com/Yotech-AI/gizai/production/scripts/get.ps1 302
  ```
- **What I couldn't see:** whether the Pages project is connected by Git (production branch `main`, `npm run build`, `dist`) or deployed by upload, and which Cloudflare plan the account is on. GW's README still says "the deploy comes in its own card" (`~/Herd/gizai-website/README.md:27-30`, at ffe5aa6). GW has no Wrangler configuration (none in the checkout, and none in pull requests #2 to #4). [V] Open question 2.
- **On the page,** the install lines are text with a Copy button, not links (GW-2's `src/data/site.ts`). [V] So crawlers that follow links don't find them, but bots that read text may. [I]

### 2.4 What this touches

- **No app code.** Nothing in `crates/` or `src-tauri/` changes. Card G1 adds one sentence to `docs/install.md` ("No telemetry", docs/install.md:153-157).
- **No website code.** Option A reads what Cloudflare already counts. Card W1 adds a few lines to GW's README.
- **A new repository**, `Yotech-AI/gizai-stats`, for the workflow and the numbers (§4.2).

## 3. The feature: the brief, answered

### 3.1 Counting `/install.sh` and `/install.ps1` per day, per script, without IP addresses

- **Where:** Cloudflare, Pages project `gizai-website`, zone `gizai.app` (§2.3). [V]
- **How (recommended, Option A):** read Cloudflare's own analytics with the GraphQL Analytics API, once a day, for the day before.
  - The dataset is `httpRequestsAdaptiveGroups`. It can be filtered by `clientRequestPath`, `userAgent`, `edgeResponseStatus` and `requestSource: "eyeball"` (real visitors, not Cloudflare's own traffic), and these filters are marked "All plans". Only the referrer filter is "Paid plans only". [V: Cloudflare, AI Crawl Control → GraphQL API]
  - "Adaptive datasets, such as `httpRequestsAdaptiveGroups` … retain at least 31 days of data on every plan." [V: Cloudflare, GraphQL Analytics API → Limits] A query may span at most a node's `maxDuration`. The `settings` node gives the exact limits for the zone (`maxDuration`, `notOlderThan`). [V] For that reason the workflow asks for one day at a time. [I]
  - The workflow keeps one number per day, per script and per kind of client: `curl`, `powershell`, `browser`, `bot`, `other`. It sorts the kinds by user agent and then forgets the user agent. The query never asks for an IP address, so none reaches us. [design, §5.3]
  - **Cost:** nothing. The API comes with the plan, at up to "300 GraphQL queries over 5-minute window" [V]; the workflow sends 3 a day. On GitHub, the private repository's workflow uses about 1 of its 2,000 free Actions minutes a day (GitHub Free for organizations). [V for the 2,000; I for 1 a day]
  - **Kept:** Cloudflare keeps at least 31 days. The workflow keeps the daily totals for as long as the repository exists. [V/I]
  - **Small numbers are exact:** Cloudflare samples only big volumes ("If the number of records is relatively small, sampling is not used"). [V]
- **Today, by hand:** the zone's dashboard (Analytics & Logs → HTTP Traffic) shows "at least the last 30 days" on every plan. [V] Whether the free plan's dashboard filters by path, I couldn't check. [I]
- **Not this way:** Cloudflare Web Analytics counts with a JavaScript beacon in the page, and `curl` and `irm` run no JavaScript. [I] Raw request logs (Logpush) aren't on the free plan [I] and would hold IP addresses anyway.

### 3.2 Does the updater ask gizai.app? What each count measures

**No.** The release check reads `https://api.github.com/repos/Yotech-AI/gizai/releases/latest` (crates/gizai-agents/src/update.rs:128-130, called at src-tauri/src/update.rs:268), and the update fetches the tag from `https://github.com/Yotech-AI/gizai.git` (update.rs:24, :329) into `<data folder>/update/source` (src-tauri/src/update.rs:377-391). [V] Only `GIZAI_REPO` or `GIZAI_RELEASES_URL` changes these, and nobody sets them to gizai.app (src-tauri/src/update.rs:57-66). [V] GA's own CI never asks gizai.app either: its checks pipe the local `scripts/get.sh` and read `get.ps1` from the file (ci.yml:118, :134). [V]

What each event adds to each count:

| What happens | gizai.app (Cloudflare) | GitHub clones of GA | GitHub views of GA | AUR | pkgstats | Homebrew analytics | Tap clones |
|---|---|---|---|---|---|---|---|
| First install with the one line | 1 request, or 2 or more when a tool is missing, since `get.sh` says "Install what is missing above, then run this again" (scripts/get.sh:157-160; get.ps1:176-179) [V] | probably 1: the first fetch into an empty `gizai-src` is a clone in all but name [I] | – | – | – | – | – |
| Update by running the one line again | 1 | 0: a fetch into a folder that has the last tag [V doc, I how GitHub tells] | – | – | – | – | – |
| `--check` or `--uninstall` through the one line | 1 each | 0 after the first | – | – | – | – | – |
| Update from inside GizAi | **0** [V] | the first update on a computer probably counts, later ones are fetches [I] | – | – | – | – | – |
| Install from a clone (`git clone`, `./install.sh`) | 0 | 1 [V: "full clones"] | likely, from reading the README [I] | – | – | – | – |
| GA's CI, each run that builds | 0 [V] | several: 3 checkouts and 4 fetches of `production` by the `get.sh`/`get.ps1` checks (ci.yml:37, :118, :135-139, :158-162) [V what runs; I that GitHub counts them] | – | – | – | – | – |
| Someone opens the URL in a browser | 1, as `browser` | – | – | – | – | – | – |
| Link previews, crawlers, scanners | 1 each, as `bot` or `browser` (user agents can be spoofed) | bots that clone | bots that browse | – | – | – | – |
| `yay -S gizai` (once published) | 0 | 0: GitHub's tarball, not a clone [V GA-165] | – | a vote only if the user votes | 1 per system with pkgstats, monthly | – | – |
| `brew install yotech-ai/tap/gizai` (once published) | 0 | 0 (tarball) | – | – | – | 1, unless analytics are off [V] | 1 for the tap's first clone [I] |
| `brew upgrade` | 0 | 0 | – | – | – | probably 1 [I] | 0: a fetch [I] |

**What no count can tell apart:** a first install from a re-run. The request is the same, and GizAi sends nothing to tell them apart, as the "No telemetry" promise says. With no IP addresses there are no unique visitors either: Cloudflare offers no uniques per path in this dataset. [I] The **ratios** still help. Requests per week by curl or PowerShell, minus the browser and bot share, is an upper bound on one-line installs and terminal updates. GitHub's unique cloners and Homebrew's and pkgstats' counts are lower bounds per channel. [I]

On 12 October GA had 62 workflow runs (GitHub's Actions API), [V] so CI alone could add well over a hundred clones a day. [I] A GitHub community answer (2020, not marked as staff) says an Action "will count as one unique cloner, no matter how many times it clones". [secondary] If that holds, **unique cloners** suffer less from CI than clones do. But `get.sh`'s checks in CI fetch without a login, so each runner may count as its own cloner. [I]

### 3.3 GitHub's 14 days: the scheduled workflow

- **What GitHub gives:** "full clones (not fetches), visitors from the past 14 days, referring sites, and popular content". "Full clones and visitor information update hourly, while referring sites and popular content sections update daily." Times are UTC. [V: GitHub Docs, Viewing traffic to a repository]
- **The API:** `GET /repos/{owner}/{repo}/traffic/clones` and `/traffic/views`. Each gives a total and a breakdown "per day or week for the last 14 days" (`per=day`, the default, or `per=week`; weeks begin on Monday; timestamps are UTC midnight). `/traffic/popular/referrers` and `/traffic/popular/paths` give "the top 10 … over the last 14 days", with no breakdown per day. [V: GitHub REST, Repository traffic] So referrers and paths are saved as a dated snapshot each day, and the snapshots overlap and can't be added up. [I]
- **The token:** the four endpoints need the repository permission **Administration: read**, for fine-grained personal access tokens and for GitHub Apps. [V: GitHub Docs, the permissions pages for fine-grained tokens and for GitHub Apps] `GITHUB_TOKEN` can't get it: its permission keys are `actions`, `artifact-metadata`, `attestations`, `checks`, `code-quality`, `contents`, `copilot-requests`, `deployments`, `discussions`, `id-token`, `issues`, `packages`, `pages`, `pull-requests`, `security-events`, `statuses` and `vulnerability-alerts`, with no `administration`. [V: workflow syntax, `permissions`] So the workflow reads traffic with a **fine-grained token Jefsev makes** (§5.5) and commits with its own `GITHUB_TOKEN`.
- **Where its data lives:** in a new private repository, `Yotech-AI/gizai-stats`, which holds the workflow, the data and the numbers file (§4.2).
- **No CI:** nothing in GA changes, so GA's CI (ci.yml:6-9) never sees it. The stats repository has no other workflow. Also, "events triggered by the `GITHUB_TOKEN` will not create a new workflow run", apart from `workflow_dispatch`, `repository_dispatch` and some pull request events. [V]
- **Scheduling:** "Scheduled workflows run on the latest commit on the default branch." Runs "can be delayed during periods of high loads … High load times include the start of every hour." "In a public repository, scheduled workflows are automatically disabled when no repository activity has occurred in 60 days", and that doesn't apply to a private one. Failure mails go to "the user who last modified the cron syntax in the workflow file". [V: GitHub Docs, events that trigger workflows] So the run is at 03:17 UTC.

### 3.4 AUR and Homebrew counts (once GA-165's packages exist)

GA-165 (on its branch, not merged) proposes the AUR package `gizai` and the tap `Yotech-AI/homebrew-tap`, with `brew install yotech-ai/tap/gizai`. Both build GitHub's tarball of the tag (packaging.md:11, :60, :135). [V]

- **AUR, votes and popularity.**
  - A vote needs an AUR account, given on the web or over SSH. [V: Arch wiki, Arch User Repository]
  - The RPC (`/rpc/v5/info`) returns `NumVotes` and `Popularity`. [V: aurweb's `rpc.py`] Popularity is the sum over the votes of 0.98 to the power of the days since each vote: `coalesce(_sum(func.pow(0.98, (now - PackageVote.VoteTS) / 86400)), 0.0)`. [V: aurweb's `popupdate.py`]
  - So both count people who chose to vote, and popularity is recent votes. **The AUR counts no downloads or installs.** [I: no such field in the RPC]
  - aurweb's README says its home is gitlab.archlinux.org. I read the same source on its GitHub copy, as GitLab and aur.archlinux.org answer my fetches with a bot check. [V]
- **pkgstats** counts AUR packages too.
  - Users opt in by installing `pkgstats`, which "sends a list of all installed packages, the architecture and the mirror" once a week. [V: Arch wiki, pkgstats]
  - `https://pkgstats.archlinux.de/api/packages/yay` answers `{"name":"yay","samples":31956,"count":14973,"popularity":46.86,"startMonth":202609,"endMonth":202609}`. `yay` isn't in Arch's own repositories (archlinux.org's package search returns none), so pkgstats counts AUR packages. [V] It is a monthly count of systems, which is the closest thing to an install count the AUR side has. [I]
- **Homebrew's own analytics include public third-party taps.**
  - Events "can include: the package and non-private GitHub tap names", Homebrew "retains analytics events in InfluxDB for 365 days", and "Aggregate reports and their JSON representations are available from the Homebrew analytics site". Users opt out with `brew analytics off` or `HOMEBREW_NO_ANALYTICS=1`. [V: docs.brew.sh/Analytics]
  - `https://formulae.brew.sh/api/analytics/install/30d.json` (13 Sep to 13 Oct, 22,448 formulae) lists tap formulae by their full name, like `anomalyco/tap/opencode` with 40,958 installs. [V]
  - So `yotech-ai/tap/gizai` would appear there, and in `90d.json` and `365d.json`, counted only on Macs that left analytics on. Whether a formula with very few installs is listed I couldn't see, as the file was too big for my fetch. [I]
- **The tap's GitHub traffic.** `brew install yotech-ai/tap/gizai` taps the repository first, which clones it, so the tap's clones roughly count first installs on Macs. `brew update` fetches, and fetches aren't counted. [I] The workflow saves the tap's traffic with the same token once the tap exists (§5.5, step 7).
- **No download counts for the tarball.** GitHub counts downloads of release assets only, not of its automatic source archives. GizAi's releases have no assets. [V: releases API; I: the archives, backed by the long-open request isaacs/github#593] See open question 5.

### 3.5 One place to read them

`README.md` in `Yotech-AI/gizai-stats`, written by the workflow every Monday (and when run by hand). It has the last 8 weeks, week by week, plus the packages, the referrers, the most viewed pages and what each number means. GitHub shows it on the repository's front page. Format and sample in §5.4.

### 3.6 The sentence for "No telemetry"

To add after the first paragraph of "No telemetry" (docs/install.md:155), once card S1 runs:

> The website, gizai.app, only counts how often its two install lines are fetched: from Cloudflare's own totals, it keeps one number a day for each script and kind of program (curl, PowerShell, a browser), and no IP addresses or anything else about who fetched them.

If Jefsev wants the other counts named too (open question 8), a second sentence:

> GitHub, the AUR and Homebrew keep their own public counts of views, clones, votes and installs, as they do for every project.

### 3.7 Where the brief is wrong or incomplete

1. **"`get.sh` clones … which GitHub's clone count may include."** `get.sh` never clones. It runs `git init` and then a shallow `git fetch` of the tag (scripts/get.sh:133-140). [V] GitHub counts "full clones (not fetches)". [V] A re-run fetches into a folder that has the last tag, so it's a fetch. Whether GitHub counts the *first* fetch into an empty folder as a full clone, its docs don't say. [I: probably, as a clone is that same exchange]
2. **"GitHub Insights keeps views, clones and referrers for only 14 days."** Right, and popular paths too. [V] But referrers and paths come only as top-10 totals over the 14 days, updated daily, not per day. [V] So the workflow keeps a dated copy of each day's list.
3. **"Whether `GITHUB_TOKEN` can read the traffic API":** it can't (§3.3). [V]
4. **"CI runs" in the install count:** GA's CI never asks gizai.app (ci.yml:118, :134) [V], but it adds to GitHub's clone count many times a day [I].
5. **"Cloudflare Pages connected by Git":** the Pages project serves the site [V]. Git or upload I couldn't see (open question 2).
6. **"AUR votes and popularity":** they count votes, not installs, and popularity fades with time [V]. pkgstats is the better count [V].
7. **"Whatever counts a Homebrew tap offers":** more than a tap's own traffic, as Homebrew's public analytics name third-party taps [V].
8. **"An Action … must not need a pull request":** true with the separate repository. With a branch in GA (Option Q), adding the workflow file to `main` takes one pull request, though its daily runs need none.

## 4. Options

### 4.1 How to count gizai.app's install lines

**A. Cloudflare's own totals, read daily by the stats workflow (recommended).**
- *What it takes:* a Cloudflare API token with Zone → Analytics → Read for `gizai.app` [V: Cloudflare's permission is "Analytics Read: Grants read access to analytics"], the zone ID, and one step in the workflow (§5.3).
- *Libraries:* none; `curl` and `jq` on GitHub's Ubuntu runner. [I]
- *Risks:*
  - Cloudflare could narrow what the free plan's API offers. [I] The dashboard and Option B remain as fallbacks.
  - User agents can be faked ("Can be spoofed", Cloudflare). [V] The client kinds are a rough split.
  - If the site ever moves off the zone, the counts stop. [I]
- *Size:* S, inside card S1.

**B. Count in the website, with a Pages Function and Workers Analytics Engine.**
- *What it takes:* a Function for each of the two paths. It writes one data point (script, kind of client) and returns the 302 itself, because "Redirects defined in the `_redirects` file are not applied to requests served by Pages Functions" [V]. Then an Analytics Engine binding, a `_routes.json` that keeps every other path static, and a SQL query to read it all.
- *Limits:* Analytics Engine is free up to "100,000 included per day" [V] and keeps data "for three months" [V]. Function requests count against the free plan's 100,000 a day, which "resets at midnight UTC" [V].
- *Libraries:* none beyond Cloudflare's runtime.
- *Risks:* the install line would then depend on our code. A bug, or a bot that uses up the day's free requests, could break installs. What happens to a Function past the free limit I didn't check. [I] It is also more like counting *in* the product, which is what the promise is wary of. [I]
- *Size:* M, a GW card plus Cloudflare settings.

**C. Only the dashboard, by hand.**
- *What it takes:* nothing; look at Analytics & Logs → HTTP Traffic now and then.
- *Risks:* no history past about 30 days, no weekly file, and the free plan's filters are unchecked. [I]
- *Size:* none. Good enough until S1 runs.

**Recommendation: A.** It counts exactly what's asked (per day, per script, totals, no IP addresses) at no cost and with no new code in the install path. B gives nothing A doesn't at GizAi's numbers, and it puts code between the one line and GitHub.

### 4.2 Where the numbers live

**P. A new private repository, `Yotech-AI/gizai-stats` (recommended).**
- It holds the workflow, `readme.jq`, `data/` and `README.md`.
- *Pros:*
  - The numbers stay private until Jefsev decides.
  - Nothing changes in GA: no workflow, no secrets, no runs in GA's Actions tab.
  - A private repository's schedule isn't switched off after 60 quiet days. [V]
  - It can hold the website's numbers and the packages' numbers too, which aren't GA's.
- *Cons:*
  - One more repository, and one more Gizai project if agents are to build in it.
  - It uses Actions minutes, about 30 of the 2,000 a month. [I/V]

**Q. An orphan branch `stats` in GA (public numbers).**
- The workflow file goes on GA's `main` with one pull request, as "scheduled workflows run on the latest commit on the default branch" [V]. Its daily runs check out `stats`, commit there and push with `GITHUB_TOKEN`.
- *No CI:* ci.yml only runs on pushes to `main` and on pull requests (ci.yml:6-9) [V], and a push by `GITHUB_TOKEN` starts no run [V].
- *Pros:* no new repository, and the numbers are public, which the "No telemetry" section could link to.
- *Cons:*
  - The numbers are public from the first day.
  - The secrets sit in GA, where workflows from pull requests on the agents' card branches can read them. [I]
  - A ruleset on all branches would block the push. [I]
  - Readers of GA's branch list see `stats`.

**R. GW's repository:** rejected. A push to GW's `main` deploys the site, and Pages usually builds a preview for other branches. [I]

**Recommendation: P**, with Q as the answer if Jefsev wants the numbers public (open question 1). The workflow in §5.3 is written for P; §5.3 ends with what changes for Q.

### 4.3 Which token reads GitHub's traffic

- **A fine-grained personal access token (recommended).**
  - Resource owner `Yotech-AI`, only the repositories `Yotech-AI/gizai` (and the tap later), permission Administration: Read-only.
  - "Infinite lifetimes are allowed but may be blocked by a maximum lifetime policy set by your organization". If the organisation requires approval, the token is "pending until it is reviewed by an organization administrator". [V: GitHub Docs]
  - I'd give it one year and a calendar reminder. The workflow fails, and mails Jefsev, when it expires. [I]
- **A GitHub App** with Administration: read, installed on those repositories. Installation tokens work for these endpoints. [V] It doesn't expire and isn't tied to Jefsev's account, but it needs an app, a private key and a token step. Worth it later, if the yearly token gets in the way. [I]

## 5. The design (Options A and P)

### 5.1 Screens

There is no screen in the app or on the website. The only thing anyone reads is `README.md` in the stats repository, shown on its GitHub page:

```
┌ Yotech-AI/gizai-stats (private) ─────────────────────────────────────────────┐
│ # GizAi in numbers                                                            │
│ Written on 2026-11-09 by the Stats workflow ...                               │
│                                                                               │
│ ## Week by week                                                               │
│ Week of     install.sh  install.ps1  Other  GitHub views  GitHub clones  Tap  │
│ 2026-11-02          38           11      7     1204 (388)       77 (31)    –  │
│ 2026-10-26          ...                                                       │
│                                                                               │
│ ## Packages          ## Where GitHub's visitors came from   ## Most viewed    │
│ ## What they measure                                                          │
└───────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 The data

Every file is JSON, keyed by date (`YYYY-MM-DD`, UTC) or month, with sorted keys, so a day's commit is a small diff. [design]

| File | What | Kept by the source | How a run merges it |
|---|---|---|---|
| `data/website/install-lines.json` | `{"2026-10-12": {"install.sh": {"curl": 17, "browser": 3, "bot": 2}, "install.ps1": {"powershell": 4}}}` | ≥31 days (Cloudflare) [V] | the three finished days before today are asked again and replaced |
| `data/github/gizai/views-day.json`, `clones-day.json` | `{"2026-10-12": {"count": 212, "uniques": 41}}` | 14 days [V] | each day keeps the highest numbers it was seen with |
| `data/github/gizai/views-week.json`, `clones-week.json` | the same, keyed by each week's Monday | 14 days [V] | the highest (a week's numbers shrink again as its first days leave the 14 days) [I] |
| `data/github/gizai/referrers/<date>.json`, `paths/<date>.json` | GitHub's answer as it is: the top 10 over the 14 days to that date | 14 days [V] | one file a day, never merged |
| `data/github/homebrew-tap/…` | the same, for the tap, once it exists | 14 days | the same |
| `data/packages/aur.json` | `{"2026-11-09": {"votes": 3, "popularity": 2.71}}` | the RPC gives only today's value [I] | one entry a week |
| `data/packages/pkgstats.json` | `{"202610": {"count": 4, "samples": 31800, "popularity": 0.01}}` | monthly [V] | one entry a month |
| `data/packages/homebrew.json` | `{"2026-11-09": {"installs_30d": 9, "installs_90d": 9, "installs_365d": 9}}` | events 365 days [V] | one entry a week |

**Why "the highest":** GitHub's numbers for a day grow until the day is over, and then stay the same until the day leaves the 14 days. The workflow runs every day, so each day is read about 13 times after it is over, and its highest value is its final one. [I] Cloudflare's days are over when they are read, so they're simply replaced.

### 5.3 The workflow, in full (draft, untested)

Two files in `Yotech-AI/gizai-stats`: `.github/workflows/stats.yml` and `readme.jq`. Nothing else is needed: the first run makes `data/`, and the first Monday run, or a run by hand, writes `README.md`.

`.github/workflows/stats.yml`:

```yaml
# Saves GizAi's numbers once a day, in this repository:
# - GitHub's traffic for Yotech-AI/gizai (views, clones, referrers and popular paths), which GitHub keeps for 14 days
#   only, and the same for the Homebrew tap once vars.TAP_REPO names it;
# - how often gizai.app's two install lines were fetched, from Cloudflare's own analytics: totals per day, per script
#   and per kind of client. Never an IP address (the query doesn't ask for one) or a user agent.
# On Mondays, and when run by hand, it also saves the packages' numbers (the AUR, pkgstats, Homebrew) and writes
# README.md from readme.jq. It only reads GitHub, Cloudflare and the package sites, and commits only here.
name: Stats

on:
  schedule:
    - cron: "17 3 * * *" # 03:17 UTC: yesterday is over, and it's away from the start of the hour, when runs queue
  workflow_dispatch:

permissions:
  contents: write # to commit the numbers here; nothing else

concurrency:
  group: stats
  cancel-in-progress: false

defaults:
  run:
    shell: bash

env:
  REPO: Yotech-AI/gizai
  TAP: ${{ vars.TAP_REPO }} # Yotech-AI/homebrew-tap once it exists; empty until then
  AUR_PACKAGE: gizai
  FORMULA: yotech-ai/tap/gizai

jobs:
  save:
    runs-on: ubuntu-24.04
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4

      # Views and clones per day and per week, and the top 10 referrers and paths, over GitHub's last 14 days. A day's
      # numbers grow until it is over, and a week's shrink again as its first days leave the 14 days: so each day and
      # week keeps the highest numbers it was seen with. Referrers and paths are kept as GitHub gives them, one file a
      # day. The token needs Administration: read on these repositories (GITHUB_TOKEN can't have it).
      - name: GitHub traffic
        env:
          GH_TOKEN: ${{ secrets.TRAFFIC_TOKEN }}
        run: |
          set -euo pipefail
          today=$(date -u +%F)
          for repo in $REPO $TAP; do
            dir="data/github/${repo#*/}"
            mkdir -p "$dir/referrers" "$dir/paths"
            for kind in views clones; do
              for per in day week; do
                gh api "repos/$repo/traffic/$kind?per=$per" > new.json
                file="$dir/$kind-$per.json"
                [ -f "$file" ] || echo '{}' > "$file"
                jq -S --slurpfile new new.json --arg kind "$kind" '
                  reduce $new[0][$kind][] as $r (.;
                    $r.timestamp[0:10] as $d
                    | .[$d] = {count: ([.[$d].count // 0, $r.count] | max),
                               uniques: ([.[$d].uniques // 0, $r.uniques] | max)})' "$file" > merged.json
                mv merged.json "$file"
              done
            done
            gh api "repos/$repo/traffic/popular/referrers" > "$dir/referrers/$today.json"
            gh api "repos/$repo/traffic/popular/paths" > "$dir/paths/$today.json"
          done
          rm -f new.json

      # gizai.app's install lines: the three days before today, which are over, asked again in case Cloudflare's
      # numbers came in late. Requests from real clients only (requestSource "eyeball"), answered without an error.
      # What goes into the file: per day, per script and per kind of client, a count. Not the user agent itself.
      - name: gizai.app's install lines
        env:
          CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}
          ZONE_ID: ${{ vars.CLOUDFLARE_ZONE_ID }}
        run: |
          set -euo pipefail
          mkdir -p data/website
          file=data/website/install-lines.json
          [ -f "$file" ] || echo '{}' > "$file"
          query='query ($zone: string!, $from: Time!, $to: Time!) {
            viewer { zones(filter: {zoneTag: $zone}) {
              httpRequestsAdaptiveGroups(limit: 10000, filter: {
                datetime_geq: $from, datetime_lt: $to, requestSource: "eyeball",
                clientRequestHTTPHost: "gizai.app", clientRequestPath_in: ["/install.sh", "/install.ps1"]
              }) { count dimensions { clientRequestPath userAgent edgeResponseStatus } }
            } } }'
          for ago in 3 2 1; do
            day=$(date -u -d "$ago days ago" +%F)
            next=$(date -u -d "$((ago - 1)) days ago" +%F)
            jq -n --arg q "$query" --arg zone "$ZONE_ID" --arg from "${day}T00:00:00Z" --arg to "${next}T00:00:00Z" \
              '{query: $q, variables: {zone: $zone, from: $from, to: $to}}' > body.json
            curl -fsS https://api.cloudflare.com/client/v4/graphql \
              -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" -H "Content-Type: application/json" \
              --data @body.json > answer.json
            if ! jq -e '.errors == null and .data.viewer.zones[0] != null' answer.json > /dev/null; then
              jq '.errors' answer.json
              exit 1
            fi
            jq -S --slurpfile a answer.json --arg day "$day" '
              def client:
                if test("^curl/") then "curl"
                elif test("PowerShell") then "powershell"
                elif test("bot|crawl|spider|preview|facebookexternalhit"; "i") then "bot"
                elif test("^Mozilla/") then "browser"
                else "other" end;
              .[$day] = (reduce ($a[0].data.viewer.zones[0].httpRequestsAdaptiveGroups[]
                                 | select(.dimensions.edgeResponseStatus < 400)) as $g ({};
                .[$g.dimensions.clientRequestPath | ltrimstr("/")][$g.dimensions.userAgent // "" | client] += $g.count))
            ' "$file" > merged.json
            mv merged.json "$file"
          done
          rm -f body.json answer.json

      # Once a week: the AUR's votes and popularity, pkgstats' last month and Homebrew's install counts. Each one is
      # skipped while its package isn't published; a site that doesn't answer gives a warning, not a failure.
      - name: Packages (Mondays, and when run by hand)
        run: |
          set -euo pipefail
          [ "$(date -u +%u)" = 1 ] || [ "$GITHUB_EVENT_NAME" = workflow_dispatch ] || exit 0
          today=$(date -u +%F)
          mkdir -p data/packages
          for f in aur pkgstats homebrew; do [ -f "data/packages/$f.json" ] || echo '{}' > "data/packages/$f.json"; done
          # The AUR's RPC (curl's --globoff, for the [] in arg[]): NumVotes and Popularity. No package: no result.
          # A bot check answers with a web page, not JSON: that's a warning too.
          if curl -fsS --globoff "https://aur.archlinux.org/rpc/v5/info?arg[]=$AUR_PACKAGE" > new.json \
              && jq -e '.resultcount' new.json > /dev/null 2>&1; then
            if [ "$(jq '.resultcount' new.json)" = 1 ]; then
              jq -S --slurpfile n new.json --arg day "$today" \
                '.[$day] = ($n[0].results[0] | {votes: .NumVotes, popularity: .Popularity})' data/packages/aur.json > merged.json
              mv merged.json data/packages/aur.json
            fi
          else
            echo "::warning::The AUR's RPC didn't answer: no AUR numbers this week."
          fi
          # pkgstats: of the Arch systems that sent their package list last month, how many had the package.
          if [ "$(jq 'length' data/packages/aur.json)" -gt 0 ]; then
            if curl -fsS "https://pkgstats.archlinux.de/api/packages/$AUR_PACKAGE" > new.json; then
              jq -S --slurpfile n new.json '.[$n[0].startMonth | tostring] = ($n[0] | {count, samples, popularity})' \
                data/packages/pkgstats.json > merged.json
              mv merged.json data/packages/pkgstats.json
            else
              echo "::warning::pkgstats didn't answer."
            fi
          fi
          # Homebrew's public analytics: installs of the formula over the last 30, 90 and 365 days ("1,203" is text).
          counts='{}'
          for days in 30 90 365; do
            if n=$(curl -fsS "https://formulae.brew.sh/api/analytics/install/${days}d.json" \
                | jq --arg f "$FORMULA" '[.items[] | select(.formula == $f) | .count | gsub(","; "") | tonumber] | first'); then
              counts=$(jq -c --arg k "installs_${days}d" --argjson n "$n" '.[$k] = $n' <<< "$counts")
            else
              echo "::warning::Homebrew's ${days}-day analytics didn't answer."
            fi
          done
          if [ "$(jq '.installs_365d' <<< "$counts")" != null ]; then
            jq -S --arg day "$today" --argjson c "$counts" '.[$day] = $c' data/packages/homebrew.json > merged.json
            mv merged.json data/packages/homebrew.json
          fi
          rm -f new.json

      # README.md: the last 8 weeks, the packages, the referrers and the most viewed pages, and what they measure.
      - name: README.md (Mondays, and when run by hand)
        run: |
          set -euo pipefail
          [ "$(date -u +%u)" = 1 ] || [ "$GITHUB_EVENT_NAME" = workflow_dispatch ] || exit 0
          echo '{}' > "$RUNNER_TEMP/none.json"
          have() { if [ -n "$1" ] && [ -f "$1" ]; then echo "$1"; else echo "$RUNNER_TEMP/none.json"; fi; }
          newest() { ls "$1"/*.json 2>/dev/null | tail -n 1 || true; }
          ref=$(newest data/github/gizai/referrers)
          paths=$(newest data/github/gizai/paths)
          jq -nr -f readme.jq \
            --arg today "$(date -u +%F)" \
            --arg monday "$(date -u -d "$(( $(date -u +%u) - 1 )) days ago" +%F)" \
            --arg refday "$(basename "${ref:-none.json}" .json)" \
            --slurpfile lines "$(have data/website/install-lines.json)" \
            --slurpfile views "$(have data/github/gizai/views-week.json)" \
            --slurpfile clones "$(have data/github/gizai/clones-week.json)" \
            --slurpfile tap "$(have data/github/homebrew-tap/clones-week.json)" \
            --slurpfile referrers "$(have "$ref")" \
            --slurpfile paths "$(have "$paths")" \
            --slurpfile aur "$(have data/packages/aur.json)" \
            --slurpfile pkgstats "$(have data/packages/pkgstats.json)" \
            --slurpfile brew "$(have data/packages/homebrew.json)" \
            > README.md

      # One commit a day, with this repository's GITHUB_TOKEN: it starts no workflow run.
      - name: Commit
        run: |
          set -euo pipefail
          git config user.name "github-actions[bot]"
          git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
          git add -A
          if git diff --cached --quiet; then echo "Nothing new."; exit 0; fi
          git commit -q -m "Numbers of $(date -u +%F)"
          git push -q
```

`readme.jq`:

```jq
# Writes README.md from the files in data/ (the Stats workflow runs it on Mondays, and when run by hand). Weeks run
# Monday to Sunday, in UTC. "–" means no numbers: from before they were saved, or a package not published yet.

def secs: strptime("%Y-%m-%d") | mktime;
def plus($days): (secs + $days * 86400) | strftime("%Y-%m-%d");
def show: if . == null then "–" else tostring end;
def pair: if . == null then "–" else "\(.count) (\(.uniques))" end;
# f of each saved day of the week that starts on $w, added up; null when none of its days was saved
def week($w; f): [range(0; 7) as $i | $lines[0][$w | plus($i)] | select(. != null) | f]
  | if length == 0 then null else add end;
def runs_sh: .["install.sh"].curl // 0;
def runs_ps1: .["install.ps1"].powershell // 0;
def others: ([.[][]] | add // 0) - runs_sh - runs_ps1;
def newest: to_entries | last;

($monday | [range(1; 9) as $k | plus(-7 * $k)]) as $weeks
| [
    "# GizAi in numbers",
    "",
    "Written on \($today) by the Stats workflow, from the files in `data/`. Weeks run Monday to Sunday, in UTC.",
    "These count requests, clones, votes and installs, not people: see [What they measure](#what-they-measure).",
    "",
    "## Week by week",
    "",
    "| Week of | install.sh with curl | install.ps1 with PowerShell | Other fetches of the two | GitHub views (visitors) | GitHub clones (cloners) | Tap clones (cloners) |",
    "|---|--:|--:|--:|--:|--:|--:|",
    ($weeks[] as $w
      | "| \($w) | \(week($w; runs_sh) | show) | \(week($w; runs_ps1) | show) | \(week($w; others) | show) | \($views[0][$w] | pair) | \($clones[0][$w] | pair) | \($tap[0][$w] | pair) |"),
    "",
    "## Packages",
    "",
    "| Where | Saved | Numbers |",
    "|---|---|---|",
    ($aur[0] | newest | if . == null then "| AUR, `gizai` | – | not published yet |"
      else "| AUR, `gizai` | \(.key) | \(.value.votes) votes, popularity \(.value.popularity) |" end),
    ($pkgstats[0] | newest | if . == null then "| pkgstats, `gizai` | – | – |"
      else "| pkgstats, `gizai` | month \(.key) | on \(.value.count) of the \(.value.samples) Arch systems that sent their list (\(.value.popularity)%) |" end),
    ($brew[0] | newest | if . == null then "| Homebrew, `yotech-ai/tap/gizai` | – | not published yet |"
      else "| Homebrew, `yotech-ai/tap/gizai` | \(.key) | \(.value.installs_30d | show) installs in 30 days, \(.value.installs_90d | show) in 90, \(.value.installs_365d | show) in 365 |" end),
    "",
    "## Where GitHub's visitors came from",
    "",
    "The top 10 sites over the 14 days to \($refday), as GitHub gives them (it leaves out search engines and GitHub itself).",
    "",
    "| Site | Views | Visitors |",
    "|---|--:|--:|",
    (($referrers[0] // [])[] | "| \(.referrer) | \(.count) | \(.uniques) |"),
    "",
    "## Most viewed on GitHub",
    "",
    "The top 10 pages of Yotech-AI/gizai over the same 14 days.",
    "",
    "| Page | Views | Visitors |",
    "|---|--:|--:|",
    (($paths[0] // [])[] | "| \(.path) | \(.count) | \(.uniques) |"),
    "",
    "## What they measure",
    "",
    "- **install.sh and install.ps1** are requests for gizai.app's two install lines, from Cloudflare's own totals per day; no IP address is kept. One run of the line is one request. People run it to install, to update from a terminal, again after installing a missing tool, and for `--check` and `--uninstall`, so it counts more runs than installs. GizAi's own update never asks gizai.app.",
    "- **Other fetches** are the two addresses opened in a browser (to read the script), link previews, crawlers and other programs, sorted by their user agent, which can be faked.",
    "- **GitHub views** are page views on github.com/Yotech-AI/gizai, and *visitors* GitHub's unique count for the week.",
    "- **GitHub clones** are full clones of Yotech-AI/gizai (GitHub doesn't count fetches): builds from a clone, GizAi's own CI, bots, and probably a computer's first one-line install or first update. *Cloners* are GitHub's unique count.",
    "- **Tap clones** are full clones of the Homebrew tap: mostly a Mac's first `brew install yotech-ai/tap/gizai`.",
    "- **AUR votes** come from AUR users who chose to vote; *popularity* adds up the votes, each worth 2% less every day since it was cast.",
    "- **pkgstats** counts the Arch systems that run pkgstats (their owners opted in) and had the package installed that month.",
    "- **Homebrew installs** come from Homebrew's public analytics, from Macs that haven't switched them off.",
    ""
  ]
| join("\n")
```

**For Option Q (a branch in GA) instead:** the workflow goes into GA as `.github/workflows/stats.yml`, and `readme.jq` as `.github/stats/readme.jq`, with one pull request into `main`.
- Its first step checks out `main` with `sparse-checkout: .github`. A second `actions/checkout@v4` with `ref: stats` and `path: stats` checks out the data, and every step runs in `stats/` with `-f ../.github/stats/readme.jq`.
- The commit step ends with `git push -q origin HEAD:stats`.
- `REPO` is `${{ github.repository }}`, and both secrets and the variables go into GA's settings.
- Someone makes the `stats` branch once by hand: `git switch --orphan stats && git commit --allow-empty -m "Numbers" && git push origin stats`.
- No CI run starts: ci.yml runs only on pushes to `main` and on pull requests (ci.yml:6-9). [V]

What I verified of this draft, and what not:
- **Verified:** the endpoints, permissions and response fields (`count`, `uniques`, `clones[]`/`views[]` with `timestamp`; `referrer`; `path`, `title`) [V: GitHub REST]. The GraphQL filters and operators (`_geq`, `_lt`, `_in`), `requestSource`, `clientRequestPath`, `userAgent`, `clientRequestHTTPHost`, `edgeResponseStatus` and `limit: 10000` [V: Cloudflare docs]. `--globoff` and the AUR, pkgstats and Homebrew fields [V].
- **Run here, in parts:** jq 1.8.2 ran the building blocks on sample answers. Sorting a sample Cloudflare answer by script and client, with a 403 left out, gave `{"install.ps1":{"powershell":3},"install.sh":{"bot":2,"browser":1,"curl":3,"other":2}}`. The nested `+=` counts, the date arithmetic of `readme.jq` (2026-11-09 minus 7 days is 2026-11-02), `"1,203"` to 1203 and the "–" helpers also worked. [V] The whole programs couldn't run here, as this run refuses any command with a `$` in it.
- **Checked by card S1 first:**
  - the variable types `string!` and `Time!` (Cloudflare's examples use `string` [V]; `Time` I didn't see written) [I];
  - that the free plan allows a one-day range [I];
  - that requests for the Pages custom domain appear in the zone's `httpRequestsAdaptiveGroups` [I];
  - that `jq` 1.7 on `ubuntu-24.04` runs `readme.jq` as written [I].
- **The user agents:** PowerShell 7 sends `Mozilla/5.0 (<platform>; <OS>; <culture>) PowerShell/<version>` (PowerShell's `PSUserAgent.cs`). [V] Windows PowerShell 5.1's `WindowsPowerShell/5.1…` [I] matches the same `PowerShell` test. curl sends `curl/<version>` unless told otherwise. [I]

### 5.4 The numbers file: format and sample

A sample `README.md`, with made-up numbers, as `readme.jq` writes it on Monday 9 November 2026:

```markdown
# GizAi in numbers

Written on 2026-11-09 by the Stats workflow, from the files in `data/`. Weeks run Monday to Sunday, in UTC.
These count requests, clones, votes and installs, not people: see [What they measure](#what-they-measure).

## Week by week

| Week of | install.sh with curl | install.ps1 with PowerShell | Other fetches of the two | GitHub views (visitors) | GitHub clones (cloners) | Tap clones (cloners) |
|---|--:|--:|--:|--:|--:|--:|
| 2026-11-02 | 38 | 11 | 7 | 1204 (388) | 640 (52) | 3 (3) |
| 2026-10-26 | 41 | 9 | 12 | 1391 (402) | 702 (49) | – |
| 2026-10-19 | 25 | 6 | 4 | 980 (311) | 655 (44) | – |
| 2026-10-12 | – | – | – | 1120 (350) | 811 (47) | – |
| 2026-10-05 | – | – | – | – | – | – |
| … (8 weeks in all) |

## Packages

| Where | Saved | Numbers |
|---|---|---|
| AUR, `gizai` | 2026-11-09 | 3 votes, popularity 2.71 |
| pkgstats, `gizai` | month 202610 | on 4 of the 31800 Arch systems that sent their list (0.01%) |
| Homebrew, `yotech-ai/tap/gizai` | 2026-11-09 | 9 installs in 30 days, 9 in 90, 9 in 365 |

## Where GitHub's visitors came from

The top 10 sites over the 14 days to 2026-11-09, as GitHub gives them (it leaves out search engines and GitHub itself).

| Site | Views | Visitors |
|---|--:|--:|
| gizai.app | 412 | 160 |
| news.ycombinator.com | 96 | 81 |

## Most viewed on GitHub
…

## What they measure
…
```

### 5.5 The settings Jefsev changes by hand

Nothing in this list is done by this card. In order:

1. **GitHub: make the repository** `Yotech-AI/gizai-stats`: private, empty, default branch `main`. Leave the default rules alone: the workflow pushes to `main` itself, so `main` needs no pull-request rule.
2. **Gizai: add it as a project** (for example key `GS`), so cards S1 and S2 can be built there. Or paste §5.3's two files into the repository yourself, and S1 becomes a check.
3. **GitHub: make the fine-grained token.** Settings → Developer settings → Personal access tokens → Fine-grained tokens → Generate new token. [V path]
   - Name: "GizAi stats: traffic". Resource owner: Yotech-AI.
   - Only select repositories: `Yotech-AI/gizai`. Repository permissions → Administration: Read-only.
   - Expiration: one year, with a reminder in your calendar.
   - If the organisation asks for approval, approve it in Yotech-AI → Settings → Personal access tokens → Pending requests. [V: "pending until it is reviewed"; I: the menu's place]
4. **GitHub: the secret.** In `gizai-stats`, Settings → Secrets and variables → Actions: add `TRAFFIC_TOKEN` with that token.
5. **Cloudflare: make the API token.** My Profile → API Tokens → Create Token → Custom token: permission Zone → Analytics → Read, for the zone `gizai.app` only. Save it in `gizai-stats` as the secret `CLOUDFLARE_API_TOKEN`. With the cf CLI (CLAUDE.md asks for it, and GW has no Wrangler configuration): `cf auth login`, then `cf cli search "create an API token"` finds the command. I couldn't run cf here, so I name no exact command. [I]
6. **Cloudflare: the zone ID.** gizai.app → Overview → API → Zone ID (or `cf cli search "list zones"`). Save it in `gizai-stats` as the *variable* `CLOUDFLARE_ZONE_ID`; it isn't secret.
7. **Once the tap exists (GA-165):** edit the token to add `Yotech-AI/homebrew-tap`, and add the variable `TAP_REPO` = `Yotech-AI/homebrew-tap`.
8. **After S1 merges:** Actions → Stats → Run workflow, once, to see the first numbers. A token that expires shows as a failed run and a mail to whoever last changed the cron line. [V]
9. **Just look, change nothing:** in Cloudflare, Workers & Pages → `gizai-website` → Settings: is it built from Git (`Yotech-AI/gizai-website`, branch `main`)? Is `gizai.app` its custom domain? Is Web Analytics off? It can stay off, as it adds a script to the site and counts nothing here. Tell the card's comments what you see (open question 2).

### 5.6 Security and privacy

- **What we keep:**
  - per day: counts per script and kind of client;
  - GitHub's own totals and uniques;
  - the referring sites and repository pages GitHub names;
  - the package sites' public totals.
- **What we don't keep:** IP addresses, user agents, rows per request, cookies, anything from the app. The Cloudflare query never asks for an IP address (§5.3). [design]
- **What Cloudflare sees:** each request, with its IP address, as it does today for every page of the site it serves. What Cloudflare keeps is its own policy, and the free plan doesn't change it. [I] The sentence in §3.6 says "keeps", about us, for that reason.
- **The tokens only read:**
  - Administration: read on one or two repositories. That also shows those repositories' settings, so it covers no other repository. [I]
  - Analytics: read on one zone.
  - Both are secrets of the private repository. A workflow on a pull request from one of the repository's own branches gets its secrets, so a card that edits the workflow could print them. [I] So review S1's and S2's pull requests as usual. GitHub masks secrets in logs. [I]
- **No third-party actions:** only `actions/checkout@v4`, as GA's CI uses (ci.yml:37). [V]
- **Public or not:** the numbers are private in Option P (open question 1). Option Q makes them public from day one.

### 5.7 How it's tested

- **Card S1, by hand:**
  - First the `settings` query (§3.1) for `httpRequestsAdaptiveGroups`: enabled, `maxDuration`, `notOlderThan`.
  - Then one day's install-lines query, compared with the dashboard's count for `/install.sh` on that day.
  - Then a run by hand (`workflow_dispatch`), and a second one the same day: no number may go down, and a run with nothing new commits nothing.
  - Then a run with a wrong `CLOUDFLARE_ZONE_ID`, which must fail and show Cloudflare's error.
- **Card S2:** the package rows, tested with stand-ins for `AUR_PACKAGE` and `FORMULA` (`yay` and `anomalyco/tap/opencode` exist today), then with GizAi's own packages once published.
- **Optional, by hand, once:** whether GitHub counts a fetch into an empty folder as a clone.
  - Make a throwaway public repository.
  - In a scratch folder, run `git init` and `git fetch --depth 1 <url> +refs/tags/v1:refs/tags/v1`.
  - Look at its Insights → Traffic an hour later (clones "update hourly" [V]).
  - The result tells what "GitHub clones" means for one-line installs (§3.2).

### 5.8 What is left out

- Any change to the app, `get.sh` or `get.ps1`, or what they send (out of scope).
- Unique installers: these would need IP addresses or IDs.
- Counting updates from inside GizAi: that would be telemetry.
- A web page with the numbers.
- Cloudflare Web Analytics (a script in the page, which curl never runs).
- Logpush.
- Counts of GitHub's source tarballs (open question 5).
- GW's own GitHub traffic: the repository is private, and its visitors are us.
- A separate `gizai.app/update.sh` to tell terminal updates from installs (open question 6).

## 6. Proposed cards

In order. None needs a screen design, so all start in **To do**. There is no frontend card, so there is no backend–frontend contract. The contract between S1 and S2 is the file formats of §5.2, word for word.

### S1. Stats: save GitHub's traffic and gizai.app's install lines every day, and the week's numbers in README.md

- **Project:** the stats repository's (step 2 of §5.5), or GA under Option Q (open question 1).
- **Column:** To do. **Role:** backend (the DevOps Agent can take it).
- **Builds on:** §5.5 steps 1 to 6 (the repository, the token, the two secrets and the variable).
- **Builds:** `.github/workflows/stats.yml` and `readme.jq` from §5.3, fixed where the checks of §5.7 show they must be. Every fix goes into the summary. Its pull request goes into the stats repository's `main`. Nothing in `Yotech-AI/gizai` changes.
- **Acceptance:**
  - [ ] The `settings` query's answer for `httpRequestsAdaptiveGroups` (enabled, `maxDuration`, `notOlderThan`) is in the summary.
  - [ ] A run by hand commits `data/github/gizai/{views,clones}-{day,week}.json`, `referrers/<date>.json`, `paths/<date>.json`, `data/website/install-lines.json` and `README.md`.
  - [ ] For one day, `install-lines.json`'s count for `install.sh` matches Cloudflare's dashboard, and the summary gives both.
  - [ ] A second run the same day lowers no number, and a run with nothing new makes no commit.
  - [ ] `install-lines.json` holds only dates, the two script names, the five kinds of client and counts: no IP address, no user agent.
  - [ ] A wrong token or zone ID makes the run fail, with Cloudflare's or GitHub's error in its log.
  - [ ] No workflow file in `Yotech-AI/gizai` changes, and GA's CI starts no run.

### S2. Stats: the AUR and Homebrew numbers, once the packages are published

- **Project:** the same as S1. **Column:** To do. **Role:** backend.
- **Builds on:** S1, and GA-165's packages, published (its cards PK3 and PK4).
- **Builds:** the "Packages" step and `readme.jq`'s Packages rows, tested as in §5.7, and fixed where the sites answer differently.
  - If the AUR's RPC turns GitHub's runners away (aur.archlinux.org asks browsers for a bot check), the summary says so. The AUR row then says to read the package's page by hand.
  - It adds the tap's traffic once §5.5 step 7 is done.
- **Acceptance:**
  - [ ] With the stand-ins (`yay`, `anomalyco/tap/opencode`), a run by hand fills `aur.json`, `pkgstats.json` and `homebrew.json`, and README.md shows them.
  - [ ] With `gizai` and `yotech-ai/tap/gizai`, the rows show GizAi's numbers, or "not published yet".
  - [ ] The tap's clones show in the week-by-week table.
  - [ ] A package site that doesn't answer gives a warning in the run, not a failed run.

### G1 (GA). "No telemetry" says what the website counts

- **Column:** To do. **Role:** backend (docs).
- **Builds on:** S1, merged and run for a week, so the sentence describes what is really counted. It can run at the same time as W1.
- **Builds:** the sentence of §3.6 in `docs/install.md`, after the first paragraph of "No telemetry" (docs/install.md:155). Also the second sentence, if open question 8 says yes, and a link to the numbers, if open question 1 makes them public.
- **Acceptance:**
  - [ ] The sentence is §3.6's, word for word, or as Jefsev changed it in this card's review.
  - [ ] It names no raw GitHub address. scripts/get.test.mjs:469 checks that `docs/install.md` shows gizai.app's lines and no raw GitHub address, and `npm test` passes.
  - [ ] Nothing else in the docs changes.

### W1 (GW). Website README: where gizai.app runs and what it counts

- **Column:** To do. **Role:** backend (docs). **Builds on:** S1. It can run at the same time as G1.
- **Builds:** a short part in GW's `README.md`:
  - gizai.app is served by Cloudflare Pages, project `gizai-website`, with whatever Jefsev found in §5.5 step 9;
  - `public/_redirects` sends the two install lines to `get.sh` and `get.ps1` on GA's `production`;
  - the stats workflow in `Yotech-AI/gizai-stats` reads Cloudflare's totals for exactly those two paths, so renaming them or moving the site off the zone stops the counts.
  - It also replaces "the deploy comes in its own card" (README.md:30 at ffe5aa6) with how the site is deployed now.
- **Acceptance:**
  - [ ] The README names the Pages project, the two paths and the stats repository.
  - [ ] It says the site keeps no IP addresses, in the words of §3.6.
  - [ ] Nothing outside `README.md` changes.

**At the same time:** G1 and W1. S2 waits for GA-165's packages, and can come before or after G1 and W1.

## 7. Open questions for Jefsev

1. **Public or private numbers?** In GA (Option Q) they're public, and the "No telemetry" section could link to them. In `gizai-stats` (Option P) they're private, and making them public later is one switch. *Recommendation:* private now (P). Publish later if you like what they show.
2. **How is the Pages project deployed?** I found it live and serving gizai.app, but couldn't see whether it builds from Git. *Recommendation:* look (§5.5 step 9). Counting doesn't depend on it; W1 writes it down.
3. **Keep the kinds of client, or only one total per script?** *Recommendation:* keep them (curl, PowerShell, browser, bot, other). They show how many fetches are runs of the line, and no user agent is stored.
4. **A token that expires, or a GitHub App?** *Recommendation:* a fine-grained token for a year now, and an App if renewing gets in the way.
5. **Should the AUR package and the tap download a release asset instead of GitHub's tarball?** GitHub would then count each package build per release (`download_count`). *Recommendation:* not now. It adds an upload to every release and changes GA-165's step 5, and Homebrew's analytics and pkgstats already count the packages.
6. **A separate `gizai.app/update.sh` for updates from a terminal**, so they count apart from installs? *Recommendation:* no. Re-runs after a missing tool still mix in, and it's one more line to remember.
7. **Who should get the failure mails?** GitHub sends them to whoever last changed the cron line. *Recommendation:* you. If an agent's commit changes it, change it once yourself, or watch the Actions tab.
8. **The "No telemetry" text: one sentence or two?** *Recommendation:* both of §3.6. The second says plainly that GitHub, the AUR and Homebrew count in their own ways.
9. **Say on the website itself what it counts** (a line in the footer, say)? *Recommendation:* later, with the website's next design card. The docs and GW's README say it first.
10. **The Gizai project for the stats repository:** add it (key `GS`?) so agents build S1 and S2, or paste §5.3 yourself? *Recommendation:* add it. It's two cards now and more later.

## 8. Sources

All read on 2026-10-13.

| Source | Link |
|---|---|
| gizai.app, the site and its redirects (fetched) | https://gizai.app, https://gizai.app/install.sh, https://gizai.app/install.ps1, https://gizai.app/features/board/ |
| Cloudflare's trace for gizai.app | https://gizai.app/cdn-cgi/trace |
| The Pages project's own address | https://gizai-website.pages.dev/, https://gizai-website.pages.dev/install.sh |
| DNS (Google's resolver): NS, A, www | https://dns.google/resolve?name=gizai.app&type=NS, …&type=A, https://dns.google/resolve?name=www.gizai.app&type=CNAME |
| GW pull request #2 (`public/_redirects`, the install lines) | `gh pr view 2` / `gh pr diff 2 --repo Yotech-AI/gizai-website` |
| GA's releases and workflow runs (public API) | https://api.github.com/repos/Yotech-AI/gizai/releases?per_page=5, https://api.github.com/repos/Yotech-AI/gizai/actions/runs?per_page=2&created=2026-10-12 |
| GitHub Docs, Viewing traffic to a repository | https://docs.github.com/en/repositories/viewing-activity-and-data-for-your-repository/viewing-traffic-to-a-repository |
| GitHub REST, Repository traffic | https://docs.github.com/en/rest/metrics/traffic (as https://docs.github.com/api/article/body?pathname=/en/rest/metrics/traffic) |
| GitHub Docs, permissions for fine-grained tokens | https://docs.github.com/en/rest/authentication/permissions-required-for-fine-grained-personal-access-tokens |
| GitHub Docs, permissions for GitHub Apps | https://docs.github.com/en/rest/authentication/permissions-required-for-github-apps |
| GitHub Docs, workflow syntax (`permissions`) | https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax |
| GitHub Docs, events that trigger workflows (`schedule`) | https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows |
| GitHub Docs, triggering a workflow (GITHUB_TOKEN) | https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow |
| GitHub Docs, managing personal access tokens | https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens |
| GitHub Docs, Actions billing (included minutes) | https://docs.github.com/en/billing/concepts/product-billing/github-actions |
| GitHub Community, Actions and traffic stats (2020, secondary) | https://github.com/orgs/community/discussions/23806 |
| Request for source archive download counts (secondary) | https://github.com/isaacs/github/issues/593 |
| Cloudflare, GraphQL Analytics API limits | https://developers.cloudflare.com/analytics/graphql-api/limits/ |
| Cloudflare, GraphQL settings node | https://developers.cloudflare.com/analytics/graphql-api/features/discovery/settings/ |
| Cloudflare, GraphQL filtering | https://developers.cloudflare.com/analytics/graphql-api/features/filtering/ |
| Cloudflare, sampling | https://developers.cloudflare.com/analytics/graphql-api/sampling/ |
| Cloudflare, AI Crawl Control GraphQL API (filters per plan, example query) | https://developers.cloudflare.com/ai-crawl-control/reference/graphql-api/ |
| Cloudflare, API token permissions | https://developers.cloudflare.com/fundamentals/api/reference/permissions/ |
| Cloudflare, Zone analytics | https://developers.cloudflare.com/analytics/account-and-zone-analytics/zone-analytics/ |
| Cloudflare, Pages redirects | https://developers.cloudflare.com/pages/configuration/redirects/ |
| Cloudflare, Pages Functions pricing | https://developers.cloudflare.com/pages/functions/pricing/ |
| Cloudflare, Workers Analytics Engine pricing and limits | https://developers.cloudflare.com/analytics/analytics-engine/pricing/, https://developers.cloudflare.com/analytics/analytics-engine/limits/ |
| The cf CLI (npm `cf` 1.0.0-beta.16; guide of 5 Oct 2026) | `npm view cf`, https://github.com/cloudflare/cf, https://flaviocopes.com/cloudflare-cf/ |
| aurweb (GitHub copy of gitlab.archlinux.org/archlinux/aurweb): `rpc.py`, `popupdate.py`, README | https://raw.githubusercontent.com/archlinux/aurweb/master/aurweb/rpc.py, …/aurweb/scripts/popupdate.py, …/README.md |
| Arch wiki (Kiwix copy, 2026-04): Arch User Repository, pkgstats | https://wikipedia.jakami.de/content/archlinux_en_all_maxi_2026-04/Arch_User_Repository, …/Pkgstats |
| pkgstats API | https://pkgstats.archlinux.de/api/packages/yay |
| Arch package search (yay isn't in the repositories) | https://archlinux.org/packages/search/json/?name=yay |
| Homebrew, Analytics | https://docs.brew.sh/Analytics |
| Homebrew, formulae.brew.sh API and analytics files | https://formulae.brew.sh/docs/api/, https://formulae.brew.sh/api/analytics/install/30d.json, https://formulae.brew.sh/api/analytics/build-error/30d.json |
| PowerShell's user agent | https://raw.githubusercontent.com/PowerShell/PowerShell/master/src/Microsoft.PowerShell.Commands.Utility/commands/utility/WebCmdlet/PSUserAgent.cs |
| GA-165's research (AUR package and tap names, the tarball) | `docs/research/packaging.md` on `origin/gizai/ga-165-research-install-gizai-with-yay-aur-and` (4f8579c) |
