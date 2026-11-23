use super::schema_version;
use crate::CoreError;
use libsqlite3_sys::{
    sqlite3_backup_finish, sqlite3_backup_init, sqlite3_backup_step, SQLITE_DONE, SQLITE_OK,
};
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection, SqlitePool};
use std::{fs, io, path::Path};
use uuid::Uuid;

pub async fn before_migration(
    pool: &SqlitePool,
    root: &Path,
    latest: i64,
) -> Result<(), CoreError> {
    let initialized: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='_sqlx_migrations' AND type='table')",
    )
    .fetch_one(pool)
    .await?;
    let current = if initialized {
        schema_version(pool).await?
    } else {
        0
    };
    if current >= latest {
        return Ok(());
    }

    let directory = root.join("backups");
    fs::create_dir_all(&directory).map_err(backup_error)?;
    let name = format!("workbench-before-v{latest}-{}", Uuid::new_v4());
    let partial = directory.join(format!("{name}.partial"));
    let result = copy_database(pool, &partial).await;
    if let Err(error) = result {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    fs::rename(&partial, directory.join(format!("{name}.db"))).map_err(backup_error)?;
    Ok(())
}

async fn copy_database(pool: &SqlitePool, path: &Path) -> Result<(), CoreError> {
    let mut source = pool.acquire().await?;
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let mut destination = SqliteConnection::connect_with(&options).await?;

    {
        let mut source_handle = source.lock_handle().await?;
        let mut destination_handle = destination.lock_handle().await?;
        // Both worker threads are paused while these guard objects hold the raw handles.
        // The backup uses SQLx's own SQLite library, and both databases are in its main schema.
        let status = unsafe {
            let backup = sqlite3_backup_init(
                destination_handle.as_raw_handle().as_ptr(),
                c"main".as_ptr(),
                source_handle.as_raw_handle().as_ptr(),
                c"main".as_ptr(),
            );
            if backup.is_null() {
                libsqlite3_sys::sqlite3_errcode(destination_handle.as_raw_handle().as_ptr())
            } else {
                let step = sqlite3_backup_step(backup, -1);
                let finish = sqlite3_backup_finish(backup);
                if step == SQLITE_DONE {
                    finish
                } else {
                    step
                }
            }
        };
        if status != SQLITE_OK {
            return Err(backup_error(io::Error::other(format!(
                "SQLite backup failed ({status})"
            ))));
        }
    }
    destination.close().await?;
    Ok(())
}

fn backup_error(error: io::Error) -> CoreError {
    CoreError::Io("migration_backup_failed", error)
}
