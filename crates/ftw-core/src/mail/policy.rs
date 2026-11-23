use super::{audit, check_text, new_id, user_verification, Failure, MailService, Reply, Result};
use ftw_ipc::{
    crm::Source,
    mail::{AgentMailPolicy, PolicyReview, RegisteredAgent},
};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};

pub const PURPOSES: &[&str] = &[
    "first_outreach",
    "outreach_followup",
    "quote_communication",
    "sample_communication",
    "other_business",
];

pub(super) enum Change {
    Policy {
        policy: AgentMailPolicy,
        agents: Vec<String>,
    },
    Unblock {
        target: String,
    },
    AllowAsset {
        id: String,
    },
}

impl MailService {
    pub(super) async fn policy_settings(&self) -> Result<Reply> {
        let mut connection = self.pool.acquire().await?;
        expire_review(&mut connection).await?;
        let policy = current(&mut connection).await?;
        let agents = agents(&mut connection).await?;
        let review = sqlx::query(
            "SELECT id,state,outcome FROM mail_agent_reviews ORDER BY rowid DESC LIMIT 1",
        )
        .map(|row: sqlx::sqlite::SqliteRow| PolicyReview {
            id: row.get("id"),
            state: row.get("state"),
            outcome: row.get("outcome"),
        })
        .fetch_optional(&mut *connection)
        .await?;
        Ok(Reply::AgentMailSettings {
            policy,
            agents,
            review,
            automatic_available: self.ledger.is_some(),
        })
    }

    pub(super) async fn register_agent(&self, name: String, token: &str) -> Result<Reply> {
        check_text(name.trim(), 80)?;
        let digest = digest(token)?;
        let id = new_id();
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO mail_agent_clients(id,name,token_digest) VALUES (?,?,?)")
            .bind(&id)
            .bind(name.trim())
            .bind(digest)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            Source::Manual,
            "mail_agent_settings",
            &id,
            "register_agent",
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved { id })
    }

    pub(super) async fn revoke_mail_agent(&self, id: String) -> Result<Reply> {
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query(
            "UPDATE mail_agent_clients SET revoked=1,automatic=0 WHERE id=? AND NOT revoked",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() == 0 {
            return Err(Failure::Code("mail_not_found"));
        }
        audit(
            &mut tx,
            Source::Manual,
            "mail_agent_settings",
            &id,
            "revoke_agent",
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved { id })
    }

    pub(super) async fn cancel_policy_review(&self, id: String) -> Result<Reply> {
        sqlx::query("UPDATE mail_agent_reviews SET state='cancelled',outcome='agent_policy_cancelled' WHERE id=? AND state='waiting'")
            .bind(&id).execute(&self.pool).await?;
        Ok(Reply::Saved { id })
    }

    pub(super) async fn change_policy(
        &self,
        policy: AgentMailPolicy,
        selected: Option<Vec<String>>,
        owner: isize,
    ) -> Result<Reply> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let old = current(&mut tx).await?;
        validate(&mut tx, &policy, old.revision).await?;
        let registered = agents(&mut tx).await?;
        let current_grants: Vec<String> = registered
            .iter()
            .filter(|agent| agent.automatic && !agent.revoked)
            .map(|agent| agent.id.clone())
            .collect();
        let desired = if policy.mode == "automatic" {
            selected.unwrap_or_else(|| current_grants.clone())
        } else {
            current_grants.clone()
        };
        validate_grants(&policy, &desired, &registered)?;
        let requires_review = policy.mode == "automatic"
            && (old.mode != "automatic"
                || policy.daily_limit > old.daily_limit
                || policy.task_limit > old.task_limit
                || policy.interval_seconds < old.interval_seconds
                || policy
                    .accounts
                    .iter()
                    .any(|account| !old.accounts.contains(account))
                || policy
                    .purposes
                    .iter()
                    .any(|purpose| !old.purposes.contains(purpose))
                || desired.iter().any(|agent| !current_grants.contains(agent)));
        if requires_review {
            if self.ledger.is_none() {
                return Err(Failure::Code("agent_send_history_unavailable"));
            }
            let accounts: Vec<String> = sqlx::query_scalar("SELECT email_address FROM email_accounts WHERE id IN (SELECT value FROM json_each(?)) ORDER BY email_address")
                .bind(encode(&policy.accounts)?).fetch_all(&mut *tx).await?;
            let names: Vec<_> = registered
                .iter()
                .filter(|agent| desired.contains(&agent.id))
                .map(|agent| agent.name.as_str())
                .collect();
            let summary = format!(
                "授权 Agent 自动发信：{}。
发件邮箱：{}。
每任务 {} 封，每 24 小时 {} 封，间隔 {} 秒。
请使用 Windows 登录账户的密码核验。",
                names.join("、"),
                accounts.join("、"),
                policy.task_limit,
                policy.daily_limit,
                policy.interval_seconds
            );
            tx.rollback().await?;
            return self
                .start_policy_change(
                    Change::Policy {
                        policy,
                        agents: desired,
                    },
                    summary,
                    owner,
                )
                .await;
        }
        apply_policy(&mut tx, &policy, &desired).await?;
        tx.commit().await?;
        Ok(Reply::Saved {
            id: "profile".into(),
        })
    }

    pub(super) async fn authorize_asset(&self, id: String, owner: isize) -> Result<Reply> {
        let filename: String = sqlx::query_scalar("SELECT filename FROM mail_assets WHERE id=?")
            .bind(&id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Failure::Code("attachment_not_available"))?;
        self.start_policy_change(
            Change::AllowAsset { id },
            format!("允许已登记的 FTW Agent 使用附件「{filename}」。"),
            owner,
        )
        .await
    }

    pub(super) async fn unblock_address(&self, target: String, owner: isize) -> Result<Reply> {
        let summary = format!("将「{target}」移出邮件拒发名单。");
        self.start_policy_change(Change::Unblock { target }, summary, owner)
            .await
    }

    async fn start_policy_change(
        &self,
        change: Change,
        summary: String,
        owner: isize,
    ) -> Result<Reply> {
        let guard = self
            .prompt
            .clone()
            .try_lock_owned()
            .map_err(|_| Failure::Code("approval_already_open"))?;
        let id = new_id();
        sqlx::query("INSERT INTO mail_agent_reviews(id,state) VALUES (?,'waiting')")
            .bind(&id)
            .execute(&self.pool)
            .await?;
        let service = self.clone();
        let review = id.clone();
        tokio::spawn(async move {
            let _guard = guard;
            let verified =
                tokio::task::spawn_blocking(move || user_verification::verify(&summary, owner))
                    .await
                    .unwrap_or(Err("agent_policy_verification_failed"));
            let result = match verified {
                Ok(()) => service.commit_policy_change(&review, change).await,
                Err(reason) => Err(Failure::Code(reason)),
            };
            if let Err(error) = result {
                let state = if error.code() == "agent_policy_cancelled" {
                    "cancelled"
                } else {
                    "failed"
                };
                if sqlx::query("UPDATE mail_agent_reviews SET state=?,outcome=? WHERE id=? AND state='waiting'")
                    .bind(state).bind(error.code()).bind(&review).execute(&service.pool).await.is_err()
                {
                    eprintln!("FTW could not persist the mail policy review outcome");
                }
            }
        });
        Ok(Reply::Saved { id })
    }

    async fn commit_policy_change(&self, review: &str, change: Change) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let consumed = sqlx::query("UPDATE mail_agent_reviews SET state='saved' WHERE id=? AND state='waiting' AND changed_at>unixepoch()-300")
            .bind(review).execute(&mut *tx).await?;
        if consumed.rows_affected() != 1 {
            return Err(Failure::Code("agent_policy_review_expired"));
        }
        match change {
            Change::Policy {
                policy,
                agents: desired,
            } => {
                let old = current(&mut tx).await?;
                validate(&mut tx, &policy, old.revision).await?;
                validate_grants(&policy, &desired, &agents(&mut tx).await?)?;
                apply_policy(&mut tx, &policy, &desired).await?;
            }
            Change::AllowAsset { id } => {
                sqlx::query("UPDATE mail_assets SET agent_allowed=1 WHERE id=?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                audit(
                    &mut tx,
                    Source::Manual,
                    "mail_agent_settings",
                    &id,
                    "allow_agent_attachment",
                )
                .await?;
            }
            Change::Unblock { target } => {
                sqlx::query("DELETE FROM suppression_list WHERE normalized=?")
                    .bind(&target)
                    .execute(&mut *tx)
                    .await?;
                audit(
                    &mut tx,
                    Source::Manual,
                    "mail_agent_settings",
                    &target,
                    "unblock",
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn agents(db: &mut SqliteConnection) -> Result<Vec<RegisteredAgent>> {
    Ok(
        sqlx::query("SELECT id,name,revoked,automatic FROM mail_agent_clients ORDER BY name,id")
            .map(|row: sqlx::sqlite::SqliteRow| RegisteredAgent {
                id: row.get("id"),
                name: row.get("name"),
                revoked: row.get("revoked"),
                automatic: row.get("automatic"),
            })
            .fetch_all(db)
            .await?,
    )
}

async fn expire_review(db: &mut SqliteConnection) -> Result<()> {
    sqlx::query("UPDATE mail_agent_reviews SET state='failed',outcome='agent_policy_review_expired' WHERE state='waiting' AND changed_at<=unixepoch()-300")
        .execute(db).await?;
    Ok(())
}

fn validate_grants(
    policy: &AgentMailPolicy,
    desired: &[String],
    registered: &[RegisteredAgent],
) -> Result<()> {
    if policy.mode != "automatic" {
        return Ok(());
    }
    if desired.is_empty() {
        return Err(Failure::Code("mail_agent_selection_required"));
    }
    if desired.iter().any(|id| {
        !registered
            .iter()
            .any(|agent| &agent.id == id && !agent.revoked)
    }) {
        return Err(Failure::Code("mail_agent_unregistered"));
    }
    Ok(())
}

async fn apply_policy(
    db: &mut SqliteConnection,
    policy: &AgentMailPolicy,
    desired: &[String],
) -> Result<()> {
    sqlx::query("UPDATE mail_agent_policy SET mode=?,ledger_required=ledger_required OR ?='automatic',revision=revision+1,accounts_json=?,purposes_json=?,daily_limit=?,task_limit=?,interval_seconds=? WHERE id=1")
        .bind(&policy.mode).bind(&policy.mode).bind(encode(&policy.accounts)?).bind(encode(&policy.purposes)?)
        .bind(policy.daily_limit).bind(policy.task_limit).bind(policy.interval_seconds).execute(&mut *db).await?;
    if policy.mode == "automatic" {
        sqlx::query("UPDATE mail_agent_clients SET automatic=(id IN (SELECT value FROM json_each(?))) WHERE NOT revoked")
            .bind(encode(desired)?).execute(&mut *db).await?;
    }
    audit(
        db,
        Source::Manual,
        "mail_agent_settings",
        "profile",
        "set_policy",
    )
    .await
}

pub(super) fn digest(token: &str) -> Result<Vec<u8>> {
    if token.len() != 64 || !token.bytes().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(Failure::Code("mail_agent_unregistered"));
    }
    Ok(Sha256::digest(token.as_bytes()).to_vec())
}

pub(super) async fn current(db: &mut SqliteConnection) -> Result<AgentMailPolicy> {
    let row = sqlx::query("SELECT * FROM mail_agent_policy WHERE id=1")
        .fetch_one(db)
        .await?;
    Ok(AgentMailPolicy {
        revision: row.get("revision"),
        mode: row.get("mode"),
        accounts: decode(row.get("accounts_json"))?,
        purposes: decode(row.get("purposes_json"))?,
        daily_limit: row.get("daily_limit"),
        task_limit: row.get("task_limit"),
        interval_seconds: row.get("interval_seconds"),
    })
}

async fn validate(
    db: &mut SqliteConnection,
    policy: &AgentMailPolicy,
    revision: i64,
) -> Result<()> {
    if policy.revision != revision {
        return Err(Failure::Code("agent_policy_changed"));
    }
    if !matches!(policy.mode.as_str(), "automatic" | "manual" | "disabled")
        || !(1..=30).contains(&policy.daily_limit)
        || !(1..=20).contains(&policy.task_limit)
        || !(60..=3600).contains(&policy.interval_seconds)
        || policy.accounts.len() > 20
        || policy.purposes.len() > PURPOSES.len()
        || policy
            .purposes
            .iter()
            .any(|purpose| !PURPOSES.contains(&purpose.as_str()))
        || (policy.mode == "automatic"
            && (policy.accounts.is_empty() || policy.purposes.is_empty()))
    {
        return Err(Failure::Code("agent_policy_invalid"));
    }
    for id in &policy.accounts {
        if !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM email_accounts WHERE id=?)")
            .bind(id)
            .fetch_one(&mut *db)
            .await?
        {
            return Err(Failure::Code("agent_policy_invalid"));
        }
    }
    Ok(())
}

fn decode(data: &str) -> Result<Vec<String>> {
    serde_json::from_str(data).map_err(|_| Failure::Code("agent_policy_invalid"))
}

fn encode(data: &[String]) -> Result<String> {
    serde_json::to_string(data).map_err(|_| Failure::Code("agent_policy_invalid"))
}

pub(super) async fn registered(db: &mut SqliteConnection, digest: Vec<u8>) -> Result<String> {
    sqlx::query_scalar("SELECT id FROM mail_agent_clients WHERE token_digest=? AND NOT revoked")
        .bind(digest)
        .fetch_optional(db)
        .await?
        .ok_or(Failure::Code("mail_agent_unregistered"))
}
