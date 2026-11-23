use super::{Failure, Result};
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::path::Path;

#[derive(Clone)]
pub(super) struct Ledger(SqlitePool);

impl Ledger {
    pub(super) async fn open(path: &Path) -> sqlx::Result<Self> {
        let database = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .journal_mode(SqliteJournalMode::Wal)
                    .synchronous(SqliteSynchronous::Full),
            )
            .await?;
        sqlx::raw_sql(
            "CREATE TABLE IF NOT EXISTS sends (
                job_id TEXT PRIMARY KEY NOT NULL,
                recipient BLOB NOT NULL,
                sender BLOB NOT NULL,
                task TEXT NOT NULL,
                attempted_at INTEGER NOT NULL DEFAULT(unixepoch()),
                state TEXT NOT NULL DEFAULT 'uncertain' CHECK(state IN ('uncertain','accepted','rejected'))
            );
            CREATE INDEX IF NOT EXISTS sends_recipient ON sends(recipient) WHERE state<>'rejected';
            CREATE INDEX IF NOT EXISTS sends_sender ON sends(sender,attempted_at);",
        ).execute(&database).await?;
        Ok(Self(database))
    }

    pub(super) async fn has_active(&self, recipient: &str, first: bool) -> Result<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sends WHERE recipient=? AND (state='uncertain' OR (? AND state='accepted')))")
            .bind(address_key(recipient)).bind(first).fetch_one(&self.0).await?)
    }

    pub(super) async fn reserve(
        &self,
        job: &str,
        draft: &super::Snapshot,
        task: &str,
        purpose: &str,
        policy: &ftw_ipc::mail::AgentMailPolicy,
    ) -> Result<()> {
        let recipient = address_key(&draft.recipient);
        let sender = address_key(&draft.sender);
        let mut tx = self.0.begin_with("BEGIN IMMEDIATE").await?;
        if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sends WHERE recipient=? AND (state='uncertain' OR (? AND state='accepted')))")
            .bind(&recipient).bind(purpose == "first_outreach").fetch_one(&mut *tx).await?
        {
            return Err(Failure::Code("outreach_already_recorded"));
        }
        let task_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sends WHERE task=?")
            .bind(task)
            .fetch_one(&mut *tx)
            .await?;
        if task_count >= i64::from(policy.task_limit) {
            return Err(Failure::Code("agent_task_limit"));
        }
        let (attempts, recent): (i64, bool) = sqlx::query_as(
            "SELECT COUNT(*),COALESCE(MAX(attempted_at)>unixepoch()-?,0) FROM sends WHERE sender=? AND attempted_at>unixepoch()-86400",
        ).bind(policy.interval_seconds).bind(&sender).fetch_one(&mut *tx).await?;
        if attempts >= i64::from(policy.daily_limit) {
            return Err(Failure::Code("agent_daily_limit"));
        }
        if recent {
            return Err(Failure::Code("agent_rate_waiting"));
        }
        sqlx::query("INSERT INTO sends(job_id,recipient,sender,task) VALUES (?,?,?,?)")
            .bind(job)
            .bind(recipient)
            .bind(sender)
            .bind(task)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn settle(&self, job: &str, state: &str) -> Result<()> {
        sqlx::query("UPDATE sends SET state=? WHERE job_id=?")
            .bind(state)
            .bind(job)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}

fn address_key(address: &str) -> Vec<u8> {
    Sha256::digest(address.to_ascii_lowercase().as_bytes()).to_vec()
}
