use super::{
    account, check_recipient, check_suppression, new_id, smtp, snapshot, AccountRow, Failure,
    MailService, Result, Snapshot,
};
use sqlx::{FromRow, SqliteConnection};
use std::time::Duration;

#[derive(FromRow)]
struct Job {
    id: String,
    draft_id: String,
    revision: i64,
    message_id_header: String,
    content_sha256: String,
    valid: bool,
}

impl MailService {
    pub(super) async fn send_next(&self) -> Result<()> {
        self.send_next_with(smtp::transport).await
    }

    pub(super) async fn send_next_with(
        &self,
        build: impl FnOnce(&AccountRow) -> Result<lettre::AsyncSmtpTransport<lettre::Tokio1Executor>>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let Some(job) = sqlx::query_as::<_,Job>(
            "SELECT j.id,j.draft_id,j.revision,j.message_id_header,a.content_sha256, a.expires_at>unixepoch() valid
                FROM email_outbox_jobs j JOIN email_approvals a ON a.id=j.approval_id AND a.draft_id=j.draft_id AND a.revision=j.revision
                WHERE j.state='queued' ORDER BY j.created_at,j.id LIMIT 1",
        ).fetch_optional(&mut *tx).await? else { return Ok(()); };

        let (draft, account) = match verify(&mut tx, &job).await {
            Ok(value) => value,
            Err(Failure::Code(reason)) => {
                finish(&mut tx, &job, "failed", reason).await?;
                tx.commit().await?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let prepared = build(&account).and_then(|transport| {
            smtp::message(&draft, job.message_id_header.clone()).map(|message| (transport, message))
        });
        let (transport, message) = match prepared {
            Ok(value) => value,
            Err(error) => {
                finish(&mut tx, &job, "failed", error.code()).await?;
                tx.commit().await?;
                return Ok(());
            }
        };
        sqlx::query("UPDATE email_outbox_jobs SET state='sending',attempt_count=attempt_count+1,last_attempt_at=unixepoch() WHERE id=?")
            .bind(&job.id).execute(&mut *tx).await?;
        sqlx::query("UPDATE email_drafts SET state='sending',updated_at=unixepoch() WHERE id=?")
            .bind(&job.draft_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        let (state, outcome) =
            tokio::time::timeout(Duration::from_secs(40), smtp::send(&transport, message))
                .await
                .unwrap_or(("delivery_unknown", "smtp_result_unknown"));
        self.record_send_result(&job, &draft, state, outcome).await
    }

    async fn record_send_result(
        &self,
        job: &Job,
        draft: &Snapshot,
        state: &str,
        outcome: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        if state == "smtp_accepted" {
            let message_id = new_id();
            sqlx::query("INSERT INTO email_messages(id,contact_id,account_id,outbox_id,message_id_header,from_address,to_address,subject,body_text,submitted_at)
                VALUES (?,?,?,?,?,?,?,?,?,unixepoch())")
                .bind(&message_id).bind(&draft.contact_id).bind(&draft.account_id).bind(&job.id).bind(&job.message_id_header)
                .bind(&draft.sender).bind(&draft.recipient).bind(&draft.subject).bind(&draft.body).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO activities(id,contact_id,event_type,occurred_at,summary,details,related_message_id,source,actor_id)
                VALUES (?,?,'email_sent',unixepoch(),?,'SMTP 服务器已接受。',?,'system','smtp-worker')")
                .bind(new_id()).bind(&draft.contact_id).bind(&draft.subject).bind(message_id).execute(&mut *tx).await?;
        }
        finish(&mut tx, job, state, outcome).await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn verify(db: &mut SqliteConnection, job: &Job) -> Result<(Snapshot, AccountRow)> {
    let (draft, revision, state) = snapshot(db, &job.draft_id).await?;
    if !job.valid {
        return Err(Failure::Code("approval_expired"));
    }
    if revision != job.revision || state != "queued" || draft.digest()? != job.content_sha256 {
        return Err(Failure::Code("approval_snapshot_changed"));
    }
    check_recipient(db, &draft.contact_id, &draft.recipient).await?;
    check_suppression(db, &draft.recipient).await?;
    let account = account(db, &draft.account_id).await?;
    if account.email_address != draft.sender {
        return Err(Failure::Code("approval_snapshot_changed"));
    }
    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM email_outbox_jobs j JOIN email_drafts d ON d.id=j.draft_id WHERE d.account_id=? AND j.last_attempt_at>unixepoch()-86400")
        .bind(&draft.account_id).fetch_one(&mut *db).await?;
    if attempts >= account.daily_limit {
        return Err(Failure::Code("smtp_daily_limit"));
    }
    Ok((draft, account))
}

async fn finish(db: &mut SqliteConnection, job: &Job, state: &str, outcome: &str) -> Result<()> {
    sqlx::query("UPDATE email_outbox_jobs SET state=?, outcome=? WHERE id=?")
        .bind(state)
        .bind(outcome)
        .bind(&job.id)
        .execute(&mut *db)
        .await?;
    sqlx::query("UPDATE email_drafts SET state=?,updated_at=unixepoch() WHERE id=?")
        .bind(state)
        .bind(&job.draft_id)
        .execute(&mut *db)
        .await?;
    sqlx::query("INSERT INTO audit_logs(id,actor_id,source,entity,entity_id,operation) VALUES (?,'smtp-worker','system','email_outbox',?,?)")
        .bind(new_id()).bind(&job.id).bind(outcome).execute(db).await?;
    Ok(())
}
