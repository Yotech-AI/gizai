//! The product tour's fake data (scripts/tour-capture.sh, GA-148 and GA-149), made up from start to end: no real project,
//! client, folder or person. Its owner is Alex Morgan, its projects Northwind Shop, Lumen Health and Orbit CRM, and their
//! folders sit in <home>/Projects, the only Editor folder, so every path shows as ~/…. Each of the tour's scenes has
//! something to show:
//!  - the board: cards in every column, the Backend Agent at work on NWS-1 (Claude Code is the tour's fake);
//!  - the Northwind Shop repository: a small Laravel shop on main, a change not committed yet (the Editor's git status);
//!  - NWS-2 in Review: its branch and worktree with two commits, pull request #42, and the QA Agent's passed run;
//!  - the Team Lead chat: a request, the two cards it made, and a third card waiting for your Approve;
//!  - Settings → Coding CLIs: Claude Code at ~/.local/bin/claude and Codex at ~/.local/bin/codex, and the limits each
//!    reported, for Usage → Subscription;
//!  - Memory: notes that link each other, enough for a graph;
//!  - the Backend Agent's Tools: its allowed commands and an MCP server;
//!  - the Inbox: the Backend Agent's question with commands to run for you, and a card waiting for your review.
//! Prints the ids the scenes open, as shell assignments (NWS1=…).
//! usage: cargo run -p gizai-core --example prep_tour -- <data dir> <home>
use std::fs;
use std::path::Path;
use std::process::Command;

use gizai_core::model::*;
use gizai_core::{chat, clients, clis, comments, db::Db, editor, ids, limits, mcp_servers, memory, pending, projects, runs, seed::{self, ensure_seed},
    settings, tasks, team, users, workflow};
use serde_json::json;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(dir)
        .args(["-c", "user.email=agents@studio.example", "-c", "user.name=GizAi", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"])
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

/// A repository on main with one commit of `files`.
fn small_repo(dir: &Path, files: &[(&str, &str)], message: &str) {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    for (path, body) in files { write(dir, path, body); }
    commit(dir, message);
}

/// Northwind Shop on main: a small Laravel webshop with a Filament admin.
const SHOP: &[(&str, &str)] = &[
    ("README.md", "# Northwind Shop\n\nNorthwind Traders' webshop and its admin: orders, stock and customers.\n\n## Develop\n\n```sh\ncomposer install\nphp artisan migrate --seed\nphp artisan serve\n```\n"),
    ("composer.json", "{\n    \"name\": \"northwind/shop\",\n    \"type\": \"project\",\n    \"require\": {\n        \"php\": \"^8.3\",\n        \"filament/filament\": \"^4.0\",\n        \"laravel/framework\": \"^12.0\"\n    },\n    \"require-dev\": {\n        \"pestphp/pest\": \"^3.0\"\n    }\n}\n"),
    ("app/Models/Order.php", "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\nuse Illuminate\\Database\\Eloquent\\Relations\\BelongsTo;\n\nclass Order extends Model\n{\n    protected $casts = [\n        'date' => 'date',\n        'total' => 'decimal:2',\n    ];\n\n    public function customer(): BelongsTo\n    {\n        return $this->belongsTo(Customer::class);\n    }\n}\n"),
    ("app/Models/Shipment.php", "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass Shipment extends Model\n{\n    protected $casts = ['delivered_at' => 'datetime'];\n\n    public function trackingCode(): string\n    {\n        return strtoupper($this->code);\n    }\n}\n"),
    ("app/Models/Customer.php", "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass Customer extends Model\n{\n    protected $fillable = ['name', 'email', 'phone'];\n}\n"),
    ("app/Exports/OrderExporter.php", "<?php\n\nnamespace App\\Exports;\n\nuse Filament\\Actions\\Exports\\Exporter;\n\nclass OrderExporter extends Exporter\n{\n}\n"),
    ("app/Filament/Resources/OrderResource.php", "<?php\n\nnamespace App\\Filament\\Resources;\n\nuse App\\Models\\Order;\nuse Filament\\Resources\\Resource;\nuse Filament\\Tables\\Table;\n\nclass OrderResource extends Resource\n{\n    protected static ?string $model = Order::class;\n\n    public static function table(Table $table): Table\n    {\n        return $table\n            ->columns([])\n            ->headerActions([]);\n    }\n}\n"),
    ("app/Http/Controllers/TrackingController.php", "<?php\n\nnamespace App\\Http\\Controllers;\n\nuse App\\Models\\Shipment;\n\nclass TrackingController extends Controller\n{\n    public function show(string $code)\n    {\n        $shipment = Shipment::where('code', $code)->firstOrFail();\n\n        return [\n            'code' => $shipment->trackingCode(),\n            'status' => $shipment->status,\n            'delivered_at' => $shipment->delivered_at,\n        ];\n    }\n}\n"),
    ("routes/web.php", "<?php\n\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::get('/', fn () => view('shop.home'));\n"),
    ("routes/api.php", "<?php\n\nuse App\\Http\\Controllers\\TrackingController;\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::get('/track/{code}', [TrackingController::class, 'show']);\n"),
    ("config/tracking.php", "<?php\n\nreturn [\n    'public' => true,\n    'cache_seconds' => 30,\n];\n"),
    ("tests/Feature/TrackingTest.php", "<?php\n\nuse App\\Models\\Shipment;\n\nit('shows a shipment by its code', function () {\n    $shipment = Shipment::factory()->create(['code' => 'nw-1042']);\n\n    $this->getJson('/api/track/nw-1042')\n        ->assertOk()\n        ->assertJson(['code' => 'NW-1042']);\n});\n"),
    ("database/migrations/2026_09_01_000000_create_shipments_table.php", "<?php\n\nuse Illuminate\\Database\\Migrations\\Migration;\nuse Illuminate\\Database\\Schema\\Blueprint;\nuse Illuminate\\Support\\Facades\\Schema;\n\nreturn new class extends Migration\n{\n    public function up(): void\n    {\n        Schema::create('shipments', function (Blueprint $table) {\n            $table->id();\n            $table->string('code')->unique();\n            $table->string('status');\n            $table->timestamp('delivered_at')->nullable();\n            $table->timestamps();\n        });\n    }\n};\n"),
];

/// NWS-2's branch: the public tracking endpoint gets a rate limit (two commits).
const RATE_LIMIT_1: &[(&str, &str)] = &[
    ("routes/api.php", "<?php\n\nuse App\\Http\\Controllers\\TrackingController;\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::middleware('throttle:tracking')\n    ->get('/track/{code}', [TrackingController::class, 'show']);\n"),
    ("app/Providers/AppServiceProvider.php", "<?php\n\nnamespace App\\Providers;\n\nuse Illuminate\\Cache\\RateLimiting\\Limit;\nuse Illuminate\\Http\\Request;\nuse Illuminate\\Support\\Facades\\RateLimiter;\nuse Illuminate\\Support\\ServiceProvider;\n\nclass AppServiceProvider extends ServiceProvider\n{\n    public function boot(): void\n    {\n        // the public tracking page: per IP address, so one customer can't slow it down for the others\n        RateLimiter::for('tracking', function (Request $request) {\n            return Limit::perMinute(config('tracking.per_minute'))\n                ->by($request->ip())\n                ->response(fn () => response()->json(['message' => 'Too many requests'], 429));\n        });\n    }\n}\n"),
    ("config/tracking.php", "<?php\n\nreturn [\n    'public' => true,\n    'cache_seconds' => 30,\n    // requests per minute per IP address on /api/track\n    'per_minute' => env('TRACKING_PER_MINUTE', 60),\n];\n"),
];
const RATE_LIMIT_2: &[(&str, &str)] = &[
    ("tests/Feature/TrackingRateLimitTest.php", "<?php\n\nuse App\\Models\\Shipment;\n\nit('lets 60 requests a minute through', function () {\n    Shipment::factory()->create(['code' => 'nw-1042']);\n\n    foreach (range(1, 60) as $i) {\n        $this->getJson('/api/track/nw-1042')->assertOk();\n    }\n});\n\nit('answers the 61st with 429 and Retry-After', function () {\n    Shipment::factory()->create(['code' => 'nw-1042']);\n\n    foreach (range(1, 60) as $i) {\n        $this->getJson('/api/track/nw-1042');\n    }\n\n    $this->getJson('/api/track/nw-1042')\n        ->assertStatus(429)\n        ->assertHeader('Retry-After');\n});\n\nit('counts each IP address on its own', function () {\n    Shipment::factory()->create(['code' => 'nw-1042']);\n\n    foreach (range(1, 60) as $i) {\n        $this->getJson('/api/track/nw-1042', ['REMOTE_ADDR' => '10.0.0.1']);\n    }\n\n    $this->withServerVariables(['REMOTE_ADDR' => '10.0.0.2'])\n        ->getJson('/api/track/nw-1042')\n        ->assertOk();\n});\n"),
    ("README.md", "# Northwind Shop\n\nNorthwind Traders' webshop and its admin: orders, stock and customers.\n\n## Develop\n\n```sh\ncomposer install\nphp artisan migrate --seed\nphp artisan serve\n```\n\n## Public tracking\n\n`GET /api/track/{code}` allows 60 requests a minute per IP address (`TRACKING_PER_MINUTE`).\n"),
];

/// Lumen Health's patient portal and Orbit CRM, one commit each: the Editor's other folders.
const LUMEN: &[(&str, &str)] = &[
    ("README.md", "# Lumen Health portal\n\nAppointments, lab results and messages for Lumen's clinics.\n"),
    ("app/Models/Appointment.php", "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass Appointment extends Model\n{\n    protected $casts = ['starts_at' => 'datetime'];\n}\n"),
    ("routes/web.php", "<?php\n\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::view('/', 'portal.home');\n"),
];
const ORBIT: &[(&str, &str)] = &[
    ("README.md", "# Orbit CRM\n\nContacts, deals and the weekly sales report for Orbit Labs.\n"),
    ("package.json", "{\n  \"name\": \"orbit-crm\",\n  \"private\": true,\n  \"type\": \"module\",\n  \"scripts\": { \"dev\": \"vite\", \"build\": \"vite build\", \"test\": \"vitest\" }\n}\n"),
    ("src/deals/pipeline.ts", "export const STAGES = [\"Lead\", \"Qualified\", \"Proposal\", \"Won\", \"Lost\"] as const;\nexport type Stage = (typeof STAGES)[number];\n"),
];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, home) = (&a[1], Path::new(&a[2]));
    let data = Path::new(dir);
    fs::create_dir_all(data).unwrap();
    let db = Db::open(&data.join("gizai.db")).unwrap();
    let s = ensure_seed(&db, "Alex Morgan").unwrap();
    let you = s.you_id.clone();
    let team = team::get(&db, &s.team_id).unwrap();
    let col = |n: &str| team.states.iter().find(|x| x.name == n).unwrap().id.clone();
    let label = |n: &str| team.labels.iter().find(|x| x.name == n).unwrap().id.clone();

    // Claude Code and Codex, both the tour's fakes in the fake home's ~/.local/bin, written as ~/… so no real path shows
    settings::set(&db, "claude_bin", &"~/.local/bin/claude".to_string()).unwrap();
    fs::create_dir_all(home.join(".codex")).unwrap();
    let codex = clis::save(&db, vec![clis::Cli { name: "Codex".into(), kind: "codex".into(), command: "~/.local/bin/codex".into(), ..Default::default() }])
        .unwrap().into_iter().find(|c| c.name == "Codex").unwrap().id;

    // The team: the Team Lead, Backend and Design Agents on Claude Code, the Frontend and QA Agents on Codex
    let agent = |name: &str, role: &str, cli: &str, chat: bool| {
        let on_claude = cli.is_empty();
        team::add_agent(&db, &you, &s.team_id, AgentInput { name: name.into(), role_key: role.into(), adapter: cli.into(),
            model: on_claude.then(|| "opus".into()), effort: Some("high".into()), instructions_md: Some(seed::role_template(role)),
            chat_enabled: Some(chat), ..Default::default() }).unwrap()
    };
    let lead = agent("Team Lead", "lead", "", true);
    let frontend = agent("Frontend Agent", "frontend", &codex, false);
    let backend = agent("Backend Agent", "backend", "", false);
    let design = agent("Design Agent", "design", "", false);
    let qa = agent("QA Agent", "qa", &codex, false);
    // One agent at work: the Backend Agent takes NWS-1 by itself as Gizai starts (the fake keeps working). The others
    // are paused, so they take no other card.
    for a in [&frontend, &design, &qa] { team::set_agent_status(&db, &you, a, "paused").unwrap(); }

    // People, clients and projects, each project in its folder in ~/Projects (the only Editor folder)
    users::create(&db, &you, "Priya Shah", Some("priya@studio.example")).unwrap();
    let client = |name: &str, city: &str| clients::create(&db, &you, ClientInput { name: name.into(), city: Some(city.into()), ..Default::default() }).unwrap();
    let (northwind, lumen, orbit) = (client("Northwind Traders", "Rotterdam"), client("Lumen Health", "Leiden"), client("Orbit Labs", "Eindhoven"));
    clients::upsert_contact(&db, &you, Contact { client_id: northwind.clone(), name: "Dana Brooks".into(), role: Some("Head of operations".into()),
        email: Some("dana@northwind.example".into()), is_primary: true, ..Default::default() }).unwrap();
    clients::upsert_contact(&db, &you, Contact { client_id: lumen.clone(), name: "Ravi Patel".into(), role: Some("Practice manager".into()),
        email: Some("ravi@lumen-health.example".into()), is_primary: true, ..Default::default() }).unwrap();
    let projects_dir = home.join("Projects");
    let (shop_repo, lumen_repo, orbit_repo) = (projects_dir.join("northwind-shop"), projects_dir.join("lumen-portal"), projects_dir.join("orbit-crm"));
    settings::set(&db, editor::ROOTS, &vec!["~/Projects".to_string()]).unwrap();
    // A new worktree gets nothing installed (composer would wait for the network): the agent starts at once.
    let project = |name: &str, key: &str, client: &str, color: &str, goal: &str, repo: &Path| {
        projects::create(&db, &you, ProjectInput { name: name.into(), key: key.into(), client_id: Some(client.into()), color: Some(color.into()),
            goal_md: Some(goal.into()), repo_path: Some(repo.display().to_string()), default_branch: Some("main".into()), worktree_install: Some(false),
            ..Default::default() }).unwrap()
    };
    let shop = project("Northwind Shop", "NWS", &northwind, "#e8c547", "Northwind's webshop and its admin: orders, stock and customers.", &shop_repo);
    let health = project("Lumen Health", "LUM", &lumen, "#4fc1a6", "A patient portal for Lumen's clinics: appointments, lab results and messages.", &lumen_repo);
    let crm = project("Orbit CRM", "ORB", &orbit, "#a78bfa", "Orbit's contacts and deals in one place, with a weekly sales report.", &orbit_repo);
    let add = |p: &str, title: &str, column: &str, labels: &[&str], priority: i64, body: &str| {
        let id = tasks::create(&db, &you, TaskInput { project_id: p.into(), title: title.into(), priority, description_md: body.into(),
            label_ids: labels.iter().map(|l| label(l)).collect(), ..Default::default() }).unwrap();
        tasks::move_to(&db, &you, &id, &col(column), "").unwrap();
        id
    };
    let ident = |id: &str| tasks::get(&db, id).unwrap().identifier;

    // NWS-1, the card at work, and NWS-2, the one in Review: the first two cards
    let nws1 = add(&shop, "Export orders as CSV from the admin", "In progress", &["backend", "frontend"], 2,
        "## Context\n\nNorthwind's office copies orders into Excel by hand. Add **Export CSV** to the orders table.\n\n\
- semicolons between fields, like the sample Dana sent\n- amounts with a comma (`1250,00`)\n- one line per order, in the table's order");
    tasks::update(&db, &you, &nws1, TaskPatch { assignee_id: Some(backend.clone()),
        acceptance_md: Some("- [ ] Export CSV downloads the orders shown in the table\n- [ ] Fields are separated by semicolons\n- [ ] A test covers the export".into()),
        ..Default::default() }).unwrap();
    comments::add(&db, &you, &nws1, "Use Filament's exporter. Dana's sample file is in the shared folder.", None).unwrap();
    let nws2 = add(&shop, "Rate limit the public order tracking endpoint", "Testing", &["backend"], 3, "");

    // Cards in every column, in all three projects; none above NWS-1's priority, so the Backend Agent takes NWS-1
    add(&shop, "Product filters: price range slider", "To do", &["frontend"], 3, "");
    add(&shop, "Gift cards at checkout", "Backlog", &[], 4, "");
    add(&shop, "Order emails in Dutch and English", "Deploy", &["backend"], 3, "");
    add(&shop, "Product photos load lazily", "Done", &["frontend"], 3, "");
    add(&health, "Appointment reminders by text message", "In progress", &["backend"], 3, "");
    add(&health, "Two-factor login for the patient portal", "Testing", &["backend"], 3, "");
    add(&health, "Patient messages: attach a photo", "To do", &["frontend"], 3, "");
    add(&health, "Waiting list for the dental clinic", "Backlog", &[], 4, "");
    add(&health, "Lab results: download as PDF", "Done", &["backend"], 3, "");
    add(&crm, "Import contacts from a CSV file", "To do", &["backend"], 3, "");
    add(&crm, "Deal pipeline: drag deals between stages", "In progress", &["frontend"], 3, "");
    add(&crm, "Weekly sales report by email", "Deploy", &["backend"], 3, "");
    add(&crm, "Log in with Google Workspace", "Done", &["backend"], 3, "");
    add(&crm, "Merge two duplicate contacts into one", "Backlog", &[], 4, "");
    // a card in Review that is yours to review: it waits in your Inbox
    let yours = add(&health, "Clinic opening hours on the contact page", "Review", &["frontend"], 4, "");
    tasks::update(&db, &you, &yours, TaskPatch { assignee_id: Some(you.clone()), ..Default::default() }).unwrap();

    // Northwind Shop on main, with a change not committed yet (the Editor's git status); Lumen's and Orbit's folders
    fs::create_dir_all(&shop_repo).unwrap();
    git(&shop_repo, &["init", "-q", "-b", "main"]);
    let mut base = String::new();
    for (message, prefixes) in [("Northwind Shop: Laravel and Filament", &["README", "composer", "routes/web", "app/Models/Customer"][..]),
                                ("Orders: the model, its Filament resource and an exporter", &["app/Models/Order", "app/Exports", "app/Filament"][..]),
                                ("Public tracking of shipments by their code", &["app/Models/Shipment", "app/Http", "routes/api", "config", "tests", "database"][..])] {
        for (path, body) in SHOP.iter().filter(|(p, _)| prefixes.iter().any(|x| p.starts_with(x))) { write(&shop_repo, path, body); }
        base = commit(&shop_repo, message);
    }
    assert_eq!(git(&shop_repo, &["status", "--porcelain"]), "", "every SHOP file is in a commit");
    write(&shop_repo, "app/Models/Shipment.php", &SHOP.iter().find(|(p, _)| *p == "app/Models/Shipment.php").unwrap().1
        .replace("    public function trackingCode", "    public function isLate(): bool\n    {\n        return $this->delivered_at === null && $this->created_at->lt(now()->subDays(2));\n    }\n\n    public function trackingCode"));
    write(&shop_repo, "app/Http/Middleware/LogTracking.php", "<?php\n\nnamespace App\\Http\\Middleware;\n\nclass LogTracking\n{\n    // who looks up which shipment, for the office's weekly report\n}\n");
    small_repo(&lumen_repo, LUMEN, "Lumen Health portal: appointments");
    small_repo(&orbit_repo, ORBIT, "Orbit CRM: the deal stages");

    // NWS-2 in Review: its branch in its worktree, two commits, pull request #42, and the QA Agent's passed run
    let branch = "gizai/nws-2-rate-limit-the-public-order-tracking-endpoint";
    let wt = data.join("worktrees").join("NWS").join("NWS-2");
    fs::create_dir_all(wt.parent().unwrap()).unwrap();
    git(&shop_repo, &["worktree", "add", "-q", "-b", branch, &wt.display().to_string(), &base]);
    for (path, body) in RATE_LIMIT_1 { write(&wt, path, body); }
    commit(&wt, "Rate limit /api/track: 60 requests a minute per IP address");
    for (path, body) in RATE_LIMIT_2 { write(&wt, path, body); }
    let c2 = commit(&wt, "Tests for the tracking rate limit, and the README");
    tasks::update(&db, &you, &nws2, TaskPatch { assignee_id: Some(backend.clone()),
        description_md: Some("## Context\n\nThe public order tracking page is called by some customers' scripts, hundreds of times a minute. Limit it per IP address.".into()),
        acceptance_md: Some("- [x] At most 60 requests a minute per IP address on /api/track\n- [x] The 61st gets 429 with Retry-After\n- [x] Other IP addresses are not slowed down".into()),
        ..Default::default() }).unwrap();
    db.write(None, |w| {
        w.conn().execute("UPDATE tasks SET branch=?2, pr_url='https://github.com/northwind-example/shop/pull/42', pr_state='open' WHERE id=?1",
            rusqlite::params![nws2, branch])?;
        Ok(())
    }).unwrap();
    let wt_s = wt.display().to_string();
    let run = |agent: &str, role: &str, from: &str, to: &str, outcome: &str, summary: &str, cost: i64| {
        let r = runs::create_with_trigger(&db, agent, &nws2, role, "assigned", &ids::new_id(), &wt_s, &wt_s, branch, "/dev/null").unwrap();
        runs::set_base_sha(&db, &r, from).unwrap();
        runs::set_head_sha(&db, &r, to).unwrap();
        runs::finish(&db, &r, "succeeded", Some(&Outcome { outcome: outcome.into(), summary: summary.into(), issues: vec![] }), cost, 41_000, 5_200, None).unwrap();
        r
    };
    let built = run(&backend, "backend", &base, &c2, "ready_for_testing",
        "Added a rate limit on /api/track: 60 requests a minute per IP address, set with TRACKING_PER_MINUTE. The 61st gets 429 with Retry-After. Three Pest tests cover it.", 380_000);
    comments::add(&db, &backend, &nws2, "Added a rate limit on `/api/track`: 60 requests a minute per IP address, set with `TRACKING_PER_MINUTE`. The 61st request gets 429 with Retry-After. Three Pest tests cover it.", Some(&built)).unwrap();
    let tested = run(&qa, "qa", &base, &c2, "qa_pass",
        "Passed. All three acceptance criteria hold: 60 requests went through, the 61st got 429 with Retry-After: 60, and a second IP address was not slowed down. 14 tests pass; pull request #42 is open.", 210_000);
    comments::add(&db, &qa, &nws2, "**QA passed.** All three acceptance criteria hold:\n\n- 60 requests in a minute went through\n- the 61st got 429 with `Retry-After: 60`\n- a second IP address was not slowed down\n\n14 tests pass. Pull request #42 is open for your review.", Some(&tested)).unwrap();
    tasks::move_to(&db, &qa, &nws2, &col("Review"), "").unwrap();

    // the limits Claude Code and Codex reported (Usage → Subscription)
    let now = ids::now_ms();
    let (min, hour, day) = (60_000, 3_600_000, 86_400_000);
    let read = |key: &str, used: f64, resets_in: i64, minutes: i64| limits::Reading {
        key: key.into(), used_percent: Some(used), resets_at: Some(now + resets_in), window_minutes: Some(minutes), observed_at: now - 4 * min, ..Default::default()
    };
    limits::record(&db, clis::CLAUDE_CODE, &[read(limits::FIVE_HOUR, 38.0, 2 * hour + 20 * min, 300), read(limits::SEVEN_DAY, 61.0, 3 * day, 10_080)]).unwrap();
    limits::record(&db, &codex, &[read(limits::PRIMARY, 17.0, 3 * hour + 40 * min, 300), read(limits::SECONDARY, 44.0, 4 * day, 10_080)]).unwrap();

    // The Inbox's question: the Backend Agent can't reach the warehouse's SFTP server and asks you to run a command for it
    let sftp = add(&shop, "Nightly import of the warehouse's stock files over SFTP", "In progress", &["backend"], 3,
        "## Context\n\nNorthwind's warehouse drops a CSV of the day's stock on its SFTP server at 02:00. Import it every night.");
    tasks::update(&db, &you, &sftp, TaskPatch { assignee_id: Some(backend.clone()), ..Default::default() }).unwrap();
    let sftp_ident = ident(&sftp);
    let sftp_branch = &format!("gizai/{}-nightly-import-of-stock-files", sftp_ident.to_lowercase());
    let sftp_wt = data.join("worktrees").join("NWS").join(&sftp_ident);
    git(&shop_repo, &["worktree", "add", "-q", "-b", sftp_branch, &sftp_wt.display().to_string(), &base]);
    write(&sftp_wt, "app/Console/Commands/ImportStock.php", "<?php\n\nnamespace App\\Console\\Commands;\n\nuse Illuminate\\Console\\Command;\nuse Illuminate\\Support\\Facades\\Storage;\n\nclass ImportStock extends Command\n{\n    protected $signature = 'stock:import {--date=}';\n\n    public function handle(): int\n    {\n        $file = 'outbox/stock-'.($this->option('date') ?? today()->format('Ymd')).'.csv';\n        $rows = str_getcsv(Storage::disk('warehouse-sftp')->get($file), \"\\n\");\n        $this->info(count($rows).' products read from '.$file);\n\n        return self::SUCCESS;\n    }\n}\n");
    let sftp_wt_s = sftp_wt.display().to_string();
    db.write(None, |w| { w.conn().execute("UPDATE tasks SET branch=?2 WHERE id=?1", rusqlite::params![sftp, sftp_branch])?; Ok(()) }).unwrap();
    let asked = runs::create_with_trigger(&db, &backend, &sftp, "backend", "assigned", &ids::new_id(), &sftp_wt_s, &sftp_wt_s, sftp_branch, "/dev/null").unwrap();
    runs::set_base_sha(&db, &asked, &base).unwrap();
    db.write(None, |w| { w.conn().execute("UPDATE runs SET cost_usd_micros=160000 WHERE id=?1", [&asked])?; Ok(()) }).unwrap();
    workflow::apply_outcome_with(&db, &asked, Some(&Outcome { outcome: "needs_decision".into(),
        summary: "The import command is written, but the warehouse's SFTP server refuses my key, so I can't test it against the real folder. Add the key to your SSH agent, then press Done, continue: I'll run the import for yesterday and check the 312 products.".into(),
        issues: vec![] }), &["ssh-add ~/.ssh/warehouse_sftp_ed25519".into(), "ssh -o BatchMode=yes sftp@files.northwind.example exit".into()]).unwrap();

    // The Team Lead chat: an older thread, then a request, the two cards it made, and a third that waits for your OK (it
    // read the client's spec on the web first, from outside Gizai)
    let older = chat::create_thread(&db, &you, &lead, "What needs my attention today?").unwrap();
    let say_in = |thread: &str, role: &str, author: &str, body: Option<&str>, tool: Option<(&str, serde_json::Value)>| {
        chat::add_message(&db, chat::NewMessage { thread_id: thread.into(), role: role.into(), author_id: Some(author.into()), body_md: body.map(Into::into),
            tool_name: tool.as_ref().map(|(n, _)| n.to_string()), tool: tool.map(|(_, v)| v), ..Default::default() }).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    say_in(&older, "user", &you, Some("What needs my attention today?"), None);
    say_in(&older, "agent", &lead, Some(&format!("Two things: **{}** Clinic opening hours on the contact page waits for your review, and the Backend Agent asks you to run two commands on **{sftp_ident}**.", ident(&yours))), None);
    let thread = chat::create_thread(&db, &you, &lead, "Turn the mobile app spec into cards").unwrap();
    let say = |role: &str, author: &str, body: Option<&str>, tool: Option<(&str, serde_json::Value)>| say_in(&thread, role, author, body, tool);
    say("user", &you, Some("Dana sent the spec for Northwind's mobile app: https://northwind.example/specs/mobile-app. Turn it into cards for the Backend and Frontend Agents."), None);
    say("tool", &lead, None, Some(("WebFetch", json!({"id": "w1", "input": {"url": "https://northwind.example/specs/mobile-app"}, "isError": false, "result": "Mobile app: order history, reorder in one tap, a push notification when an order ships."}))));
    let made = |title: &str, labels: &[&str]| {
        let id = tasks::create(&db, &lead, TaskInput { project_id: shop.clone(), title: title.into(), priority: 3,
            label_ids: labels.iter().map(|l| label(l)).collect(), ..Default::default() }).unwrap();
        tasks::move_to(&db, &lead, &id, &col("Backlog"), "").unwrap();
        (id.clone(), ident(&id))
    };
    let mut links = vec![];
    for (n, (title, labels)) in [("Mobile app: API for the order history", ["backend"]), ("Mobile app: the order history screen", ["frontend"])].into_iter().enumerate() {
        let (id, ident) = made(title, &labels);
        say("tool", &lead, None, Some(("mcp__gizai__create_task", json!({"id": format!("c{n}"), "input": {"project": "NWS", "title": title, "column": "Backlog", "labels": labels},
            "isError": false, "result": json!({"ok": true, "done": "created", "link": {"id": id, "label": format!("{ident} {title}"), "page": "task"}}).to_string()}))));
        links.push(ident);
    }
    say("agent", &lead, Some(&format!("I read the spec and made two cards in Backlog:\n\n- **{}** Mobile app: API for the order history, for the Backend Agent\n- **{}** Mobile app: the order history screen, for the Frontend Agent\n\nThe push notification when an order ships goes straight to **To do**, where an agent starts it, so it waits for your OK.", links[0], links[1])), None);
    pending::keep(&db, pending::NewPending { thread_id: thread.clone(), run_id: None, actor_id: lead.clone(), tool: "create_task".into(),
        args: json!({"project": "NWS", "title": "Mobile app: a push notification when an order ships", "column": "To do", "labels": ["frontend"], "priority": 2,
            "description_md": "When the warehouse ships an order, the customer's phone shows it, with the tracking link."}),
        outside_tool: "WebFetch".into(), note_key: None }).unwrap();

    // MCP servers for the agents' Tools: Laravel Boost on for the Backend Agent, Playwright not yet
    let boost = mcp_servers::save(&db, mcp_servers::McpServer { name: "laravel-boost".into(), transport: "stdio".into(), command: "php".into(),
        args: vec!["artisan".into(), "boost:mcp".into()], ..Default::default() }).unwrap();
    mcp_servers::save(&db, mcp_servers::McpServer { name: "playwright".into(), transport: "stdio".into(), command: "npx".into(),
        args: vec!["@playwright/mcp@latest".into()], ..Default::default() }).unwrap();
    mcp_servers::set_agent_tools(&db, &you, &backend, mcp_servers::AgentTools { mcp: vec![mcp_servers::AgentServer { server_id: boost.id.clone(), on: true, tools_off: vec![] }] }).unwrap();

    // Memory: notes that link each other
    memory::ensure_lead_notes(&db, &lead, "Alex").unwrap();
    let me = memory::Who::Person(you.clone());
    let note = |path: &str, body: &str| memory::write(&db, &me, path, body, None, None).unwrap().id;
    let client_note = note("Clients/Northwind Traders", "---\ntype: client\ntags: [client, retail]\n---\n# Northwind Traders\n\nA wholesaler of fine foods in Rotterdam, with a webshop for its shops and restaurants. Dana Brooks (operations) is our contact.\n\n## What we build\n\n- [[Northwind Shop]]: the webshop and its admin, in Filament\n- the mobile app, see [[Mobile app]]\n\n## How they want it\n\n- Exports open in Excel: [[CSV with semicolons]]\n- The public tracking page stays fast: [[Rate limits per IP address]]\n- Releases on Tuesday mornings, with [[Deploy steps]]\n\nFirst card: NWS-1.\n");
    for (path, body) in [
        ("Projects/Northwind Shop", "# Northwind Shop\n\nLaravel 12 and Filament 4. Client: [[Northwind Traders]]. Tests in Pest, see [[Pest tests]]. Deployed with [[Deploy steps]] to [[Forge servers]].\n"),
        ("Projects/Mobile app", "# Mobile app\n\nFor [[Northwind Traders]]' customers: order history, reorder in one tap, a notification when an order ships. Talks to the [[Northwind Shop]] API.\n"),
        ("Projects/Lumen Health", "# Lumen Health\n\nThe patient portal for [[Lumen Health clinics]]. Two-factor login, see [[Two-factor login]]. Deployed with [[Deploy steps]].\n"),
        ("Projects/Orbit CRM", "# Orbit CRM\n\nFor [[Orbit Labs]]: contacts, deals and a weekly sales report. Imports follow [[CSV with semicolons]].\n"),
        ("Clients/Lumen Health clinics", "# Lumen Health clinics\n\nThree clinics around Leiden. Ravi Patel runs the practice. Their portal: [[Lumen Health]].\n"),
        ("Clients/Orbit Labs", "# Orbit Labs\n\nA software start-up in Eindhoven, twelve people in sales. Their CRM: [[Orbit CRM]].\n"),
        ("Decisions/CSV with semicolons", "---\ntype: decision\n---\n# CSV with semicolons\n\nDutch Excel opens a CSV with semicolons and decimal commas without a wizard. Used for [[Northwind Shop]]'s exports. Tested as in [[Pest tests]].\n"),
        ("Decisions/Rate limits per IP address", "---\ntype: decision\n---\n# Rate limits per IP address\n\nPublic endpoints get a limit per IP address, 60 a minute by default. Decided on NWS-2 for [[Northwind Shop]].\n"),
        ("Decisions/Two-factor login", "---\ntype: decision\n---\n# Two-factor login\n\nPatients log in with a code from an app or a text message. For [[Lumen Health]].\n"),
        ("Decisions/Filament for admin panels", "---\ntype: decision\n---\n# Filament for admin panels\n\nOffice screens are Filament resources, not custom Blade. See [[Northwind Shop]] and [[Code style]].\n"),
        ("Workflows/Deploy steps", "---\ntype: workflow\ntags: [ops]\n---\n# Deploy steps\n\n## Build\n\nTag the release, let CI build it, then deploy on [[Forge servers]]. Check [[Release checklist]] first.\n\n## Rollback\n\nForge keeps the last five releases: activate the one before. Write down why in [[Lessons]].\n"),
        ("Workflows/Release checklist", "# Release checklist\n\n- [ ] The changelog says what changed for the client\n- [ ] Migrations run on a copy of production\n- [ ] [[Pest tests]] pass\n\nThen follow [[Deploy steps]].\n"),
        ("Workflows/Hotfix", "# Hotfix\n\nA branch from production, one fix, a test, then [[Deploy steps]] the same day.\n"),
        ("Standards/Pest tests", "# Pest tests\n\nFeature tests for every endpoint and export. Factories for data, never the client's real data. See [[Flaky tests]].\n"),
        ("Standards/Code style", "# Code style\n\nPint with the Laravel preset. Small classes, actions for business logic. Follow [[Filament for admin panels]].\n"),
        ("Standards/Commit messages", "# Commit messages\n\nPlain English that says what changed for the client. One card, small commits.\n"),
        ("Lessons/Flaky tests", "# Flaky tests\n\nTests that hit the clock flake around midnight: freeze time. Found while writing [[Pest tests]] for NWS-2.\n"),
        ("Lessons/Payment webhooks retry", "# Payment webhooks retry\n\nThe payment provider retries a webhook ten times; answer 200 quickly and queue the work. For [[Northwind Shop]].\n"),
        ("Lessons/Lessons", "# Lessons\n\nWhat went wrong once, so it doesn't twice: [[Flaky tests]], [[Payment webhooks retry]], [[Queue workers]].\n"),
        ("Lessons/Queue workers", "# Queue workers\n\nRestart the workers after a deploy, or they run old code. Part of [[Deploy steps]].\n"),
        ("Deployments/Forge servers", "# Forge servers\n\nnorthwind-prod and lumen-prod, deployed by Forge. See [[Deploy steps]] and [[Queue workers]].\n"),
        ("Dependencies/VAT rates", "# VAT rates\n\n21% by default, 9% for food. Used by [[Northwind Shop]].\n"),
    ] { note(path, body); }

    println!("THREAD={thread}");
    println!("NOTE={client_note}");
    println!("NWS1={nws1}");
    println!("NWS2={nws2}");
    println!("BACKEND={backend}");
    println!("LEAD={lead}");
}
