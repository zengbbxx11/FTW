use super::{audit_as, Failure, MailService, Result};
use crate::crm::Actor;
use ftw_ipc::{
    crm::Source,
    data::{MailJobAction, Result as Reply},
};
use sqlx::FromRow;

pub(crate) const CANCELLED: &str = "cancelled_before_submission";
pub(crate) const CAN_CANCEL: &str =
    "j.state='queued' AND d.state='queued' AND j.revision=d.revision
    AND j.attempt_count=0 AND j.last_attempt_at IS NULL";
pub(crate) const CAN_REOPEN: &str =
    "j.state='failed' AND d.state='failed' AND j.revision=d.revision
    AND j.outcome IS NOT 'cancelled_before_submission'
    AND (j.attempt_count=0 OR COALESCE(j.outcome='smtp_rejected',0))";

#[derive(FromRow)]
struct Job {
    draft_id: String,
    revision: i64,
    task_id: Option<String>,
    can_cancel: bool,
    can_reopen: bool,
}

impl MailService {
    pub(crate) async fn update_mail_job(
        &self,
        actor: Actor<'_>,
        id: &str,
        action: MailJobAction,
    ) -> std::result::Result<Reply, &'static str> {
        self.update_job(actor, id, action)
            .await
            .map_err(|error| error.code())
    }

    async fn update_job(&self, actor: Actor<'_>, id: &str, action: MailJobAction) -> Result<Reply> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let job: Job = sqlx::query_as(&format!(
            "SELECT j.draft_id,j.revision,j.task_id, ({CAN_CANCEL}) can_cancel, ({CAN_REOPEN}) can_reopen
             FROM email_outbox_jobs j JOIN email_drafts d ON d.id=j.draft_id
             WHERE j.id=? AND (? OR j.agent_id=?)"
        )).bind(id).bind(matches!(actor.source, Source::Manual)).bind(actor.id)
            .fetch_optional(&mut *tx).await?.ok_or(Failure::Code("mail_not_found"))?;
        let (operation, state, available) = match action {
            MailJobAction::Cancel => ("cancel_mail_job", "cancelled", job.can_cancel),
            MailJobAction::Reopen => ("prepare_mail_retry", "draft_ready", job.can_reopen),
        };
        let reply = Reply::MailJobUpdated {
            job_id: id.to_owned(),
            draft_id: job.draft_id.clone(),
            draft_revision: job.revision + 1,
            state: state.into(),
        };
        if !available {
            let recorded: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM audit_logs
                WHERE entity='email_outbox' AND entity_id=? AND operation=?)",
            )
            .bind(id)
            .bind(operation)
            .fetch_one(&mut *tx)
            .await?;
            return if recorded {
                Ok(reply)
            } else {
                Err(Failure::Code("mail_job_action_unavailable"))
            };
        }
        if matches!(action, MailJobAction::Cancel) {
            sqlx::query("UPDATE email_outbox_jobs SET state='failed',outcome=? WHERE id=?")
                .bind(CANCELLED)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        // A new revision invalidates the old approval and job snapshot. The next submission
        // must pass the normal authorization, suppression and deduplication checks again.
        sqlx::query("UPDATE email_drafts SET state='draft',revision=revision+1,updated_at=unixepoch() WHERE id=?")
            .bind(&job.draft_id).execute(&mut *tx).await?;
        audit_as(
            &mut tx,
            actor.source,
            actor.id,
            "email_outbox",
            id,
            operation,
            job.task_id.as_deref(),
        )
        .await?;
        tx.commit().await?;
        Ok(reply)
    }

    pub(super) async fn suppressions(&self, offset: u32) -> Result<super::Reply> {
        let mut rows = sqlx::query_scalar(
            "SELECT normalized FROM suppression_list ORDER BY normalized LIMIT 21 OFFSET ?",
        )
        .bind(i64::from(offset))
        .fetch_all(&self.pool)
        .await?;
        let has_more = rows.len() > 20;
        rows.truncate(20);
        Ok(super::Reply::Suppression { rows, has_more })
    }
}
