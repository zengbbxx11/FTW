pub(crate) mod agent_runs;
pub(crate) mod analytics;
pub(crate) mod audit_workspace;
pub(crate) mod search;
#[cfg(test)]
mod tests;
pub(crate) mod transfer;

use crate::{mail::MailService, storage::backup};
use ftw_ipc::{
    data::{
        AuditRow, Command, DailySendCount, DashboardStats, RecentActivity, Result as Reply,
        SummaryRow,
    },
    display_path, Profile, Response,
};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

pub async fn dispatch(
    pool: &SqlitePool,
    mail: &MailService,
    profile: &Profile,
    command: Command,
) -> Response {
    let reply = match command {
        Command::Dashboard => dashboard(pool).await,
        Command::DataOverview => data_overview(pool).await,
        Command::AgentRuns {
            filter,
            keyword,
            offset,
        } => agent_runs::list(pool, filter, &keyword, offset, None).await,
        Command::AgentRunMail {
            agent_id,
            task_id,
            offset,
        } => agent_runs::mail_jobs(pool, Some(&agent_id), &task_id, offset).await,
        Command::UpdateMailJob { job_id, job_action } => {
            mail.update_mail_job(crate::crm::Actor::DESKTOP, &job_id, job_action)
                .await
        }
        Command::MailAnalytics {
            days,
            include_legacy,
        } => analytics::report(pool, days, include_legacy)
            .await
            .map(|data| Reply::MailAnalytics { data }),
        Command::ExportMailAnalytics {
            days,
            include_legacy,
        } => analytics::export(pool, &profile.root, days, include_legacy).await,
        Command::Search {
            keyword,
            category,
            offset,
        } => search::search(pool, &keyword, category, offset).await,
        Command::Backups => backup::list(&profile.root)
            .map(|rows| Reply::Backups {
                directory: display_path(&profile.root.join("backups")),
                rows,
            })
            .map_err(|_| "backup_list_failed"),
        Command::CreateBackup => backup::create_manual(pool, &profile.root)
            .await
            .map(|path| Reply::File {
                path: display_path(&path),
            })
            .map_err(|_| "backup_create_failed"),
        Command::InspectBackup { filename } => backup::inspect(&profile.root, &filename)
            .await
            .map_err(|_| "backup_inspect_failed"),
        Command::Restore { filename } => mail
            .restore(&profile.root, &filename)
            .await
            .map(|safety_backup| Reply::Restored { safety_backup }),
        Command::Audit { offset } => audit(pool, offset).await,
        Command::AuditWorkspace {
            offset,
            source,
            entity,
            keyword,
            days,
        } => audit_workspace::list(pool, offset, &source, &entity, &keyword, days).await,
        Command::ImportContacts {
            csv,
            apply,
            default_stage,
        } => transfer::import(pool, &csv, apply, &default_stage).await,
        Command::ExportContacts => transfer::export(pool, &profile.root).await,
        Command::ExportSelectedContacts { scope, fields } => {
            transfer::export_selected(pool, &profile.root, scope, &fields).await
        }
    };
    match reply {
        Ok(data) => Response::Data {
            data: Box::new(data),
        },
        Err(code) => Response::Error {
            code: code.to_owned(),
            detail: None,
        },
    }
}

type Result<T> = std::result::Result<T, &'static str>;

async fn data_overview(pool: &SqlitePool) -> Result<Reply> {
    let (customers, mail_records) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM contacts WHERE merged_into IS NULL),
        (SELECT count(*) FROM email_messages)+(SELECT count(*) FROM inbox_messages)",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| "database_unavailable")?;
    Ok(Reply::DataOverview {
        customers,
        mail_records,
    })
}

async fn dashboard(pool: &SqlitePool) -> Result<Reply> {
    let mut tx = pool.begin().await.map_err(|_| "database_unavailable")?;
    let pending =
        sqlx::query_scalar("SELECT COUNT(*) FROM mail_drafts_review WHERE state='pending_approval' OR approval_expired")
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| "database_unavailable")?;
    let unmatched = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM inbox_messages i WHERE {}",
        crate::mail::INBOX_NEEDS_REVIEW
    ))
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "database_unavailable")?;
    let recent = sqlx::query(
        "SELECT a.id,a.event_type,a.source,a.summary title,c.display_name detail,a.occurred_at
         FROM activities a JOIN contacts c ON c.id=a.contact_id ORDER BY a.occurred_at DESC,a.id LIMIT 8",
    ).map(|row: SqliteRow| RecentActivity {
        id: row.get("id"), event_type: row.get("event_type"), source: row.get("source"), summary: summary(row),
    }).fetch_all(&mut *tx).await.map_err(|_| "database_unavailable")?;
    let issues = sqlx::query(
        "SELECT a.display_name title,s.outcome detail,s.last_sync occurred_at FROM imap_sync_state s JOIN email_accounts a ON a.id=s.account_id
         WHERE s.outcome NOT IN ('running','ok','more_available') ORDER BY s.last_sync DESC LIMIT 8",
    ).map(summary).fetch_all(&mut *tx).await.map_err(|_| "database_unavailable")?;
    let stats = sqlx::query(
        "SELECT (SELECT count(*) FROM contacts WHERE archived_at IS NULL) contacts,
            (SELECT count(*) FROM email_messages) smtp_accepted,
            count(*) FILTER (WHERE m.mail_status='sent_no_reply_seen') waiting,
            count(*) FILTER (WHERE m.mail_status='replied') replied, count(*) sent_contacts
         FROM mail_contact_outreach m JOIN contacts c ON c.id=m.contact_id
         WHERE c.archived_at IS NULL AND m.last_sent_at IS NOT NULL",
    )
    .map(|row: SqliteRow| DashboardStats {
        contacts: row.get("contacts"),
        smtp_accepted: row.get("smtp_accepted"),
        waiting: row.get("waiting"),
        replied: row.get("replied"),
        sent_contacts: row.get("sent_contacts"),
    })
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "database_unavailable")?;
    let trend = sqlx::query(
        "WITH RECURSIVE days(day) AS (
            SELECT date('now','localtime','-29 days')
            UNION ALL SELECT date(day,'+1 day') FROM days WHERE day<date('now','localtime')
         ), daily AS (
            SELECT date(submitted_at,'unixepoch','localtime') day,count(*) accepted
            FROM email_messages WHERE submitted_at>=unixepoch('now','-30 days') GROUP BY 1
         ) SELECT days.day,coalesce(daily.accepted,0) accepted FROM days LEFT JOIN daily USING(day) ORDER BY 1",
    ).map(|row: SqliteRow| DailySendCount { day: row.get("day"), accepted: row.get("accepted") })
        .fetch_all(&mut *tx).await.map_err(|_| "database_unavailable")?;
    let agent = sqlx::query(
        "SELECT a.*,r.name actor_name FROM audit_logs a LEFT JOIN mail_agent_clients r ON r.id=a.actor_id
         WHERE a.source='agent' ORDER BY a.created_at DESC,a.rowid DESC LIMIT 5",
    )
    .map(audit_row)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| "database_unavailable")?;
    tx.commit().await.map_err(|_| "database_unavailable")?;
    Ok(Reply::Dashboard {
        pending,
        unmatched,
        recent,
        issues,
        stats,
        trend,
        agent,
    })
}

fn summary(row: SqliteRow) -> SummaryRow {
    SummaryRow {
        title: row.get("title"),
        detail: row.get("detail"),
        occurred_at: row.get("occurred_at"),
    }
}

async fn audit(pool: &SqlitePool, offset: u32) -> Result<Reply> {
    let mut rows = sqlx::query(
        "SELECT a.*, r.name actor_name FROM audit_logs a LEFT JOIN mail_agent_clients r ON r.id=a.actor_id
         ORDER BY a.created_at DESC,a.rowid DESC LIMIT 21 OFFSET ?",
    )
    .bind(i64::from(offset))
    .map(audit_row)
    .fetch_all(pool)
    .await
    .map_err(|_| "database_unavailable")?;
    let has_more = rows.len() > 20;
    rows.truncate(20);
    Ok(Reply::Audit { rows, has_more })
}

fn audit_row(row: SqliteRow) -> AuditRow {
    AuditRow {
        occurred_at: row.get("created_at"),
        actor: row.get("actor_id"),
        actor_name: row.get("actor_name"),
        task_id: row.get("task_id"),
        source: row.get("source"),
        entity: row.get("entity"),
        entity_id: row.get("entity_id"),
        operation: row.get("operation"),
    }
}
