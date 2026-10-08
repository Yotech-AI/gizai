//! Uploaded files: blobs are stored once by content (data_dir/files/<sha[0..2]>/<sha>), rows point at them.
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::db::Db;
use crate::model::FileRow;
use crate::{Error, Result, ids, util};

const OWNER_TYPES: [&str; 7] = ["client", "project", "task", "comment", "doc", "chat_message", "actor"];
/// Larger files are refused: they belong in the repository or a shared drive, not in Gizai's data folder.
pub const MAX_BYTES: u64 = 1 << 30;

pub fn blob_path(data_dir: &Path, sha256: &str) -> PathBuf {
    data_dir.join("files").join(&sha256[..2]).join(sha256)
}

fn mime_for(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png", "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "webp" => "image/webp", "svg" => "image/svg+xml",
        "pdf" => "application/pdf", "txt" | "log" => "text/plain", "md" => "text/markdown", "csv" => "text/csv",
        "json" => "application/json", "html" | "htm" => "text/html", "zip" => "application/zip",
        "doc" => "application/msword", "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel", "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "mp4" => "video/mp4", "mov" => "video/quicktime",
        _ => return None,
    })
}

/// Copies one file into the blob store (hashing while copying) and links it to its owner.
pub fn add_from_path(db: &Db, actor: &str, data_dir: &Path, owner_type: &str, owner_id: &str, path: &Path) -> Result<FileRow> {
    if !OWNER_TYPES.contains(&owner_type) {
        return Err(Error::Invalid(format!("files can't belong to a {owner_type}")));
    }
    if owner_type == "task" {
        db.read(|c| crate::tasks::not_archived(c, owner_id))?;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let meta = std::fs::metadata(path).map_err(|_| Error::Invalid(format!("can't read {name}")))?;
    if !meta.is_file() {
        return Err(Error::Invalid(format!("{name} is a folder, not a file")));
    }
    if meta.len() > MAX_BYTES {
        return Err(Error::Invalid(format!("{name} is larger than 1 GB")));
    }

    let dir = data_dir.join("files");
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!("incoming-{}", ids::new_id()));
    let (sha, size) = {
        let mut src = std::fs::File::open(path)?;
        let mut out = std::fs::File::create(&tmp)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        let mut size = 0u64;
        loop {
            let n = src.read(&mut buf)?;
            if n == 0 { break; }
            h.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            size += n as u64;
        }
        out.sync_all()?;
        (format!("{:x}", h.finalize()), size)
    };
    let blob = blob_path(data_dir, &sha);
    if blob.exists() {
        std::fs::remove_file(&tmp)?;
    } else {
        std::fs::create_dir_all(blob.parent().unwrap())?;
        std::fs::rename(&tmp, &blob)?;
        let mut perm = std::fs::metadata(&blob)?.permissions();
        perm.set_readonly(true); // blobs never change; their name is their content hash
        std::fs::set_permissions(&blob, perm)?;
    }

    let mime = mime_for(&name);
    db.write(Some(actor), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        let id = ids::new_id();
        c.execute(
            "INSERT INTO files(id, created_at, updated_at, created_by, updated_by, org_id, owner_type, owner_id, name, mime, size_bytes, sha256, uploaded_by_actor_id)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?3)",
            rusqlite::params![id, now, actor, util::org_id(c)?, owner_type, owner_id, name, mime, size as i64, sha],
        )?;
        w.insert("files", &id, serde_json::json!({"name": name, "owner_type": owner_type, "owner_id": owner_id, "size_bytes": size}))?;
        Ok(FileRow { id, name: name.clone(), mime: mime.map(String::from), size_bytes: size as i64, sha256: sha.clone(), created_at: now })
    })
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<FileRow> {
    Ok(FileRow { id: r.get(0)?, name: r.get(1)?, mime: r.get(2)?, size_bytes: r.get(3)?, sha256: r.get(4)?, created_at: r.get(5)? })
}

/// Newest first.
pub fn list(db: &Db, owner_type: &str, owner_id: &str) -> Result<Vec<FileRow>> {
    db.read(|c| {
        let mut st = c.prepare(
            "SELECT id, name, mime, size_bytes, sha256, created_at FROM files
             WHERE owner_type = ?1 AND owner_id = ?2 AND deleted_at IS NULL ORDER BY created_at DESC, name",
        )?;
        let rows = st.query_map([owner_type, owner_id], row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

pub fn get(db: &Db, id: &str) -> Result<FileRow> {
    db.read(|c| {
        c.query_row("SELECT id, name, mime, size_bytes, sha256, created_at FROM files WHERE id = ?1 AND deleted_at IS NULL", [id], row)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("file {id}")),
                e => e.into(),
            })
    })
}

/// Soft delete; the blob stays (other rows may share it, and sync needs history).
pub fn remove(db: &Db, actor: &str, id: &str) -> Result<()> {
    db.write(Some(actor), |w| {
        let now = ids::now_ms();
        let n = w.conn().execute(
            "UPDATE files SET deleted_at = ?2, updated_at = ?2, updated_by = ?3, version = version + 1 WHERE id = ?1 AND deleted_at IS NULL",
            rusqlite::params![id, now, actor],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("file {id}")));
        }
        w.delete("files", id)
    })
}

/// A copy of the blob under its original name (data_dir/open/<file id>/<name>), so the system's
/// default app recognises the type. Always a real copy: a hard link would let an app that saves in
/// place change the stored blob. Reused when it already exists.
pub fn materialize(data_dir: &Path, f: &FileRow) -> Result<PathBuf> {
    let safe: String = f.name.chars().map(|c| if c == '/' || c == '\\' || c.is_control() { '_' } else { c }).collect();
    let safe = if safe.is_empty() || safe == "." || safe == ".." { "file".to_string() } else { safe };
    let dir = data_dir.join("open").join(&f.id);
    let out = dir.join(safe);
    if !out.exists() {
        std::fs::create_dir_all(&dir)?;
        std::fs::copy(blob_path(data_dir, &f.sha256), &out)?;
    }
    Ok(out)
}
