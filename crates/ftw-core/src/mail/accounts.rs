use super::{
    audit, check_text, email_address, hostname, policy, protect_password, Failure, MailService,
    Reply, Result,
};
use ftw_ipc::{crm::Source, mail::AccountUpdate};
use sqlx::SqliteConnection;

impl MailService {
    pub(super) async fn update_account(&self, input: AccountUpdate) -> Result<Reply> {
        check_text(&input.name, 180)?;
        check_text(&input.username, 320)?;
        let email = email_address(&input.email)?;
        let host = hostname(&input.host)?;
        if input.port == 0 || !matches!(input.security.as_str(), "tls" | "starttls") {
            return Err(Failure::Code("smtp_configuration_invalid"));
        }
        let protected = input
            .password
            .map(|password| protect_password(&password.0))
            .transpose()?;
        let _delivery = self
            .delivery
            .try_lock()
            .map_err(|_| Failure::Code("mail_account_busy"))?;
        let _imap = self
            .imap
            .try_lock()
            .map_err(|_| Failure::Code("imap_busy"))?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let previous: String = sqlx::query_scalar(
            "SELECT email_address FROM email_accounts WHERE id=? AND archived_at IS NULL",
        )
        .bind(&input.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Failure::Code("mail_not_found"))?;
        sqlx::query("UPDATE email_accounts SET display_name=?,email_address=?,smtp_host=?,smtp_port=?,smtp_security=?,smtp_username=?,updated_at=unixepoch() WHERE id=?")
            .bind(input.name.trim()).bind(&email).bind(host).bind(i64::from(input.port))
            .bind(&input.security).bind(input.username.trim()).bind(&input.id).execute(&mut *tx).await?;
        if let Some(protected) = protected {
            sqlx::query("UPDATE local_secrets SET protected=? WHERE id=(SELECT secret_ref FROM email_accounts WHERE id=?)")
                .bind(protected).bind(&input.id).execute(&mut *tx).await?;
        }
        if !previous.eq_ignore_ascii_case(&email) {
            disable_queued(&mut tx, &input.id, "mail_account_changed").await?;
            reset_approvals(&mut tx, &input.id).await?;
            remove_imap(&mut tx, &input.id).await?;
            policy::remove_account(&mut tx, &input.id).await?;
        }
        audit(
            &mut tx,
            Source::Manual,
            "email_account",
            &input.id,
            "update",
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved { id: input.id })
    }

    pub(super) async fn set_default_sender(&self, id: String) -> Result<Reply> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let result = sqlx::query(
            "INSERT INTO app_settings(key,value) SELECT 'default_sender_id',id FROM email_accounts
             WHERE id=? AND archived_at IS NULL ON CONFLICT(key) DO UPDATE SET value=excluded.value,
             updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Failure::Code("mail_not_found"));
        }
        audit(
            &mut tx,
            Source::Manual,
            "email_account",
            &id,
            "set_default_sender",
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved { id })
    }

    pub(super) async fn delete_account(&self, id: String) -> Result<Reply> {
        let _delivery = self
            .delivery
            .try_lock()
            .map_err(|_| Failure::Code("mail_account_busy"))?;
        let _imap = self
            .imap
            .try_lock()
            .map_err(|_| Failure::Code("imap_busy"))?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let changed = sqlx::query("UPDATE email_accounts SET archived_at=unixepoch(),updated_at=unixepoch() WHERE id=? AND archived_at IS NULL")
            .bind(&id).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            return Err(Failure::Code("mail_not_found"));
        }
        sqlx::query("UPDATE local_secrets SET protected=X'' WHERE id=(SELECT secret_ref FROM email_accounts WHERE id=?)")
            .bind(&id).execute(&mut *tx).await?;
        disable_queued(&mut tx, &id, "mail_account_removed").await?;
        reset_approvals(&mut tx, &id).await?;
        remove_imap(&mut tx, &id).await?;
        policy::remove_account(&mut tx, &id).await?;
        sqlx::query("DELETE FROM app_settings WHERE key='default_sender_id' AND value=?")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        audit(&mut tx, Source::Manual, "email_account", &id, "delete").await?;
        tx.commit().await?;
        Ok(Reply::Saved { id })
    }
}

async fn disable_queued(db: &mut SqliteConnection, id: &str, reason: &str) -> Result<()> {
    sqlx::query("UPDATE email_outbox_jobs SET state='failed',outcome=? WHERE state='queued' AND draft_id IN (SELECT id FROM email_drafts WHERE account_id=?)")
        .bind(reason).bind(id).execute(&mut *db).await?;
    sqlx::query("UPDATE email_drafts SET state='failed',updated_at=unixepoch() WHERE account_id=? AND state='queued'")
        .bind(id).execute(&mut *db).await?;
    Ok(())
}

async fn reset_approvals(db: &mut SqliteConnection, id: &str) -> Result<()> {
    sqlx::query("UPDATE email_drafts SET state='draft',revision=revision+1,updated_at=unixepoch() WHERE account_id=? AND state IN ('pending_approval','approved')")
        .bind(id).execute(&mut *db).await?;
    Ok(())
}

async fn remove_imap(db: &mut SqliteConnection, id: &str) -> Result<()> {
    let secret: Option<String> =
        sqlx::query_scalar("SELECT secret_ref FROM imap_accounts WHERE account_id=?")
            .bind(id)
            .fetch_optional(&mut *db)
            .await?;
    sqlx::query("DELETE FROM imap_accounts WHERE account_id=?")
        .bind(id)
        .execute(&mut *db)
        .await?;
    if let Some(secret) = secret {
        sqlx::query("DELETE FROM local_secrets WHERE id=?")
            .bind(secret)
            .execute(&mut *db)
            .await?;
    }
    sqlx::query("DELETE FROM imap_sync_state WHERE account_id=?")
        .bind(id)
        .execute(&mut *db)
        .await?;
    Ok(())
}
