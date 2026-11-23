use super::{store, Settings, FOLDER};
use crate::mail::{credentials, Failure, Result};
use futures_util::TryStreamExt;
use sqlx::SqlitePool;
use std::{sync::Arc, time::Duration};
use tokio::net::TcpStream;
use tokio_rustls::{
    client::TlsStream,
    rustls::{self, pki_types::ServerName},
    TlsConnector,
};

type Session = async_imap::Session<TlsStream<TcpStream>>;
const MAX_BYTES: u32 = 2 * 1024 * 1024;
const BATCH: usize = 25;

pub(super) async fn connect(settings: &Settings) -> Result<Session> {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| Failure::Code("imap_tls_failed"))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let domain = ServerName::try_from(settings.host.clone())
        .map_err(|_| Failure::Code("imap_configuration_invalid"))?;
    let stream = TcpStream::connect((settings.host.as_str(), settings.port as u16))
        .await
        .map_err(|_| Failure::Code("imap_connection_failed"))?;
    let encrypted = TlsConnector::from(Arc::new(config))
        .connect(domain, stream)
        .await
        .map_err(|_| Failure::Code("imap_tls_failed"))?;
    let mut client = async_imap::Client::new(encrypted);
    client
        .read_response()
        .await
        .map_err(|_| Failure::Code("imap_connection_failed"))?;
    let secret = credentials::read(&settings.protected)
        .map_err(|_| Failure::Code("credential_unavailable"))?;
    client
        .login(&settings.username, secret)
        .await
        .map_err(|_| Failure::Code("imap_login_failed"))
}

pub(super) async fn synchronize(
    pool: &SqlitePool,
    account: &str,
    settings: &Settings,
) -> Result<bool> {
    let mut session = connect(settings).await?;
    let result = collect(pool, account, &mut session).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), session.logout()).await;
    result
}

pub(super) async fn collect<T>(
    pool: &SqlitePool,
    account: &str,
    session: &mut async_imap::Session<T>,
) -> Result<bool>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    let summary = session
        .examine(FOLDER)
        .await
        .map_err(|_| Failure::Code("imap_inbox_unavailable"))?;
    let validity = summary
        .uid_validity
        .ok_or(Failure::Code("imap_missing_uidvalidity"))?;
    let prior: Option<(i64, i64)> = sqlx::query_as(
        "SELECT uid_validity,last_uid FROM imap_sync_state WHERE account_id=? AND folder=?",
    )
    .bind(account)
    .bind(FOLDER)
    .fetch_optional(pool)
    .await?;
    let last_uid = prior
        .filter(|(old, _)| *old == i64::from(validity))
        .map_or(0, |(_, last)| last);
    let start = last_uid + 1;
    sqlx::query("INSERT INTO imap_sync_state(account_id,folder,uid_validity,last_uid,outcome) VALUES (?,?,?,?,'running')
        ON CONFLICT(account_id,folder) DO UPDATE SET uid_validity=excluded.uid_validity,last_uid=excluded.last_uid")
        .bind(account).bind(FOLDER).bind(i64::from(validity)).bind(last_uid).execute(pool).await?;
    let mut ids: Vec<_> = session
        .uid_search(format!("UID {start}:*"))
        .await
        .map_err(|_| Failure::Code("imap_search_failed"))?
        .into_iter()
        .filter(|uid| i64::from(*uid) >= start)
        .collect();
    ids.sort_unstable();
    let more = ids.len() > BATCH;
    for uid in ids.into_iter().take(BATCH) {
        let spec = uid.to_string();
        let meta: Vec<_> = session
            .uid_fetch(&spec, "(UID RFC822.SIZE INTERNALDATE)")
            .await
            .map_err(|_| Failure::Code("imap_fetch_failed"))?
            .try_collect()
            .await
            .map_err(|_| Failure::Code("imap_fetch_failed"))?;
        if let Some(item) = meta.iter().find(|item| item.uid == Some(uid)) {
            let received = item.internal_date().map(|date| date.timestamp());
            let size = item.size.ok_or(Failure::Code("imap_fetch_failed"))?;
            let oversized = size > MAX_BYTES;
            let query = if oversized {
                "(UID BODY.PEEK[HEADER]<0.65536>)".to_owned()
            } else {
                format!("(UID BODY.PEEK[]<0.{MAX_BYTES}>)")
            };
            let parts: Vec<_> = session
                .uid_fetch(&spec, &query)
                .await
                .map_err(|_| Failure::Code("imap_fetch_failed"))?
                .try_collect()
                .await
                .map_err(|_| Failure::Code("imap_fetch_failed"))?;
            if let Some(message) = parts.iter().find(|item| item.uid == Some(uid)) {
                let raw = message
                    .body()
                    .or_else(|| message.header())
                    .ok_or(Failure::Code("imap_fetch_failed"))?;
                let clip = if oversized {
                    65_536
                } else {
                    MAX_BYTES as usize
                };
                store::save(
                    pool,
                    account,
                    validity,
                    uid,
                    &raw[..raw.len().min(clip)],
                    received,
                    oversized || raw.len() > clip,
                )
                .await?;
            }
        }
        // A UID expunged between SEARCH and FETCH is safe to pass. SQL errors above prevent advancement.
        sqlx::query("UPDATE imap_sync_state SET last_uid=? WHERE account_id=? AND folder=?")
            .bind(i64::from(uid))
            .bind(account)
            .bind(FOLDER)
            .execute(pool)
            .await?;
    }
    Ok(more)
}
