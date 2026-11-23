mod approval;
pub(crate) mod attachments;
mod authorized;
mod credentials;
mod inbox;
pub(crate) use inbox::NEEDS_REVIEW as INBOX_NEEDS_REVIEW;
mod ledger;
mod policy;
mod smtp;
mod user_verification;
mod worker;

use base64::{engine::general_purpose::STANDARD, Engine};
use ftw_ipc::{
    crm::{self, Source},
    mail::{self, AgentCommand, Command, DraftInput, Result as Reply},
    Response,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteRow, FromRow, Row, SqliteConnection, SqlitePool};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use uuid::Uuid;

type Result<T> = std::result::Result<T, Failure>;

#[derive(Debug)]
enum Failure {
    Code(&'static str),
    Database(sqlx::Error),
}

impl From<sqlx::Error> for Failure {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl Failure {
    fn code(&self) -> &'static str {
        match self {
            Self::Code(code) => code,
            Self::Database(error) => {
                // Only the database category is exposed across IPC.
                let _ = error;
                "database_unavailable"
            }
        }
    }
}

#[derive(FromRow)]
struct AccountRow {
    email_address: String,
    smtp_host: String,
    smtp_port: i64,
    smtp_security: String,
    smtp_username: String,
    protected: Vec<u8>,
    daily_limit: i64,
}

#[derive(serde::Serialize)]
struct Snapshot {
    contact_id: String,
    account_id: String,
    sender: String,
    recipient: String,
    subject: String,
    body: String,
    attachments: Vec<mail::Attachment>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    assets: Vec<mail::Asset>,
}

impl Snapshot {
    fn digest(&self) -> Result<String> {
        let encoded =
            serde_json::to_vec(self).map_err(|_| Failure::Code("smtp_message_invalid"))?;
        Ok(format!("{:x}", Sha256::digest(encoded)))
    }
}

#[derive(Clone)]
pub struct MailService {
    pool: SqlitePool,
    ledger: Option<ledger::Ledger>,
    object_root: Option<std::path::PathBuf>,
    prompt: Arc<Mutex<()>>,
    imap: Arc<Mutex<()>>,
    delivery: Arc<Mutex<()>>,
}

impl MailService {
    pub async fn new(pool: SqlitePool) -> std::result::Result<Self, sqlx::Error> {
        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE mail_agent_reviews SET state='failed',outcome='approval_interrupted' WHERE state='waiting'")
            .execute(&mut *tx).await?;
        sqlx::query("UPDATE email_approval_attempts SET state='interrupted',outcome='approval_interrupted',finished_at=unixepoch() WHERE state='waiting'").execute(&mut *tx).await?;
        sqlx::query("UPDATE email_drafts SET state='delivery_unknown', updated_at=unixepoch() WHERE state='sending'")
            .execute(&mut *tx).await?;
        sqlx::query("UPDATE email_outbox_jobs SET state='delivery_unknown', outcome='interrupted_during_submission' WHERE state='sending'")
            .execute(&mut *tx).await?;
        sqlx::query("UPDATE inbox_messages SET download_outcome='approval_interrupted' WHERE download_outcome='running'")
            .execute(&mut *tx).await?;
        sqlx::query("UPDATE imap_sync_state SET outcome='interrupted' WHERE outcome='running'")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Self {
            pool,
            ledger: None,
            object_root: None,
            prompt: Arc::new(Mutex::new(())),
            imap: Arc::new(Mutex::new(())),
            delivery: Arc::new(Mutex::new(())),
        })
    }

    pub async fn with_send_ledger(mut self, data: &std::path::Path) -> Self {
        self.object_root = Some(data.join("mail-objects"));
        let path = data.join("agent-send-safety.sqlite");
        let required = sqlx::query_scalar::<_, bool>(
            "SELECT ledger_required FROM mail_agent_policy WHERE id=1",
        )
        .fetch_one(&self.pool)
        .await;
        if matches!(required, Ok(false)) || path.is_file() {
            self.ledger = ledger::Ledger::open(&path).await.ok();
        }
        self
    }

    pub async fn restore(
        &self,
        root: &std::path::Path,
        filename: &str,
    ) -> std::result::Result<String, &'static str> {
        let _approval = self.prompt.try_lock().map_err(|_| "backup_mail_busy")?;
        let _sync = self.imap.try_lock().map_err(|_| "backup_mail_busy")?;
        let _delivery = self.delivery.try_lock().map_err(|_| "backup_mail_busy")?;
        crate::storage::backup::restore(&self.pool, root, filename)
            .await
            .map_err(|_| "backup_restore_failed")
    }

    pub fn approval_in_progress(&self) -> bool {
        self.prompt.try_lock().is_err()
    }

    pub fn start_worker(&self) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(450));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let Ok(_delivery) = service.delivery.try_lock() else {
                    continue;
                };
                // A failed final write leaves a durable 'sending' row for recovery on restart.
                let _ = service.send_next().await;
            }
        })
    }

    pub async fn desktop(&self, command: Command, owner: isize) -> Response {
        response(self.desktop_call(command, owner).await)
    }

    pub async fn agent(&self, command: AgentCommand) -> Response {
        let result = match command {
            AgentCommand::ListCustomers { query } => {
                return crate::crm::dispatch(
                    &self.pool,
                    crm::Command::ListContacts { query: *query },
                    agent_actor(),
                )
                .await
            }
            AgentCommand::GetCustomer { id } => {
                return crate::crm::dispatch(
                    &self.pool,
                    crm::Command::GetContact { id },
                    agent_actor(),
                )
                .await
            }
            AgentCommand::ListAssets { token } => self.list_agent_assets(&token.0).await,
            AgentCommand::AttachDraftAssets {
                token,
                id,
                revision,
                asset_ids,
            } => {
                self.attach_agent_assets(&token.0, &id, revision, &asset_ids)
                    .await
            }
            AgentCommand::CheckOutreach { recipient } => self.outreach_check(&recipient).await,
            AgentCommand::ListAccounts => self.accounts(false).await,
            AgentCommand::ListDrafts { contact_id, offset } => {
                self.drafts(contact_id, offset).await
            }
            AgentCommand::GetDraft { id } => self.get(&id).await,
            AgentCommand::ListPending { offset } => self.pending(offset).await,
            AgentCommand::CreateDraft {
                request_id,
                contact_id,
                account_id,
                recipient,
                subject,
                body,
            } => {
                let input = DraftInput {
                    contact_id,
                    account_id,
                    recipient,
                    subject,
                    body,
                    attachments: Vec::new(),
                    replace_attachments: false,
                };
                if let Some(request) = request_id {
                    if let Err(reply) =
                        crate::agent::claim(&self.pool, "create_email_draft", &request, &input)
                            .await
                    {
                        return reply;
                    }
                    let result = self.save(None, input, Source::Agent).await;
                    return crate::agent::finish(
                        &self.pool,
                        "create_email_draft",
                        &request,
                        response(result),
                    )
                    .await;
                }
                self.save(None, input, Source::Agent).await
            }
            AgentCommand::SendAuthorized {
                token,
                id,
                revision,
                purpose,
                task_id,
                request_id,
            } => match policy::digest(&token.0) {
                Ok(digest) => {
                    self.enqueue_authorized(authorized::Submission {
                        digest,
                        id,
                        revision,
                        purpose,
                        task: task_id,
                        request: request_id,
                    })
                    .await
                }
                Err(error) => Err(error),
            },
            AgentCommand::RequestApproval { id } => {
                if let Err(error) = self.agent_manual_allowed().await {
                    return response(Err(error));
                }
                self.request_approval(&id, Source::Agent).await
            }
            AgentCommand::SendApproved { id } => {
                if let Err(error) = self.agent_manual_allowed().await {
                    return response(Err(error));
                }
                self.enqueue(&id, Source::Agent).await
            }
        };
        response(result)
    }

    async fn agent_manual_allowed(&self) -> Result<()> {
        let mode: String = sqlx::query_scalar("SELECT mode FROM mail_agent_policy WHERE id=1")
            .fetch_one(&self.pool)
            .await?;
        if mode == "disabled" {
            Err(Failure::Code("agent_send_disabled"))
        } else {
            Ok(())
        }
    }

    async fn desktop_call(&self, command: Command, owner: isize) -> Result<Reply> {
        match command {
            Command::ImportAssets { paths } => self.import_assets(paths).await,
            Command::ListAssets => self.list_assets(false).await,
            Command::SaveDraftWithAssets {
                id,
                input,
                asset_ids,
            } => {
                self.save_with_assets(id, input, Source::Manual, Some(asset_ids))
                    .await
            }
            Command::SetAssetAccess { id, allowed } => {
                if allowed {
                    self.authorize_asset(id, owner).await
                } else {
                    self.disable_asset_access(id).await
                }
            }
            Command::AgentMailSettings => self.policy_settings().await,
            Command::SetAgentMailPolicy { policy, agent_ids } => {
                self.change_policy(policy, agent_ids, owner).await
            }
            Command::CancelMailPolicyReview { id } => self.cancel_policy_review(id).await,
            Command::RegisterMailAgent { name, token } => self.register_agent(name, &token.0).await,
            Command::RevokeMailAgent { id } => self.revoke_mail_agent(id).await,
            Command::ListAccounts => self.accounts(true).await,
            Command::SaveAccount {
                name,
                email,
                host,
                port,
                security,
                username,
                password,
            } => {
                check_text(&name, 180)?;
                check_text(&username, 320)?;
                let email = email_address(&email)?;
                let host = hostname(&host)?;
                if port == 0 || !matches!(security.as_str(), "tls" | "starttls") {
                    return Err(Failure::Code("smtp_configuration_invalid"));
                }
                let protected = protect_password(&password.0)?;
                let id = new_id();
                let mut tx = self.pool.begin().await?;
                sqlx::query("INSERT INTO local_secrets(id, protected) VALUES (?,?)")
                    .bind(&id)
                    .bind(protected)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("INSERT INTO email_accounts(id,display_name,email_address,smtp_host,smtp_port,smtp_security,smtp_username,secret_ref) VALUES (?,?,?,?,?,?,?,?)")
                    .bind(&id).bind(name.trim()).bind(email).bind(host).bind(i64::from(port)).bind(security)
                    .bind(username.trim()).bind(&id).execute(&mut *tx).await?;
                audit(&mut tx, Source::Manual, "email_account", &id, "create").await?;
                tx.commit().await?;
                Ok(Reply::Saved { id })
            }
            Command::ReplacePassword { id, password } => {
                let protected = protect_password(&password.0)?;
                let mut tx = self.pool.begin().await?;
                let change = sqlx::query("UPDATE local_secrets SET protected=? WHERE id=(SELECT secret_ref FROM email_accounts WHERE id=?)")
                    .bind(protected).bind(&id).execute(&mut *tx).await?;
                if change.rows_affected() != 1 {
                    return Err(Failure::Code("mail_not_found"));
                }
                audit(
                    &mut tx,
                    Source::Manual,
                    "email_account",
                    &id,
                    "replace_password",
                )
                .await?;
                tx.commit().await?;
                Ok(Reply::Saved { id })
            }
            Command::SetAccountPreferences {
                id,
                daily_limit,
                signature,
            } => {
                if !(1..=500).contains(&daily_limit)
                    || signature.len() > 2_000
                    || signature.contains('\0')
                {
                    return Err(Failure::Code("invalid_account_preferences"));
                }
                let mut tx = self.pool.begin().await?;
                let result =
                    sqlx::query("UPDATE email_accounts SET daily_limit=?,signature=? WHERE id=?")
                        .bind(i64::from(daily_limit))
                        .bind(signature.trim())
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
                    "set_preferences",
                )
                .await?;
                tx.commit().await?;
                Ok(Reply::Saved { id })
            }
            Command::TestAccount { id } => {
                let mut db = self.pool.acquire().await?;
                let account = account(&mut db, &id).await?;
                drop(db);
                let transport = smtp::transport(&account)?;
                match tokio::time::timeout(Duration::from_secs(7), transport.test_connection())
                    .await
                {
                    Ok(Ok(true)) => Ok(Reply::ConnectionVerified),
                    _ => Err(Failure::Code("smtp_connection_failed")),
                }
            }
            Command::ListDrafts { contact_id, offset } => self.drafts(contact_id, offset).await,
            Command::GetDraft { id } => self.get(&id).await,
            Command::SaveDraft { id, input } => self.save(id, input, Source::Manual).await,
            Command::RequestApproval { id } => self.request_approval(&id, Source::Manual).await,
            Command::OpenApproval { id } => self.open_approval(id).await,
            Command::SendApproved { id } => self.enqueue(&id, Source::Manual).await,
            Command::SetSuppression { target, blocked } => {
                let normalized = if target.contains('@') {
                    email_address(&target)?.to_ascii_lowercase()
                } else {
                    hostname(&target)?
                };
                if !blocked {
                    return self.unblock_address(normalized, owner).await;
                }
                let mut tx = self.pool.begin().await?;
                sqlx::query("INSERT OR IGNORE INTO suppression_list(normalized) VALUES (?)")
                    .bind(&normalized)
                    .execute(&mut *tx)
                    .await?;
                audit(&mut tx, Source::Manual, "suppression", &normalized, "block").await?;
                tx.commit().await?;
                Ok(Reply::Saved { id: normalized })
            }
        }
    }

    async fn accounts(&self, desktop: bool) -> Result<Reply> {
        let rows = sqlx::query("SELECT id,display_name,email_address,smtp_host,smtp_port,smtp_security,daily_limit,signature FROM email_accounts ORDER BY display_name,id")
            .map(|r: SqliteRow| mail::Account {
                id: r.get("id"), name: r.get("display_name"), email: r.get("email_address"),
                host: desktop.then(|| r.get("smtp_host")),
                port: desktop.then(|| r.get::<i64,_>("smtp_port") as u16),
                security: desktop.then(|| r.get("smtp_security")),
                daily_limit: desktop.then(|| r.get::<i64,_>("daily_limit") as u16),
                signature: desktop.then(|| r.get("signature")),
            }).fetch_all(&self.pool).await?;
        Ok(Reply::Accounts { rows })
    }

    async fn drafts(&self, contact: Option<String>, offset: u32) -> Result<Reply> {
        let mut rows = sqlx::query("SELECT * FROM mail_drafts_review WHERE (? IS NULL OR contact_id=? OR contact_id IN (SELECT id FROM contacts WHERE merged_into=?)) ORDER BY updated_at DESC,rowid DESC LIMIT 21 OFFSET ?")
            .bind(contact.as_deref()).bind(contact.as_deref()).bind(contact.as_deref()).bind(i64::from(offset)).map(draft_row)
            .fetch_all(&self.pool).await?;
        let has_more = rows.len() > 20;
        rows.truncate(20);
        Ok(Reply::Drafts { rows, has_more })
    }

    async fn pending(&self, offset: u32) -> Result<Reply> {
        let mut rows = sqlx::query(
            "SELECT * FROM mail_drafts_review WHERE state='pending_approval' OR approval_expired
            ORDER BY updated_at DESC,rowid DESC LIMIT 21 OFFSET ?",
        )
        .bind(i64::from(offset))
        .map(draft_row)
        .fetch_all(&self.pool)
        .await?;
        let has_more = rows.len() > 20;
        rows.truncate(20);
        Ok(Reply::Drafts { rows, has_more })
    }

    async fn get(&self, id: &str) -> Result<Reply> {
        let row = sqlx::query(
            "SELECT d.*,a.email_address,(SELECT expires_at FROM email_approvals WHERE draft_id=d.id AND revision=d.revision) expires_at,
             (SELECT outcome FROM email_outbox_jobs WHERE draft_id=d.id AND revision=d.revision) outcome,
             (SELECT 'sent:' || e.id FROM email_messages e JOIN email_outbox_jobs o ON o.id=e.outbox_id WHERE o.draft_id=d.id) thread_id,
             EXISTS(SELECT 1 FROM inbox_messages i JOIN email_messages e ON i.thread_id='sent:' || e.id JOIN email_outbox_jobs o ON o.id=e.outbox_id WHERE o.draft_id=d.id AND i.message_role='correspondence' AND i.match_state IN ('thread','manual')) has_reply
             FROM mail_drafts_review d JOIN email_accounts a ON d.account_id=a.id WHERE d.id=?",
        ).bind(id).fetch_optional(&self.pool).await?.ok_or(Failure::Code("mail_not_found"))?;
        let mut attachments = attachments(&row)?
            .iter()
            .map(attachment_info)
            .collect::<Result<Vec<_>>>()?;
        let ids = attachments::ids(&row)?;
        let mut db = self.pool.acquire().await?;
        attachments.extend(
            attachments::select(&mut db, &ids, false)
                .await?
                .iter()
                .map(asset_info),
        );
        drop(db);
        let approval_attempt = sqlx::query("SELECT id,revision,state,outcome FROM email_approval_attempts WHERE draft_id=? ORDER BY rowid DESC LIMIT 1")
            .bind(id).map(|r: SqliteRow| mail::ApprovalAttempt { id:r.get("id"), revision:r.get("revision"), state:r.get("state"), outcome:r.get("outcome") })
            .fetch_optional(&self.pool).await?;
        let approval_attempt = approval_attempt.map(|mut attempt| {
            if attempt.state == "waiting" && !self.approval_in_progress() {
                attempt.state = "interrupted".into();
                attempt.outcome = Some("approval_interrupted".into());
            }
            attempt
        });
        let record = mail::Draft {
            approval_attempt,
            sender: row.get("email_address"),
            body: row.get("body_text"),
            attachments,
            expires_at: row.get("expires_at"),
            thread_id: row.get("thread_id"),
            has_reply: row.get("has_reply"),
            outcome: row.get("outcome"),
            row: draft_row(row),
        };
        Ok(Reply::Draft {
            record: Box::new(record),
        })
    }

    async fn save(&self, id: Option<String>, input: DraftInput, source: Source) -> Result<Reply> {
        self.save_with_assets(id, input, source, None).await
    }

    async fn save_with_assets(
        &self,
        id: Option<String>,
        mut input: DraftInput,
        source: Source,
        asset_ids: Option<Vec<String>>,
    ) -> Result<Reply> {
        input.recipient = email_address(&input.recipient)?;
        check_text(&input.subject, 360)?;
        if input.body.trim().is_empty() || input.body.len() > 12_000 || input.body.contains('\0') {
            return Err(Failure::Code("invalid_mail_body"));
        }
        validate_attachments(&input.attachments)?;
        let updating = id.is_some();
        let id = id.unwrap_or_else(new_id);
        let mut tx = self.pool.begin().await?;
        check_recipient(&mut tx, &input.contact_id, &input.recipient).await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM email_accounts WHERE id=?)")
                .bind(&input.account_id)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(Failure::Code("mail_not_found"));
        }
        let replace =
            asset_ids.is_some() || input.replace_attachments || !input.attachments.is_empty();
        let ids = asset_ids.unwrap_or_default();
        if !ids.is_empty() && !input.attachments.is_empty() {
            return Err(Failure::Code("invalid_attachments"));
        }
        let assets = attachments::select(&mut tx, &ids, false).await?;
        for asset in &assets {
            attachments::read_asset(self.objects()?, asset).await?;
        }
        let encoded = json!(input.attachments).to_string();
        let encoded_ids = json!(ids).to_string();
        let affected = if updating {
            sqlx::query("UPDATE email_drafts SET contact_id=?,account_id=?,recipient=?,subject=?,body_text=?,attachments_json=CASE WHEN ? THEN ? ELSE attachments_json END,
                asset_ids_json=CASE WHEN ? THEN ? ELSE asset_ids_json END,
                revision=revision+1,state='draft',updated_at=unixepoch() WHERE id=? AND state IN ('draft','pending_approval','approved')")
                .bind(input.contact_id).bind(input.account_id).bind(input.recipient).bind(input.subject.trim()).bind(input.body)
                .bind(replace).bind(encoded).bind(replace).bind(encoded_ids).bind(&id).execute(&mut *tx).await?.rows_affected()
        } else {
            sqlx::query("INSERT INTO email_drafts(id,contact_id,account_id,recipient,subject,body_text,attachments_json,source,asset_ids_json) VALUES (?,?,?,?,?,?,?,?,?)")
                .bind(&id).bind(input.contact_id).bind(input.account_id).bind(input.recipient).bind(input.subject.trim()).bind(input.body)
                .bind(encoded).bind(source.as_str()).bind(encoded_ids).execute(&mut *tx).await?.rows_affected()
        };
        if affected != 1 {
            return Err(Failure::Code("mail_read_only"));
        }
        audit(
            &mut tx,
            source,
            "email_draft",
            &id,
            if updating { "edit" } else { "create" },
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved { id })
    }

    async fn request_approval(&self, id: &str, source: Source) -> Result<Reply> {
        let mut tx = self.pool.begin().await?;
        let state: String = sqlx::query_scalar("SELECT state FROM email_drafts WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Failure::Code("mail_not_found"))?;
        match state.as_str() {
            "draft" => {
                sqlx::query("UPDATE email_drafts SET state='pending_approval',updated_at=unixepoch() WHERE id=?")
                    .bind(id).execute(&mut *tx).await?;
                audit(&mut tx, source, "email_draft", id, "request_approval").await?;
            }
            "pending_approval" | "approved" => {}
            _ => return Err(Failure::Code("mail_read_only")),
        }
        tx.commit().await?;
        Ok(Reply::Saved { id: id.to_owned() })
    }
}

fn agent_actor() -> crate::crm::Actor<'static> {
    crate::crm::Actor {
        source: Source::Agent,
        id: "mcp",
        agent_run_id: None,
    }
}

fn response(result: Result<Reply>) -> Response {
    match result {
        Ok(data) => Response::Mail {
            data: Box::new(data),
        },
        Err(error) => Response::Error {
            code: error.code().into(),
            detail: None,
        },
    }
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

async fn audit(
    db: &mut SqliteConnection,
    source: Source,
    entity: &str,
    id: &str,
    action: &str,
) -> Result<()> {
    let actor = match source {
        Source::Manual => "desktop-user",
        Source::Agent => "mcp",
    };
    sqlx::query("INSERT INTO audit_logs(id,actor_id,source,entity,entity_id,operation) VALUES (?,?,?,?,?,?)")
        .bind(new_id()).bind(actor).bind(source.as_str()).bind(entity).bind(id).bind(action)
        .execute(db).await?;
    Ok(())
}

fn check_text(value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(Failure::Code("invalid_mail_field"));
    }
    Ok(())
}

fn email_address(value: &str) -> Result<String> {
    check_text(value, 254)?;
    let address: lettre::Address = value
        .trim()
        .parse()
        .map_err(|_| Failure::Code("invalid_mail_address"))?;
    Ok(address.to_string())
}

fn hostname(value: &str) -> Result<String> {
    check_text(value, 253)?;
    match url::Host::parse(value.trim()) {
        Ok(url::Host::Domain(host))
            if host
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.')
                && host.contains('.') =>
        {
            Ok(host.trim_end_matches('.').to_ascii_lowercase())
        }
        _ => Err(Failure::Code("invalid_smtp_host_or_domain")),
    }
}

fn protect_password(password: &str) -> Result<Vec<u8>> {
    if password.is_empty() || password.len() > 4096 || password.contains('\0') {
        return Err(Failure::Code("invalid_mail_password"));
    }
    credentials::protect(password).map_err(|_| Failure::Code("credential_unavailable"))
}

fn draft_row(row: SqliteRow) -> mail::DraftRow {
    mail::DraftRow {
        approval_expired: row.get("approval_expired"),
        purpose: row.get("purpose"),
        job_id: row.get("job_id"),
        task_id: row.get("task_id"),
        agent_name: row.get("agent_name"),
        id: row.get("id"),
        contact_id: row.get("contact_id"),
        account_id: row.get("account_id"),
        recipient: row.get("recipient"),
        subject: row.get("subject"),
        state: row.get("state"),
        revision: row.get("revision"),
        source: row.get("source"),
        updated_at: row.get("updated_at"),
    }
}

fn attachments(row: &SqliteRow) -> Result<Vec<mail::Attachment>> {
    serde_json::from_str(row.get("attachments_json"))
        .map_err(|_| Failure::Code("invalid_attachments"))
}

fn attachment_bytes(attachment: &mail::Attachment) -> Result<Vec<u8>> {
    STANDARD
        .decode(&attachment.content_base64)
        .map_err(|_| Failure::Code("invalid_attachments"))
}

fn attachment_info(attachment: &mail::Attachment) -> Result<mail::AttachmentInfo> {
    let bytes = attachment_bytes(attachment)?;
    Ok(mail::AttachmentInfo {
        asset_id: None,
        filename: attachment.filename.clone(),
        size: bytes.len(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
    })
}

fn asset_info(asset: &mail::Asset) -> mail::AttachmentInfo {
    mail::AttachmentInfo {
        asset_id: Some(asset.id.clone()),
        filename: asset.filename.clone(),
        size: asset.size as usize,
        sha256: asset.sha256.clone(),
    }
}

fn validate_attachments(attachments: &[mail::Attachment]) -> Result<()> {
    let mut total = 0;
    if attachments.len() > 4 {
        return Err(Failure::Code("attachment_limit"));
    }
    for attachment in attachments {
        check_text(&attachment.filename, 180)?;
        if attachment.filename.contains(['/', '\\']) {
            return Err(Failure::Code("invalid_attachments"));
        }
        total += attachment_bytes(attachment)?.len();
        if total > 384 * 1024 {
            return Err(Failure::Code("attachment_limit"));
        }
    }
    Ok(())
}

async fn snapshot(db: &mut SqliteConnection, id: &str) -> Result<(Snapshot, i64, String)> {
    let row = sqlx::query("SELECT d.*,a.email_address FROM email_drafts d JOIN email_accounts a ON d.account_id=a.id WHERE d.id=?")
        .bind(id).fetch_optional(&mut *db).await?.ok_or(Failure::Code("mail_not_found"))?;
    Ok((
        Snapshot {
            contact_id: row.get("contact_id"),
            account_id: row.get("account_id"),
            sender: row.get("email_address"),
            recipient: row.get("recipient"),
            subject: row.get("subject"),
            body: row.get("body_text"),
            attachments: attachments(&row)?,
            assets: attachments::select(db, &attachments::ids(&row)?, false).await?,
        },
        row.get("revision"),
        row.get("state"),
    ))
}

async fn check_recipient(db: &mut SqliteConnection, contact: &str, recipient: &str) -> Result<()> {
    let normalized = recipient.to_ascii_lowercase();
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
        SELECT 1 FROM contacts c JOIN contact_methods m ON m.contact_id=c.id
        WHERE c.id=? AND c.archived_at IS NULL AND m.kind='email' AND m.normalized=?
    )",
    )
    .bind(contact)
    .bind(&normalized)
    .fetch_one(&mut *db)
    .await?;
    if !exists {
        return Err(Failure::Code("recipient_not_on_contact"));
    }
    Ok(())
}

async fn check_suppression(db: &mut SqliteConnection, recipient: &str) -> Result<()> {
    let normalized = recipient.to_ascii_lowercase();
    let domain = normalized
        .split_once('@')
        .map(|(_, domain)| domain)
        .ok_or(Failure::Code("invalid_mail_address"))?;
    let suppressed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM suppression_list WHERE normalized IN (?,?))",
    )
    .bind(&normalized)
    .bind(domain)
    .fetch_one(db)
    .await?;
    if suppressed {
        return Err(Failure::Code("recipient_suppressed"));
    }
    Ok(())
}

async fn account(db: &mut SqliteConnection, id: &str) -> Result<AccountRow> {
    sqlx::query_as(
        "SELECT a.email_address,a.smtp_host,a.smtp_port,a.smtp_security,a.smtp_username,s.protected,a.daily_limit
        FROM email_accounts a JOIN local_secrets s ON s.id=a.secret_ref WHERE a.id=?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(Failure::Code("mail_not_found"))
}
