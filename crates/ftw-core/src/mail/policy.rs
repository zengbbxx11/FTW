use super::{audit, check_text, new_id, Failure, MailService, Reply, Result};
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

#[derive(Clone, Copy)]
pub(super) enum AgentIdentity<'a> {
    Local,
    Token(&'a str),
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

    pub(super) async fn set_agent_paused(&self, id: String, paused: bool) -> Result<Reply> {
        let mut tx = self.pool.begin().await?;
        let changed =
            sqlx::query("UPDATE mail_agent_clients SET paused=? WHERE id=? AND NOT revoked")
                .bind(paused)
                .bind(&id)
                .execute(&mut *tx)
                .await?;
        if changed.rows_affected() == 0 {
            return Err(Failure::Code("mail_not_found"));
        }
        let operation = if paused {
            "pause_agent"
        } else {
            "resume_agent"
        };
        audit(
            &mut tx,
            Source::Manual,
            "mail_agent_settings",
            &id,
            operation,
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved { id })
    }

    pub(super) async fn revoke_mail_agent(&self, id: String) -> Result<Reply> {
        if id == ftw_ipc::agent::LOCAL_AGENT_ID {
            return Err(Failure::Code("local_agent_use_pause"));
        }
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
        if policy.mode == "automatic" && self.ledger.is_none() {
            return Err(Failure::Code("agent_send_history_unavailable"));
        }
        apply_policy(&mut tx, &policy, &desired).await?;
        tx.commit().await?;
        Ok(Reply::Saved {
            id: "profile".into(),
        })
    }

    pub(super) async fn authorize_asset(&self, id: String) -> Result<Reply> {
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE mail_assets SET agent_allowed=1 WHERE id=?")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        if changed.rows_affected() != 1 {
            return Err(Failure::Code("attachment_not_available"));
        }
        audit(
            &mut tx,
            Source::Manual,
            "mail_asset",
            &id,
            "allow_agent_access",
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved { id })
    }

    pub(super) async fn unblock_address(&self, target: String) -> Result<Reply> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM suppression_list WHERE normalized=?")
            .bind(&target)
            .execute(&mut *tx)
            .await?;
        audit(&mut tx, Source::Manual, "suppression", &target, "unblock").await?;
        tx.commit().await?;
        Ok(Reply::Saved { id: target })
    }
}

async fn agents(db: &mut SqliteConnection) -> Result<Vec<RegisteredAgent>> {
    Ok(
        sqlx::query("SELECT id,name,revoked,automatic,paused,last_seen_at FROM mail_agent_clients ORDER BY name,id")
            .try_map(|row: sqlx::sqlite::SqliteRow| Ok(RegisteredAgent {
                id: row.get("id"),
                name: row.get("name"),
                revoked: row.get("revoked"),
                automatic: row.get("automatic"),
                paused: row.get("paused"),
                last_seen_at: row.get("last_seen_at"),
            }))
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

pub(super) async fn remove_account(db: &mut SqliteConnection, id: &str) -> Result<()> {
    sqlx::query("UPDATE mail_agent_policy SET accounts_json=(SELECT json_group_array(value) FROM json_each(accounts_json) WHERE value<>?),revision=revision+1 WHERE id=1 AND EXISTS(SELECT 1 FROM json_each(accounts_json) WHERE value=?)")
        .bind(id).bind(id).execute(db).await?;
    Ok(())
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
        if !sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM email_accounts WHERE id=? AND archived_at IS NULL)",
        )
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
    let (id, paused, recently_seen): (String, bool, bool) = sqlx::query_as(
        "SELECT id,paused,coalesce(last_seen_at>unixepoch()-60,0)
         FROM mail_agent_clients WHERE token_digest=? AND NOT revoked",
    )
    .bind(digest)
    .fetch_optional(&mut *db)
    .await?
    .ok_or(Failure::Code("mail_agent_unregistered"))?;
    if paused {
        return Err(Failure::Code("mail_agent_paused"));
    }
    if !recently_seen {
        sqlx::query("UPDATE mail_agent_clients SET last_seen_at=unixepoch() WHERE id=?")
            .bind(&id)
            .execute(db)
            .await?;
    }
    Ok(id)
}

pub(super) async fn ensure_local_agent(
    db: &mut SqliteConnection,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("INSERT OR IGNORE INTO mail_agent_clients(id,name,token_digest) VALUES (?,?,randomblob(32))")
        .bind(ftw_ipc::agent::LOCAL_AGENT_ID)
        .bind("本机 Agent")
        .execute(db)
        .await?;
    Ok(())
}

pub(super) async fn resolve(
    db: &mut SqliteConnection,
    identity: AgentIdentity<'_>,
) -> Result<String> {
    match identity {
        AgentIdentity::Token(token) => registered(db, digest(token)?).await,
        AgentIdentity::Local => {
            ensure_local_agent(db).await?;
            let (paused, revoked, recently_seen): (bool, bool, bool) = sqlx::query_as(
                "SELECT paused,revoked,coalesce(last_seen_at>unixepoch()-60,0) FROM mail_agent_clients WHERE id=?",
            ).bind(ftw_ipc::agent::LOCAL_AGENT_ID).fetch_one(&mut *db).await?;
            if paused || revoked {
                return Err(Failure::Code("mail_agent_paused"));
            }
            if !recently_seen {
                sqlx::query("UPDATE mail_agent_clients SET last_seen_at=unixepoch() WHERE id=?")
                    .bind(ftw_ipc::agent::LOCAL_AGENT_ID)
                    .execute(&mut *db)
                    .await?;
            }
            Ok(ftw_ipc::agent::LOCAL_AGENT_ID.to_owned())
        }
    }
}
