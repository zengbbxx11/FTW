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
    Policy(AgentMailPolicy),
    Register { name: String, digest: Vec<u8> },
    Revoke { id: String },
    Unblock { target: String },
}

impl MailService {
    pub(super) async fn policy_settings(&self) -> Result<Reply> {
        let mut connection = self.pool.acquire().await?;
        let policy = current(&mut connection).await?;
        let agents = sqlx::query("SELECT id,name,revoked FROM mail_agent_clients ORDER BY name,id")
            .map(|row: sqlx::sqlite::SqliteRow| RegisteredAgent {
                id: row.get("id"),
                name: row.get("name"),
                revoked: row.get("revoked"),
            })
            .fetch_all(&mut *connection)
            .await?;
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

    pub(super) async fn start_policy_change(&self, change: Change) -> Result<Reply> {
        let guard = self
            .prompt
            .clone()
            .try_lock_owned()
            .map_err(|_| Failure::Code("approval_already_open"))?;
        let mut db = self.pool.acquire().await?;
        let summary = match &change {
            Change::Policy(policy) => {
                validate(&mut db, policy).await?;
                if policy.mode == "automatic" && self.ledger.is_none() {
                    return Err(Failure::Code("agent_send_history_unavailable"));
                }
                let mode = match policy.mode.as_str() {
                    "automatic" => "持续授权自动发送",
                    "manual" => "逐封人工审批",
                    _ => "禁止 Agent 发送",
                };
                let mut accounts = Vec::new();
                for id in &policy.accounts {
                    let address: String =
                        sqlx::query_scalar("SELECT email_address FROM email_accounts WHERE id=?")
                            .bind(id)
                            .fetch_one(&mut *db)
                            .await?;
                    accounts.push(address);
                }
                format!("设置 Agent 发信方式为 {mode}。发件邮箱 {}。每任务 {} 封，每 24 小时 {} 封，间隔 {} 秒。",
                    accounts.join("、"), policy.task_limit, policy.daily_limit, policy.interval_seconds)
            }
            Change::Register { name, .. } => {
                check_text(name, 80)?;
                format!("登记 FTW 邮件 Agent「{name}」。它可以使用工作台已授予的发送权限。")
            }
            Change::Revoke { id } => {
                let name: String = sqlx::query_scalar(
                    "SELECT name FROM mail_agent_clients WHERE id=? AND NOT revoked",
                )
                .bind(id)
                .fetch_optional(&mut *db)
                .await?
                .ok_or(Failure::Code("mail_not_found"))?;
                format!("撤销 Agent「{name}」的邮件自动发送身份。")
            }
            Change::Unblock { target } => format!("将「{target}」移出邮件拒发名单。"),
        };
        let id = new_id();
        sqlx::query("INSERT INTO mail_agent_reviews(id,state) VALUES (?,'waiting')")
            .bind(&id)
            .execute(&mut *db)
            .await?;
        let service = self.clone();
        let review = id.clone();
        tokio::spawn(async move {
            let _guard = guard;
            let verified = tokio::task::spawn_blocking(move || user_verification::verify(&summary))
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
        let (id, action) = match change {
            Change::Policy(policy) => {
                validate(&mut tx, &policy).await?;
                sqlx::query("UPDATE mail_agent_policy SET mode=?,ledger_required=ledger_required OR ?='automatic',revision=revision+1,accounts_json=?,purposes_json=?,daily_limit=?,task_limit=?,interval_seconds=? WHERE id=1")
                    .bind(&policy.mode).bind(&policy.mode).bind(encode(&policy.accounts)?).bind(encode(&policy.purposes)?)
                    .bind(policy.daily_limit).bind(policy.task_limit).bind(policy.interval_seconds).execute(&mut *tx).await?;
                ("profile".into(), "set_policy")
            }
            Change::Register { name, digest } => {
                let id = new_id();
                sqlx::query("INSERT INTO mail_agent_clients(id,name,token_digest) VALUES (?,?,?)")
                    .bind(&id)
                    .bind(name.trim())
                    .bind(digest)
                    .execute(&mut *tx)
                    .await?;
                (id, "register_agent")
            }
            Change::Revoke { id } => {
                sqlx::query("UPDATE mail_agent_clients SET revoked=1 WHERE id=?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                (id, "revoke_agent")
            }
            Change::Unblock { target } => {
                sqlx::query("DELETE FROM suppression_list WHERE normalized=?")
                    .bind(&target)
                    .execute(&mut *tx)
                    .await?;
                (target, "unblock")
            }
        };
        audit(&mut tx, Source::Manual, "mail_agent_settings", &id, action).await?;
        tx.commit().await?;
        Ok(())
    }
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

async fn validate(db: &mut SqliteConnection, policy: &AgentMailPolicy) -> Result<()> {
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM mail_agent_policy WHERE id=1")
        .fetch_one(&mut *db)
        .await?;
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
