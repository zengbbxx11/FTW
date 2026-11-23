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
    role: &'static str,
    delivery: Option<Delivery>,
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
            role: "unparsed",
            delivery: None,
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
        ct.ctype().eq_ignore_ascii_case("multipart")
            && ct
                .subtype()
                .is_some_and(|value| value.eq_ignore_ascii_case("report"))
            && ct
                .attribute("report-type")
                .is_some_and(|value| value.eq_ignore_ascii_case("delivery-status"))
    });
    let automatic = mail
        .headers()
        .all("Auto-Submitted")
        .filter_map(|header| header.value().as_text())
        .any(|value| {
            !value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case("no")
        });
    let role = if report {
        "delivery_report"
    } else if automatic {
        "automatic"
    } else {
        "correspondence"
    };
    let delivery = if report && !oversized {
        delivery_report(&mail)
    } else {
        None
    };
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
        role,
        delivery,
    }
}

struct Delivery {
    original_id: String,
    recipient: String,
    action: String,
    status: String,
}

fn delivery_report(mail: &mail_parser::Message<'_>) -> Option<Delivery> {
    let part = mail
        .parts()
        .find(|part| part.is_content_type("message", "delivery-status"))?;
    let text = String::from_utf8(part.decoded().into_owned())
        .ok()?
        .replace("\r\n", "\n");
    let (envelope, recipients) = text.split_once("\n\n")?;
    let original = dsn_field(envelope, "Original-Message-ID")
        .map(str::to_owned)
        .or_else(|| {
            mail.parts()
                .find(|part| part.is_content_type("message", "rfc822") && part.is_message())
                .and_then(|part| part.nested()?.message_id().map(str::to_owned))
        })?;
    let original = original
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>');
    if original.is_empty() || original.len() > 512 {
        return None;
    }
    let mut blocks = recipients
        .split("\n\n")
        .filter(|block| !block.trim().is_empty());
    let recipient = blocks.next()?;
    if blocks.next().is_some() {
        return None;
    }
    let (address_type, address) = dsn_field(recipient, "Final-Recipient")?.split_once(';')?;
    if !address_type.trim().eq_ignore_ascii_case("rfc822") {
        return None;
    }
    let address = super::super::email_address(address.trim()).ok()?;
    let action = dsn_field(recipient, "Action")?.to_ascii_lowercase();
    let status = dsn_field(recipient, "Status")?;
    if !((action == "failed" && status.starts_with("5."))
        || (action == "delayed" && status.starts_with("4.")))
    {
        return None;
    }
    Some(Delivery {
        original_id: format!("<{original}>"),
        recipient: address.to_ascii_lowercase(),
        action,
        status: clip(status, 32),
    })
}

fn dsn_field<'a>(block: &'a str, name: &str) -> Option<&'a str> {
    block
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then_some(value.trim()))
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
        let delivery = if let Some(report) = &mail.delivery {
            sqlx::query_as::<_, (String, String, String)>(
                "SELECT outbox_id,contact_id,'sent:' || id FROM email_messages
                WHERE account_id=? AND message_id_header=? AND lower(to_address)=?",
            )
            .bind(account)
            .bind(&report.original_id)
            .bind(&report.recipient)
            .fetch_optional(&mut *tx)
            .await?
        } else {
            None
        };
        let (contact, thread, match_state) = if let Some((_, contact, thread)) = &delivery {
            (Some(contact.clone()), thread.clone(), "delivery")
        } else if mail.role == "delivery_report" {
            (None, format!("inbox:{id}"), "pending")
        } else {
            let (contact, thread) = associate(&mut tx, account, &id, &mail).await?;
            let state = if thread.starts_with("sent:") {
                "thread"
            } else if contact.is_some() {
                "contact"
            } else {
                "pending"
            };
            (contact, thread, state)
        };
        let at = received.filter(|at| (0..=253_402_300_799).contains(at));
        sqlx::query("INSERT INTO inbox_messages(id,account_id,contact_id,thread_id,message_id_header,raw_sha256,sender,subject,body_text,attachment_names,received_at,raw,kind,message_role,match_state,body_complete)
            VALUES (?,?,?,?,?,?,?,?,?,?,COALESCE(?,unixepoch()),?,?,?,?,?)")
            .bind(&id).bind(account).bind(contact.as_deref()).bind(thread).bind(&mail.header_id).bind(hash).bind(&mail.sender)
            .bind(&mail.subject).bind(mail.body).bind(serde_json::json!(mail.attachments).to_string()).bind(at).bind(raw).bind(mail.kind)
            .bind(mail.role).bind(match_state).bind(!oversized)
            .execute(&mut *tx).await?;
        if let (Some((outbox, _, _)), Some(report)) = (delivery, mail.delivery) {
            sqlx::query("INSERT INTO mail_delivery_events(inbox_id,outbox_id,action,status) VALUES (?,?,?,?)")
                .bind(&id).bind(outbox).bind(report.action).bind(report.status).execute(&mut *tx).await?;
        }
        if let Some(contact) = contact.filter(|_| mail.role == "correspondence") {
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
    link_contact(pool, id, contact, None).await
}

pub(super) async fn review_link(
    pool: &SqlitePool,
    id: &str,
    contact: &str,
    expected_contact: Option<String>,
    expected_match: String,
) -> Result<Reply> {
    link_contact(pool, id, contact, Some((expected_contact, expected_match))).await
}

async fn link_contact(
    pool: &SqlitePool,
    id: &str,
    contact: &str,
    expected: Option<(Option<String>, String)>,
) -> Result<Reply> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if let Some(expected) = expected {
        let current: (Option<String>, String) =
            sqlx::query_as("SELECT contact_id,match_state FROM inbox_messages WHERE id=?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Failure::Code("mail_not_found"))?;
        if current != expected {
            return Err(Failure::Code("mail_link_changed"));
        }
    }
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
         contact_id=?,match_state='manual' WHERE id=?",
    ).bind(contact).bind(contact).bind(id).execute(&mut *tx).await?;
    if updated.rows_affected() != 1 {
        return Err(Failure::Code("mail_not_found"));
    }
    let correspondence: bool = sqlx::query_scalar("SELECT message_role NOT IN ('automatic','delivery_report','unparsed') FROM inbox_messages WHERE id=?")
        .bind(id).fetch_one(&mut *tx).await?;
    if correspondence {
        activity(&mut tx, id, contact).await?;
    }
    audit(&mut tx, Source::Manual, "inbox_message", id, "link_contact").await?;
    tx.commit().await?;
    Ok(Reply::Saved { id: id.to_owned() })
}

pub(super) async fn review_candidates(pool: &SqlitePool, id: &str) -> Result<Reply> {
    use ftw_ipc::inbox::ReviewCandidate;
    let rows = sqlx::query(
        "SELECT c.id,c.display_name,c.company_name,c.country_region,
            EXISTS(SELECT 1 FROM contact_methods e WHERE e.kind='email' AND e.contact_id=c.id
              AND e.normalized=lower(i.sender)) exact_sender
         FROM inbox_messages i JOIN contacts c ON c.id=i.contact_id OR c.id IN (
             SELECT contact_id FROM contact_methods m WHERE m.kind='email' AND m.normalized=lower(i.sender))
         WHERE i.id=? AND c.archived_at IS NULL ORDER BY exact_sender DESC,c.display_name,c.id LIMIT 21",
    ).bind(id).map(|row: SqliteRow| ReviewCandidate {
        id: row.get("id"), name: row.get("display_name"), company: row.get("company_name"),
        country: row.get("country_region"), exact_sender: row.get("exact_sender"),
    }).fetch_all(pool).await?;
    Ok(Reply::ReviewCandidates { rows })
}

pub(crate) const NEEDS_REVIEW: &str = "(i.contact_id IS NULL OR i.match_state='contact')";

const RECEIVE: &str = "SELECT i.id,i.account_id,i.contact_id,c.display_name contact_name,i.thread_id,i.sender,i.subject,i.received_at,i.kind,1 received,i.body_text,i.attachment_names,i.message_role,i.match_state,i.body_complete,i.download_outcome
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
        query.push(" AND ").push(NEEDS_REVIEW);
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
            e.from_address,e.subject,e.submitted_at,'smtp_accepted',0,e.body_text,'[]','outgoing','thread',1,NULL
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
        message_role: r.get("message_role"),
        match_state: r.get("match_state"),
        body_complete: r.get("body_complete"),
        download_outcome: r.get("download_outcome"),
    }
}

pub(super) async fn complete(
    pool: &SqlitePool,
    root: &std::path::Path,
    id: &str,
    raw: &[u8],
) -> Result<()> {
    use crate::mail::attachments;
    let parsed = parse(raw, false);
    if parsed.role == "unparsed" {
        return Err(Failure::Code("imap_download_incomplete"));
    }
    let sha = attachments::store(root, raw)
        .await
        .map_err(attachments::file_error)?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE inbox_messages SET body_text=?,attachment_names=?,body_complete=1,full_sha256=?,
        full_size=?,download_outcome='ok' WHERE id=?",
    )
    .bind(parsed.body)
    .bind(serde_json::json!(parsed.attachments).to_string())
    .bind(sha)
    .bind(raw.len() as i64)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if let Some(report) = parsed.delivery {
        let account: String =
            sqlx::query_scalar("SELECT account_id FROM inbox_messages WHERE id=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if let Some((job,contact,thread)) = sqlx::query_as::<_,(String,String,String)>(
            "SELECT outbox_id,contact_id,'sent:' || id FROM email_messages WHERE account_id=? AND message_id_header=? AND lower(to_address)=?",
        ).bind(account).bind(report.original_id).bind(report.recipient).fetch_optional(&mut *tx).await? {
            sqlx::query("INSERT OR IGNORE INTO mail_delivery_events(inbox_id,outbox_id,action,status) VALUES (?,?,?,?)")
                .bind(id).bind(job).bind(report.action).bind(report.status).execute(&mut *tx).await?;
            sqlx::query("UPDATE inbox_messages SET contact_id=?,thread_id=?,match_state='delivery' WHERE id=? AND match_state<>'manual'")
                .bind(contact).bind(thread).bind(id).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

pub(super) async fn full_bytes(
    pool: &SqlitePool,
    root: &std::path::Path,
    id: &str,
) -> Result<Vec<u8>> {
    let (inline, sha, size): (Vec<u8>, Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT raw,full_sha256,full_size FROM inbox_messages WHERE id=? AND body_complete",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(Failure::Code("imap_body_not_loaded"))?;
    match (sha, size) {
        (Some(sha), Some(size)) => crate::mail::attachments::read_object(root, &sha, size as usize)
            .await
            .map_err(crate::mail::attachments::file_error),
        _ => Ok(inline),
    }
}

pub(super) fn body_page(raw: &[u8], offset: u32) -> Result<Reply> {
    let mail = MessageParser::new()
        .max_parts(160)
        .parse(raw)
        .ok_or(Failure::Code("imap_download_incomplete"))?;
    let text = mail.body_text(0).unwrap_or_default();
    let text = clip(&text, text.len());
    let start = offset as usize;
    if !text.is_char_boundary(start) {
        return Err(Failure::Code("invalid_mail_field"));
    }
    let part = clip(&text[start..], 65_536);
    let end = start + part.len();
    Ok(Reply::Body {
        text: part,
        next_offset: (end < text.len()).then_some(end as u32),
    })
}

pub(super) fn inbound_attachment(raw: &[u8], index: u16) -> Result<Vec<u8>> {
    let parser = MessageParser::new().max_parts(160);
    let mail = parser
        .parse(raw)
        .ok_or(Failure::Code("imap_download_incomplete"))?;
    let part = mail
        .attachments()
        .filter(|part| part.attachment_name().is_some())
        .take(25)
        .nth(index as usize)
        .ok_or(Failure::Code("attachment_not_available"))?;
    Ok(part.decoded().into_owned())
}
