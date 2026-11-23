#[cfg(test)]
mod tests;
mod transfer;

use crate::{mail::MailService, storage::backup};
use ftw_ipc::{
    data::{AuditRow, Command, Result as Reply, SummaryRow},
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
        Command::Backups => backup::list(&profile.root)
            .map(|rows| Reply::Backups {
                directory: display_path(&profile.root.join("backups")),
                rows,
            })
            .map_err(|_| "backup_list_failed"),
        Command::CreateBackup => backup::create(pool, &profile.root, "manual")
            .await
            .map(|path| Reply::File {
                path: display_path(&path),
            })
            .map_err(|_| "backup_create_failed"),
        Command::Restore { filename } => mail
            .restore(&profile.root, &filename)
            .await
            .map(|safety_backup| Reply::Restored { safety_backup }),
        Command::Audit { offset } => audit(pool, offset).await,
        Command::ImportContacts {
            csv,
            apply,
            default_stage,
        } => transfer::import(pool, &csv, apply, &default_stage).await,
        Command::ExportContacts => transfer::export(pool, &profile.root).await,
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

async fn dashboard(pool: &SqlitePool) -> Result<Reply> {
    let pending =
        sqlx::query_scalar("SELECT COUNT(*) FROM mail_drafts_review WHERE state='pending_approval' OR approval_expired")
            .fetch_one(pool)
            .await
            .map_err(|_| "database_unavailable")?;
    let unmatched =
        sqlx::query_scalar("SELECT COUNT(*) FROM inbox_messages WHERE contact_id IS NULL")
            .fetch_one(pool)
            .await
            .map_err(|_| "database_unavailable")?;
    let recent = sqlx::query(
        "SELECT a.summary title,c.display_name detail,a.occurred_at FROM activities a JOIN contacts c ON c.id=a.contact_id
         ORDER BY a.occurred_at DESC,a.id LIMIT 8",
    ).map(summary).fetch_all(pool).await.map_err(|_| "database_unavailable")?;
    let issues = sqlx::query(
        "SELECT a.display_name title,s.outcome detail,s.last_sync occurred_at FROM imap_sync_state s JOIN email_accounts a ON a.id=s.account_id
         WHERE s.outcome NOT IN ('running','ok','more_available') ORDER BY s.last_sync DESC LIMIT 8",
    ).map(summary).fetch_all(pool).await.map_err(|_| "database_unavailable")?;
    Ok(Reply::Dashboard {
        pending,
        unmatched,
        recent,
        issues,
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
        "SELECT * FROM audit_logs ORDER BY created_at DESC,rowid DESC LIMIT 21 OFFSET ?",
    )
    .bind(i64::from(offset))
    .map(|row: SqliteRow| AuditRow {
        occurred_at: row.get("created_at"),
        actor: row.get("actor_id"),
        source: row.get("source"),
        entity: row.get("entity"),
        entity_id: row.get("entity_id"),
        operation: row.get("operation"),
    })
    .fetch_all(pool)
    .await
    .map_err(|_| "database_unavailable")?;
    let has_more = rows.len() > 20;
    rows.truncate(20);
    Ok(Reply::Audit { rows, has_more })
}
