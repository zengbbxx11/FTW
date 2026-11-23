use crate::CoreError;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::{path::Path, time::Duration};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn open(data: &Path) -> Result<SqlitePool, CoreError> {
    let options = SqliteConnectOptions::new()
        .filename(data.join("workbench.db"))
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    if let Err(error) = MIGRATOR.run(&pool).await {
        pool.close().await;
        return Err(error.into());
    }
    Ok(pool)
}

pub async fn schema_version(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1")
        .fetch_one(pool)
        .await
}
