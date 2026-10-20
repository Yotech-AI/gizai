# Backups: format 1

GizAi backs up your data as snapshots in a backups folder (GA-216). This page describes that folder as format 1: its layout, a snapshot's name, the manifest field by field, and how a snapshot is made and removed. Gizai Cloud will write its backups in the same format (see [For Gizai Cloud](#for-gizai-cloud)), so one restore reads both. The restore itself comes with GA-218. The code is `gizai_core::backups`, and the app's side is `src-tauri/src/backups.rs`.

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
- **Before a restore** (GA-218), with reason `before-restore`, in a folder of the restore's own: `gizai_lib::backups::back_up`.

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

## For Gizai Cloud

- A cloud snapshot is the daily snapshot of the hosted copy, plus the files. It is written in this format, so the same restore reads it.
- `last_change` is where a sync continues: the device that restores a snapshot has every change up to it, and asks for the ones after it.
- `files` is what a sync uploads, by hash: a file the cloud has already isn't sent again, as a backups folder stores each file once.
