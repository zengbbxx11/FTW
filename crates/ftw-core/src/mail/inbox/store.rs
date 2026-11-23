use super::{audit, new_id, Failure, Result, FOLDER};
use ftw_ipc::{
    crm::Source,
    inbox::{Message, Result as Reply, Row as InboxRow},
};
use mail_parser::MessageParser;
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, SqliteConnection, SqlitePool};
use std::collections::HashSet;

const PREVIEW: usize = 24_000;
const THREAD_PAGE: usize = 10;

struct Parsed {
    sender: String,
    subject: String,
    header_id: String,
    reply_ids: Vec<String>,
    body: String,
    attachments: Vec<String>,
    kind: &'static str,
}

fn clip(text: &str, max: usize) -> String {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end]
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\r' | '\n' | '\t'))
        .collect()
}

fn parse(raw: &[u8], oversized: bool) -> Parsed {
    let parser = MessageParser::new().max_parts(160);
    let Some(mail) = parser.parse(raw) else {
        return Parsed {
            sender: String::new(),
            subject: "无法解析的邮件".into(),
            header_id: String::new(),
            reply_ids: vec![],
            body: String::new(),
            attachments: vec![],
            kind: "unparsed",
        };
    };
    let sender = mail
        .from()
        .and_then(|list| list.first())
        .and_then(|address| address.address())
        .unwrap_or_default();
    let mut reply_ids = Vec::new();
    for list in [mail.in_reply_to(), mail.references()]
        .into_iter()
        .flatten()
    {
        reply_ids.extend(
            list.iter()
                .rev()
                .filter(|id| id.len() <= 512)
                .take(12)
                .map(str::to_owned),
        );
    }
    let report = mail.content_type().is_some_and(|ct| {
        ct.attribute("report-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("delivery-status"))
    });
    let kind = if oversized {
        "large_message"
    } else if report {
        "delivery_report"
    } else {
        "message"
    };
    let body = if oversized {
        "邮件超过 2 MiB，已保留邮件头。".to_owned()
    } else {
        mail.body_text(0)
            .map_or_else(String::new, |body| clip(&body, PREVIEW))
    };
    Parsed {
        sender: clip(sender, 320),
        subject: clip(mail.subject().unwrap_or("（无主题）"), 1000),
        header_id: clip(mail.message_id().unwrap_or_default(), 512),
        reply_ids,
        body,
        attachments: mail
            .attachments()
            .take(25)
            .filter_map(|part| part.attachment_name())
            .map(|n| clip(n, 240))
            .collect(),
        kind,
    }
}

pub(super) async fn save(
    pool: &SqlitePool,
    account: &str,
    validity: u32,
    uid: u32,
    raw: &[u8],
    received: Option<i64>,
    oversized: bool,
) -> Result<()> {
    let hash = format!("{:x}", Sha256::digest(raw));
    let mut tx = pool.begin().await?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM inbox_messages WHERE account_id=? AND raw_sha256=?")
            .bind(account)
            .bind(&hash)
            .fetch_optional(&mut *tx)
            .await?;
    let id = if let Some(id) = existing {
        id
    } else {
        let mail = parse(raw, oversized);
        let id = new_id();
        let (contact, thread) = associate(&mut tx, account, &id, &mail).await?;
        let at = received.filter(|at| (0..=253_402_300_799).contains(at));
        sqlx::query("INSERT INTO inbox_messages(id,account_id,contact_id,thread_id,message_id_header,raw_sha256,sender,subject,body_text,attachment_names,received_at,raw,kind)
            VALUES (?,?,?,?,?,?,?,?,?,?,COALESCE(?,unixepoch()),?,?)")
            .bind(&id).bind(account).bind(contact.as_deref()).bind(thread).bind(&mail.header_id).bind(hash).bind(&mail.sender)
            .bind(&mail.subject).bind(mail.body).bind(serde_json::json!(mail.attachments).to_string()).bind(at).bind(raw).bind(mail.kind)
            .execute(&mut *tx).await?;
        if let Some(contact) = contact {
            activity(&mut tx, &id, &contact).await?;
        }
        id
    };
    sqlx::query("INSERT INTO inbox_locations(account_id,folder,uid_validity,uid,message_id) VALUES (?,?,?,?,?)
        ON CONFLICT(account_id,folder,uid_validity,uid) DO NOTHING")
        .bind(account).bind(FOLDER).bind(i64::from(validity)).bind(i64::from(uid)).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

async fn associate(
    db: &mut SqliteConnection,
    account: &str,
    id: &str,
    mail: &Parsed,
) -> Result<(Option<String>, String)> {
    let fallback_thread = format!("inbox:{id}");
    let address = mail.sender.to_ascii_lowercase();
    let matching: HashSet<String> = sqlx::query_scalar(
        "SELECT DISTINCT c.id FROM contacts c JOIN contact_methods m ON m.contact_id=c.id
         WHERE c.archived_at IS NULL AND m.kind='email' AND m.normalized=?",
    )
    .bind(&address)
    .fetch_all(&mut *db)
    .await?
    .into_iter()
    .collect();

    for reference in &mail.reply_ids {
        let known: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT 'sent:' || id, contact_id FROM email_messages WHERE account_id=? AND message_id_header=?
             UNION SELECT thread_id,contact_id FROM inbox_messages WHERE account_id=? AND message_id_header=? LIMIT 3",
        ).bind(account).bind(format!("<{reference}>")).bind(account).bind(reference).fetch_all(&mut *db).await?;
        if let [(thread, Some(contact))] = known.as_slice() {
            // An unverified sender may supply arbitrary References. Only an address belonging to
            // the matched contact can inherit that contact's thread automatically.
            if matching.contains(contact) {
                return Ok((Some(contact.clone()), thread.clone()));
            }
        }
        if !known.is_empty() {
            return Ok((None, fallback_thread));
        }
    }
    let contact = if matching.len() == 1 {
        matching.into_iter().next()
    } else {
        None
    };
    Ok((contact, fallback_thread))
}

async fn activity(db: &mut SqliteConnection, message: &str, contact: &str) -> Result<()> {
    let update = sqlx::query("UPDATE activities SET contact_id=?,updated_at=unixepoch() WHERE related_message_id=? AND event_type='email_received'")
        .bind(contact).bind(message).execute(&mut *db).await?;
    if update.rows_affected() == 0 {
        sqlx::query("INSERT INTO activities(id,contact_id,event_type,occurred_at,summary,details,related_message_id,source,actor_id,channel)
            SELECT ?,?,'email_received',received_at,subject,'',id,'system','imap','邮件' FROM inbox_messages WHERE id=?")
            .bind(new_id()).bind(contact).bind(message).execute(db).await?;
    }
    Ok(())
}

pub(super) async fn link(pool: &SqlitePool, id: &str, contact: &str) -> Result<Reply> {
    let mut tx = pool.begin().await?;
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM contacts WHERE id=? AND archived_at IS NULL)",
    )
    .bind(contact)
    .fetch_one(&mut *tx)
    .await?;
    if !allowed {
        return Err(Failure::Code("crm_not_found"));
    }
    let updated = sqlx::query(
        "UPDATE inbox_messages SET thread_id=CASE WHEN contact_id IS ? THEN thread_id ELSE 'inbox:' || id END,
         contact_id=? WHERE id=?",
    ).bind(contact).bind(contact).bind(id).execute(&mut *tx).await?;
    if updated.rows_affected() != 1 {
        return Err(Failure::Code("mail_not_found"));
    }
    activity(&mut tx, id, contact).await?;
    audit(&mut tx, Source::Manual, "inbox_message", id, "link_contact").await?;
    tx.commit().await?;
    Ok(Reply::Saved { id: id.to_owned() })
}

const RECEIVE: &str = "SELECT i.id,i.account_id,i.contact_id,c.display_name contact_name,i.thread_id,i.sender,i.subject,i.received_at,i.kind,1 received,i.body_text,i.attachment_names
    FROM inbox_messages i LEFT JOIN contacts c ON c.id=i.contact_id";

pub(super) async fn list(
    pool: &SqlitePool,
    account: Option<String>,
    contact: Option<String>,
    unmatched: bool,
    offset: u32,
) -> Result<Reply> {
    let mut query = QueryBuilder::new(RECEIVE);
    query.push(" WHERE 1=1");
    if let Some(account) = account {
        query.push(" AND i.account_id=").push_bind(account);
    }
    if let Some(contact) = contact {
        query.push(" AND i.contact_id=").push_bind(contact);
    }
    if unmatched {
        query.push(" AND i.contact_id IS NULL");
    }
    query
        .push(" ORDER BY i.received_at DESC,i.id LIMIT 21 OFFSET ")
        .push_bind(i64::from(offset));
    let mut rows = query.build().map(row).fetch_all(pool).await?;
    let has_more = rows.len() > 20;
    rows.truncate(20);
    Ok(Reply::Rows { rows, has_more })
}

pub(super) async fn get(pool: &SqlitePool, id: &str) -> Result<Reply> {
    let query = format!("{RECEIVE} WHERE i.id=?");
    let record = sqlx::query(&query)
        .bind(id)
        .map(message)
        .fetch_optional(pool)
        .await?
        .ok_or(Failure::Code("mail_not_found"))?;
    Ok(Reply::Message {
        record: Box::new(record),
    })
}

pub(super) async fn thread(pool: &SqlitePool, id: &str, offset: u32) -> Result<Reply> {
    let query = format!(
        "{RECEIVE} WHERE i.thread_id=?
         UNION ALL SELECT e.id,e.account_id,e.contact_id,c.display_name,'sent:' || e.id,
            e.from_address,e.subject,e.submitted_at,'smtp_accepted',0,e.body_text,'[]'
         FROM email_messages e JOIN contacts c ON c.id=e.contact_id
           WHERE 'sent:' || e.id=?
         ORDER BY 8,1 LIMIT ? OFFSET ?",
    );
    let mut messages = sqlx::query(&query)
        .bind(id)
        .bind(id)
        .bind((THREAD_PAGE + 1) as i64)
        .bind(i64::from(offset))
        .map(message)
        .fetch_all(pool)
        .await?;
    let has_more = messages.len() > THREAD_PAGE;
    messages.truncate(THREAD_PAGE);
    if messages.is_empty() && offset == 0 {
        return Err(Failure::Code("mail_not_found"));
    }
    Ok(Reply::Thread { messages, has_more })
}

fn message(r: SqliteRow) -> Message {
    let body = clip(r.get("body_text"), PREVIEW);
    let attachments = serde_json::from_str(r.get("attachment_names")).unwrap_or_default();
    Message {
        row: row(r),
        body,
        attachments,
    }
}

fn row(r: SqliteRow) -> InboxRow {
    InboxRow {
        id: r.get("id"),
        account_id: r.get("account_id"),
        contact_id: r.get("contact_id"),
        contact_name: r.get("contact_name"),
        thread_id: r.get("thread_id"),
        sender: r.get("sender"),
        subject: r.get("subject"),
        received_at: r.get("received_at"),
        kind: r.get("kind"),
        received: r.get("received"),
    }
}
