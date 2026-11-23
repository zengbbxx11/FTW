mod backup;

use crate::CoreError;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::{path::Path, time::Duration};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn open(data: &Path, root: &Path) -> Result<SqlitePool, CoreError> {
    let filename = data.join("workbench.db");
    let existing = filename
        .try_exists()
        .map_err(|error| CoreError::Io("data_unavailable", error))?;
    let options = SqliteConnectOptions::new()
        .filename(filename)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let migration = async {
        if let Some(latest) = MIGRATOR.iter().last().filter(|_| existing) {
            backup::before_migration(&pool, root, latest.version).await?;
        }
        MIGRATOR.run(&pool).await?;
        Ok::<_, CoreError>(())
    }
    .await;
    if let Err(error) = migration {
        pool.close().await;
        return Err(error);
    }
    Ok(pool)
}

pub async fn schema_version(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1")
        .fetch_one(pool)
        .await
}
