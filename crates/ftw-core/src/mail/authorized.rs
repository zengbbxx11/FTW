use super::{
    check_recipient, check_suppression, check_text, new_id, policy, snapshot, Failure, MailService,
    Reply, Result, Snapshot,
};
use sqlx::{Row, SqliteConnection};

pub(super) struct Submission {
    pub digest: Vec<u8>,
    pub id: String,
    pub revision: i64,
    pub purpose: String,
    pub task: String,
    pub request: String,
}

impl MailService {
    pub(super) async fn outreach_check(&self, recipient: &str) -> Result<Reply> {
        let recipient = super::email_address(recipient)?.to_ascii_lowercase();
        let domain = recipient
            .rsplit_once('@')
            .ok_or(Failure::Code("invalid_mail_address"))?
            .1;
        let mut db = self.pool.acquire().await?;
        let contact_ids = sqlx::query_scalar(
            "SELECT DISTINCT contact_id FROM contact_methods
            WHERE kind='email' AND normalized=? ORDER BY contact_id LIMIT 20",
        )
        .bind(&recipient)
        .fetch_all(&mut *db)
        .await?;
        let same_domain_contact_ids = sqlx::query_scalar(
            "SELECT DISTINCT contact_id FROM contact_methods
            WHERE kind='email' AND normalized<>? AND substr(normalized,instr(normalized,'@')+1)=?
            ORDER BY contact_id LIMIT 20",
        )
        .bind(&recipient)
        .bind(domain)
        .fetch_all(&mut *db)
        .await?;
        let (sent, active) = outreach_history(&mut db, &recipient, None).await?;
        let outcome = match check_suppression(&mut db, &recipient).await {
            Err(Failure::Code(code)) => Some(code.to_owned()),
            Err(error) => return Err(error),
            Ok(()) if sent || active => Some("outreach_already_recorded".to_owned()),
            Ok(()) => match &self.ledger {
                Some(ledger) if ledger.has_active(&recipient, true).await? => {
                    Some("outreach_already_recorded".to_owned())
                }
                None => Some("agent_send_history_unavailable".to_owned()),
                _ => None,
            },
        };
        Ok(Reply::OutreachCheck {
            allowed_for_first_outreach: outcome.is_none(),
            outcome,
            contact_ids,
            same_domain_contact_ids,
        })
    }

    pub(super) async fn enqueue_authorized(&self, submission: Submission) -> Result<Reply> {
        check_text(&submission.task, 96)?;
        check_text(&submission.request, 96)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let agent = policy::registered(&mut tx, submission.digest).await?;
        let config = policy::current(&mut tx).await?;
        let (draft, revision, state) = snapshot(&mut tx, &submission.id).await?;
        if revision != submission.revision {
            return Err(Failure::Code("approval_snapshot_changed"));
        }
        if let Some(row) = sqlx::query("SELECT id,draft_id,revision,purpose,task_id,state FROM email_outbox_jobs WHERE agent_id=? AND request_id=?")
            .bind(&agent).bind(&submission.request).fetch_optional(&mut *tx).await?
        {
            if row.get::<String,_>("draft_id") != submission.id || row.get::<i64,_>("revision") != revision
                || row.get::<String,_>("purpose") != submission.purpose || row.get::<String,_>("task_id") != submission.task
            {
                return Err(Failure::Code("idempotency_conflict"));
            }
            return Ok(Reply::Queued { id: submission.id, job_id: row.get("id"), state: row.get("state") });
        }
        if state != "draft" {
            return Err(Failure::Code("mail_read_only"));
        }
        check_scope(
            &mut tx,
            &config,
            config.revision,
            &agent,
            &draft,
            &submission.purpose,
        )
        .await?;
        check_recipient(&mut tx, &draft.contact_id, &draft.recipient).await?;
        check_suppression(&mut tx, &draft.recipient).await?;
        let recipient = draft.recipient.to_ascii_lowercase();
        check_outreach(&mut tx, &draft, &recipient, &submission.purpose, None).await?;
        let ledger = self
            .ledger
            .as_ref()
            .ok_or(Failure::Code("agent_send_history_unavailable"))?;
        if ledger
            .has_active(&recipient, submission.purpose == "first_outreach")
            .await?
        {
            return Err(Failure::Code("outreach_already_recorded"));
        }
        let queued_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FILTER(WHERE j.state='queued' OR last_attempt_at>unixepoch()-86400)
             FROM email_outbox_jobs j JOIN email_drafts d ON d.id=j.draft_id
             WHERE j.agent_id IS NOT NULL AND d.account_id=?",
        )
        .bind(&draft.account_id)
        .fetch_one(&mut *tx)
        .await?;
        let total_task: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM email_outbox_jobs WHERE agent_id IS NOT NULL AND task_id=?",
        )
        .bind(&submission.task)
        .fetch_one(&mut *tx)
        .await?;
        if total_task >= i64::from(config.task_limit) {
            return Err(Failure::Code("agent_task_limit"));
        }
        if queued_count >= i64::from(config.daily_limit) {
            return Err(Failure::Code("agent_daily_limit"));
        }
        let job = new_id();
        sqlx::query("INSERT INTO email_outbox_jobs(id,draft_id,revision,message_id_header,state,policy_revision,agent_id,task_id,request_id,content_sha256,purpose,normalized_recipient,queued_source)
             VALUES (?,?,?,?,'queued',?,?,?,?,?,?,?,'agent')")
            .bind(&job).bind(&submission.id).bind(revision).bind(format!("<{job}@ftw.local>"))
            .bind(config.revision).bind(&agent).bind(&submission.task).bind(&submission.request)
            .bind(draft.digest()?).bind(&submission.purpose).bind(&recipient).execute(&mut *tx).await?;
        sqlx::query("UPDATE email_drafts SET state='queued',updated_at=unixepoch() WHERE id=?")
            .bind(&submission.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO audit_logs(id,actor_id,source,entity,entity_id,operation) VALUES (?,?,'agent','email_outbox',?,'authorized_enqueue')")
            .bind(new_id()).bind(&agent).bind(&job).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Reply::Queued {
            id: submission.id,
            job_id: job,
            state: "queued".into(),
        })
    }
}

pub(super) async fn check_scope(
    db: &mut SqliteConnection,
    config: &ftw_ipc::mail::AgentMailPolicy,
    revision: i64,
    agent: &str,
    draft: &Snapshot,
    purpose: &str,
) -> Result<()> {
    if config.mode != "automatic"
        || config.revision != revision
        || draft.assets.iter().any(|asset| !asset.agent_allowed)
        || !config.accounts.contains(&draft.account_id)
        || !config.purposes.iter().any(|item| item == purpose)
    {
        return Err(Failure::Code("agent_policy_not_authorized"));
    }
    let grant: Option<bool> =
        sqlx::query_scalar("SELECT automatic FROM mail_agent_clients WHERE id=? AND NOT revoked")
            .bind(agent)
            .fetch_optional(db)
            .await?;
    match grant {
        Some(true) => {}
        Some(false) => return Err(Failure::Code("mail_agent_not_authorized")),
        None => return Err(Failure::Code("mail_agent_unregistered")),
    }
    Ok(())
}

pub(super) async fn check_outreach(
    db: &mut SqliteConnection,
    draft: &Snapshot,
    recipient: &str,
    purpose: &str,
    except_job: Option<&str>,
) -> Result<()> {
    let (sent, active) = outreach_history(db, recipient, except_job).await?;
    if active || (purpose == "first_outreach" && sent) {
        return Err(Failure::Code("outreach_already_recorded"));
    }
    if !sent && purpose != "first_outreach" {
        let inbound: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM inbox_messages WHERE lower(sender)=? AND contact_id=? AND account_id=? AND message_role='correspondence' AND match_state IN ('thread','manual'))",
        ).bind(recipient).bind(&draft.contact_id).bind(&draft.account_id).fetch_one(db).await?;
        if !inbound || purpose == "outreach_followup" {
            return Err(Failure::Code("outreach_requires_history"));
        }
    }
    Ok(())
}

async fn outreach_history(
    db: &mut SqliteConnection,
    recipient: &str,
    except_job: Option<&str>,
) -> Result<(bool, bool)> {
    let (sent, active): (bool,bool) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM email_messages WHERE lower(to_address)=?),
                EXISTS(SELECT 1 FROM email_outbox_jobs j JOIN email_drafts d ON d.id=j.draft_id
                    WHERE lower(d.recipient)=? AND j.state IN ('queued','sending','delivery_unknown') AND (? IS NULL OR j.id<>?))",
    ).bind(recipient).bind(recipient).bind(except_job).bind(except_job).fetch_one(&mut *db).await?;
    Ok((sent, active))
}
