use crate::db::Db;
use crate::model::{Client, ClientInput, Contact};
use crate::util::{clean, org_id};
use crate::{Error, Result, ids};
use rusqlite::{Connection, OptionalExtension, Row};

const SELECT: &str = "SELECT c.id, c.name, c.legal_name, c.kind, c.vat_number, c.coc_number, c.iban, c.email, c.phone,
    c.website, c.street, c.postal_code, c.city, c.country, c.currency, c.payment_terms_days, c.status, c.notes_md,
    (SELECT name FROM contacts k WHERE k.client_id = c.id AND k.deleted_at IS NULL ORDER BY k.is_primary DESC, k.created_at LIMIT 1),
    (SELECT count(*) FROM tasks t LEFT JOIN projects p ON p.id = t.project_id
       WHERE t.deleted_at IS NULL AND (t.client_id = c.id OR p.client_id = c.id) AND t.state_category NOT IN ('done','cancelled')),
    (SELECT count(*) FROM projects p WHERE p.client_id = c.id AND p.deleted_at IS NULL),
    c.updated_at
  FROM clients c";

fn row(r: &Row) -> rusqlite::Result<Client> {
    Ok(Client {
        id: r.get(0)?, name: r.get(1)?, legal_name: r.get(2)?, kind: r.get(3)?, vat_number: r.get(4)?,
        coc_number: r.get(5)?, iban: r.get(6)?, email: r.get(7)?, phone: r.get(8)?, website: r.get(9)?,
        street: r.get(10)?, postal_code: r.get(11)?, city: r.get(12)?, country: r.get(13)?, currency: r.get(14)?,
        payment_terms_days: r.get(15)?, status: r.get(16)?, notes_md: r.get(17)?, main_contact: r.get(18)?,
        open_tasks: r.get(19)?, projects: r.get(20)?, updated_at: r.get(21)?,
    })
}

pub fn list(db: &Db) -> Result<Vec<Client>> {
    db.read(|c| {
        let mut st = c.prepare(&format!("{SELECT} WHERE c.deleted_at IS NULL ORDER BY c.name COLLATE NOCASE"))?;
        Ok(st.query_map([], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

pub fn get(db: &Db, id: &str) -> Result<Client> {
    db.read(|c| get_in(c, id))
}

fn get_in(c: &Connection, id: &str) -> Result<Client> {
    c.query_row(&format!("{SELECT} WHERE c.id = ?1"), [id], row)
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("client {id}")))
}

fn validate(input: &ClientInput) -> Result<String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(Error::Invalid("client name is required".into()));
    }
    if let Some(k) = clean(&input.kind) {
        if k != "company" && k != "person" {
            return Err(Error::Invalid("kind must be company or person".into()));
        }
    }
    if let Some(s) = clean(&input.status) {
        if !["lead", "active", "inactive"].contains(&s.as_str()) {
            return Err(Error::Invalid("status must be lead, active or inactive".into()));
        }
    }
    Ok(name)
}

pub fn create(db: &Db, actor: &str, input: ClientInput) -> Result<String> {
    let name = validate(&input)?;
    db.write(Some(actor), |w| {
        let id = ids::new_id();
        let now = ids::now_ms();
        let org = org_id(w.conn())?;
        w.conn().execute(
            "INSERT INTO clients(id, created_at, updated_at, created_by, updated_by, org_id, name, legal_name, kind, vat_number,
               coc_number, iban, email, phone, website, street, postal_code, city, country, payment_terms_days, status, notes_md)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, coalesce(?7, 'company'), ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
               coalesce(?17, 'NL'), coalesce(?18, 30), coalesce(?19, 'active'), ?20)",
            rusqlite::params![
                id, now, actor, org, name, clean(&input.legal_name), clean(&input.kind), clean(&input.vat_number),
                clean(&input.coc_number), clean(&input.iban), clean(&input.email), clean(&input.phone), clean(&input.website),
                clean(&input.street), clean(&input.postal_code), clean(&input.city), clean(&input.country),
                input.payment_terms_days, clean(&input.status), clean(&input.notes_md)
            ],
        )?;
        w.insert("clients", &id, serde_json::to_value(&input)?)?;
        Ok(id)
    })
}

pub fn update(db: &Db, actor: &str, id: &str, input: ClientInput) -> Result<()> {
    let name = validate(&input)?;
    db.write(Some(actor), |w| {
        let n = w.conn().execute(
            "UPDATE clients SET name=?2, legal_name=?3, kind=coalesce(?4, kind), vat_number=?5, coc_number=?6, iban=?7,
               email=?8, phone=?9, website=?10, street=?11, postal_code=?12, city=?13, country=coalesce(?14, country),
               payment_terms_days=coalesce(?15, payment_terms_days), status=coalesce(?16, status), notes_md=?17,
               updated_at=?18, updated_by=?19, version=version+1
             WHERE id=?1 AND deleted_at IS NULL",
            rusqlite::params![
                id, name, clean(&input.legal_name), clean(&input.kind), clean(&input.vat_number), clean(&input.coc_number),
                clean(&input.iban), clean(&input.email), clean(&input.phone), clean(&input.website), clean(&input.street),
                clean(&input.postal_code), clean(&input.city), clean(&input.country), input.payment_terms_days,
                clean(&input.status), clean(&input.notes_md), ids::now_ms(), actor
            ],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("client {id}")));
        }
        w.update("clients", id, serde_json::to_value(&input)?)
    })
}

pub fn archive(db: &Db, actor: &str, id: &str) -> Result<()> {
    db.write(Some(actor), |w| {
        let now = ids::now_ms();
        w.conn().execute(
            "UPDATE clients SET deleted_at=?2, updated_at=?2, updated_by=?3, version=version+1 WHERE id=?1 AND deleted_at IS NULL",
            rusqlite::params![id, now, actor],
        )?;
        w.delete("clients", id)
    })
}

pub fn contacts(db: &Db, client_id: &str) -> Result<Vec<Contact>> {
    db.read(|c| {
        let mut st = c.prepare(
            "SELECT id, client_id, name, role, email, phone, is_primary FROM contacts
             WHERE client_id=?1 AND deleted_at IS NULL ORDER BY is_primary DESC, name COLLATE NOCASE",
        )?;
        let rows = st.query_map([client_id], |r| {
            Ok(Contact { id: r.get(0)?, client_id: r.get(1)?, name: r.get(2)?, role: r.get(3)?, email: r.get(4)?,
                         phone: r.get(5)?, is_primary: r.get::<_, i64>(6)? != 0 })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// Insert (empty id) or update a contact. Marking one primary clears the others.
pub fn upsert_contact(db: &Db, actor: &str, contact: Contact) -> Result<String> {
    let name = contact.name.trim().to_string();
    if name.is_empty() {
        return Err(Error::Invalid("contact name is required".into()));
    }
    db.write(Some(actor), |w| {
        let now = ids::now_ms();
        if contact.is_primary {
            w.conn().execute(
                "UPDATE contacts SET is_primary=0, updated_at=?2 WHERE client_id=?1 AND is_primary=1",
                rusqlite::params![contact.client_id, now],
            )?;
        }
        let id = if contact.id.is_empty() {
            let id = ids::new_id();
            w.conn().execute(
                "INSERT INTO contacts(id, created_at, updated_at, created_by, updated_by, client_id, name, role, email, phone, is_primary)
                 VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![id, now, actor, contact.client_id, name, clean(&contact.role), clean(&contact.email),
                                  clean(&contact.phone), contact.is_primary as i64],
            )?;
            w.insert("contacts", &id, serde_json::to_value(&contact)?)?;
            id
        } else {
            w.conn().execute(
                "UPDATE contacts SET name=?2, role=?3, email=?4, phone=?5, is_primary=?6, updated_at=?7, updated_by=?8,
                   version=version+1 WHERE id=?1",
                rusqlite::params![contact.id, name, clean(&contact.role), clean(&contact.email), clean(&contact.phone),
                                  contact.is_primary as i64, now, actor],
            )?;
            w.update("contacts", &contact.id, serde_json::to_value(&contact)?)?;
            contact.id.clone()
        };
        Ok(id)
    })
}

/// Soft delete a contact. When it was the primary one, the first contact left (by name, as
/// `contacts` lists them) becomes primary, so a client with contacts always has one.
pub fn remove_contact(db: &Db, actor: &str, id: &str) -> Result<()> {
    db.write(Some(actor), |w| {
        let now = ids::now_ms();
        let (client_id, was_primary): (String, bool) = w.conn()
            .query_row("SELECT client_id, is_primary FROM contacts WHERE id=?1 AND deleted_at IS NULL", [id],
                       |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)))
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("contact {id}")))?;
        w.conn().execute(
            "UPDATE contacts SET deleted_at=?2, is_primary=0, updated_at=?2, updated_by=?3, version=version+1 WHERE id=?1",
            rusqlite::params![id, now, actor],
        )?;
        w.delete("contacts", id)?;
        if was_primary {
            let next: Option<String> = w.conn()
                .query_row("SELECT id FROM contacts WHERE client_id=?1 AND deleted_at IS NULL ORDER BY name COLLATE NOCASE, created_at, id LIMIT 1",
                           [&client_id], |r| r.get(0))
                .optional()?;
            if let Some(next) = next {
                w.conn().execute(
                    "UPDATE contacts SET is_primary=1, updated_at=?2, updated_by=?3, version=version+1 WHERE id=?1",
                    rusqlite::params![next, now, actor],
                )?;
                w.update("contacts", &next, serde_json::json!({ "isPrimary": true }))?;
            }
        }
        Ok(())
    })
}
