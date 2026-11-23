use super::{schema_version, MIGRATOR};
use crate::CoreError;
use ftw_ipc::{data::Backup, display_path};
use libsqlite3_sys::{
    sqlite3_backup_finish, sqlite3_backup_init, sqlite3_backup_step, SQLITE_DONE, SQLITE_OK,
};
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection, SqlitePool};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
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
    if current < latest {
        create(pool, root, &format!("before-v{latest}")).await?;
    }
    Ok(())
}

pub async fn create(pool: &SqlitePool, root: &Path, reason: &str) -> Result<PathBuf, CoreError> {
    let directory = root.join("backups");
    fs::create_dir_all(&directory).map_err(backup_error)?;
    let name = format!("workbench-{reason}-{}", Uuid::new_v4());
    let partial = directory.join(format!("{name}.partial"));
    let mut source = pool.acquire().await?;
    if let Err(error) = copy_to(&mut source, &partial).await {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    let destination = directory.join(format!("{name}.db"));
    fs::rename(&partial, &destination).map_err(backup_error)?;
    Ok(destination)
}

pub fn list(root: &Path) -> Result<Vec<Backup>, CoreError> {
    let path = root.join("backups");
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(backup_error(error)),
    };
    let mut rows = Vec::new();
    for entry in entries {
        let entry = entry.map_err(backup_error)?;
        let filename = entry.file_name().to_string_lossy().into_owned();
        if !valid_name(&filename) || !entry.file_type().map_err(backup_error)?.is_file() {
            continue;
        }
        let metadata = entry.metadata().map_err(backup_error)?;
        let created_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |time| time.as_secs() as i64);
        rows.push(Backup {
            filename,
            bytes: metadata.len(),
            created_at,
        });
    }
    rows.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.filename.cmp(&b.filename))
    });
    Ok(rows)
}

pub async fn restore(pool: &SqlitePool, root: &Path, filename: &str) -> Result<String, CoreError> {
    let directory = root.join("backups");
    if !valid_name(filename) {
        return Err(invalid());
    }
    let input = directory.join(filename);
    if !fs::symlink_metadata(&input)
        .map_err(backup_error)?
        .file_type()
        .is_file()
    {
        return Err(invalid());
    }
    let mut original = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(&input).read_only(true),
    )
    .await?;
    let stage = directory.join(format!("restore-{}.partial", Uuid::new_v4()));
    let result = async {
        validate(&mut original).await?;
        copy_to(&mut original, &stage).await?;
        let mut staging = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&stage)).await?;
        let prepared = async {
            MIGRATOR.run(&mut staging).await?;
            // An old snapshot can contain pending approvals or unsent network jobs.
            // Require fresh review and never resume historical submissions.
            sqlx::raw_sql("UPDATE email_approvals SET expires_at=0;
                UPDATE email_drafts SET state='pending_approval' WHERE state='approved';
                UPDATE email_drafts SET state='delivery_unknown' WHERE state IN ('queued','sending');
                UPDATE email_outbox_jobs SET state='delivery_unknown',outcome='restored_without_resend' WHERE state IN ('queued','sending');
                UPDATE imap_sync_state SET outcome='interrupted' WHERE outcome='running';")
                .execute(&mut staging).await?;
            sqlx::query("INSERT INTO audit_logs(id,actor_id,source,entity,entity_id,operation)
                 VALUES (?,'desktop-user','manual','backup',?,'restore')")
                .bind(Uuid::new_v4().to_string()).bind(filename).execute(&mut staging).await?;
            let backup = create(pool, root, "before-restore").await?;
            let mut destination = pool.acquire().await?;
            copy(&mut staging, &mut destination).await?;
            Ok::<_,CoreError>(display_path(&backup))
        }.await;
        let _ = staging.close().await;
        prepared
    }.await;
    let _ = original.close().await;
    let _ = fs::remove_file(&stage);
    result
}

fn valid_name(name: &str) -> bool {
    name.starts_with("workbench-")
        && name.ends_with(".db")
        && name.len() < 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'))
}

async fn validate(db: &mut SqliteConnection) -> Result<(), CoreError> {
    let integrity: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&mut *db)
        .await?;
    if integrity != "ok" {
        return Err(invalid());
    }
    let applied: Vec<(i64, Vec<u8>, bool)> =
        sqlx::query_as("SELECT version,checksum,success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(db)
            .await?;
    if applied.is_empty() || applied.len() > MIGRATOR.iter().count() {
        return Err(invalid());
    }
    for ((version, checksum, success), expected) in applied.iter().zip(MIGRATOR.iter()) {
        if !success
            || *version != expected.version
            || checksum.as_slice() != expected.checksum.as_ref()
        {
            return Err(invalid());
        }
    }
    Ok(())
}

async fn copy_to(source: &mut SqliteConnection, path: &Path) -> Result<(), CoreError> {
    let mut destination = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true),
    )
    .await?;
    let result = copy(source, &mut destination).await;
    destination.close().await?;
    result
}

async fn copy(
    source: &mut SqliteConnection,
    destination: &mut SqliteConnection,
) -> Result<(), CoreError> {
    let mut source = source.lock_handle().await?;
    let mut destination = destination.lock_handle().await?;
    // The guards suspend both SQLite workers until the backup operation finishes.
    let status = unsafe {
        let backup = sqlite3_backup_init(
            destination.as_raw_handle().as_ptr(),
            c"main".as_ptr(),
            source.as_raw_handle().as_ptr(),
            c"main".as_ptr(),
        );
        if backup.is_null() {
            libsqlite3_sys::sqlite3_errcode(destination.as_raw_handle().as_ptr())
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
    Ok(())
}

fn invalid() -> CoreError {
    backup_error(io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid FTW backup",
    ))
}
fn backup_error(error: io::Error) -> CoreError {
    CoreError::Io("migration_backup_failed", error)
}
