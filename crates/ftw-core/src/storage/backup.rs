use super::{schema_version, MIGRATOR};
use crate::{mail::attachments, CoreError};
use ftw_ipc::{
    data::{Backup, Result as DataReply},
    display_path,
};
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
    let mut source = pool.acquire().await?;
    create_from(&mut source, root, reason).await
}

async fn create_from(
    source: &mut SqliteConnection,
    root: &Path,
    reason: &str,
) -> Result<PathBuf, CoreError> {
    let directory = root.join("backups");
    fs::create_dir_all(&directory).map_err(backup_error)?;
    let name = format!("workbench-{reason}-{}", Uuid::new_v4());
    let partial = directory.join(format!("{name}.partial"));
    let written = copy_to(source, &partial).await;
    let result = async {
        written?;
        let mut snapshot = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&partial)
                .read_only(true),
        )
        .await?;
        copy_objects(
            &mut snapshot,
            &root.join("data/mail-objects"),
            &directory.join("mail-objects"),
        )
        .await?;
        snapshot.close().await?;
        Ok::<_, CoreError>(())
    }
    .await;
    if let Err(error) = result {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    let destination = directory.join(format!("{name}.db"));
    fs::rename(&partial, &destination).map_err(backup_error)?;
    Ok(destination)
}

pub async fn create_manual(pool: &SqlitePool, root: &Path) -> Result<PathBuf, CoreError> {
    let mut source = pool.acquire().await?;
    let snapshot = create_from(&mut source, root, "manual").await?;
    let filename = snapshot.file_name().ok_or_else(invalid)?.to_string_lossy();
    if let Err(error) = sqlx::query(
        "INSERT INTO audit_logs(id,actor_id,source,entity,entity_id,operation)
        VALUES (?,'desktop-user','manual','backup',?,'create')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(filename.as_ref())
    .execute(&mut *source)
    .await
    {
        let _ = fs::remove_file(&snapshot);
        return Err(error.into());
    }
    Ok(snapshot)
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
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .unwrap_or_default();
        rows.push((
            modified,
            Backup {
                filename,
                bytes: metadata.len(),
                created_at: modified.as_secs() as i64,
            },
        ));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.filename.cmp(&b.1.filename)));
    Ok(rows.into_iter().map(|(_, row)| row).collect())
}

pub async fn restore(pool: &SqlitePool, root: &Path, filename: &str) -> Result<String, CoreError> {
    let directory = root.join("backups");
    let mut original = open_snapshot(&directory, filename).await?;
    let stage = directory.join(format!("restore-{}.partial", Uuid::new_v4()));
    let result = async {
        validate(&mut original).await?;
        copy_objects(&mut original, &directory.join("mail-objects"), &root.join("data/mail-objects")).await?;
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
                UPDATE app_change_epoch SET epoch=lower(hex(randomblob(16)));
                UPDATE email_approval_attempts SET state='interrupted',outcome='approval_interrupted',finished_at=unixepoch() WHERE state='waiting';
                UPDATE mail_agent_policy SET mode='manual',revision=revision+1 WHERE mode='automatic';
                INSERT OR IGNORE INTO mail_agent_clients(id,name,token_digest) VALUES ('local-mcp','本机 Agent',randomblob(32));
                UPDATE mail_agent_clients SET revoked=1 WHERE NOT revoked AND id<>'local-mcp';
                UPDATE mail_agent_clients SET revoked=0,automatic=0 WHERE id='local-mcp';
                UPDATE mail_assets SET agent_allowed=0 WHERE agent_allowed;
                UPDATE mail_agent_reviews SET state='failed',outcome='approval_interrupted' WHERE state='waiting';
                UPDATE inbox_messages SET download_outcome='approval_interrupted' WHERE download_outcome='running';
                UPDATE imap_sync_state SET outcome='interrupted' WHERE outcome='running';")
                .execute(&mut staging).await?;
            sqlx::query("INSERT INTO audit_logs(id,actor_id,source,entity,entity_id,operation)
                 VALUES (?,'desktop-user','manual','backup',?,'restore')")
                .bind(Uuid::new_v4().to_string()).bind(filename).execute(&mut staging).await?;
            let mut destination = pool.acquire().await?;
            // Keep the single workspace connection between the safety copy and activation.
            let backup = create_from(&mut destination, root, "before-restore").await?;
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

pub async fn inspect(root: &Path, filename: &str) -> Result<DataReply, CoreError> {
    let directory = root.join("backups");
    let mut snapshot = open_snapshot(&directory, filename).await?;
    let schema_version = validate(&mut snapshot).await?;
    let customers = if schema_version >= 2 {
        let sql = if schema_version >= 13 {
            "SELECT count(*) FROM contacts WHERE merged_into IS NULL"
        } else {
            "SELECT count(*) FROM contacts"
        };
        sqlx::query_scalar(sql).fetch_one(&mut snapshot).await?
    } else {
        0
    };
    let mut mail_records = 0;
    for (version, sql) in [
        (3, "SELECT count(*) FROM email_messages"),
        (4, "SELECT count(*) FROM inbox_messages"),
    ] {
        if schema_version >= version {
            mail_records += sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&mut snapshot)
                .await?;
        }
    }
    let objects = referenced_objects(&mut snapshot).await?;
    let source = directory.join("mail-objects");
    for (sha, size) in &objects {
        attachments::read_object(&source, sha, *size as usize)
            .await
            .map_err(backup_error)?;
    }
    Ok(DataReply::BackupInspection {
        filename: filename.to_owned(),
        schema_version,
        customers,
        mail_records,
        objects: objects.len(),
    })
}

async fn open_snapshot(directory: &Path, filename: &str) -> Result<SqliteConnection, CoreError> {
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
    Ok(SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(&input).read_only(true),
    )
    .await?)
}

async fn copy_objects(
    db: &mut SqliteConnection,
    source: &Path,
    destination: &Path,
) -> Result<(), CoreError> {
    for (sha, size) in referenced_objects(db).await? {
        let bytes = attachments::read_object(source, &sha, size as usize)
            .await
            .map_err(backup_error)?;
        attachments::store(destination, &bytes)
            .await
            .map_err(backup_error)?;
    }
    Ok(())
}

async fn referenced_objects(db: &mut SqliteConnection) -> Result<Vec<(String, i64)>, CoreError> {
    let available: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='mail_assets' AND type='table')",
    )
    .fetch_one(&mut *db)
    .await?;
    if !available {
        return Ok(Vec::new());
    }
    let documents: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='customer_documents' AND type='table')")
        .fetch_one(&mut *db).await?;
    let mut query = String::from(
        "SELECT sha256,size FROM mail_assets
        UNION SELECT full_sha256,full_size FROM inbox_messages WHERE full_sha256 IS NOT NULL",
    );
    if documents {
        query.push_str(" UNION SELECT sha256,size FROM customer_documents");
    }
    Ok(sqlx::query_as(&query).fetch_all(db).await?)
}

fn valid_name(name: &str) -> bool {
    name.starts_with("workbench-")
        && name.ends_with(".db")
        && name.len() < 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'))
}

async fn validate(db: &mut SqliteConnection) -> Result<i64, CoreError> {
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
    Ok(applied.last().ok_or_else(invalid)?.0)
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
