//! The product tour's fake data (scripts/tour-capture.sh, GA-148), on top of `example demo` and `example prep_readme`
//! (whose KADE repository is <repo> and whose Claude Code is <fake claude>), so each of the tour's ten scenes has
//! something to show:
//!  - the board: cards in every column, a few more per project;
//!  - the KADE repository: a small Laravel portal on main, a change not committed yet (the Editor's git status);
//!  - KADE-2 in Review: its branch and worktree with two commits, pull request #42, and the QA Agent's passed run;
//!  - the Team Lead chat: a request, the two cards it made, and a third card waiting for your Approve;
//!  - Settings → Coding CLIs: a second Claude Code account (a fake CLAUDE_CONFIG_DIR in <home>), and the limits both
//!    accounts reported, for Usage → Subscription;
//!  - Memory: notes that link each other, enough for a graph;
//!  - the Backend Agent's Tools: its allowed commands and an MCP server;
//!  - the Inbox: the Frontend Agent's question with commands to run for you, and a card waiting for your review.
//! Prints the ids the scenes open, as shell assignments (KADE1=…).
//! usage: cargo run -p gizai-core --example prep_tour -- <data dir> <repo> <home> <fake claude>
use std::fs;
use std::path::Path;
use std::process::Command;

use gizai_core::model::*;
use gizai_core::{chat, clis, comments, db::Db, ids, limits, mcp_servers, memory, pending, projects, runs, seed::ensure_seed, tasks, team, workflow};
use serde_json::json;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(dir)
        .args(["-c", "user.email=agents@kade.example", "-c", "user.name=Gizai", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"])
        .args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

fn commit(dir: &Path, message: &str) -> String {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

/// The Kade portal on main: a small Laravel app.
const PORTAL: &[(&str, &str)] = &[
    ("README.md", "# Kade portal\n\nOne place for Kade Logistics' office: shipments, invoices and drivers.\n\n## Develop\n\n```sh\ncomposer install\nphp artisan migrate --seed\nphp artisan serve\n```\n"),
    ("composer.json", "{\n    \"name\": \"kade/portal\",\n    \"type\": \"project\",\n    \"require\": {\n        \"php\": \"^8.3\",\n        \"filament/filament\": \"^4.0\",\n        \"laravel/framework\": \"^12.0\"\n    },\n    \"require-dev\": {\n        \"pestphp/pest\": \"^3.0\"\n    }\n}\n"),
    ("app/Models/Invoice.php", "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\nuse Illuminate\\Database\\Eloquent\\Relations\\BelongsTo;\n\nclass Invoice extends Model\n{\n    protected $casts = [\n        'date' => 'date',\n        'amount' => 'decimal:2',\n    ];\n\n    public function client(): BelongsTo\n    {\n        return $this->belongsTo(Client::class);\n    }\n}\n"),
    ("app/Models/Shipment.php", "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass Shipment extends Model\n{\n    protected $casts = ['delivered_at' => 'datetime'];\n\n    public function trackingCode(): string\n    {\n        return strtoupper($this->code);\n    }\n}\n"),
    ("app/Models/Driver.php", "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass Driver extends Model\n{\n    protected $fillable = ['name', 'phone', 'licence'];\n}\n"),
    ("app/Exports/InvoiceExporter.php", "<?php\n\nnamespace App\\Exports;\n\nuse Filament\\Actions\\Exports\\Exporter;\n\nclass InvoiceExporter extends Exporter\n{\n}\n"),
    ("app/Filament/Resources/InvoiceResource.php", "<?php\n\nnamespace App\\Filament\\Resources;\n\nuse App\\Models\\Invoice;\nuse Filament\\Resources\\Resource;\nuse Filament\\Tables\\Table;\n\nclass InvoiceResource extends Resource\n{\n    protected static ?string $model = Invoice::class;\n\n    public static function table(Table $table): Table\n    {\n        return $table\n            ->columns([])\n            ->headerActions([]);\n    }\n}\n"),
    ("app/Http/Controllers/TrackingController.php", "<?php\n\nnamespace App\\Http\\Controllers;\n\nuse App\\Models\\Shipment;\n\nclass TrackingController extends Controller\n{\n    public function show(string $code)\n    {\n        $shipment = Shipment::where('code', $code)->firstOrFail();\n\n        return [\n            'code' => $shipment->trackingCode(),\n            'status' => $shipment->status,\n            'delivered_at' => $shipment->delivered_at,\n        ];\n    }\n}\n"),
    ("routes/web.php", "<?php\n\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::get('/', fn () => redirect('/admin'));\n"),
    ("routes/api.php", "<?php\n\nuse App\\Http\\Controllers\\TrackingController;\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::get('/track/{code}', [TrackingController::class, 'show']);\n"),
    ("config/tracking.php", "<?php\n\nreturn [\n    'public' => true,\n    'cache_seconds' => 30,\n];\n"),
    ("tests/Feature/TrackingTest.php", "<?php\n\nuse App\\Models\\Shipment;\n\nit('shows a shipment by its code', function () {\n    $shipment = Shipment::factory()->create(['code' => 'kd-1042']);\n\n    $this->getJson('/api/track/kd-1042')\n        ->assertOk()\n        ->assertJson(['code' => 'KD-1042']);\n});\n"),
    ("database/migrations/2026_09_01_000000_create_shipments_table.php", "<?php\n\nuse Illuminate\\Database\\Migrations\\Migration;\nuse Illuminate\\Database\\Schema\\Blueprint;\nuse Illuminate\\Support\\Facades\\Schema;\n\nreturn new class extends Migration\n{\n    public function up(): void\n    {\n        Schema::create('shipments', function (Blueprint $table) {\n            $table->id();\n            $table->string('code')->unique();\n            $table->string('status');\n            $table->timestamp('delivered_at')->nullable();\n            $table->timestamps();\n        });\n    }\n};\n"),
];

/// KADE-2's branch: the public tracking endpoint gets a rate limit (two commits).
const RATE_LIMIT_1: &[(&str, &str)] = &[
    ("routes/api.php", "<?php\n\nuse App\\Http\\Controllers\\TrackingController;\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::middleware('throttle:tracking')\n    ->get('/track/{code}', [TrackingController::class, 'show']);\n"),
    ("app/Providers/AppServiceProvider.php", "<?php\n\nnamespace App\\Providers;\n\nuse Illuminate\\Cache\\RateLimiting\\Limit;\nuse Illuminate\\Http\\Request;\nuse Illuminate\\Support\\Facades\\RateLimiter;\nuse Illuminate\\Support\\ServiceProvider;\n\nclass AppServiceProvider extends ServiceProvider\n{\n    public function boot(): void\n    {\n        // the public tracking page: per IP address, so one client can't slow it down for the others\n        RateLimiter::for('tracking', function (Request $request) {\n            return Limit::perMinute(config('tracking.per_minute'))\n                ->by($request->ip())\n                ->response(fn () => response()->json(['message' => 'Too many requests'], 429));\n        });\n    }\n}\n"),
    ("config/tracking.php", "<?php\n\nreturn [\n    'public' => true,\n    'cache_seconds' => 30,\n    // requests per minute per IP address on /api/track\n    'per_minute' => env('TRACKING_PER_MINUTE', 60),\n];\n"),
];
const RATE_LIMIT_2: &[(&str, &str)] = &[
    ("tests/Feature/TrackingRateLimitTest.php", "<?php\n\nuse App\\Models\\Shipment;\n\nit('lets 60 requests a minute through', function () {\n    Shipment::factory()->create(['code' => 'kd-1042']);\n\n    foreach (range(1, 60) as $i) {\n        $this->getJson('/api/track/kd-1042')->assertOk();\n    }\n});\n\nit('answers the 61st with 429 and Retry-After', function () {\n    Shipment::factory()->create(['code' => 'kd-1042']);\n\n    foreach (range(1, 60) as $i) {\n        $this->getJson('/api/track/kd-1042');\n    }\n\n    $this->getJson('/api/track/kd-1042')\n        ->assertStatus(429)\n        ->assertHeader('Retry-After');\n});\n\nit('counts each IP address on its own', function () {\n    Shipment::factory()->create(['code' => 'kd-1042']);\n\n    foreach (range(1, 60) as $i) {\n        $this->getJson('/api/track/kd-1042', ['REMOTE_ADDR' => '10.0.0.1']);\n    }\n\n    $this->withServerVariables(['REMOTE_ADDR' => '10.0.0.2'])\n        ->getJson('/api/track/kd-1042')\n        ->assertOk();\n});\n"),
    ("README.md", "# Kade portal\n\nOne place for Kade Logistics' office: shipments, invoices and drivers.\n\n## Develop\n\n```sh\ncomposer install\nphp artisan migrate --seed\nphp artisan serve\n```\n\n## Public tracking\n\n`GET /api/track/{code}` allows 60 requests a minute per IP address (`TRACKING_PER_MINUTE`).\n"),
];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, repo, home, fake) = (&a[1], Path::new(&a[2]), Path::new(&a[3]), &a[4]);
    let data = Path::new(dir);
    let db = Db::open(&data.join("gizai.db")).unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let you = s.you_id.clone();
    let team = team::get(&db, &s.team_id).unwrap();
    let col = |n: &str| team.states.iter().find(|x| x.name == n).unwrap().id.clone();
    let label = |n: &str| team.labels.iter().find(|x| x.name == n).unwrap().id.clone();
    let agent = |n: &str| team::all_agents(&db).unwrap().into_iter().find(|(_, m)| m.name == n).unwrap().1.actor_id;
    let (lead, frontend, backend, qa) = (agent("Team Lead"), agent("Frontend Agent"), agent("Backend Agent"), agent("QA Agent"));
    let project = |key: &str| projects::list(&db).unwrap().into_iter().find(|p| p.key == key).unwrap();
    let (kade, gfw) = (project("KADE"), project("GFW"));
    let card = |ident: &str| tasks::list(&db, &TaskFilter::default()).unwrap().into_iter().find(|t| t.identifier == ident).unwrap();
    let add = |p: &str, title: &str, column: &str, labels: &[&str], priority: i64, body: &str| {
        let id = tasks::create(&db, &you, TaskInput { project_id: p.into(), title: title.into(), priority, description_md: body.into(),
            label_ids: labels.iter().map(|l| label(l)).collect(), ..Default::default() }).unwrap();
        tasks::move_to(&db, &you, &id, &col(column), "").unwrap();
        id
    };

    // One agent at work: the Backend Agent takes KADE-1 by itself as Gizai starts (the fake keeps working). The others
    // are paused, so they take no other card, and GFW-1 waits behind KADE-1.
    for a in [&frontend, &agent("Design Agent"), &qa] { team::set_agent_status(&db, &you, a, "paused").unwrap(); }
    tasks::update(&db, &you, &card("GFW-1").id, TaskPatch { priority: Some(3), ..Default::default() }).unwrap();
    tasks::update(&db, &you, &card("KADE-1").id, TaskPatch {
        description_md: Some("## Context\n\nKade's office copies invoices into Excel by hand. Add **Export CSV** to the invoices table.\n\n\
- semicolons between fields, like the sample Marloes sent\n- amounts with a comma (`1250,00`)\n- one line per invoice, in the table's order".into()),
        acceptance_md: Some("- [ ] Export CSV downloads the invoices shown in the table\n- [ ] Fields are separated by semicolons\n- [ ] A test covers the export".into()),
        ..Default::default() }).unwrap();

    // Cards in every column
    add(&kade.id, "Driver app: push a notification when a route changes", "Backlog", &["frontend"], 0, "");
    add(&gfw.id, "Gift cards at checkout", "Backlog", &[], 4, "");
    add(&kade.id, "Shipments list: filter by driver and date", "To do", &["frontend"], 3, "");
    add(&kade.id, "Invoice PDF: show the client's VAT number", "Testing", &["backend"], 3, "");
    add(&gfw.id, "Order emails in Dutch and English", "Deploy", &["backend"], 3, "");
    add(&kade.id, "Log in with the office's Microsoft accounts", "Done", &["backend"], 2, "");
    add(&gfw.id, "Product photos load lazily", "Done", &["frontend"], 3, "");

    // A new worktree gets nothing installed (composer would wait for the network): the agent starts at once.
    projects::update(&db, &you, &kade.id, ProjectInput { name: kade.name.clone(), key: kade.key.clone(), client_id: kade.client_id.clone(),
        status: Some(kade.status.clone()), goal_md: kade.goal_md.clone(), repo_path: Some(repo.display().to_string()), color: kade.color.clone(),
        default_branch: Some("main".into()), worktree_install: Some(false), ..Default::default() }).unwrap();

    // the Kade portal on main, with a change not committed yet (the Editor's git status)
    git(repo, &["init", "-q", "-b", "main"]);
    let mut base = String::new();
    for (message, prefixes) in [("The Kade portal: Laravel and Filament", &["README", "composer", "routes/web", "app/Models/Driver"][..]),
                                ("Invoices: the model, its Filament resource and an exporter", &["app/Models/Invoice", "app/Exports", "app/Filament"][..]),
                                ("Public tracking of shipments by their code", &["app/Models/Shipment", "app/Http", "routes/api", "config", "tests", "database"][..])] {
        for (path, body) in PORTAL.iter().filter(|(p, _)| prefixes.iter().any(|x| p.starts_with(x))) { write(repo, path, body); }
        base = commit(repo, message);
    }
    assert_eq!(git(repo, &["status", "--porcelain"]), "", "every PORTAL file is in a commit");
    write(repo, "app/Models/Shipment.php", &PORTAL.iter().find(|(p, _)| *p == "app/Models/Shipment.php").unwrap().1
        .replace("    public function trackingCode", "    public function isLate(): bool\n    {\n        return $this->delivered_at === null && $this->created_at->lt(now()->subDays(2));\n    }\n\n    public function trackingCode"));
    write(repo, "app/Http/Middleware/LogTracking.php", "<?php\n\nnamespace App\\Http\\Middleware;\n\nclass LogTracking\n{\n    // who looks up which shipment, for the office's weekly report\n}\n");

    // KADE-2 in Review: its branch in its worktree, two commits, pull request #42, and the QA Agent's passed run
    let k2 = card("KADE-2");
    let branch = format!("gizai/kade-2-rate-limit-the-public-tracking-endpoint");
    let wt = data.join("worktrees").join(&kade.key).join("KADE-2");
    fs::create_dir_all(wt.parent().unwrap()).unwrap();
    git(repo, &["worktree", "add", "-q", "-b", &branch, &wt.display().to_string(), &base]);
    for (path, body) in RATE_LIMIT_1 { write(&wt, path, body); }
    commit(&wt, "Rate limit /api/track: 60 requests a minute per IP address");
    for (path, body) in RATE_LIMIT_2 { write(&wt, path, body); }
    let c2 = commit(&wt, "Tests for the tracking rate limit, and the README");
    tasks::update(&db, &you, &k2.id, TaskPatch { assignee_id: Some(backend.clone()),
        description_md: Some("## Context\n\nThe public tracking page is called by scripts of some clients, hundreds of times a minute. Limit it per IP address.".into()),
        acceptance_md: Some("- [x] At most 60 requests a minute per IP address on /api/track\n- [x] The 61st gets 429 with Retry-After\n- [x] Other IP addresses are not slowed down".into()),
        ..Default::default() }).unwrap();
    db.write(None, |w| {
        w.conn().execute("UPDATE tasks SET branch=?2, pr_url='https://github.com/kade-logistics/portal/pull/42', pr_state='open' WHERE id=?1",
            rusqlite::params![k2.id, branch])?;
        Ok(())
    }).unwrap();
    let wt_s = wt.display().to_string();
    let run = |agent: &str, role: &str, from: &str, to: &str, outcome: &str, summary: &str, cost: i64| {
        let r = runs::create_with_trigger(&db, agent, &k2.id, role, "assigned", &ids::new_id(), &wt_s, &wt_s, &branch, "/dev/null").unwrap();
        runs::set_base_sha(&db, &r, from).unwrap();
        runs::set_head_sha(&db, &r, to).unwrap();
        runs::finish(&db, &r, "succeeded", Some(&Outcome { outcome: outcome.into(), summary: summary.into(), issues: vec![] }), cost, 41_000, 5_200, None).unwrap();
        r
    };
    let built = run(&backend, "backend", &base, &c2, "ready_for_testing",
        "Added a rate limit on /api/track: 60 requests a minute per IP address, set with TRACKING_PER_MINUTE. The 61st gets 429 with Retry-After. Three Pest tests cover it.", 380_000);
    comments::add(&db, &backend, &k2.id, "Added a rate limit on `/api/track`: 60 requests a minute per IP address, set with `TRACKING_PER_MINUTE`. The 61st request gets 429 with Retry-After. Three Pest tests cover it.", Some(&built)).unwrap();
    let tested = run(&qa, "qa", &base, &c2, "qa_pass",
        "Passed. All three acceptance criteria hold: 60 requests went through, the 61st got 429 with Retry-After: 60, and a second IP address was not slowed down. 14 tests pass; pull request #42 is open.", 210_000);
    comments::add(&db, &qa, &k2.id, "**QA passed.** All three acceptance criteria hold:\n\n- 60 requests in a minute went through\n- the 61st got 429 with `Retry-After: 60`\n- a second IP address was not slowed down\n\n14 tests pass. Pull request #42 is open for your review.", Some(&tested)).unwrap();
    tasks::move_to(&db, &qa, &k2.id, &col("Review"), "").unwrap();

    // a second Claude Code account (its folder in the fake home), and the limits both reported (Usage → Subscription)
    fs::create_dir_all(home.join(".claude-2")).unwrap();
    let saved = clis::save(&db, vec![
        clis::Cli { name: "Claude Code 2".into(), kind: "claude_code".into(), command: fake.clone(), env: vec!["CLAUDE_CONFIG_DIR=~/.claude-2".into()], ..Default::default() },
    ]).unwrap();
    let cc2 = saved.iter().find(|c| c.name == "Claude Code 2").unwrap().id.clone();
    db.write(None, |w| {
        w.conn().execute("UPDATE agent_configs SET adapter = ?2 WHERE actor_id IN (?1, ?3)", rusqlite::params![lead, cc2, qa])?;
        Ok(())
    }).unwrap();
    let now = ids::now_ms();
    let (min, hour, day) = (60_000, 3_600_000, 86_400_000);
    let read = |key: &str, used: f64, resets_in: i64, minutes: i64| limits::Reading {
        key: key.into(), used_percent: Some(used), resets_at: Some(now + resets_in), window_minutes: Some(minutes), observed_at: now - 4 * min, ..Default::default()
    };
    limits::record(&db, clis::CLAUDE_CODE, &[read(limits::FIVE_HOUR, 38.0, 2 * hour + 20 * min, 300), read(limits::SEVEN_DAY, 61.0, 3 * day, 10_080)]).unwrap();
    limits::record(&db, &cc2, &[read(limits::FIVE_HOUR, 12.0, 4 * hour, 300), read(limits::SEVEN_DAY, 27.0, 5 * day, 10_080)]).unwrap();

    // GFW-2 in Review is yours to review: it waits in your Inbox
    tasks::update(&db, &you, &card("GFW-2").id, TaskPatch { assignee_id: Some(you.clone()), ..Default::default() }).unwrap();

    // The Inbox's question: the Backend Agent can't reach Kade's SFTP server and asks you to run a command for it
    let sftp = add(&kade.id, "Nightly import of Kade's shipment files over SFTP", "In progress", &["backend"], 3,
        "## Context\n\nKade's warehouse drops a CSV of the day's shipments on its SFTP server at 02:00. Import it every night.");
    tasks::update(&db, &you, &sftp, TaskPatch { assignee_id: Some(backend.clone()), ..Default::default() }).unwrap();
    let sftp_ident = tasks::get(&db, &sftp).unwrap().identifier;
    let sftp_branch = &format!("gizai/{}-nightly-import-of-shipment-files", sftp_ident.to_lowercase());
    let sftp_wt = data.join("worktrees").join(&kade.key).join(&sftp_ident);
    git(repo, &["worktree", "add", "-q", "-b", sftp_branch, &sftp_wt.display().to_string(), &base]);
    write(&sftp_wt, "app/Console/Commands/ImportShipments.php", "<?php\n\nnamespace App\\Console\\Commands;\n\nuse Illuminate\\Console\\Command;\nuse Illuminate\\Support\\Facades\\Storage;\n\nclass ImportShipments extends Command\n{\n    protected $signature = 'shipments:import {--date=}';\n\n    public function handle(): int\n    {\n        $file = 'outbox/shipments-'.($this->option('date') ?? today()->format('Ymd')).'.csv';\n        $rows = str_getcsv(Storage::disk('kade-sftp')->get($file), \"\\n\");\n        $this->info(count($rows).' shipments read from '.$file);\n\n        return self::SUCCESS;\n    }\n}\n");
    let sftp_wt_s = sftp_wt.display().to_string();
    db.write(None, |w| { w.conn().execute("UPDATE tasks SET branch=?2 WHERE id=?1", rusqlite::params![sftp, sftp_branch])?; Ok(()) }).unwrap();
    let asked = runs::create_with_trigger(&db, &backend, &sftp, "backend", "assigned", &ids::new_id(), &sftp_wt_s, &sftp_wt_s, sftp_branch, "/dev/null").unwrap();
    runs::set_base_sha(&db, &asked, &base).unwrap();
    db.write(None, |w| { w.conn().execute("UPDATE runs SET cost_usd_micros=160000 WHERE id=?1", [&asked])?; Ok(()) }).unwrap();
    workflow::apply_outcome_with(&db, &asked, Some(&Outcome { outcome: "needs_decision".into(),
        summary: "The import command is written, but Kade's SFTP server refuses my key, so I can't test it against the real folder. Add the key to your SSH agent, then press Done, continue: I'll run the import for yesterday and check the 214 shipments.".into(),
        issues: vec![] }), &["ssh-add ~/.ssh/kade_sftp_ed25519".into(), "ssh -o BatchMode=yes sftp@files.kade-logistics.example exit".into()]).unwrap();

    // The Team Lead chat: a request, the two cards it made, and a third that waits for your OK (it read the client's
    // spec on the web first, from outside Gizai)
    let thread = chat::list_threads(&db).unwrap().into_iter().next().unwrap().id;
    let say = |role: &str, author: &str, body: Option<&str>, tool: Option<(&str, serde_json::Value)>| {
        chat::add_message(&db, chat::NewMessage { thread_id: thread.clone(), role: role.into(), author_id: Some(author.into()), body_md: body.map(Into::into),
            tool_name: tool.as_ref().map(|(n, _)| n.to_string()), tool: tool.map(|(_, v)| v), ..Default::default() }).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    say("user", &you, Some("Marloes sent the driver app spec: https://kade-logistics.example/specs/driver-app. Turn it into cards for the Backend and Frontend Agents."), None);
    say("tool", &lead, None, Some(("WebFetch", json!({"id": "w1", "input": {"url": "https://kade-logistics.example/specs/driver-app"}, "isError": false, "result": "Driver app: today's route, scans at each stop, works offline."}))));
    let made = |title: &str, labels: &[&str]| {
        let id = tasks::create(&db, &lead, TaskInput { project_id: kade.id.clone(), title: title.into(), priority: 3,
            label_ids: labels.iter().map(|l| label(l)).collect(), ..Default::default() }).unwrap();
        tasks::move_to(&db, &lead, &id, &col("Backlog"), "").unwrap();
        (id.clone(), tasks::get(&db, &id).unwrap().identifier)
    };
    let mut links = vec![];
    for (n, (title, labels)) in [("Driver app: API for today's route", ["backend"]), ("Driver app: the route screen with its stops", ["frontend"])].into_iter().enumerate() {
        let (id, ident) = made(title, &labels);
        say("tool", &lead, None, Some(("mcp__gizai__create_task", json!({"id": format!("c{n}"), "input": {"project": "KADE", "title": title, "column": "Backlog", "labels": labels},
            "isError": false, "result": json!({"ok": true, "done": "created", "link": {"id": id, "label": format!("{ident} {title}"), "page": "task"}}).to_string()}))));
        links.push(ident);
    }
    say("agent", &lead, Some(&format!("I read the spec and made two cards in Backlog:\n\n- **{}** Driver app: API for today's route, for the Backend Agent\n- **{}** Driver app: the route screen with its stops, for the Frontend Agent\n\nThe scans at each stop should work offline. That card goes straight to **To do**, where an agent starts it, so it waits for your OK.", links[0], links[1])), None);
    pending::keep(&db, pending::NewPending { thread_id: thread.clone(), run_id: None, actor_id: lead.clone(), tool: "create_task".into(),
        args: json!({"project": "KADE", "title": "Driver app: scans at each stop, also offline", "column": "To do", "labels": ["frontend"], "priority": 2,
            "description_md": "Scan each parcel at its stop. Without a network the scans wait on the phone and go out once it is back."}),
        outside_tool: "WebFetch".into(), note_key: None }).unwrap();

    // MCP servers for the agents' Tools: Laravel Boost on for the Backend Agent, Playwright not yet
    let boost = mcp_servers::save(&db, mcp_servers::McpServer { name: "laravel-boost".into(), transport: "stdio".into(), command: "php".into(),
        args: vec!["artisan".into(), "boost:mcp".into()], ..Default::default() }).unwrap();
    mcp_servers::save(&db, mcp_servers::McpServer { name: "playwright".into(), transport: "stdio".into(), command: "npx".into(),
        args: vec!["@playwright/mcp@latest".into()], ..Default::default() }).unwrap();
    mcp_servers::set_agent_tools(&db, &you, &backend, mcp_servers::AgentTools { mcp: vec![mcp_servers::AgentServer { server_id: boost.id.clone(), on: true, tools_off: vec![] }] }).unwrap();

    // Memory: notes that link each other
    memory::ensure_lead_notes(&db, &lead, "Jeffrey").unwrap();
    let me = memory::Who::Person(you.clone());
    let note = |path: &str, body: &str| memory::write(&db, &me, path, body, None, None).unwrap().id;
    let kade_note = note("Clients/Kade Logistics", "---\ntype: client\ntags: [client, logistics]\n---\n# Kade Logistics\n\nA logistics company in Amsterdam: 40 drivers, an office of six. Marloes Visser (finance) is our contact.\n\n## What we build\n\n- [[Kade portal]]: shipments, invoices and drivers, in Filament\n- the driver app, see [[Driver app]]\n\n## How they want it\n\n- Exports open in Excel: [[CSV with semicolons]]\n- The public tracking page stays fast: [[Rate limits per IP address]]\n- Releases on Tuesday mornings, with [[Deploy steps]]\n\nFirst card: KADE-1.\n");
    for (path, body) in [
        ("Projects/Kade portal", "# Kade portal\n\nLaravel 12 and Filament 4. Client: [[Kade Logistics]]. Tests in Pest, see [[Pest tests]]. Deployed with [[Deploy steps]] to [[Forge servers]].\n"),
        ("Projects/Driver app", "# Driver app\n\nFor [[Kade Logistics]]' drivers: today's route and scans at each stop, also offline. Talks to the [[Kade portal]] API.\n"),
        ("Projects/Groene Fiets webshop", "# Groene Fiets webshop\n\nFor [[De Groene Fiets]]. Payments with Mollie: see [[Mollie webhooks retry]]. Deployed with [[Deploy steps]].\n"),
        ("Clients/De Groene Fiets", "# De Groene Fiets\n\nA bike shop in Utrecht with a webshop: [[Groene Fiets webshop]]. BTW 9% on books, see [[VAT rates]].\n"),
        ("Decisions/CSV with semicolons", "---\ntype: decision\n---\n# CSV with semicolons\n\nDutch Excel opens a CSV with semicolons and decimal commas without a wizard. Used for [[Kade portal]]'s exports. Tested as in [[Pest tests]].\n"),
        ("Decisions/Rate limits per IP address", "---\ntype: decision\n---\n# Rate limits per IP address\n\nPublic endpoints get a limit per IP address, 60 a minute by default. Decided on KADE-2 for [[Kade portal]].\n"),
        ("Decisions/Filament for admin panels", "---\ntype: decision\n---\n# Filament for admin panels\n\nOffice screens are Filament resources, not custom Blade. See [[Kade portal]] and [[Code style]].\n"),
        ("Workflows/Deploy steps", "---\ntype: workflow\ntags: [ops]\n---\n# Deploy steps\n\n## Build\n\nTag the release, let CI build it, then deploy on [[Forge servers]]. Check [[Release checklist]] first.\n\n## Rollback\n\nForge keeps the last five releases: activate the one before. Write down why in [[Lessons]].\n"),
        ("Workflows/Release checklist", "# Release checklist\n\n- [ ] The changelog says what changed for the client\n- [ ] Migrations run on a copy of production\n- [ ] [[Pest tests]] pass\n\nThen follow [[Deploy steps]].\n"),
        ("Workflows/Hotfix", "# Hotfix\n\nA branch from production, one fix, a test, then [[Deploy steps]] the same day.\n"),
        ("Standards/Pest tests", "# Pest tests\n\nFeature tests for every endpoint and export. Factories for data, never the client's real data. See [[Flaky tests]].\n"),
        ("Standards/Code style", "# Code style\n\nPint with the Laravel preset. Small classes, actions for business logic. Follow [[Filament for admin panels]].\n"),
        ("Standards/Commit messages", "# Commit messages\n\nPlain English that says what changed for the client. One card, small commits.\n"),
        ("Lessons/Flaky tests", "# Flaky tests\n\nTests that hit the clock flake around midnight: freeze time. Found while writing [[Pest tests]] for KADE-2.\n"),
        ("Lessons/Mollie webhooks retry", "# Mollie webhooks retry\n\nMollie retries a webhook ten times; answer 200 quickly and queue the work. For [[Groene Fiets webshop]].\n"),
        ("Lessons/Lessons", "# Lessons\n\nWhat went wrong once, so it doesn't twice: [[Flaky tests]], [[Mollie webhooks retry]], [[Queue workers]].\n"),
        ("Lessons/Queue workers", "# Queue workers\n\nRestart the workers after a deploy, or they run old code. Part of [[Deploy steps]].\n"),
        ("Deployments/Forge servers", "# Forge servers\n\nkade-prod and gfw-prod on Hetzner, deployed by Forge. See [[Deploy steps]] and [[Queue workers]].\n"),
        ("Dependencies/VAT rates", "# VAT rates\n\n21% by default, 9% for books and food. Used by [[Groene Fiets webshop]].\n"),
    ] { note(path, body); }

    println!("THREAD={thread}");
    println!("NOTE={kade_note}");
    println!("KADE1={}", card("KADE-1").id);
    println!("KADE2={}", k2.id);
    println!("BACKEND={backend}");
    println!("LEAD={lead}");
    let _ = frontend;
}
