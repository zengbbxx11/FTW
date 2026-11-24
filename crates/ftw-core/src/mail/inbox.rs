mod store;
pub(crate) use store::NEEDS_REVIEW;
#[cfg(test)]
mod tests;
mod transport;

use super::{audit, check_text, hostname, new_id, protect_password, Failure, MailService, Result};
use ftw_ipc::{
    crm::Source,
    inbox::{self, AgentCommand, Command, Result as Reply},
    Response,
};
use sqlx::{FromRow, Row};
use std::time::Duration;

const FOLDER: &str = "INBOX";

#[derive(FromRow)]
struct Settings {
    host: String,
    port: i64,
    username: String,
    protected: Vec<u8>,
}

impl MailService {
    pub async fn inbox(&self, command: Command) -> Response {
        let result = match command {
            Command::DownloadFull { id } => self.download_full(id).await,
            Command::ReadBody { id, offset } => self.read_inbox_body(&id, offset).await,
            Command::SaveAttachment {
                id,
                index,
                destination,
            } => self.save_inbox_attachment(&id, index, &destination).await,
            Command::Accounts => self.imap_accounts().await,
            Command::Configure {
                account_id,
                host,
                port,
                username,
                password,
            } => {
                self.configure_imap(account_id, host, port, username, password)
                    .await
            }
            Command::Test { account_id } => self.test_imap(&account_id).await,
            Command::Sync { account_id } => self.start_sync(account_id).await,
            Command::List {
                account_id,
                contact_id,
                unmatched,
                offset,
            } => store::list(&self.pool, account_id, contact_id, unmatched, offset).await,
            Command::ReviewCandidates { id } => store::review_candidates(&self.pool, &id).await,
            Command::ReviewLink {
                id,
                contact_id,
                expected_contact,
                expected_match,
            } => {
                store::review_link(
                    &self.pool,
                    &id,
                    &contact_id,
                    expected_contact,
                    expected_match,
                )
                .await
            }
            Command::Get { id } => store::get(&self.pool, &id).await,
            Command::Thread { id, offset } => store::thread(&self.pool, &id, offset).await,
            Command::Link { id, contact_id } => store::link(&self.pool, &id, &contact_id).await,
        };
        response(result)
    }

    pub async fn agent_inbox(&self, command: AgentCommand) -> Response {
        let result = match command {
            AgentCommand::DownloadFull { id } => self.download_full(id).await,
            AgentCommand::ReadBody { id, offset } => self.read_inbox_body(&id, offset).await,
            AgentCommand::Sync { account_id } => self.start_sync(account_id).await,
            AgentCommand::List { contact_id, offset } => {
                store::list(&self.pool, None, contact_id, false, offset).await
            }
            AgentCommand::Thread { id, offset } => store::thread(&self.pool, &id, offset).await,
        };
        response(result)
    }

    async fn configure_imap(
        &self,
        id: String,
        host: String,
        port: u16,
        username: String,
        password: ftw_ipc::mail::Secret,
    ) -> Result<Reply> {
        let _lock = self
            .imap
            .try_lock()
            .map_err(|_| Failure::Code("imap_busy"))?;
        check_text(&username, 320)?;
        let host = hostname(&host)?;
        if port == 0 {
            return Err(Failure::Code("imap_configuration_invalid"));
        }
        let protected = protect_password(&password.0)?;
        let mut tx = self.pool.begin().await?;
        let registered: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM email_accounts WHERE id=? AND archived_at IS NULL)",
        )
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
        if !registered {
            return Err(Failure::Code("mail_not_found"));
        }
        let old: Option<(String, String, String)> =
            sqlx::query_as("SELECT secret_ref,host,username FROM imap_accounts WHERE account_id=?")
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await?;
        let secret = old.as_ref().map_or_else(new_id, |row| row.0.clone());
        sqlx::query("INSERT INTO local_secrets(id,protected) VALUES (?,?) ON CONFLICT(id) DO UPDATE SET protected=excluded.protected")
            .bind(&secret).bind(protected).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO imap_accounts(account_id,host,port,username,secret_ref) VALUES (?,?,?,?,?)
          ON CONFLICT(account_id) DO UPDATE SET host=excluded.host,port=excluded.port,username=excluded.username,updated_at=unixepoch()")
            .bind(&id).bind(&host).bind(i64::from(port)).bind(username.trim()).bind(secret).execute(&mut *tx).await?;
        if old.is_some_and(|(_, previous_host, previous_username)| {
            previous_host != host || previous_username != username.trim()
        }) {
            sqlx::query("DELETE FROM imap_sync_state WHERE account_id=?")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM inbox_locations WHERE account_id=?")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
        }
        audit(
            &mut tx,
            Source::Manual,
            "email_account",
            &id,
            "configure_imap",
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved { id })
    }

    async fn imap_accounts(&self) -> Result<Reply> {
        let rows = sqlx::query("SELECT a.id,a.display_name,a.email_address,i.host,i.port,i.username,s.last_sync,s.outcome
          FROM email_accounts a LEFT JOIN imap_accounts i ON i.account_id=a.id
          LEFT JOIN imap_sync_state s ON s.account_id=a.id AND s.folder='INBOX' WHERE a.archived_at IS NULL ORDER BY a.display_name,a.id")
            .map(|r: sqlx::sqlite::SqliteRow| inbox::Account {
                account_id: r.get("id"), display_name: r.get("display_name"), email_address: r.get("email_address"),
                host: r.get("host"), port: r.get::<Option<i64>, _>("port").map(|v| v as u16),
                username: r.get("username"), last_sync: r.get("last_sync"), outcome: r.get("outcome"),
            }).fetch_all(&self.pool).await?;
        Ok(Reply::Accounts { rows })
    }

    async fn settings(&self, id: &str) -> Result<Settings> {
        sqlx::query_as("SELECT i.host,i.port,i.username,s.protected FROM imap_accounts i JOIN local_secrets s ON s.id=i.secret_ref JOIN email_accounts a ON a.id=i.account_id WHERE i.account_id=? AND a.archived_at IS NULL")
            .bind(id).fetch_optional(&self.pool).await?.ok_or(Failure::Code("imap_not_configured"))
    }

    async fn test_imap(&self, id: &str) -> Result<Reply> {
        let settings = self.settings(id).await?;
        let outcome = tokio::time::timeout(Duration::from_secs(7), async {
            let mut session = transport::connect(&settings).await?;
            session
                .examine(FOLDER)
                .await
                .map_err(|_| Failure::Code("imap_inbox_unavailable"))?;
            Ok::<_, Failure>(Reply::ConnectionVerified)
        })
        .await;
        outcome.unwrap_or(Err(Failure::Code("imap_timeout")))
    }

    async fn download_full(&self, id: String) -> Result<Reply> {
        let guard = self
            .imap
            .clone()
            .try_lock_owned()
            .map_err(|_| Failure::Code("imap_busy"))?;
        let available: bool =
            sqlx::query_scalar("SELECT body_complete FROM inbox_messages WHERE id=?")
                .bind(&id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(Failure::Code("mail_not_found"))?;
        if available {
            return Ok(Reply::Saved { id });
        }
        let (account, validity, uid): (String, u32, u32) = sqlx::query_as(
            "SELECT l.account_id,l.uid_validity,l.uid FROM inbox_locations l JOIN imap_sync_state s
                ON s.account_id=l.account_id AND s.folder=l.folder AND s.uid_validity=l.uid_validity
             WHERE l.message_id=? AND l.folder='INBOX' ORDER BY l.uid DESC LIMIT 1",
        )
        .bind(&id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Failure::Code("imap_uid_changed"))?;
        let settings = self.settings(&account).await?;
        let root = self.objects()?.to_owned();
        sqlx::query("UPDATE inbox_messages SET download_outcome='running' WHERE id=?")
            .bind(&id)
            .execute(&self.pool)
            .await?;
        let service = self.clone();
        let expected = id.clone();
        tokio::spawn(async move {
            let _guard = guard;
            let result = async {
                let raw = tokio::time::timeout(
                    Duration::from_secs(90),
                    transport::full_message(&settings, validity, uid),
                )
                .await
                .unwrap_or(Err(Failure::Code("imap_timeout")))?;
                store::complete(&service.pool, &root, &expected, &raw).await
            }
            .await;
            if let Err(error) = result {
                let _ = sqlx::query("UPDATE inbox_messages SET download_outcome=? WHERE id=?")
                    .bind(error.code())
                    .bind(expected)
                    .execute(&service.pool)
                    .await;
            }
        });
        Ok(Reply::Saved { id })
    }

    async fn read_inbox_body(&self, id: &str, offset: u32) -> Result<Reply> {
        let raw = store::full_bytes(&self.pool, self.objects()?, id).await?;
        store::body_page(&raw, offset)
    }

    async fn save_inbox_attachment(
        &self,
        id: &str,
        index: u16,
        destination: &str,
    ) -> Result<Reply> {
        use tokio::io::AsyncWriteExt;
        let path = std::path::Path::new(destination);
        if !path.is_absolute() {
            return Err(Failure::Code("invalid_attachments"));
        }
        let raw = store::full_bytes(&self.pool, self.objects()?, id).await?;
        let attachment = store::inbound_attachment(&raw, index)?;
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .await
            .map_err(|_| Failure::Code("attachment_destination_unavailable"))?;
        let result = async {
            file.write_all(&attachment).await?;
            file.sync_all().await
        }
        .await;
        drop(file);
        if result.is_err() {
            let _ = tokio::fs::remove_file(path).await;
            return Err(Failure::Code("attachment_destination_unavailable"));
        }
        Ok(Reply::Saved { id: id.to_owned() })
    }

    async fn start_sync(&self, account_id: String) -> Result<Reply> {
        let guard = self
            .imap
            .clone()
            .try_lock_owned()
            .map_err(|_| Failure::Code("imap_busy"))?;
        let settings = self.settings(&account_id).await?;
        let needed: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM imap_sync_state WHERE account_id=? AND folder=? AND last_sync>=unixepoch()-3)")
            .bind(&account_id).bind(FOLDER).fetch_one(&self.pool).await?;
        if !needed {
            return Err(Failure::Code("imap_recent_sync"));
        }
        sqlx::query("INSERT INTO imap_sync_state(account_id,folder,uid_validity,last_uid,outcome) VALUES (?,?,0,0,'running')
            ON CONFLICT(account_id,folder) DO UPDATE SET outcome='running'")
            .bind(&account_id).bind(FOLDER).execute(&self.pool).await?;
        let service = self.clone();
        let id = account_id.clone();
        tokio::spawn(async move {
            let _guard = guard;
            let report = match tokio::time::timeout(
                Duration::from_secs(45),
                transport::synchronize(&service.pool, &id, &settings),
            )
            .await
            {
                Ok(Ok(true)) => "more_available",
                Ok(Ok(false)) => "ok",
                Ok(Err(ref error)) => error.code(),
                Err(_) => "imap_timeout",
            };
            let _ = sqlx::query("UPDATE imap_sync_state SET last_sync=unixepoch(),outcome=? WHERE account_id=? AND folder=?")
                .bind(report).bind(id).bind(FOLDER).execute(&service.pool).await;
        });
        Ok(Reply::SyncStarted { account_id })
    }
}

fn response(result: Result<Reply>) -> Response {
    match result {
        Ok(data) => Response::Inbox {
            data: Box::new(data),
        },
        Err(error) => Response::Error {
            code: error.code().into(),
            detail: None,
        },
    }
}
