# Backups: format 1

GizAi backs up your data as snapshots in a backups folder (GA-216). This page describes that folder as format 1: its layout, a snapshot's name, the manifest field by field, and how a snapshot is made and removed. GizAi Cloud will write its backups in the same format (see [For GizAi Cloud](#for-gizai-cloud)), so one restore reads both. The restore is GA-218's (see [Restore](#restore)). The code is `gizai_core::backups` and `gizai_core::restore`, and the app's side is `src-tauri/src/backups.rs` and `src-tauri/src/restore.rs`.

A snapshot holds only the database and the stored files it names (attachments and screenshots). Nothing from the OS keychain, a worktree, the logs (`runs/`, `chat/`), the Team Lead's code copies or `update/` goes into it. Snapshots are not encrypted: whoever can open the folder can read them, and a synced folder gets the data as it is.

## The backups folder

Settings → General → Backups names a folder: a USB disk, or a folder Dropbox or Syncthing keeps.

- A folder that holds `gizai-backups.json` is a backups folder, and GizAi uses it as it is.
- Any other folder gets a backups folder inside it, named `Gizai backups`. The first snapshot makes it.
- A folder given to list the snapshots in, or to restore from, is read the same way: the folder itself when it holds `gizai-backups.json`, else its `Gizai backups`.

```
Gizai backups/
  gizai-backups.json             {"format":"gizai-backups","format_version":1}
  README.txt                     what this folder is, and: Restore in Gizai: Settings → General → Backups.
  snapshots/
    <name>.json                  the manifest: a snapshot exists once this file does
    <name>.db.zst                its database, compressed with zstd
  files/<sha256[..2]>/<sha256>   every stored file, once, shared by all snapshots (as in the data folder)
```

- `gizai-backups.json` and `README.txt` are written once, by the first GizAi that backs up there. A GizAi whose format is older than the folder's `format_version` doesn't back up there; it says a newer GizAi made the folder.
- Several computers may share one folder. Each writes and removes only its own snapshots; the files in `files/` are shared by all. A computer is known by its `devices.id` (the row with `is_self = 1`), so a restore must keep the restoring computer's own row: two computers with one id would take each other's snapshots for their own.
- A file in `files/` is named by the SHA-256 of its content, in lowercase hex, inside a folder named by its first two characters, as in the data folder. It only ever arrives whole (renamed from a `.part` file) and never changes.

## A snapshot's name

`gizai-<reason>-<stamp>-<device>`, for example `gizai-daily-20261021-090200-123-9f3c2a1b`.

| Part | What it is |
|---|---|
| reason | `daily`, `weekly`, `manual` (Back up now), `quit` (Quit GizAi completely with On quit on) or `before-restore` (made just before a restore) |
| stamp | when the database was copied, in the computer's local time (`db::stamp`): year, month and day, then hours, minutes and seconds, then milliseconds |
| device | the last 8 characters of the computer's `devices.id`, letters and digits only. Ids are UUIDv7, so their first characters are their creation time and their last ones are random. |

A snapshot is two files in `snapshots/`: `<name>.json`, its manifest, and `<name>.db.zst`, its database. It exists once its manifest does.

## The manifest

```json
{
  "format": "gizai-backup",
  "format_version": 1,
  "name": "gizai-daily-20261021-090200-123-9f3c2a1b",
  "reason": "daily",
  "made_at": 1792573322123,
  "app_version": "0.13.0",
  "schema_version": 20,
  "device": { "id": "<devices.id>", "name": "<devices.name>" },
  "db": { "path": "snapshots/<name>.db.zst", "compression": "zstd", "bytes": 0, "sha256": "<sha256 of the uncompressed database>", "db_bytes": 0 },
  "last_change": { "seq": 0, "id": "<changes.id>", "hlc": "<changes.hlc>" },
  "files": [ { "sha256": "<64 lowercase hex>", "bytes": 0 } ],
  "files_missing": [ "<sha256>" ]
}
```

GizAi writes it as indented JSON in UTF-8, its fields in this order.

| Field | What it holds |
|---|---|
| `format` | always `"gizai-backup"` |
| `format_version` | `1`. A GizAi that reads a higher one lists the snapshot but doesn't restore it. |
| `name` | the snapshot's name, as above: the same as its files' names |
| `reason` | `daily`, `weekly`, `manual`, `quit` or `before-restore` |
| `made_at` | when the database was copied: milliseconds since 1970 (UTC) |
| `app_version` | the GizAi version that made it |
| `schema_version` | the copied database's schema version (`PRAGMA user_version`). A GizAi whose own is lower doesn't restore it. |
| `device.id`, `device.name` | the computer that made it: its `devices` row with `is_self = 1` |
| `db.path` | `snapshots/<name>.db.zst`, from the backups folder |
| `db.compression` | `"zstd"` (level 3, one zstd frame that records the database's size) |
| `db.bytes` | the compressed file's size |
| `db.sha256` | the SHA-256 of the uncompressed database, in lowercase hex |
| `db.db_bytes` | the uncompressed database's size |
| `last_change` | the copied database's newest `changes` row (the highest `seq`): its `seq`, `id` and `hlc`. `null` when it has none. It is where a sync continues. |
| `files` | every blob the copied database names, each once, sorted by hash: the `sha256` of each `files` row (removed ones too, as their blob is kept), and each `artifact_versions` row's `sha256` and `thumbnail_sha256`. `bytes` is its size; for a missing one, the size its `files` row gives, else 0. |
| `files_missing` | the hashes among `files` the snapshot couldn't store: neither the backups folder nor the data folder had them, or the data folder's copy no longer matched its hash. The snapshot is still made, and says so here. |

The uncompressed database is a complete SQLite database in one file (journal mode DELETE, no `-wal` beside it).

## How a snapshot is made

One snapshot runs at a time on a computer, off the main thread, and GizAi stays usable meanwhile. In this order, so an interrupted snapshot leaves no manifest, only `.part` files:

1. **The folder:** the backups folder, `gizai-backups.json`, `README.txt`, `snapshots/` and `files/` are made when they're missing. Then this computer's `.part` files that are more than an hour old go (see below).
2. **The database:** `VACUUM INTO` a temporary copy, `<data folder>/backups/.snapshot-<name>.db`, as the copies before updates are made. It is consistent, while GizAi goes on writing.
3. **The files:** each blob the copy names goes into `files/`. One the folder has already isn't copied: its modified time is set to now (see Keep). Any other is copied from the data folder as `files/<xx>/<sha256>.<device>.part`, hashed while it is copied, then renamed. One the data folder doesn't have goes into `files_missing`.
4. **The compressed database:** the copy is compressed with zstd (level 3) into `snapshots/<name>.db.zst.part`, hashed while it is read, then renamed.
5. **The manifest:** written as `snapshots/<name>.json.part`, then renamed. The temporary copy is removed. The snapshot exists from now on.
6. **Keep:** see below.

A failure removes this snapshot's `.part` files, its compressed database and the temporary copy, and is kept as the last error (Settings shows it until a snapshot works). A snapshot that was interrupted (GizAi was killed, the computer went off) leaves its `.part` files: the next snapshot on that computer removes them once they are more than an hour old, with a compressed database whose manifest never came and its temporary copy. A `.part` file is always one computer's own: its name ends in that computer's device.

## When snapshots are made

- **Back up now**, with reason `manual`.
- **Daily or weekly** (Every in Settings; daily by default): due when this computer's newest good snapshot in the folder is 24 hours or 7 days old. A good snapshot is one whose manifest reads and whose database file is there, of any reason but `before-restore`, and not dated after now (made while the clock was ahead); with none, the first one is due at once. GizAi checks 2 minutes after it starts and then every hour by the wall clock, which it looks at every 5 minutes, so the hours a computer slept count too. A check up to 5 minutes early counts, so the snapshot doesn't move an hour later each day. A folder that can't be reached now (a USB disk that isn't plugged in) is passed over quietly, and Settings shows why.
- **On quit** (off by default): Quit GizAi completely (the tray's or Settings' Quit, Cmd+Q on macOS) makes a snapshot with reason `quit` once agents at work have stopped, before GizAi exits, waiting at most 5 minutes. A signal, logging out, shutting down, and the restart after an update or a restore don't back up.
- **Before a restore** (GA-218), with reason `before-restore`: in the backups folder set in Settings, or in the folder restored from (see [Restore](#restore)). `gizai_lib::backups::back_up_within` makes it.

## Keep: how snapshots and files are removed

After each snapshot:

1. This computer's snapshots of reasons `daily`, `weekly`, `manual` and `quit`, beyond the newest Keep (1 to 100, 10 by default), are removed: the manifest first, then its `.db.zst`. Never another computer's snapshot, and never a `before-restore` one. A snapshot dated after now (made while the clock was ahead) counts as the oldest, so it never takes the place of the one just made.
2. Then a blob in `files/` is removed when no manifest in the folder names it (`.part` manifests included) and it hasn't been modified for 7 days. So the blobs another computer's snapshot has found while it runs stay. When a manifest can't be read, no blob goes that time (it may name any), unless it is a `.part` manifest more than an hour old.

## Restorable or not

Listing a folder shows each snapshot, newest first, and whether this GizAi can restore it. It can't, and says why, when a newer GizAi made it (its `format_version` or `schema_version` is higher than this GizAi's), when its manifest can't be read as format 1's, when its database file is missing, or when files it names are missing from `files/` ("3 files are missing"). A manifest that can't be read whole is read field by field, so a newer GizAi's snapshot still shows; Keep and the schedule only count manifests that read whole.

## Settings

This computer's, kept in its database; a restore keeps them. Saving checks Keep and a newly chosen folder (a full path GizAi can write in); the folder saved already isn't checked again, so an unplugged USB disk doesn't keep the other settings from changing.

| Key | Value |
|---|---|
| `backup_folder` | the chosen folder, a full path, or `null`: backups are off (the default) |
| `backup_every` | `"daily"` (the default), `"weekly"` or `"manual"` |
| `backup_on_quit` | `true` or `false` (the default) |
| `backup_keep` | 1 to 100 (10 by default) |
| `backup_last_error` | the last snapshot that failed, `{ "at", "message" }`; `null` once one has been made since |

## Restore

A restore (GA-218) brings a snapshot back as this computer's data. It never makes two computers share one device id, this computer's own settings stay this computer's, and GizAi notes which snapshot it came back from, so a sync knows where this computer's data starts. The same rules hold for sync across devices. Nothing in the OS keychain is restored or removed: the restored data asks for the keys this computer doesn't have, as a new install does.

`start_restore(folder, name)` runs in the background, one restore at a time, and is refused while a backup or another restore runs. `folder` is read as for listing (the folder itself when it holds `gizai-backups.json`, else its `Gizai backups`). Each step goes out in `restore-changed` with the status (contract BR's `RestoreStatus`). A restore that is ready already gives way to a new one.

### 1. The checks

Each failure stops the restore with its reason, and nothing is staged:

- the manifest is a snapshot's (`format` is `gizai-backup`) and reads as format 1's;
- its `format_version` is 1 or lower, and its `schema_version` is at most this GizAi's (`db::SCHEMA_VERSION`). A newer one is refused with `Made by a newer Gizai (database version N). Update Gizai first.` (for the format: `backup format N`);
- the database file is there and decompresses, its SHA-256 and size match `db.sha256` and `db.db_bytes`, it passes `PRAGMA integrity_check`, and its `user_version` is the manifest's `schema_version`;
- every blob in `files` is in the folder's `files/`, with its size and its SHA-256. A snapshot with `files_missing` is refused too: those blobs aren't there.

The database is decompressed into `<data folder>/restore/gizai.db.part` for the checks.

### 2. Today's data is saved

A snapshot of today's data with reason `before-restore`, made as any snapshot is: in the backups folder set in Settings, or, when none is set (or it can't be written now), in the folder restored from. It waits at most 5 minutes for a snapshot that runs already. When neither can be written, the restore stops: `Gizai can't save today's data first: <why>. Choose a backups folder in Settings → General → Backups.`

### 3. Staging

```
<data folder>/restore/
  gizai.db                    the snapshot's database, decompressed and checked
  files/<sha256[..2]>/<sha256>  the blobs the data folder's files/ lacks, copied and hashed again
  restore.json                the manifest, the folder, the before-restore snapshot's name and when: written last
  failed/                     a restored database that couldn't be opened (see below)
```

Once `restore.json` is there, the restore is ready: it waits for a restart, and today's data keeps working until then. The blobs the data folder has already get their modified time set to now, so the housekeeping (which leaves blobs changed in the last day alone) keeps them until the restart. `restart_to_restore()` quits GizAi the usual way (agents at work are stopped first, and it doesn't back up on quit) and starts the same GizAi again: the installed launcher, as after an update, or else the program that runs now. `cancel_restore()` removes what is staged (`restore/`, apart from `restore/failed/`); the before-restore snapshot stays in its folder, and the next start is a normal one.

### 4. The swap at the next start

Before the database opens, when `restore/restore.json` says ready, in this order, so that whatever fails today's data stays whole:

1. This computer's device (its `devices` row with `is_self = 1`: `id`, `name`, `created_at`) and today's values of this computer's settings are read from today's `gizai.db`, and its WAL is folded back into it (`PRAGMA wal_checkpoint(TRUNCATE)`).
2. The staged database gets them, on a plain connection in one transaction (see the device rule and this computer's settings below), with `agents_paused` set to true and `last_restore`.
3. `restore.json` notes the name today's database gets, then today's database moves to `backups/gizai-before-restore-<stamp>.db`, and the staged one takes its place as `gizai.db`. A start after a swap that was interrupted there carries on from where it was.
4. The staged blobs move into `files/`; a blob that is there already stays. `restore.json` goes: a restore is applied once.
5. The database opens as usual: an older schema is upgraded, with its copy in `backups/gizai-before-v<N>-<stamp>.db`.
6. When the restored database can't be opened (or step 2 fails), it moves to `restore/failed/<snapshot>.db`, today's database comes back, and GizAi starts with it.
7. `restore/` goes, apart from `restore/failed/`. The result (`{ ok, at, snapshot, fromComputer, message }`) is kept as this computer's setting `restore_result`, for the window (`restore_status().result`), until `dismiss_restore_result`.

### The device rule

When the restored database's `is_self` device isn't this computer's, every row gets `is_self = 0`, and this computer's row is added (or updated, when the snapshot knew it) with `is_self = 1` and today's `name` and `created_at`. The snapshot's computer stays in `devices` as another computer, so its `changes` rows keep their device, and new `changes` rows carry this computer's id. This computer's own snapshot leaves `devices` as it was.

### This computer's settings

`gizai_core::scope` lists the app-wide settings (`org_id` '') that belong to this computer; `scope::this_computer(key)` says whether one does. Today's values of these go over the restored ones; a restored value stays only where today had none. Every other app-wide setting, and every org's setting, is shared: it comes from the snapshot.

| Group | Keys |
|---|---|
| programs and paths | `claude_bin`, `gh_bin`, `clis`, `mcp_servers`, `mcp_tools`, `mcp_last_run`, `mcp_browser`, `cli_tools_seen`, `cli_tools_asked`, `editor_roots`, `github_push_over`, `claude_memory_imports` |
| capacity and state | `runs_memory_gb`, `max_concurrent_runs`, `agents_paused`, `subscription_limits`, `onboarding`, `update_auto_check`, `update_last_check`, `preview_port_from`, `preview_port_to` (the ports this computer's previews take, checked against `max_concurrent_runs`) |
| desktop notifications | every `notify_*` key: `notify_hold`, `notify_waiting`, `notify_lead_asks`, `notify_lead_answered` |
| backups | `backup_folder`, `backup_every`, `backup_on_quit`, `backup_keep`, `backup_last_error` (the last error belongs to this computer's folder) |
| restores | `last_restore`, `restore_result` |

Sync will send the shared settings only. A new setting must go on one side: one that is only true on one computer (a path, a program, a port) goes in `scope`'s list.

### `last_restore`

```json
{ "snapshot": "<name>", "made_by": "<device id>", "made_at": 0, "last_change": { "seq": 0, "id": "<changes.id>", "hlc": "<changes.hlc>" }, "restored_at": 0, "saved_today": "<the before-restore snapshot's name>" }
```

`made_by` and `made_at` are the snapshot's computer and when it was made, `last_change` is its manifest's (`null` when it had none), and `saved_today` is the before-restore snapshot of the data it replaced. It is one of this computer's settings, so a later restore keeps it until it writes its own.

## For GizAi Cloud

- A cloud snapshot is the daily snapshot of the hosted copy, plus the files. It is written in this format, so the same restore reads it.
- `last_change` is where a sync continues: the device that restores a snapshot has every change up to it, and asks for the ones after it.
- `files` is what a sync uploads, by hash: a file the cloud has already isn't sent again, as a backups folder stores each file once.
- A device that restores keeps its own id (see [the device rule](#the-device-rule)), and `last_restore` says which snapshot its data starts from: a sync asks for the changes after that snapshot's `last_change`.
- Sync sends the shared settings only, never this computer's (see [this computer's settings](#this-computers-settings)).
- Beyond settings, parts of the database are this computer's too, and a restore brings them back as the snapshot has them: projects' linked folders (`repos.local_path`), agents' folders (`agent_configs.folders_json`), and the worktree paths and process ids in `runs` (`cwd`, `worktree_path`, `pid`). Sync must not send them as they are, nor apply another device's.
- A change's `hlc` is `<unix_ms>-<counter>-<device>`, and its device part is the first 8 characters of the device id. Those are the id's creation time (ids are UUIDv7), so two devices made in the same moment can share them: sync should order changes by the time, the counter and the whole device id (`changes.device_id`), never by the hlc's device part alone.
